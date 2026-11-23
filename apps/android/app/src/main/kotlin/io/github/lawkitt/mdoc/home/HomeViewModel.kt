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
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.DocumentPhase
import io.github.lawkitt.mdoc.core.DocumentSummary
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.ProjectId
import io.github.lawkitt.mdoc.core.ProjectSummary
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.ProjectDeletion
import io.github.lawkitt.mdoc.core.ProjectDetails
import io.github.lawkitt.mdoc.core.ProjectNameRow
import io.github.lawkitt.mdoc.core.RestoreResult
import io.github.lawkitt.mdoc.core.SavedReplyRow
import io.github.lawkitt.mdoc.core.Originals
import io.github.lawkitt.mdoc.core.ScanSession
import io.github.lawkitt.mdoc.core.SourceKind
import io.github.lawkitt.mdoc.jobs.CancelReason
import io.github.lawkitt.mdoc.jobs.Jobs
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import java.io.FileInputStream
import java.io.RandomAccessFile
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

    /** The Project whose page it was started from: the add sheet starts on it (#1091 Decision 35). */
    val into: ProjectId?

    data class Text(override val suggestedTitle: String, override val into: ProjectId? = null) : PendingAdd

    data class Received(
        val source: ImportSource,
        override val suggestedTitle: String,
        val folder: File,
        /** The name of the file it came from, for "Already in mdoc". */
        val fileName: String? = null,
        override val into: ProjectId? = null,
    ) : PendingAdd
}

/**
 * An import whose file is already in mdoc (#1091 Decision 7): it asks Open
 * Existing or Import Again.
 */
data class RepeatedImport(
    val pending: PendingAdd.Received,
    /** The document to open instead: the most recently updated of those imported from the same bytes. */
    val existing: DocumentId,
    /** The question's name for it: the file's. */
    val name: String,
)

/** Delete's first half: the documents leave the lists while a snackbar offers Undo. */
data class PendingDelete(val ids: List<DocumentId>, val title: String?, val serial: Long)

/** A saved reply shown restored: null while it is restored, [failed] when it can't be. */
data class ShownReply(val reply: SavedReplyRow, val restored: RestoreResult? = null, val failed: Boolean = false)

/** A Project's page beyond its documents: its replies and its names. */
data class ProjectPageData(
    val project: ProjectId,
    val replies: List<SavedReplyRow> = emptyList(),
    val details: ProjectDetails = ProjectDetails(),
)

/** The library's view state, built from the core's snapshots: Home, the drawer and the lists. */
data class HomeState(
    val documents: List<DocumentSummary> = emptyList(),
    /** Pinned first, then most recently updated (#1091 Decision 28). */
    val projects: List<ProjectSummary> = emptyList(),
    /** Above the detector floor: Find names is offered. */
    val canFindNames: Boolean = true,
    /** The library has been read at least once. */
    val loaded: Boolean = false,
    val error: String? = null,
    val adding: PendingAdd? = null,
    val scanPageCount: Int = 0,
    /** A file mdoc could not take. */
    val refused: String? = null,
    /** Deleted, their snackbar still offering Undo. */
    val pendingDelete: PendingDelete? = null,
    /** "Deleted <name>" after a Project's delete. */
    val deletedProject: String? = null,
    val repeated: RepeatedImport? = null,
    val page: ProjectPageData? = null,
    /** A saved reply a Project's Replies shows restored. */
    val shownReply: ShownReply? = null,
) {
    private val hidden: Set<DocumentId> get() = pendingDelete?.ids?.toSet().orEmpty()

    /** Every document the lists show, most recently changed first. */
    val listed: List<DocumentSummary> get() = documents.filter { it.id !in hidden }

    /** The documents with names left to review, for the drawer's To review. */
    val toReview: List<DocumentSummary> get() = listed.filter { it.phase == DocumentPhase.Reviewing }

    /** The document most worth going back to: the newest with names left to review (Home's Continue). */
    val continueDocument: DocumentSummary? get() = toReview.maxByOrNull { it.updated }

    /** The drawer's Projects: the first five. */
    val topProjects: List<ProjectSummary> get() = projects.take(SIDEBAR_PROJECTS)

    /** Unfiled: the documents in no Project. */
    val unfiled: List<DocumentSummary> get() = listed.filter { it.projectId == null }

    fun project(id: ProjectId?): ProjectSummary? = projects.firstOrNull { it.id == id }

    /** A Project's documents, most recently changed first. */
    fun documentsIn(project: ProjectId): List<DocumentSummary> = listed.filter { it.projectId == project }

    /** Search over titles and Project names (#1091 Decision 25): documents, then Projects. */
    fun search(query: String): Pair<List<DocumentSummary>, List<ProjectSummary>> {
        val needle = query.trim()
        if (needle.isEmpty()) return emptyList<DocumentSummary>() to emptyList()
        val documents = listed.filter { document ->
            document.title.contains(needle, ignoreCase = true) ||
                project(document.projectId)?.name?.contains(needle, ignoreCase = true) == true
        }
        return documents to projects.filter { it.name.contains(needle, ignoreCase = true) }
    }

    companion object {
        const val SIDEBAR_PROJECTS = 5
    }
}

class HomeViewModel(
    private val core: () -> AppCore,
    private val jobs: Jobs? = null,
    /** The scan in progress: the app's, so a destroyed activity keeps it. */
    private val scan: ScanSession = ScanSession(),
    /** Where the scan is saved for a process stopped while the camera is open. */
    private val saved: SavedStateHandle = SavedStateHandle(),
    /** Work that must finish though the view model goes: a committed delete. */
    private val outliving: kotlinx.coroutines.CoroutineScope = Outliving,
) : ViewModel() {
    private val mutable = MutableStateFlow(HomeState())
    val state: StateFlow<HomeState> = mutable.asStateFlow()
    private var receiving: Job? = null

    init {
        // A new process: the scan comes back from the saved state, its shot
        // in flight included (cleaned again if the camera had returned), and
        // shots no scan waits for go.
        scan.restore(
            ScanSession.State(
                pages = saved.get<ArrayList<String>>(SCAN_PAGES).orEmpty().map(::File),
                capture = saved.get<String>(SCAN_CAPTURE)?.let(::File),
                returned = saved.get<Boolean>(SCAN_RETURNED) == true,
                // No saved state at all: a fresh start.
                sequence = saved.get<Long>(SCAN_SEQUENCE) ?: -1,
            ),
        ) { Originals(core().storage) }
        scan.sweep()
        if (!spillsSwept) {
            spillsSwept = true
            sweepSpills()
        }
        // Each change of the scan is saved: whether a scan is under way is
        // what a restored activity asks; the scan's record says the rest.
        viewModelScope.launch {
            scan.state.collect { scanned ->
                saved[SCAN_PAGES] = ArrayList(scanned.pages.map { it.path })
                saved[SCAN_CAPTURE] = scanned.capture?.path
                saved[SCAN_RETURNED] = scanned.returned
                saved[SCAN_SEQUENCE] = scanned.sequence
                val count = if (scanned.capture == null) scanned.pages.size else 0
                mutable.update { it.copy(scanPageCount = count, refused = scanned.refused ?: it.refused) }
                if (scanned.refused != null) scan.refusalShown()
            }
        }
    }

    /** Delete from a list, as iOS's swipe does: the document leaves the lists and a snackbar offers Undo. */
    fun delete(id: DocumentId) = delete(listOf(id))

    private var deletes = 0L

    /**
     * Delete of documents selected together (Unfiled's Select): one
     * snackbar offers Undo for all of them, as Delete does for one. They
     * are deleted when it ends, or when mdoc stops.
     */
    fun delete(ids: List<DocumentId>) {
        commitPendingDelete()
        val listed = state.value.listed.filter { it.id in ids }
        if (listed.isEmpty()) return
        val title = listed.singleOrNull()?.title
        mutable.update { it.copy(pendingDelete = PendingDelete(listed.map { document -> document.id }, title, ++deletes)) }
    }

    /** Undo of Delete: the documents are back where they were. */
    fun undoDelete() = mutable.update { it.copy(pendingDelete = null) }

    /**
     * The Delete waiting on its snackbar happens now. A job on one of them
     * stops first, so it cannot save the document again.
     */
    fun commitPendingDelete() {
        val before = mutable.getAndUpdate {
            it.copy(pendingDelete = null, documents = it.documents.filter { document -> document.id !in it.pendingDelete?.ids.orEmpty() })
        }
        val pending = before.pendingDelete ?: return
        val deleting = before.documents.filter { it.id in pending.ids }
        // Not the view model's scope: a delete answered as mdoc closes still
        // happens when the activity finishes and the view model goes.
        outliving.launch {
            stopJob(deleting)
            withContext(Dispatchers.IO) {
                runCatching {
                    val documents = core().documents
                    if (pending.ids.size == 1) documents.delete(pending.ids.single()) else documents.deleteDocuments(pending.ids)
                }
            }.onFailure { error -> mutable.update { it.copy(refused = error.message) } }
            // Their replies went with them; after a failure, the ones deleted before it.
            refresh()
        }
    }

    /**
     * A job on one of `ids` stops, and is waited for. A document imported
     * this run is listed by its import's id, while its jobs after the first
     * save run under its Vault id: both are matched, as the document's own
     * screen does.
     */
    private suspend fun stopJob(documents: List<DocumentSummary>) {
        val jobs = jobs ?: return
        val running = jobs.current.value?.documentId ?: return
        if (documents.any { it.id == running || it.vaultId == running.raw }) {
            jobs.cancel(CancelReason.User)
            jobs.current.first { it == null }
        }
    }

    // Projects (#1091 Decisions 9, 33, 34)

    /**
     * New Project: named as every new Project is (#677), trimmed and
     * numbered when a Project has the name already; null for a blank name.
     */
    suspend fun createProject(name: String): ProjectId? {
        val trimmed = name.trim()
        if (trimmed.isEmpty()) return null
        val taken = state.value.projects.map { it.name }
        return withContext(Dispatchers.IO) {
            runCatching { core().documents.createProject(unusedName(trimmed, taken)).id }
                .onFailure { error -> mutable.update { it.copy(refused = error.message) } }
                .getOrNull()
        }.also { refreshNow() }
    }

    /** Rename…: a blank name changes nothing. */
    fun renameProject(id: ProjectId, name: String) = viewModelScope.launch {
        val trimmed = name.trim()
        if (trimmed.isEmpty()) return@launch
        withContext(Dispatchers.IO) { runCatching { core().documents.renameProject(id, trimmed) } }
            .onFailure { error -> mutable.update { it.copy(refused = error.message) } }
        refresh()
    }

    /**
     * Delete Project, answered: the Project and its own AI replies go, and
     * its documents go to Unfiled or with it. No Undo: the question was the
     * confirmation. True when it is gone.
     */
    suspend fun deleteProject(id: ProjectId, documents: ProjectDeletion): Boolean {
        commitPendingDelete()
        val project = state.value.project(id) ?: return false
        if (documents == ProjectDeletion.DeleteDocuments) {
            stopJob(state.value.documents.filter { it.projectId == id })
        }
        val deleted = withContext(Dispatchers.IO) { runCatching { core().documents.deleteProject(id, documents) } }
            .onFailure { error -> mutable.update { it.copy(refused = error.message) } }
            .isSuccess
        refreshNow()
        if (deleted) mutable.update { it.copy(deletedProject = project.name) }
        return deleted
    }

    fun deletedProjectShown() = mutable.update { it.copy(deletedProject = null) }

    /** A Project page's replies and names, read from the Vault as they are now. */
    fun loadPage(id: ProjectId) = viewModelScope.launch(Dispatchers.IO) {
        val page = runCatching {
            val documents = core().documents
            documents.refresh()
            ProjectPageData(id, documents.projectReplies(id), documents.projectDetails(id))
        }.getOrElse { ProjectPageData(id) }
        mutable.update { it.copy(page = page) }
    }

    /** A Project page's Remove on an Always or Never wording. */
    fun removeListed(id: ProjectId, wording: String, always: Boolean) = viewModelScope.launch {
        withContext(Dispatchers.IO) { runCatching { core().documents.removeListed(id, wording, always) } }
            .onFailure { error -> mutable.update { it.copy(refused = error.message) } }
        loadPage(id)
    }

    /** A saved reply restored, or null when it can't be. */
    suspend fun restoreSavedReply(reply: String): RestoreResult? =
        withContext(Dispatchers.IO) { runCatching { core().documents.restoreSavedReply(reply) }.getOrNull() }

    /** A Project's Replies shows `reply` restored, with the people it named when saved, as they are now. */
    fun showSavedReply(reply: SavedReplyRow) {
        mutable.update { it.copy(shownReply = ShownReply(reply)) }
        viewModelScope.launch {
            val restored = restoreSavedReply(reply.id)
            mutable.update { state ->
                if (state.shownReply?.reply != reply) state else state.copy(shownReply = ShownReply(reply, restored, restored == null))
            }
        }
    }

    fun hideSavedReply() = mutable.update { it.copy(shownReply = null) }

    // Repeated imports (#1091 Decision 7)

    /** "Already in mdoc" answered Open Existing: what was brought in goes, and the caller opens the returned document. */
    fun openExisting(): DocumentId? {
        val repeated = mutable.getAndUpdate { it.copy(repeated = null) }.repeated ?: return null
        repeated.pending.folder.deleteRecursively()
        return repeated.existing
    }

    /** Answered Import Again: it goes on to the add sheet, as any other import. */
    fun importAgain() {
        val repeated = mutable.getAndUpdate { it.copy(repeated = null) }.repeated ?: return
        replaceAdding(repeated.pending)
    }

    /** Answered Cancel: nothing is imported, and what was brought in goes. */
    fun cancelRepeated() {
        mutable.getAndUpdate { it.copy(repeated = null) }.repeated?.pending?.folder?.deleteRecursively()
    }

    /**
     * The saved document `pending` repeats, when its file's bytes are those
     * of one listed (#1091 Decision 7): the most recently updated. Only a
     * file, or a single photo, has a source to compare: camera pages and
     * photos picked together become a PDF the core makes, and a paste
     * keeps no source.
     */
    private fun repeatOf(pending: PendingAdd.Received): DocumentId? {
        val file = sourceFile(pending.source) ?: return null
        // One deleted, its Undo still offered, is not opened.
        val deleting = state.value.pendingDelete?.ids.orEmpty()
        return core().documents.documentsFromSource(file).firstOrNull { it !in deleting }
    }

    /** Something brought in waits for its title and Project, unless its file is already in mdoc: then it asks first. */
    private fun offer(pending: PendingAdd.Received, existing: DocumentId?) {
        if (existing == null) {
            replaceAdding(pending)
        } else {
            cancelRepeated()
            mutable.update { it.copy(repeated = RepeatedImport(pending, existing, pending.fileName ?: pending.suggestedTitle)) }
        }
    }

    /** Reads the library off the main thread; a core that failed to open is tried again. */
    fun refresh() {
        viewModelScope.launch { refreshNow() }
    }

    /** [refresh], done when it returns. */
    suspend fun refreshNow() {
        withContext(Dispatchers.IO) {
            val next = runCatching {
                val core = core()
                val library = core.documents.snapshot()
                HomeState(
                    documents = library.documents,
                    projects = library.projects,
                    canFindNames = core.models.snapshot().detector != null,
                    loaded = true,
                )
            }.getOrElse { HomeState(loaded = true, error = it.message ?: it.toString()) }
            mutable.update {
                next.copy(
                    adding = it.adding, scanPageCount = it.scanPageCount, refused = it.refused,
                    pendingDelete = it.pendingDelete, deletedProject = it.deletedProject, repeated = it.repeated, page = it.page,
                    shownReply = it.shownReply,
                )
            }
        }
        // A Project's page reads its replies and names again: a reply Restore
        // with mdoc saved changes no document.
        state.value.page?.project?.let(::loadPage)
    }

    /** Paste, started from a Project's page (`into`) or elsewhere. */
    fun startPaste(title: String, into: ProjectId? = null) {
        receiving?.cancel()
        replaceAdding(PendingAdd.Text(title, into))
    }

    /**
     * The Project whose page Files or Photos was opened from, for what the
     * picker returns; "Open in mdoc" from another app starts nowhere.
     */
    var pickingInto: ProjectId?
        get() = saved.get<String>(PICKING_INTO)?.let(::ProjectId)
        set(value) {
            saved[PICKING_INTO] = value?.raw
        }

    /**
     * Stages clean copies before asking for a title and Project; `into`,
     * the Project whose page it was started from.
     */
    fun received(resolver: ContentResolver, uris: List<Uri>, kind: SourceKind?, fallbackTitle: String, into: ProjectId? = null) {
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
                    val descriptor = checkNotNull(resolver.openAssetFileDescriptor(uri, "r"))
                    // The size the provider declares, checked before any image
                    // is read into memory.
                    val declared = descriptor.length
                    descriptor.createInputStream().buffered().use { input ->
                        val sourceKind = kind ?: receivedKind(input, target, extension, mime)
                        if (index == 0) {
                            firstKind = sourceKind
                            title = target.nameWithoutExtension.ifEmpty { fallbackTitle }
                        } else {
                            check(firstKind?.needsRecognition == true && sourceKind.needsRecognition) { name }
                        }
                        if (sourceKind.needsRecognition) {
                            originals.stageImage(target, readImage(input, declared))
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
                val received = PendingAdd.Received(source, title, stage, staged.singleOrNull()?.name, into)
                val existing = repeatOf(received)
                withContext(Dispatchers.Main.immediate) {
                    offer(received, existing)
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
    fun takePage(context: Context, into: ProjectId? = null): Uri? {
        if (scan.state.value.capture != null) return null
        // A scan's first page says where it lands.
        if (scan.state.value.pages.isEmpty()) saved[SCAN_INTO] = into?.raw
        return runCatching { CameraCapture.create(context) }
            .onSuccess { scan.begin(checkNotNull(CameraCapture.file(context, it))) }
            .onFailure { error -> mutable.update { it.copy(refused = error.message) } }
            .getOrNull()
    }

    /**
     * The camera returned: the scan cleans its shot into the scan's stage,
     * in a job of its own that Home's going doesn't stop.
     */
    fun cameraResult(taken: Boolean, launchError: String? = null) {
        scan.clean(taken) { Originals(core().storage) }
        if (launchError != null) mutable.update { it.copy(refused = launchError) }
    }

    fun finishScan(title: String) {
        val current = scan.state.value
        if (current.pages.isEmpty() || current.capture != null) return
        receiving?.cancel()
        val (folder, files) = Originals(core().storage).finishScanStage(scan.finish())
        val into = saved.get<String>(SCAN_INTO)?.let(::ProjectId)
        replaceAdding(PendingAdd.Received(ImportSource.Pages(files, SourceKind.Scan), title, folder, into = into))
    }

    fun cancelAdd() {
        receiving?.cancel()
        replaceAdding(null)
        cancelRepeated()
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

    /** Keeps the document, Unfiled or in `project`, and returns its id; the caller opens it and reads it. */
    suspend fun add(text: String?, title: String, project: ProjectId? = null): DocumentId? {
        val adding = mutable.getAndUpdate { it.copy(adding = null) }.adding ?: return null
        val source = when (adding) {
            is PendingAdd.Text -> ImportSource.Text(text.orEmpty())
            is PendingAdd.Received -> adding.source
        }
        var handedOff = false
        try {
            return withContext(Dispatchers.IO) {
                core().documents.importDocument(source, title, project).id.also { handedOff = true }
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
        private const val MAX_IMAGE_BYTES = 256L * 1024 * 1024
        private const val TOO_LARGE = "This image is too large to import."
        private const val TOO_LARGE_HERE = "This image is too large to import on this phone."
        private const val SCAN_PAGES = "scan.pages"
        private const val SCAN_CAPTURE = "scan.capture"
        private const val SCAN_RETURNED = "scan.returned"
        private const val SCAN_SEQUENCE = "scan.sequence"
        private const val SCAN_INTO = "scan.into"
        private const val PICKING_INTO = "picking.into"

        /**
         * An image's bytes in one array of exactly its size: no growing
         * buffer and no second full copy. The `declared` size is checked
         * against the import limit and the memory this process may still
         * use (the bytes, their clean copy and the core's copy) before
         * anything is allocated. An image of unknown size is first copied,
         * within the limit, to a temporary file outside `originals/`,
         * deleted at once, so its size is known.
         */
        internal fun readImage(
            input: InputStream,
            declared: Long,
            limit: Long = MAX_IMAGE_BYTES,
            free: () -> Long = ::freeMemory,
            unlink: (File) -> Boolean = File::delete,
        ): ByteArray {
            if (declared < 0) {
                // Unknown size: spilled to a file unlinked before any byte is
                // written, read back through its open descriptor, so nothing
                // of the image is left on disk however the process ends. A
                // host that can't unlink an open file (a Windows test host,
                // never a phone) deletes it right after the readback, and the
                // start sweep ([sweepSpills]) catches a stopped process.
                val spill = File.createTempFile(SPILL_PREFIX, ".image")
                try {
                    return RandomAccessFile(spill, "rw").use { file ->
                        unlink(spill)
                        val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                        var total = 0L
                        while (true) {
                            val read = input.read(buffer)
                            if (read < 0) break
                            total += read
                            check(total <= limit) { TOO_LARGE }
                            file.write(buffer, 0, read)
                        }
                        file.seek(0)
                        readImage(FileInputStream(file.fd), total, limit, free)
                    }
                } finally {
                    spill.delete()
                }
            }
            check(declared <= limit) { TOO_LARGE }
            check(declared <= free() / 3) { TOO_LARGE_HERE }
            val bytes = ByteArray(declared.toInt())
            var at = 0
            while (at < bytes.size) {
                val read = input.read(bytes, at, bytes.size - at)
                if (read < 0) break
                at += read
            }
            // A provider that declared less than it has: refused, not grown.
            check(input.read() < 0) { TOO_LARGE }
            return if (at == bytes.size) bytes else bytes.copyOf(at)
        }

        /** The process's own scope, for work the view model's going must not stop. */
        private val Outliving = kotlinx.coroutines.CoroutineScope(kotlinx.coroutines.SupervisorJob() + Dispatchers.Main.immediate)

        private const val SPILL_PREFIX = "intake-"
        private var spillsSwept = false

        /**
         * Deletes the intake spills an older process left in the temporary
         * folder, once a process (they are unlinked at once now; a version
         * before that left them named).
         */
        internal fun sweepSpills(folder: File = File(checkNotNull(System.getProperty("java.io.tmpdir")))) {
            folder.listFiles()?.forEach { if (it.name.startsWith(SPILL_PREFIX)) it.delete() }
        }

        /** What this process may still allocate on its heap. */
        private fun freeMemory(): Long = Runtime.getRuntime().let { it.maxMemory() - (it.totalMemory() - it.freeMemory()) }

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
            "jpg", "jpeg", "png", "heic", "heif", "webp", "gif" -> SourceKind.Image
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
            // TIFF on the phones is #1064: refused with the formats mdoc opens.
            else -> null
        }

        /**
         * The one file whose bytes a saved document records as its source
         * (#1091 Decision 7): a file, or a single photo. Camera pages and
         * photos picked together become a PDF the core makes, and a paste
         * keeps no source.
         */
        fun sourceFile(source: ImportSource): File? = when (source) {
            is ImportSource.File -> source.file
            is ImportSource.Pages -> source.pages.singleOrNull()?.takeIf { source.kind == SourceKind.Image }
            is ImportSource.Text -> null
        }

        /** A new Project's name: `name`, or numbered from 2 when a Project has it already (iOS `unusedName`). */
        fun unusedName(name: String, among: List<String>): String {
            val taken = among.toSet()
            if (name !in taken) return name
            return generateSequence(2) { it + 1 }.map { "$name $it" }.first { it !in taken }
        }

        /**
         * A Project's names matching `query` (by alias or wording), grouped by
         * category in the categories' order, each group by alias.
         */
        fun names(names: List<ProjectNameRow>, query: String): List<Pair<Category, List<ProjectNameRow>>> {
            val needle = query.trim()
            val found = names.filter { name ->
                needle.isEmpty() || name.alias.contains(needle, ignoreCase = true) ||
                    name.originals.any { it.contains(needle, ignoreCase = true) }
            }
            return Category.entries.mapNotNull { category ->
                found.filter { it.category == category }.sortedBy { it.alias.lowercase() }.takeIf { it.isNotEmpty() }?.let { category to it }
            }
        }

        /** The wordings of a list matching `query`. */
        fun wordings(wordings: List<String>, query: String): List<String> {
            val needle = query.trim()
            return if (needle.isEmpty()) wordings else wordings.filter { it.contains(needle, ignoreCase = true) }
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
