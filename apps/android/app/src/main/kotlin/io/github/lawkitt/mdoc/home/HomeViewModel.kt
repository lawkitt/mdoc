package io.github.lawkitt.mdoc.home

import android.content.ContentResolver
import android.net.Uri
import android.provider.OpenableColumns
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import io.github.lawkitt.mdoc.MdocApplication
import io.github.lawkitt.mdoc.core.AppCore
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.CaseSummary
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.DocumentPhase
import io.github.lawkitt.mdoc.core.DocumentSummary
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.SourceKind
import io.github.lawkitt.mdoc.jobs.CancelReason
import io.github.lawkitt.mdoc.jobs.Jobs
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** What the add sheet is adding. */
sealed interface PendingAdd {
    val suggestedTitle: String

    data class Text(override val suggestedTitle: String) : PendingAdd

    data class Received(val source: ImportSource, override val suggestedTitle: String) : PendingAdd
}

/** Home's view state, built from the core's snapshots. */
data class HomeState(
    val documents: List<DocumentSummary> = emptyList(),
    val cases: List<CaseSummary> = emptyList(),
    /** Above the detector floor: Find names is offered (ADR 0088, 0089). */
    val canFindNames: Boolean = true,
    /** The library has been read at least once. */
    val loaded: Boolean = false,
    val error: String? = null,
    val adding: PendingAdd? = null,
    /** A file mdoc could not take. */
    val refused: String? = null,
)

class HomeViewModel(private val core: () -> AppCore, private val jobs: Jobs? = null) : ViewModel() {
    private val mutable = MutableStateFlow(HomeState())
    val state: StateFlow<HomeState> = mutable.asStateFlow()

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
            mutable.update { next.copy(adding = it.adding, refused = it.refused) }
        }
    }

    fun startPaste(title: String) = mutable.update { it.copy(adding = PendingAdd.Text(title)) }

    /** Copies picked files into mdoc's storage, then asks for a title and case. */
    fun received(resolver: ContentResolver, uris: List<Uri>, kind: SourceKind?, fallbackTitle: String) {
        if (uris.isEmpty()) return
        viewModelScope.launch {
            val outcome = withContext(Dispatchers.IO) {
                runCatching {
                    val storage = core().storage
                    val folder = File(storage.dir(MdocStorage.Area.Originals), java.util.UUID.randomUUID().toString()).apply { mkdirs() }
                    val files = uris.mapIndexed { index, uri ->
                        val name = displayName(resolver, uri) ?: "${fallbackTitle}.${extension(resolver, uri)}"
                        // Each in a folder of its own: two photos may share a name.
                        val target = File(File(folder, "${index + 1}").apply { mkdirs() }, MdocStorage.safeFileName(name))
                        checkNotNull(resolver.openInputStream(uri)).use { input -> target.outputStream().use(input::copyTo) }
                        target
                    }
                    val first = files.first()
                    val detected = kind ?: kindOf(first) ?: error(first.name)
                    val source = if (detected == SourceKind.Image || detected == SourceKind.Scan) {
                        ImportSource.Pages(files, detected)
                    } else {
                        ImportSource.File(first, detected)
                    }
                    PendingAdd.Received(source, first.nameWithoutExtension.ifEmpty { fallbackTitle })
                }
            }
            outcome.onSuccess { add -> mutable.update { it.copy(adding = add) } }
                .onFailure { error -> mutable.update { it.copy(refused = error.message) } }
        }
    }

    /** Camera pages already in mdoc's storage. */
    fun scanned(pages: List<File>, title: String) {
        if (pages.isNotEmpty()) mutable.update { it.copy(adding = PendingAdd.Received(ImportSource.Pages(pages, SourceKind.Scan), title)) }
    }

    fun cancelAdd() = mutable.update { it.copy(adding = null) }

    fun refusalShown() = mutable.update { it.copy(refused = null) }

    /** Stores the document in its case and returns its id; the caller opens it and reads it. */
    suspend fun add(text: String?, title: String, case: CaseChoice): DocumentId? {
        val adding = mutable.value.adding ?: return null
        val source = when (adding) {
            is PendingAdd.Text -> ImportSource.Text(text.orEmpty())
            is PendingAdd.Received -> adding.source
        }
        return withContext(Dispatchers.IO) {
            runCatching { core().documents.importDocument(source, title, case).id }
                .onFailure { error -> mutable.update { it.copy(refused = error.message) } }
                .getOrNull()
        }.also {
            mutable.update { it.copy(adding = null) }
            refresh()
        }
    }

    private fun displayName(resolver: ContentResolver, uri: Uri): String? =
        resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) cursor.getString(0) else null
        }

    private fun extension(resolver: ContentResolver, uri: Uri): String = when (resolver.getType(uri)) {
        "application/pdf" -> "pdf"
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" -> "docx"
        "image/png" -> "png"
        else -> "jpg"
    }

    companion object {
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
            "jpg", "jpeg", "png", "heic", "heif", "webp" -> SourceKind.Image
            else -> null
        }

        fun factory(app: MdocApplication): ViewModelProvider.Factory = viewModelFactory {
            initializer { HomeViewModel({ app.core }, app.jobs) }
        }

        /** The status line under a document's title. */
        fun status(document: DocumentSummary, canFindNames: Boolean): StatusLine = when (document.phase) {
            DocumentPhase.Recognizing ->
                if (document.kind.needsRecognition) StatusLine.NotRecognized else StatusLine.ImportNotFinished
            DocumentPhase.Unscanned -> if (canFindNames) StatusLine.ReadyToFind else StatusLine.ReadyToMark
            DocumentPhase.Reviewing -> StatusLine.ToReview(document.pendingCount)
            // Find names ran and found nothing (ADR 0087).
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
