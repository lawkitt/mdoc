package io.github.lawkitt.mdoc.home

import android.content.ContentResolver
import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import io.github.lawkitt.mdoc.MdocApplication
import io.github.lawkitt.mdoc.core.AppCore
import io.github.lawkitt.mdoc.core.CameraCapture
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.CaseSummary
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.DocumentPhase
import io.github.lawkitt.mdoc.core.DocumentSummary
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.Originals
import io.github.lawkitt.mdoc.core.ScanSession
import io.github.lawkitt.mdoc.core.SourceKind
import io.github.lawkitt.mdoc.jobs.CancelReason
import io.github.lawkitt.mdoc.jobs.Jobs
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStream
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.getAndUpdate
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** What the add sheet is adding. */
sealed interface PendingAdd {
    val suggestedTitle: String

    data class Text(override val suggestedTitle: String) : PendingAdd

    data class Received(val source: ImportSource, override val suggestedTitle: String, val folder: File) : PendingAdd
}

/** Home's view state, built from the core's snapshots. */
data class HomeState(
    val documents: List<DocumentSummary> = emptyList(),
    val cases: List<CaseSummary> = emptyList(),
    /** Above the detector floor: Find names is offered. */
    val canFindNames: Boolean = true,
    /** The library has been read at least once. */
    val loaded: Boolean = false,
    val error: String? = null,
    val adding: PendingAdd? = null,
    val scanPageCount: Int = 0,
    /** A file mdoc could not take. */
    val refused: String? = null,
)

class HomeViewModel(
    private val core: () -> AppCore,
    private val jobs: Jobs? = null,
    /** The scan in progress: the app's, so a destroyed activity keeps it. */
    private val scan: ScanSession = ScanSession(),
    /** Where the scan is saved for a process stopped while the camera is open. */
    private val saved: SavedStateHandle = SavedStateHandle(),
) : ViewModel() {
    private val mutable = MutableStateFlow(HomeState())
    val state: StateFlow<HomeState> = mutable.asStateFlow()
    private var receiving: Job? = null

    init {
        // A new process: the scan comes back from the saved state, its shot
        // in flight included, and shots no scan waits for go.
        if (scan.pages.isEmpty() && scan.capture == null) {
            scan.pages = saved.get<ArrayList<String>>(SCAN_PAGES).orEmpty().map(::File).filter { it.isFile }
            scan.capture = saved.get<String>(SCAN_CAPTURE)?.let(::File)?.takeIf { it.isFile }
        }
        scan.sweep()
        saveScan()
        if (scan.capture == null && !scan.processing) mutable.update { it.copy(scanPageCount = scan.pages.size) }
    }

    private fun saveScan() {
        saved[SCAN_PAGES] = ArrayList(scan.pages.map { it.path })
        saved[SCAN_CAPTURE] = scan.capture?.path
    }

    /**
     * Deletes a document from Home, as iOS's swipe does. A job on it stops
     * first, so it cannot save the document again.
     */
    fun delete(id: DocumentId) = viewModelScope.launch {
        val jobs = jobs
        if (jobs != null && jobs.current.value?.documentId == id) {
            jobs.cancel(CancelReason.User)
            jobs.current.first { it == null }
        }
        withContext(Dispatchers.IO) { runCatching { core().documents.delete(id) } }
            .onFailure { error -> mutable.update { it.copy(refused = error.message) } }
        refresh()
    }

    /** Reads the library off the main thread; a core that failed to open is tried again. */
    fun refresh() {
        viewModelScope.launch(Dispatchers.IO) {
            val next = runCatching {
                val core = core()
                val library = core.documents.snapshot()
                HomeState(
                    documents = library.documents,
                    cases = library.cases,
                    canFindNames = core.models.snapshot().detector != null,
                    loaded = true,
                )
            }.getOrElse { HomeState(loaded = true, error = it.message ?: it.toString()) }
            mutable.update { next.copy(adding = it.adding, scanPageCount = it.scanPageCount, refused = it.refused) }
        }
    }

    fun startPaste(title: String) {
        receiving?.cancel()
        replaceAdding(PendingAdd.Text(title))
    }

    /** Stages clean copies before asking for a title and case. */
    fun received(resolver: ContentResolver, uris: List<Uri>, kind: SourceKind?, fallbackTitle: String) {
        if (uris.isEmpty()) return
        receiving?.cancel()
        receiving = viewModelScope.launch(Dispatchers.IO) {
            var folder: File? = null
            var handedOff = false
            var refusal: String? = null
            try {
                val originals = Originals(core().storage)
                val stage = originals.newStage()
                folder = stage
                var firstKind: SourceKind? = null
                var title = fallbackTitle
                val staged = uris.mapIndexed { index, uri ->
                    ensureActive()
                    val mime = resolver.getType(uri)
                    val extension = extensionFor(mime)
                    val name = displayName(resolver, uri) ?: when {
                        extension != null -> "$fallbackTitle.$extension"
                        kind?.needsRecognition == true -> fallbackTitle
                        else -> error(mime ?: fallbackTitle)
                    }
                    val parent = File(stage, "${index + 1}").apply { check(mkdir()) }
                    val target = File(parent, MdocStorage.safeFileName(name))
                    checkNotNull(resolver.openInputStream(uri)).buffered().use { input ->
                        val sourceKind = kind ?: receivedKind(input, target, extension, mime)
                        if (index == 0) {
                            firstKind = sourceKind
                            title = target.nameWithoutExtension.ifEmpty { fallbackTitle }
                        } else {
                            check(firstKind?.needsRecognition == true && sourceKind.needsRecognition) { name }
                        }
                        if (sourceKind.needsRecognition) {
                            originals.stageImage(target, readImage(input))
                        } else {
                            target.outputStream().use(input::copyTo)
                            target
                        }
                    }
                }
                val detected = checkNotNull(firstKind)
                val source = if (detected.needsRecognition) {
                    ImportSource.Pages(staged, detected)
                } else {
                    ImportSource.File(staged.single(), detected)
                }
                withContext(Dispatchers.Main.immediate) {
                    replaceAdding(PendingAdd.Received(source, title, stage))
                    handedOff = true
                }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                refusal = error.message ?: fallbackTitle
            } finally {
                if (!handedOff) folder?.deleteRecursively()
                if (refusal != null) mutable.update { it.copy(refused = refusal) }
            }
        }
    }

    private fun receivedKind(input: InputStream, target: File, extension: String?, mime: String?): SourceKind {
        // A photo may have no extension or a misleading name. Check the
        // photo header before allowing a plain file copy to disk.
        input.mark(12)
        val header = ByteArray(12)
        var length = 0
        while (length < header.size) {
            val read = input.read(header, length, header.size - length)
            if (read < 0) break
            length += read
        }
        input.reset()
        val named = kindOf(target)
        val typed = extension?.let { kindOf(File("file.$it")) }
        val signature = String(header, 0, 4, Charsets.US_ASCII)
        val image = Originals.imageExtension(header) ?: when {
            signature in setOf("II*\u0000", "MM\u0000*", "II+\u0000", "MM\u0000+") -> "tiff"
            length == 12 && String(header, 4, 4, Charsets.US_ASCII) == "ftyp" -> "heic"
            else -> null
        }
        if (image != null) return kindOf(File("file.$image")) ?: error(target.name)
        if (typed == SourceKind.Image) return typed
        if (mime?.startsWith("image/", ignoreCase = true) == true && named != SourceKind.Image) error(target.name)
        return named ?: typed ?: error(target.name)
    }

    /**
     * The camera's output for the next page: a raw file outside
     * `originals/`, saved with the scan so a stopped process finds it.
     */
    fun takePage(context: Context): Uri? {
        if (scan.processing || scan.capture != null) return null
        return runCatching { CameraCapture.create(context) }
            .onSuccess {
                scan.capture = CameraCapture.file(context, it)
                saveScan()
                mutable.update { state -> state.copy(scanPageCount = 0) }
            }
            .onFailure { error -> mutable.update { it.copy(refused = error.message) } }
            .getOrNull()
    }

    /** The camera returned: its shot is cleaned into the scan's stage and the raw file deleted at once. */
    fun cameraResult(taken: Boolean, launchError: String? = null) {
        val raw = scan.capture ?: return
        scan.capture = null
        scan.processing = true
        saveScan()
        viewModelScope.launch {
            val previousFolder = scan.pages.firstOrNull()?.parentFile
            var folder = previousFolder
            var kept: File? = null
            try {
                kept = withContext(Dispatchers.IO) {
                    try {
                        val size = raw.length()
                        if (!taken || size <= 0) return@withContext null
                        check(size <= MAX_IMAGE_BYTES) { "This image is too large to import." }
                        val bytes = raw.readBytes()
                        raw.delete()
                        val originals = Originals(core().storage)
                        val stage = previousFolder ?: originals.newScanStage().also { folder = it }
                        originals.stageImage(File(stage, "page-${scan.pages.size + 1}.jpg"), bytes)
                    } finally {
                        raw.delete()
                    }
                }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                mutable.update { it.copy(refused = error.message) }
            } finally {
                if (kept != null) scan.pages += kept else if (previousFolder == null) folder?.deleteRecursively()
                scan.processing = false
                saveScan()
                mutable.update { it.copy(scanPageCount = scan.pages.size, refused = launchError ?: it.refused) }
            }
        }
    }

    fun finishScan(title: String) {
        if (scan.pages.isEmpty() || scan.processing) return
        receiving?.cancel()
        val (folder, files) = Originals(core().storage).finishScanStage(scan.pages)
        scan.pages = emptyList()
        saveScan()
        mutable.update { it.copy(scanPageCount = 0) }
        replaceAdding(PendingAdd.Received(ImportSource.Pages(files, SourceKind.Scan), title, folder))
    }

    fun cancelAdd() {
        receiving?.cancel()
        replaceAdding(null)
    }

    private fun replaceAdding(adding: PendingAdd?) {
        val previous = mutable.getAndUpdate { it.copy(adding = adding) }.adding
        (previous as? PendingAdd.Received)?.folder?.deleteRecursively()
    }

    // The scan in progress stays: it is the app's, and a destroyed
    // activity comes back to it.
    override fun onCleared() {
        cancelAdd()
    }

    fun refusalShown() = mutable.update { it.copy(refused = null) }

    /** Stores the document in its case and returns its id; the caller opens it and reads it. */
    suspend fun add(text: String?, title: String, case: CaseChoice): DocumentId? {
        val adding = mutable.getAndUpdate { it.copy(adding = null) }.adding ?: return null
        val source = when (adding) {
            is PendingAdd.Text -> ImportSource.Text(text.orEmpty())
            is PendingAdd.Received -> adding.source
        }
        var handedOff = false
        try {
            return withContext(Dispatchers.IO) {
                core().documents.importDocument(source, title, case).id.also { handedOff = true }
            }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            mutable.update { it.copy(refused = error.message) }
            return null
        } finally {
            if (!handedOff) (adding as? PendingAdd.Received)?.folder?.deleteRecursively()
            refresh()
        }
    }

    private fun displayName(resolver: ContentResolver, uri: Uri): String? =
        resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) cursor.getString(0) else null
        }

    companion object {
        private const val MAX_IMAGE_BYTES = 256 * 1024 * 1024
        private const val SCAN_PAGES = "scan.pages"
        private const val SCAN_CAPTURE = "scan.capture"

        private fun readImage(input: InputStream): ByteArray {
            val out = ByteArrayOutputStream()
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            while (true) {
                val size = input.read(buffer)
                if (size < 0) return out.toByteArray()
                check(size <= MAX_IMAGE_BYTES - out.size()) { "This image is too large to import." }
                out.write(buffer, 0, size)
            }
        }

        /**
         * A pasted text's title: its first line without Markdown's heading
         * marks, at most 60 characters, as on iOS.
         */
        fun suggestedTitle(text: String, fallback: String): String =
            text.trim().lineSequence().firstOrNull().orEmpty().trim('#', ' ').take(60).trim().ifEmpty { fallback }

        /** What a picked file is, by its extension: mdoc opens PDF, DOCX and images. */
        fun kindOf(file: File): SourceKind? = when (file.extension.lowercase()) {
            "pdf" -> SourceKind.Pdf
            "docx" -> SourceKind.Docx
            "jpg", "jpeg", "png", "heic", "heif", "webp", "gif", "tif", "tiff" -> SourceKind.Image
            else -> null
        }

        /**
         * The extension a picked file without a name gets from its MIME
         * type: each format's own, so the core reads it as what it is; null
         * for a type mdoc doesn't open, which is refused with the supported
         * formats.
         */
        fun extensionFor(mime: String?): String? = when (mime?.lowercase()) {
            "application/pdf" -> "pdf"
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" -> "docx"
            "image/jpeg", "image/jpg", "image/pjpeg" -> "jpg"
            "image/png" -> "png"
            "image/webp" -> "webp"
            "image/gif" -> "gif"
            "image/heic" -> "heic"
            "image/heif" -> "heif"
            "image/tiff" -> "tiff"
            else -> null
        }

        fun factory(app: MdocApplication): ViewModelProvider.Factory = viewModelFactory {
            initializer { HomeViewModel({ app.core }, app.jobs, app.scan, createSavedStateHandle()) }
        }

        /** The status line under a document's title. */
        fun status(document: DocumentSummary, canFindNames: Boolean): StatusLine = when (document.phase) {
            DocumentPhase.Recognizing ->
                if (document.kind.needsRecognition) StatusLine.NotRecognized else StatusLine.ImportNotFinished
            DocumentPhase.Unscanned -> if (canFindNames) StatusLine.ReadyToFind else StatusLine.ReadyToMark
            DocumentPhase.Reviewing -> StatusLine.ToReview(document.pendingCount)
            // Find names ran and found nothing.
            DocumentPhase.Ready -> if (document.candidateCount == 0) StatusLine.NothingFound else StatusLine.ReadyToShare
        }
    }
}

sealed interface StatusLine {
    data object NotRecognized : StatusLine

    data object ImportNotFinished : StatusLine

    data object ReadyToFind : StatusLine

    data object ReadyToMark : StatusLine

    data class ToReview(val count: Int) : StatusLine

    data object ReadyToShare : StatusLine

    data object NothingFound : StatusLine
}
