package io.github.lawkitt.mdoc.core

import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.UUID
import org.json.JSONArray
import org.json.JSONObject
import uniffi.mdoc_ffi.CoreException
import uniffi.mdoc_ffi.DocumentInfo
import uniffi.mdoc_ffi.DocumentMove
import uniffi.mdoc_ffi.DocumentSession as FfiSession
import uniffi.mdoc_ffi.DocumentSourceKind
import uniffi.mdoc_ffi.FloorReason
import uniffi.mdoc_ffi.ImportOptions
import uniffi.mdoc_ffi.JobProgress
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.Mention
import uniffi.mdoc_ffi.ProjectDocuments
import uniffi.mdoc_ffi.Readiness
import uniffi.mdoc_ffi.RestoredReply
import uniffi.mdoc_ffi.ReviewSnapshot
import uniffi.mdoc_ffi.SharedFrom
import uniffi.mdoc_ffi.TextRange
import uniffi.mdoc_ffi.Vault
import uniffi.mdoc_ffi.discardKeptImages
import uniffi.mdoc_ffi.discardKeptImport
import uniffi.mdoc_ffi.markdownStyling
import uniffi.mdoc_ffi.Category as FfiCategory
import uniffi.mdoc_ffi.MentionState as FfiMentionState
import uniffi.mdoc_ffi.Scope as FfiScope

/** Progress and cooperative cancellation for a long core call. */
interface JobObserver {
    fun stage(stage: JobStage)

    /** A scan's estimate of the seconds it has left. */
    fun remaining(seconds: Long) {}

    /** Runs `handler` once when the job is cancelled (at once if it already is). */
    fun onCancel(handler: () -> Unit)
}

/**
 * Documents and Projects in the encrypted Vault (ADR 0125, #1091), over
 * the core's `Vault`: the twin of the iOS app's `RealLibrary`. Documents
 * are known by the Vault's ids; an import keeps the `new:` id it had
 * before its first save for the rest of the run. An import the core has
 * not read yet is the one thing kept outside the Vault: its received files
 * and its record wait in the intake folder until its first save.
 */
class Documents(
    private val vault: Vault,
    private val storage: MdocStorage,
    private val models: Models?,
    private val device: DeviceConditions = DeviceConditions.Unknown,
) {
    private val sessions = mutableMapOf<DocumentId, Session>()

    /** Vault id → the id this run gave its document. */
    private val ids = mutableMapOf<String, DocumentId>()
    private val originals = Originals(storage)
    private val pending = PendingImports(storage, originals::importFolder)

    /** The largest original viewed from memory, about 16 MiB (#1091 Decision 24); the tests lower it. */
    internal var originalsInMemory: ULong = IN_MEMORY

    /** Tests: runs on the saving thread as an import's first save shows its Vault id, before the save ends. */
    @Volatile internal var firstSaveShown: ((DocumentId) -> Unit)? = null

    /** The core's delete of a Project; the tests make it stop part way. */
    @Volatile internal var deleteInVault: (String, ProjectDocuments) -> Unit = { project, documents ->
        vault.deleteProject(project, documents)
    }

    /** Runs the core's move or its Undo; the tests make it stop part way. */
    @Volatile internal var moveInVault: (() -> DocumentMove) -> DocumentMove = { it() }

    /**
     * Held for the whole of a document's move and of a Project's delete,
     * before any session's save lock: a move never lands in a Project
     * after its delete read the documents it has, which the delete would
     * then move or delete behind the moved session.
     */
    private val projectChanges = Any()

    /** Where text was shared from last: Restore with mdoc restores with it first. */
    val lastShared = LastShared(File(storage.intake, "last-shared.json"))

    init {
        // Viewing copies a stop left behind go at launch (#1091 Decision 24).
        clearViewing()
        // Imports not yet read when mdoc last stopped come back, ready to read again.
        val entries = pending.all()
        // A pending list that couldn't be read says nothing of what it
        // owned: no stage is cleared on its word.
        if (!pending.setAside) originals.clearAbandonedStages(entries.flatMap { it.source.files })
        for (entry in entries) {
            sessions[entry.id] = Session(entry.id, entry.title, entry.projectId, entry.source, entry.ocrStarted)
        }
    }

    /** The saved documents from the Vault's manifest, which opens none of them, then the imports not saved yet. */
    @Synchronized
    fun snapshot(): LibrarySnapshot {
        // What Restore with mdoc saved through its own handle comes in.
        runCatching { vault.refresh() }
        val listed = vault.projects()
        synchronized(projectNames) {
            projectNames.clear()
            listed.forEach { projectNames[it.id] = it.name }
        }
        val projects = listed.map { ProjectSummary(ProjectId(it.id), it.name, projectColour(it.hue)) }
        val documents = mutableListOf<DocumentSummary>()
        val seen = mutableSetOf<DocumentId>()
        for (info in vault.documents()) {
            val id = ids[info.id] ?: DocumentId(info.id)
            seen += id
            val updated = info.updated.toLong() * 1000
            documents += sessions[id]?.loadedSummary(updated) ?: summary(id, info, updated)
        }
        documents += sessions.values.filter { it.id !in seen }.map { it.summary(it.created) }
        return LibrarySnapshot(projects = projects, documents = documents.sortedByDescending { it.updated })
    }

    /** What Restore with mdoc saved since comes in; true when anything did. */
    fun refresh(): Boolean = runCatching { vault.refresh() }.getOrDefault(false)

    /** Its session; one a move left stale opens again first, when it can. */
    fun session(id: DocumentId): Session =
        synchronized(this) { sessions[id] ?: stored(id.raw) }.also { runCatching { it.reopenIfStale() } }

    /** Reads a document's record to open it again after a move; the tests make it fail. */
    @Volatile internal var opening: (String) -> DocumentInfo = { vault.document(it) }

    /**
     * The tests' stand-in for what the Vault says of a saved document, by
     * its Vault id: changed since its review and excluded from AI come only
     * from a computer today (iOS's `MDOC_UI_READINESS`). Null: the Vault's.
     */
    @Volatile internal var readiness: ((String) -> SavedReadiness?)? = null

    /** The session of a saved document, opened from the Vault the first time. */
    private fun stored(vaultId: String): Session {
        ids[vaultId]?.let { id -> sessions[id]?.let { return it } }
        val info = runCatching { vault.document(vaultId) }.getOrElse { throw AppError.NotFound() }
        val session = Session(DocumentId(vaultId), info.title, null, null)
        session.reopen(vaultId, info)
        sessions[session.id] = session
        ids[vaultId] = session.id
        return session
    }

    /**
     * Keeps the source in the intake folder and returns its session, ready
     * to read: it lands in Unfiled, or in `project` (#1091 Decision 35). No
     * import makes a Project. One started into a Project being deleted
     * lands in Unfiled, as one started after the delete does: the delete
     * counted its imports already.
     */
    @Synchronized
    fun importDocument(source: ImportSource, title: String, project: ProjectId? = null): Session {
        val prepared = originals.prepare(source, title)
        val id = DocumentId("new:${UUID.randomUUID()}")
        val into = project?.raw?.takeUnless { it in deletingProjects }
        pending.add(PendingImports.Entry(id, title, into, prepared, ocrStarted = false))
        return Session(id, title, into, prepared).also { sessions[id] = it }
    }

    /** The Projects a delete is under way for, once per delete; under this object's lock. */
    private val deletingProjects = mutableListOf<String>()

    @Synchronized
    fun delete(id: DocumentId) {
        val session = sessions[id]
        if (session == null) {
            // A listed document not opened this run: its id is the Vault's.
            core { vault.deleteDocument(id.raw) }
            ids.remove(id.raw)
            return
        }
        // An import still being read must not save itself back afterwards.
        session.discard()
        val vaultId = session.vaultId
        val source = session.pendingSource
        if (vaultId != null) {
            core { vault.deleteDocument(vaultId) }
            ids.remove(vaultId)
        } else if (source != null) {
            // OCR's kept pages go first, found by the source files' digests.
            val kept = storage.dir(MdocStorage.Area.Ocr).path
            when (source.input) {
                is Originals.Input.Images -> core { discardKeptImages(kept, source.files.map { it.path }) }
                else -> source.file?.let { core { discardKeptImport(kept, it.path) } }
            }
        }
        session.closeOriginal()
        sessions.remove(id)
        pending.remove(id)
    }

    /**
     * Unfiled's multi-select delete. Those this run holds a session for go
     * one by one, as a single Delete does; the rest in one call. A failure
     * stops it, and the documents deleted before it stay deleted.
     */
    fun deleteDocuments(documents: List<DocumentId>) {
        val (held, rest) = synchronized(this) { documents.partition { sessions[it] != null } }
        held.forEach(::delete)
        if (rest.isEmpty()) return
        core { vault.deleteDocuments(rest.map { it.raw }) }
        synchronized(this) { rest.forEach { ids.remove(it.raw) } }
    }

    // MARK: Projects (#1091 Decisions 9, 33, 34)

    /** New Project: an empty Project named `name`; a blank name is refused. */
    fun createProject(name: String): ProjectSummary {
        val made = core { vault.createProject(name) }
        return ProjectSummary(ProjectId(made.id), made.name, projectColour(made.hue))
    }

    /** Renames a Project; its documents keep their titles. */
    fun renameProject(project: ProjectId, name: String) {
        core { vault.renameProject(project.raw, name) }
        synchronized(projectNames) { projectNames.remove(project.raw) }
    }

    /** A Project page's names and its Always and Never lists. */
    fun projectDetails(project: ProjectId): ProjectDetails {
        val page = core { vault.projectPage(project.raw) }
        return ProjectDetails(
            names = page.names.map { ProjectNameRow(it.alias, category(it.category), it.originals) },
            always = page.always.map { AlwaysWording(it.wording, category(it.category)) },
            never = page.never,
        )
    }

    /** Takes `wording` off a Project's Always list, or its Never list when `always` is false. */
    fun removeListed(project: ProjectId, wording: String, always: Boolean) {
        core { vault.removeListed(project.raw, wording, always) }
    }

    /** A Project's saved AI replies, newest first: its own and those of the documents in it now. */
    fun projectReplies(project: ProjectId): List<SavedReplyRow> =
        vault.projectReplies(project.raw).map { SavedReplyRow(it.id, it.title, it.created.toLong() * 1000) }

    /** A saved reply restored with the people it named when saved, as they are now. */
    fun restoreSavedReply(reply: String): RestoreResult = restoreResult(core { vault.restoreReply(reply) })

    /** Saved reply `reply` as lists show it; `null` once it is deleted. */
    fun replyInfo(reply: String): SavedReplyRow? =
        runCatching { vault.replyInfo(reply) }.getOrNull()?.let { SavedReplyRow(it.id, it.title, it.created.toLong() * 1000) }

    /**
     * Promote to Document (#1091 Decision 3): a new document in saved reply
     * `reply`'s place (its Project, or Unfiled), whose text is the reply
     * restored as it restores now, real names back; not reviewed, so its
     * names are proposed again. The reply stays. The reply restores with
     * the names as the taps left them: changes not saved yet go first.
     * Returns the id this run lists the new document by. Under the Project
     * changes' lock, as a move is: a Project's delete either counts the new
     * document or ends before it is made.
     */
    fun promoteReply(reply: String): DocumentId = synchronized(projectChanges) {
        // Notes that fail to save stay pending, and nothing the reply
        // restores with depends on them: the promote goes on.
        saveNotes()
        val promoted = core { vault.promoteReply(reply) }
        synchronized(this) { ids[promoted.id] ?: DocumentId(promoted.id) }
    }

    /**
     * Every open session's notes not saved yet are saved; a failure stops
     * none of the others, and leaves its notes pending. A session with none
     * is never waited for.
     */
    fun saveNotes() {
        val owing = synchronized(this) { sessions.values.filter { it.owesNotes } }
        owing.forEach { runCatching { it.saveNotes() } }
    }

    /**
     * The saved documents imported from a file of the same bytes, most
     * recently updated first (#1091 Decision 7); none when it can't be read.
     */
    fun documentsFromSource(file: File): List<DocumentId> {
        val found = runCatching { vault.documentsFromSource(file.path) }.getOrDefault(emptyList())
        return synchronized(this) { found.map { ids[it.id] ?: DocumentId(it.id) } }
    }

    /**
     * Delete Project: it goes with its own AI replies, and its documents
     * go to Unfiled or with it. Their sessions follow, as iOS's do: a moved
     * one opens again in Unfiled; a deleted one goes through [delete], so
     * nothing saves it again. An import not read yet that was to join the
     * Project lands in Unfiled, or is deleted with the rest; one started
     * while the delete runs lands in Unfiled. The caller stops a job on a
     * document it deletes first. No move runs meanwhile. With
     * `confirmed`, the documents the question counted, Delete them deletes
     * nothing when the Project holds any other (one moved in meanwhile).
     */
    fun deleteProject(
        project: ProjectId,
        documents: ProjectDeletion,
        confirmed: Set<DocumentId>? = null,
    ): Unit = synchronized(projectChanges) {
        // A moved document's notes not saved yet stay with it as it opens
        // again ([Session.reopen]): no save of a member is waited for here.
        val raw = project.raw
        // The imports that would join it, first; the Documents' lock only to
        // read them: a save holds its session's save lock, then takes this one.
        // From here until the delete ends, no new import joins it.
        val imports = synchronized(this) {
            deletingProjects += raw
            sessions.values.filter { it.joins(raw) }
        }
        // A move that stopped part way: its error, raised once the moved are open again.
        var failure: Exception? = null
        try {
            // No import's join and first save runs meanwhile, so the members
            // read now are all of them: each import lands in Unfiled, or,
            // saved in the Project before, moves with the rest.
            val open = holdingSaves(imports) {
                val stored = vault.documents().filter { it.projectId == raw }.map { it.id }
                val members = synchronized(this) { stored.map { it to (ids[it] ?: DocumentId(it)) } }
                when (documents) {
                    ProjectDeletion.DeleteDocuments -> {
                        val deleting = imports.filter { it.joins(raw) }.map { it.id } + members.map { it.second }
                        if (confirmed != null && deleting.any { it !in confirmed }) throw AppError.Failed(PROJECT_CHANGED)
                        deleting.forEach(::delete)
                        core { deleteInVault(raw, ProjectDocuments.DELETE) }
                        emptyList<Pair<String, Session>>()
                    }
                    ProjectDeletion.MoveToUnfiled -> {
                        // Unfiled in the pending list first: a stop before the
                        // Project's delete leaves it Unfiled in a Project that
                        // stays, which its first save takes as it is.
                        val leaving = imports.filter { it.leaveProject(raw) }
                        leaving.forEach { pending.moveToUnfiled(it.id) }
                        try {
                            core { deleteInVault(raw, ProjectDocuments.MOVE_TO_UNFILED) }
                        } catch (error: Exception) {
                            failure = error
                            // Each import rejoins on its own: a pending list
                            // that can't be written stops neither the others
                            // nor the reopening below, and the delete's error
                            // is the one raised.
                            leaving.forEach {
                                it.rejoin(raw)
                                runCatching { pending.rejoin(it.id, raw) }.exceptionOrNull()?.let(error::addSuppressed)
                            }
                        }
                        val held = synchronized(this) { members.mapNotNull { (vaultId, id) -> sessions[id]?.let { vaultId to it } } }
                        if (failure == null) {
                            held
                        } else {
                            // Stopped part way: those it moved open again, or
                            // they would still show the Project and refuse to
                            // save; those still in it keep running (a scan's
                            // job included). Every one, when the Vault can't say.
                            val still = runCatching { vault.documents().filter { it.projectId == raw }.map { it.id }.toSet() }
                                .getOrNull()
                            if (still == null) held else held.filter { (vaultId, _) -> vaultId !in still }
                        }
                    }
                }
            }
            // The core rewrote their text: each opens again as it is now.
            for ((vaultId, session) in open) session.reopenAfterMove(vaultId)
            failure?.let { throw it }
        } finally {
            synchronized(this) { deletingProjects -= raw }
            synchronized(projectNames) { projectNames.clear() }
        }
    }

    /**
     * Move to Project, Move to New Project or Move to Unfiled (#1091
     * Decision 9): the document's names take the aliases of the place it
     * joins and its text is rewritten with them; the Project it left keeps
     * its names. Refused for the place it is in already, and for an import
     * not read yet. A new Project's name is the caller's, as typed.
     */
    fun moveDocument(id: DocumentId, target: MoveTarget): ProjectMove = move(id) { vaultId ->
        when (target) {
            is MoveTarget.Existing -> vault.moveDocument(vaultId, target.project.raw)
            is MoveTarget.NewProject -> vault.moveDocumentToNewProject(vaultId, target.name)
            MoveTarget.Unfiled -> vault.moveDocument(vaultId, null)
        }
    }

    /**
     * Undo of a move: the document moves back and its names take their old
     * aliases there. The names the move added go again where nothing uses
     * them, unless text was shared from there since.
     */
    fun undoMove(move: ProjectMove): ProjectMove = move(move.document) { vault.undoMove(move.undo) }

    /**
     * A saved document's move through the Vault, as iOS's: the move has
     * happened once the core returns, so its session opens it again, as it
     * is now. A move that stops part way may have moved it, or written it
     * back as it was: its session opens it again too, and the error is
     * raised once it has.
     */
    private fun move(id: DocumentId, call: (String) -> DocumentMove): ProjectMove {
        synchronized(projectChanges) { return moveHeld(id, call) }
    }

    /** [move], under the Project changes' lock. */
    private fun moveHeld(id: DocumentId, call: (String) -> DocumentMove): ProjectMove {
        // Its session, read under this object's lock, where a first save
        // shows its Vault id together with its mapping: an import is either
        // saved and known by its Vault id, or refused as not read yet.
        val held: Session
        val vaultId: String
        synchronized(this) {
            // A listed document not opened this run: its id is the Vault's,
            // and no session opens it while this lock is held.
            held = sessions[id] ?: ids[id.raw]?.let { sessions[it] } ?: return moved(id, core { moveInVault { call(id.raw) } })
            vaultId = held.vaultId ?: throw AppError.Failed(NOT_READ_YET)
        }
        // No save of it runs meanwhile, nor starts: a save after the move
        // and before the session opens it again would be refused as stale.
        return held.holdingSave {
            // Notes typed in the last pause go first: the move opens the
            // document again from the Vault.
            runCatching { held.saveNotes() }
            val result = try {
                core { moveInVault { call(vaultId) } }
            } catch (error: Exception) {
                held.reopenAfterMove(vaultId)?.let(error::addSuppressed)
                synchronized(projectNames) { projectNames.clear() }
                throw error
            }
            // Its text and Project changed in the Vault: the session opens it
            // again. The move stands even if it can't: it is then opened again
            // before its next change, or when it is next shown.
            held.reopenAfterMove(vaultId)
            moved(held.id, result)
        }
    }

    /** What a move tells the app. */
    private fun moved(id: DocumentId, moved: DocumentMove): ProjectMove {
        synchronized(projectNames) { projectNames.clear() }
        return ProjectMove(
            document = id,
            to = moved.project?.let { ProjectSummary(ProjectId(it.id), it.name, projectColour(it.hue)) },
            from = moved.left?.let { ProjectId(it.id) },
            undo = moved.undo,
        )
    }

    /** Runs `body` while none of `held` saves. */
    private fun <T> holdingSaves(held: List<Session>, body: () -> T): T =
        if (held.isEmpty()) body() else held.first().holdingSave { holdingSaves(held.drop(1), body) }

    /** A Vault document's list row, from the manifest. */
    private fun summary(id: DocumentId, info: DocumentInfo, updated: Long): DocumentSummary {
        val candidates = (info.proposed + info.replaced + info.kept).toInt()
        return DocumentSummary(
            id, info.title, info.projectId?.let(::ProjectId), sourceKind(info.sourceKind),
            phase(info.scanned, candidates, info.proposed.toInt()), info.proposed.toInt(), updated, candidates,
            vaultId = info.id,
        )
    }

    /** Project names by id, read once each: every snapshot shows its Project's name. */
    private val projectNames = mutableMapOf<String, String>()

    private fun projectName(id: String?): String? {
        id ?: return null
        synchronized(projectNames) { projectNames[id] }?.let { return it }
        val name = runCatching { vault.project(id).name }.getOrNull() ?: return null
        synchronized(projectNames) { projectNames[id] = name }
        return name
    }

    /**
     * Below the name finder's floor no name is found (ADR 0088): a save
     * with no proposals left then records the document Ready, its names
     * decided by hand (#1091 Q4, the maintainer's (b)). A phone at the
     * detector tier is offered Find names, even before its model is in.
     */
    private fun findsNoNames(): Boolean = models?.snapshot()?.tier != DeviceTier.Detector

    /**
     * What the Original view shows of a Vault document's original: its kind
     * from the name it was imported with, decrypted when the view first
     * reads it, into memory when 16 MiB or less, else into a folder of its
     * own in the viewing cache (#1091 Decision 24). `None` for a document
     * that keeps no original: pasted, or pages picked together.
     */
    private fun storedOriginal(vaultId: String): OriginalSource {
        val file = runCatching { vault.original(vaultId) }.getOrNull() ?: return OriginalSource.None
        val folder = File(storage.viewing, vaultId)
        val data = OriginalData.Stored(
            decrypt = {
                if (file.length <= originalsInMemory) {
                    OriginalData.Decrypted.Memory(core { vault.originalBytes(vaultId) })
                } else {
                    folder.deleteRecursively()
                    check(folder.mkdirs()) { "mdoc could not open the original." }
                    OriginalData.Decrypted.Copy(File(core { vault.copyOriginal(vaultId, folder.path) }))
                }
            },
            remove = { folder.deleteRecursively() },
        )
        return OriginalSource.stored(file.fileName, data)
    }

    private fun clearViewing() {
        storage.viewing.listFiles()?.forEach { it.deleteRecursively() }
        storage.viewing.mkdirs()
    }

    /** One document's import, OCR, review and hand-off: the twin of the iOS `RealSession`. */
    inner class Session internal constructor(
        val id: DocumentId,
        /** The title chosen at import, until the core reads the document. */
        private val title: String,
        /**
         * The Project chosen at import, until its first save: the core holds
         * its Project from then on. `null`: Unfiled.
         */
        private var chosenProject: String?,
        private var source: Originals.Prepared?,
        private var ocrStarted: Boolean = false,
    ) {
        val created = System.currentTimeMillis()
        private val ffi = FfiSession()
        private var review: ReviewSnapshot? = null
        private var kind: SourceKind = source?.kind ?: SourceKind.Text
        private var original: OriginalSource = OriginalSource.of(source?.files.orEmpty())

        /** The Project it is in once saved, or the one chosen; `null`: Unfiled. */
        var projectId: String? = chosenProject
            private set

        /** Its id in the Vault, once its first save made it or it was opened from there. */
        var vaultId: String? = null
            private set

        /** The reply the Reply pane shows, restored, while its text and the names stay. */
        private var shownReply: Pair<ReplyKey, ReplySnapshot?>? = null

        /** The import not yet read, if any. */
        internal val pendingSource: Originals.Prepared? get() = synchronized(this) { source }

        @Volatile private var discarded = false

        /**
         * Held by every save, and by a Project's delete while it takes
         * imports off the Project: an import's join and first save are one
         * step, so it lands in Unfiled or was saved in the Project first.
         * Never taken while holding the Documents' lock.
         */
        private val saving = Any()

        internal fun discard() {
            discarded = true
            ffi.cancel()
        }

        /**
         * Opens a saved document from the Vault: its review as saved, its
         * names lined up with its Project's as they are now. Notes typed and
         * not saved yet (a move or a Project's delete opens it again, or
         * their save failed) stay with it, and its next save writes them.
         */
        internal fun reopen(vaultId: String, info: DocumentInfo) {
            val typed = if (notesUnsaved) ffi.notes() else null
            val opened = core { ffi.openFromVault(vault, vaultId) }
            typed?.let { core { ffi.setNotes(it) } }
            val stored = storedOriginal(vaultId)
            synchronized(this) {
                this.vaultId = vaultId
                review = opened
                projectId = info.projectId
                kind = sourceKind(info.sourceKind)
                original.close()
                original = stored
                this.opened += 1
                stale = false
            }
        }

        /** Times it opened from the Vault; see [DocumentSnapshot.opened]. */
        private var opened = 0

        /**
         * A move went through but this session could not open the document
         * again: it does before its next change, and when next asked for.
         */
        @Volatile private var stale = false

        /** Opens it again after a move; on failure it stays [stale], and the error is returned. */
        internal fun reopenAfterMove(vaultId: String): Throwable? =
            runCatching { reopen(vaultId, opening(vaultId)) }.exceptionOrNull()?.also { stale = true }

        /** Opens it again if a move left it [stale]; throws when it still can't. */
        internal fun reopenIfStale() = holdingSave { reopenHeldIfStale() }

        /** [reopenIfStale], the save lock held. */
        private fun reopenHeldIfStale() {
            if (!stale) return
            val vaultId = vaultId ?: return
            reopen(vaultId, opening(vaultId))
        }

        /** The document closed: its decrypted original goes (#1091 Decision 24). */
        fun closeOriginal() = synchronized(this) { original }.close()

        internal fun summary(updated: Long): DocumentSummary {
            val current = snapshot()
            return DocumentSummary(
                id, current.title, current.projectId, current.kind, current.phase,
                current.pendingCount, updated, current.candidates.size, vaultId = vaultId,
            )
        }

        /** The list row of a document this session holds, from its review without the reply or the styling. */
        internal fun loadedSummary(updated: Long): DocumentSummary? {
            val review = synchronized(this) { review } ?: return null
            return DocumentSummary(
                id, review.title, projectId?.let(::ProjectId), kind,
                phase(review.scanned, review.mentions.size, review.proposed.toInt()),
                review.proposed.toInt(), updated, review.mentions.size, vaultId = vaultId,
            )
        }

        @Synchronized
        fun snapshot(): DocumentSnapshot {
            val chosenProject = chosenProject
            val review = review
                ?: return DocumentSnapshot(
                    id, title, chosenProject?.let(::ProjectId), projectName(chosenProject), kind, DocumentPhase.Recognizing,
                    "", null, emptyList(), emptyList(), null, resumesOcr = ocrStarted, original = original, opened = opened,
                )
            val candidates = review.mentions.map(::candidate)
            val identities = review.identities.map { row ->
                val id = IdentityId(row.id.toString())
                val mine = candidates.filter { it.identityId == id }
                val state = when {
                    row.proposed > 0uL -> MentionState.Proposed
                    mine.isNotEmpty() && mine.all { it.state == MentionState.Kept } -> MentionState.Kept
                    else -> MentionState.Replaced
                }
                Identity(
                    id, row.alias, category(row.category), row.originals.firstOrNull() ?: row.alias,
                    row.originals, maxOf(row.mentions.toInt(), mine.size), state,
                )
            }
            return DocumentSnapshot(
                id, review.title, projectId?.let(::ProjectId), projectName(projectId),
                kind, phase(review.scanned, review.mentions.size, review.proposed.toInt()), review.text,
                markdownStyling(review.text), candidates, identities, reply(review),
                warning = review.warning,
                original = original,
                opened = opened,
                promotedFrom = promotedFrom(),
            )
        }

        /**
         * The saved reply this document was promoted from, from the Vault's
         * manifest: deleted once it is not listed there.
         */
        private fun promotedFrom(): PromotedFrom? {
            val reply = ffi.fromReply() ?: return null
            return replyInfo(reply)?.let(PromotedFrom::Reply) ?: PromotedFrom.DeletedReply
        }

        // Notes (#1091 Decision 3)

        /** Notes set and not saved yet: the next save writes them. */
        @Volatile private var notesUnsaved = false

        /** Whether notes were set since its last save. */
        internal val owesNotes: Boolean get() = notesUnsaved

        /** The document's notes as the core holds them; empty before it is read. */
        fun notes(): String = holdingSave {
            reopenHeldIfStale()
            ffi.notes()
        }

        /**
         * Typed in Notes: the core holds them with the document, and
         * [saveNotes] (or any save before it) writes them. Refused before
         * the document is read: nothing typed could be kept.
         */
        fun setNotes(notes: String) = holdingSave {
            reopenHeldIfStale()
            core { ffi.setNotes(notes) }
            notesUnsaved = true
        }

        /** Saves notes set since the last save; nothing when there are none. */
        fun saveNotes() = holdingSave {
            if (notesUnsaved) saveHeld()
        }

        /**
         * The newest reply saved for this document's place, restored with
         * the people it named when saved: its Project's replies, or its own
         * while Unfiled. Restored again only once the reply, the text or the
         * names changed.
         */
        private fun reply(review: ReviewSnapshot): ReplySnapshot? {
            val vaultId = vaultId ?: return null
            val project = projectId
            val newest = runCatching {
                (if (project != null) vault.projectReplies(project) else vault.documentReplies(vaultId)).firstOrNull()
            }.getOrNull()
            val key = ReplyKey(newest?.id, review.textRevision, review.identities)
            shownReply?.let { (shownKey, shown) -> if (shownKey == key) return shown }
            val shown = newest?.let { info ->
                runCatching {
                    val restored = restoreResult(vault.restoreReply(info.id))
                    ReplySnapshot(vault.reply(info.id).markdown, restored, markdownStyling(restored.text), info.id)
                }.getOrNull()
            }
            shownReply = key to shown
            return shown
        }

        /**
         * Import and OCR; resumes after the pages already finished. A locked
         * DOCX or PDF throws [AppError.NeedsPassword] until [password] unlocks it;
         * the password goes no further than this call.
         */
        fun recognize(observer: JobObserver, password: String? = null): DocumentSnapshot {
            val source = synchronized(this) { source } ?: return snapshot()
            observer.onCancel { ffi.cancel() }
            val progress = Progress(observer, device)
            // OCR keeps its finished pages in the intake folder: after a
            // cancel, or mdoc stopping, this resumes with the pages left.
            val kept = storage.dir(MdocStorage.Area.Ocr).path
            observer.stage(JobStage.READING)
            when (val input = source.input) {
                is Originals.Input.Text -> core { ffi.importText(input.text, title) }
                is Originals.Input.File -> {
                    val ocr = models?.ocrPaths()
                    if (input.needsOcr && ocr == null) throw AppError.ModelMissing()
                    if (ocr != null && (input.needsOcr || source.kind == SourceKind.Pdf) && !ocrStarted) {
                        ocrStarted = true
                        pending.markOcrStarted(id)
                    }
                    core {
                        ffi.importFileResuming(
                            kept, input.file.path, ImportOptions(ocr, nativeTextOnly = false, password = password), progress,
                        )
                    }
                }
                is Originals.Input.Images -> {
                    val ocr = models?.ocrPaths() ?: throw AppError.ModelMissing()
                    if (!ocrStarted) {
                        ocrStarted = true
                        pending.markOcrStarted(id)
                    }
                    // One document, a page each, built in memory by the core.
                    core {
                        ffi.importImagesResuming(
                            kept, input.files.map { it.path }, title,
                            ImportOptions(ocr, nativeTextOnly = false, password = null), progress,
                        )
                    }
                }
            }
            // Camera pages are a scan; the core takes every image as a photo.
            if (source.kind == SourceKind.Scan) core { ffi.setSourceKind(DocumentSourceKind.SCAN) }
            // Joining and the first save are one step under the save lock: a
            // Project deleted meanwhile ([leaveProject]) is deleted either
            // before it, which leaves the import Unfiled, or after.
            synchronized(saving) {
                if (discarded) throw AppError.Cancelled()
                // Started in a Project: it joins it before its first save.
                val joining = synchronized(this) { chosenProject.takeIf { vaultId == null } }
                if (joining != null && runCatching { vault.project(joining) }.isFailure) {
                    // Its Project was deleted, and a stop came before the
                    // pending list said so: it lands in Unfiled, as the
                    // delete meant.
                    synchronized(this) { chosenProject = null }
                    pending.moveToUnfiled(id)
                } else if (joining != null) {
                    core { ffi.joinProject(vault, joining) }
                }
                // The import stays to read again until the Vault holds it: a
                // failed save leaves Recognize offered, not an empty document.
                save()
            }
            synchronized(this) {
                this.source = null
                ocrStarted = false
            }
            return snapshot()
        }

        /** Minutes a scan takes on this phone, when more than about one. */
        fun scanEstimate(): Int? = models?.detectorPaths()?.let { ffi.scanEstimate(it)?.toInt() }

        /** Finds candidates with the detector; a cancelled scan reruns from the start. */
        fun scan(observer: JobObserver): DocumentSnapshot {
            // A detector installed before mdoc timed it is timed first, in this
            // job, so the scan has its estimate; too slow, it never
            // scans.
            if (models?.snapshot()?.needsTiming == true) {
                observer.stage(JobStage.SCANNING)
                if (models.timeDetector(observer).belowFloor == FloorReason.SPEED) {
                    // Below the floor now: saved again, its names decided by
                    // hand are Ready (#1091 Q4 (b)).
                    if (vaultId != null) save()
                    throw AppError.Failed("The name finder is too slow on this phone. Select text to mark a name.")
                }
            }
            val detector = models?.detectorPaths() ?: throw AppError.ModelMissing()
            observer.onCancel { ffi.cancel() }
            val scanned = core { ffi.scan(detector, Progress(observer, device)) }
            synchronized(this) { review = scanned }
            save()
            return snapshot()
        }

        /** `seen`: the [DocumentSnapshot.opened] the candidate was picked from. */
        fun decide(candidate: CandidateId, decision: Decision, scope: Scope, seen: Int? = null): DocumentSnapshot {
            val mention = snapshot().candidate(candidate) ?: throw AppError.NotFound()
            val range = TextRange(mention.range.start.toULong(), mention.range.end.toULong())
            val reach = if (scope == Scope.AllOccurrences) FfiScope.ALL else FfiScope.THIS
            return change(seen) {
                when (decision) {
                    Decision.Replace -> ffi.replace(range, reach)
                    Decision.Keep -> ffi.keep(range, reach)
                }
            }
        }

        fun replaceAllPending(): DocumentSnapshot = change(null) { ffi.replaceAll() }

        /**
         * Proposals as a scan would give them, for UI tests and the survey on
         * an emulator with no detector (the iOS app's seedForUITesting).
         */
        internal fun seedProposals(candidates: List<uniffi.mdoc_ffi.Candidate>): DocumentSnapshot =
            change(null) { ffi.addCandidates(candidates) }

        /**
         * Replace…: a manual identity for the selected text, replaced in
         * every exact repeat; `seen`, the [DocumentSnapshot.opened] it was
         * selected in.
         */
        fun addReplacement(range: SourceRange, category: Category, seen: Int? = null): DocumentSnapshot {
            if (range.length <= 0) throw AppError.EmptySelection()
            val selection = TextRange(range.start.toULong(), range.end.toULong())
            return change(seen) {
                val proposed = ffi.addSelection(selection, ffiCategory(category))
                val mention = proposed.mentions.firstOrNull {
                    it.range.start <= selection.start && it.range.end >= selection.end
                } ?: return@change proposed
                ffi.replace(mention.range, FfiScope.ALL)
            }
        }

        /**
         * Copy for AI and Share (#1091 Decision 19): refused while proposals
         * are pending; the text comes from the Vault, which checks the
         * document is Ready against its Project's names as they are now, and
         * notes the share, so a move's Undo keeps the names a reply may use.
         * Its place is kept for Restore with mdoc. The document's notes
         * never go: the Vault's text is the document's alone.
         */
        fun copyForAi(): HandOff = synchronized(projectChanges) {
            // No move or Project delete meanwhile: the place kept is the one
            // whose names the text was shared in.
            val current = snapshot()
            // Every refusal in the app's own words, never the core's (#1209).
            fun refused(refusal: HandOffRefusal): Nothing = throw AppError.HandOffRefused(refusal)
            val vaultId = vaultId
            if (current.phase == DocumentPhase.Recognizing || vaultId == null) refused(HandOffRefusal.NotRead)
            if (current.pendingCount > 0) refused(HandOffRefusal.toDecide(current))
            val findsNames = !findsNoNames()
            if (current.phase == DocumentPhase.Unscanned && findsNames) refused(HandOffRefusal.NotScanned)
            // The review is done but the record was not saved Ready (a phone
            // fallen below the floor since): saved again, as iOS's tap does.
            // The Vault still decides.
            if (savedReadiness() == SavedReadiness.NotReviewed) runCatching { save() }.onFailure { refused(HandOffRefusal.Failed) }
            when (val saved = savedReadiness()) {
                // At the detector tier: names marked by hand, never scanned.
                SavedReadiness.NotReviewed -> refused(if (findsNames) HandOffRefusal.NotScanned else HandOffRefusal.NotChecked)
                null -> Unit
                else -> HandOffRefusal.of(saved)?.let(::refused)
            }
            // The title masked with every name its Project knows (#1192).
            val fileName = runCatching { core { vault.shareFileName(vaultId) } }.getOrElse { refused(HandOffRefusal.Failed) }
            val text = runCatching { core { vault.textForAi(vaultId) } }
                // A change between the check and the read: why, as it is now.
                .getOrElse { refused(savedReadiness()?.let(HandOffRefusal::of) ?: HandOffRefusal.Failed) }
            lastShared.set(projectId?.let { Place.Project(ProjectId(it)) } ?: Place.Unfiled(vaultId))
            HandOff(text, fileName)
        }

        /**
         * What the Vault says of the document as last saved: whether it may
         * leave for an AI now, checked against its Project's names as they
         * are now. Null before its first save, or when it can't be read.
         * Reads the record: off the main thread.
         */
        fun savedReadiness(): SavedReadiness? {
            val vaultId = vaultId ?: return null
            readiness?.let { return it(vaultId) }
            return runCatching {
                if (vault.document(vaultId).excludeFromAi) return SavedReadiness.ExcludedFromAi
                when (val readiness = vault.readiness(vaultId)) {
                    Readiness.Ready -> SavedReadiness.Ready
                    Readiness.NotReviewed -> SavedReadiness.NotReviewed
                    Readiness.ChangedSinceReview -> SavedReadiness.ChangedSinceReview
                    is Readiness.NamesNotReplaced -> SavedReadiness.NamesNotReplaced(readiness.count.toInt())
                }
            }.getOrNull()
        }

        /**
         * Opens the document again from the Vault, its names lined up with
         * its Project's as they are now: a name another document added
         * since this session opened it is proposed (Review's way to it).
         */
        fun lineUpWithProject(): DocumentSnapshot {
            holdingSave {
                val vaultId = vaultId ?: return@holdingSave
                reopen(vaultId, core { vault.document(vaultId) })
            }
            return snapshot()
        }

        /**
         * Restores `reply` with the document's names and saves it in the
         * Vault as an AI Reply to text shared from this document: its
         * Project's, or its own while Unfiled. The Reply pane shows it.
         */
        fun restoreReply(reply: String): DocumentSnapshot {
            core { ffi.restoreReply(reply) }
            val vaultId = vaultId ?: throw AppError.NotFound()
            val title = synchronized(this) { review?.title } ?: title
            core { vault.saveReply(SharedFrom.Document(vaultId), replyTitle(title), null, reply) }
            return snapshot()
        }

        /** Remove reply: `shown`, the saved reply the pane showed, is deleted from the Vault. */
        fun clearReply(shown: String): DocumentSnapshot {
            core { vault.deleteReply(shown) }
            return snapshot()
        }

        /**
         * A review transition, then the document saved into the Vault, as
         * desktop autosaves: one step under the save lock, so a move never
         * opens the document again between them and loses the change. One
         * made on a snapshot from before such an opening (`seen`) is refused:
         * its ranges were of the old text.
         */
        private fun change(seen: Int?, transition: () -> ReviewSnapshot): DocumentSnapshot {
            synchronized(saving) {
                reopenHeldIfStale()
                if (seen != null && seen != synchronized(this) { opened }) throw AppError.Failed(MOVED_MEANWHILE)
                val changed = core(transition)
                synchronized(this) { review = changed }
                saveHeld()
            }
            return snapshot()
        }

        /**
         * Saves into the Vault: Unfiled intake on a first save unless it
         * joined a Project, then its text, review and counts every time.
         * Its first save keeps the original in the Vault and ends its
         * import: the received files go.
         */
        private fun save() = synchronized(saving) { saveHeld() }

        /**
         * The Project an import not yet read was to join is deleted, its
         * documents moved to Unfiled: the import lands there too. True when
         * it was to join `project`.
         */
        internal fun leaveProject(project: String): Boolean = synchronized(this) {
            if (vaultId != null || chosenProject != project) return false
            chosenProject = null
            true
        }

        /** Undoes [leaveProject] when the Project's delete failed. */
        internal fun rejoin(project: String) = synchronized(this) {
            if (vaultId == null) chosenProject = project
        }

        /** Whether this import, not yet saved, was to join `project`. */
        internal fun joins(project: String): Boolean = synchronized(this) { vaultId == null && chosenProject == project }

        /** Runs `body` while no save of this session runs, nor starts. */
        internal fun <T> holdingSave(body: () -> T): T = synchronized(saving, body)

        private fun saveHeld() {
            if (discarded) throw AppError.Cancelled()
            ffi.setNoNameFinder(findsNoNames())
            val saved = core { ffi.saveIntoVault(vault) }
            // The save wrote the notes the core holds, whatever asked for it.
            notesUnsaved = false
            // Its Vault id is mapped to this session in the same step that
            // shows it: a Project's delete finds an import either not saved
            // yet (it waits on this save) or saved and known by its Vault id,
            // never saved and unknown to it.
            val first = synchronized(this@Documents) {
                ids[saved.id] = id
                synchronized(this) {
                    val first = vaultId == null
                    vaultId = saved.id
                    projectId = saved.project?.id
                    // The core holds its Project from now on: the one chosen at
                    // import would show it there after a move to Unfiled.
                    chosenProject = null
                    review = saved.review
                    first
                }
            }
            if (first) firstSaveShown?.invoke(id)
            if (first) {
                // The Vault's original takes the received files' place, and they go.
                val stored = storedOriginal(saved.id)
                synchronized(this) {
                    original.close()
                    original = stored
                }
                pending.remove(id)
            }
        }
    }

    /** The core's job stages and a scan's time left, and what it asks about the phone. */
    internal class Progress(private val observer: JobObserver, private val device: DeviceConditions) : JobProgress {
        override fun stage(stage: JobStage) = observer.stage(stage)

        // Android shows stages and a scan's time left, not a fraction.
        override fun advanced(done: ULong, total: ULong) = Unit

        override fun remaining(seconds: ULong) = observer.remaining(seconds.toLong())

        override fun thermal() = device.thermal()

        override fun freeMemory() = device.freeMemory()?.toULong()
    }

    internal companion object {
        /** The largest original viewed from memory (#1091 Decision 24). */
        val IN_MEMORY: ULong = 16uL * 1024uL * 1024uL

        /** A saved reply's title. */
        fun replyTitle(title: String) = "Reply to $title"

        /** Why an import not read yet does not move, in iOS's words. */
        const val NOT_READ_YET = "Move the document once it is read."

        /** Delete them refused: the Project holds a document the question did not count. */
        const val PROJECT_CHANGED = "A document joined the Project meanwhile, so nothing was deleted. Look at it again first."

        /** A review action on a snapshot from before a move. */
        const val MOVED_MEANWHILE = "The document moved meanwhile. Try again."

        /** As Home and the document show it: read, names searched for or marked, proposals left. */
        fun phase(scanned: Boolean, candidates: Int, proposed: Int) = when {
            !scanned && candidates == 0 -> DocumentPhase.Unscanned
            proposed > 0 -> DocumentPhase.Reviewing
            else -> DocumentPhase.Ready
        }

        /** The app's kind of a Vault document's file family. */
        fun sourceKind(kind: DocumentSourceKind) = when (kind) {
            DocumentSourceKind.PDF -> SourceKind.Pdf
            DocumentSourceKind.WORD -> SourceKind.Docx
            DocumentSourceKind.IMAGE -> SourceKind.Image
            DocumentSourceKind.SCAN -> SourceKind.Scan
            else -> SourceKind.Text
        }

        fun candidate(mention: Mention) = Candidate(
            id = CandidateId("${mention.range.start}-${mention.range.end}"),
            identityId = IdentityId(mention.identity?.toString() ?: "mention-${mention.range.start}"),
            range = SourceRange(mention.range.start.toInt(), mention.range.end.toInt()),
            original = mention.original,
            alias = mention.alias.orEmpty(),
            category = category(mention.category),
            state = when (mention.state) {
                FfiMentionState.PROPOSED -> MentionState.Proposed
                FfiMentionState.REPLACED -> MentionState.Replaced
                FfiMentionState.KEPT -> MentionState.Kept
            },
        )

        fun category(category: FfiCategory) = when (category) {
            FfiCategory.PERSON -> Category.Person
            FfiCategory.ORGANIZATION -> Category.Organization
            FfiCategory.EMAIL -> Category.Email
            FfiCategory.URL -> Category.Url
            FfiCategory.PHONE -> Category.Phone
            FfiCategory.ADDRESS -> Category.Address
            FfiCategory.IDENTITY -> Category.Identity
            FfiCategory.TAX -> Category.Tax
            FfiCategory.BANK -> Category.Bank
            FfiCategory.CASE_NUMBER -> Category.CaseNumber
            FfiCategory.DATE -> Category.Date
            FfiCategory.OTHER -> Category.Other
        }

        fun ffiCategory(category: Category) = when (category) {
            Category.Person -> FfiCategory.PERSON
            Category.Organization -> FfiCategory.ORGANIZATION
            Category.Email -> FfiCategory.EMAIL
            Category.Url -> FfiCategory.URL
            Category.Phone -> FfiCategory.PHONE
            Category.Address -> FfiCategory.ADDRESS
            Category.Identity -> FfiCategory.IDENTITY
            Category.Tax -> FfiCategory.TAX
            Category.Bank -> FfiCategory.BANK
            Category.CaseNumber -> FfiCategory.CASE_NUMBER
            Category.Date -> FfiCategory.DATE
            Category.Other -> FfiCategory.OTHER
        }

        /** The swatch nearest a Project's stored hue. */
        fun projectColour(hue: UInt): ProjectColour {
            val hues = listOf(
                ProjectColour.Red to 0.0, ProjectColour.Orange to 30.0, ProjectColour.Amber to 45.0, ProjectColour.Green to 130.0,
                ProjectColour.Teal to 175.0, ProjectColour.Blue to 215.0, ProjectColour.Violet to 270.0, ProjectColour.Pink to 330.0,
            )
            val value = (hue % 360u).toDouble()
            return hues.minBy { (_, h) -> minOf(kotlin.math.abs(h - value), 360 - kotlin.math.abs(h - value)) }.first
        }
    }
}

/** What the Reply pane's restored reply was made of: the reply, the text and the names. */
private data class ReplyKey(val reply: String?, val textRevision: ULong, val identities: List<uniffi.mdoc_ffi.IdentityRow>)

/**
 * Where text was shared from last (Copy for AI or Share): Restore with
 * mdoc restores with it first. An id only, a Project's or an Unfiled
 * document's, in a file of its own beside the imports (#1091, the
 * maintainer's choice for the Restore default).
 */
class LastShared(private val file: File) {
    @Synchronized
    fun get(): Place? = runCatching {
        val item = JSONObject(file.readText())
        item.optString("project").takeIf { it.isNotEmpty() }?.let { Place.Project(ProjectId(it)) }
            ?: Place.Unfiled(item.getString("document"))
    }.getOrNull()

    @Synchronized
    fun set(place: Place) {
        val item = when (place) {
            is Place.Project -> JSONObject().put("project", place.id.raw)
            is Place.Unfiled -> JSONObject().put("document", place.document)
        }
        file.parentFile?.mkdirs()
        // Best effort: a share never fails for want of this.
        runCatching {
            Originals.writeAtomically(file) {
                it.write(item.toString().toByteArray())
                true
            }
        }
    }
}

/** A restored reply as display segments, from the UTF-8 ranges the core reports. */
fun restoreResult(reply: RestoredReply): RestoreResult {
    val bytes = reply.text.toByteArray(Charsets.UTF_8)
    val marks = (reply.restored.map { it to true } + reply.unresolved.map { it to false })
        .sortedBy { it.first.start }
    val segments = mutableListOf<RestoredSegment>()
    var cursor = 0
    fun piece(from: Int, to: Int) = String(bytes, from, to - from, Charsets.UTF_8)
    for ((range, restored) in marks) {
        val start = range.start.toInt()
        val end = range.end.toInt()
        if (start < cursor || end > bytes.size) continue
        if (start > cursor) segments += RestoredSegment.Text(piece(cursor, start))
        segments += if (restored) RestoredSegment.Restored(piece(start, end)) else RestoredSegment.Unresolved(piece(start, end))
        cursor = end
    }
    if (cursor < bytes.size) segments += RestoredSegment.Text(piece(cursor, bytes.size))
    return RestoreResult(segments, reply.values.toInt(), reply.unrecognised.toInt())
}

/** Runs a core call and turns its error into the app's. */
internal fun <T> core(call: () -> T): T = try {
    call()
} catch (error: AppError) {
    throw error
} catch (error: CoreException.Cancelled) {
    throw AppError.Cancelled()
} catch (error: CoreException.NeedsOcr) {
    throw AppError.ModelMissing()
} catch (error: CoreException.NeedsPassword) {
    throw AppError.NeedsPassword()
} catch (error: CoreException.WrongPassword) {
    throw AppError.WrongPassword()
} catch (error: CoreException) {
    throw AppError.Failed(error.detailOrMessage())
}

private fun CoreException.detailOrMessage(): String = when (this) {
    is CoreException.Import -> detail
    is CoreException.Ocr -> detail
    is CoreException.Detect -> detail
    is CoreException.Review -> detail
    is CoreException.Project -> detail
    is CoreException.Setup -> detail
    is CoreException.Invalid -> detail
    is CoreException.Io -> detail
    else -> message ?: toString()
}

/**
 * Imports not yet read, kept on disk until their first save, so they
 * survive mdoc stopping (`pending.json` in the intake folder).
 */
internal class PendingImports(storage: MdocStorage, private val owner: (File) -> File?) {
    data class Entry(
        val id: DocumentId,
        val title: String,
        /** The Project it was started in; `null`: Unfiled. */
        val projectId: String?,
        val source: Originals.Prepared,
        val ocrStarted: Boolean,
    )

    private val file = storage.pendingFile

    /**
     * `pending.json` couldn't be read and was moved aside
     * (`pending.json.corrupt`): imports start from an empty list. It stays
     * true at every start while that copy is kept, since nothing else says
     * which staged pages its imports owned: no stage is cleared meanwhile.
     */
    val setAside: Boolean get() = readFailed || corrupt.isFile

    /** This process found `pending.json` unreadable, moved aside or not. */
    @Volatile
    private var readFailed = false

    private val corrupt = File(file.parentFile, "pending.json.corrupt")

    @Synchronized
    fun all(): List<Entry> = load().values.toList()

    @Synchronized
    fun add(entry: Entry) = save(load() + (entry.id.raw to entry))

    @Synchronized
    fun markOcrStarted(id: DocumentId) {
        val entries = load()
        entries[id.raw]?.let { save(entries + (id.raw to it.copy(ocrStarted = true))) }
    }

    /** An import's Project was deleted, its documents moved: it lands in Unfiled. */
    @Synchronized
    fun moveToUnfiled(id: DocumentId) {
        val entries = load()
        entries[id.raw]?.takeIf { it.projectId != null }?.let { save(entries + (id.raw to it.copy(projectId = null))) }
    }

    /** Undoes [moveToUnfiled] when the Project's delete failed. */
    @Synchronized
    fun rejoin(id: DocumentId, project: String) {
        val entries = load()
        entries[id.raw]?.let { save(entries + (id.raw to it.copy(projectId = project))) }
    }

    /** Forgets an import, and its received files go. */
    @Synchronized
    fun remove(id: DocumentId) {
        val entries = load()
        val entry = entries[id.raw] ?: return
        // Retired first: a stop in between leaves files no entry owns,
        // which the next start clears, never an entry without its files.
        save(entries - id.raw)
        entry.source.file?.let(owner)?.deleteRecursively()
    }

    private fun load(): Map<String, Entry> {
        if (!file.isFile) return emptyMap()
        return runCatching { parse(file.readText()) }.getOrElse {
            // One damaged list must not stop every later import, nor, if it
            // can't even be moved aside, let a start clear its stages.
            readFailed = true
            runCatching {
                Files.move(
                    file.toPath(), corrupt.toPath(),
                    StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE,
                )
            }
            emptyMap()
        }
    }

    private fun parse(text: String): Map<String, Entry> {
        val array = JSONArray(text)
        return (0 until array.length()).associate { i ->
            val item = array.getJSONObject(i)
            val kind = SourceKind.valueOf(item.getString("kind"))
            val images = item.optJSONArray("images")
            val input = when {
                item.has("text") -> Originals.Input.Text(item.getString("text"))
                images != null -> Originals.Input.Images((0 until images.length()).map { File(images.getString(it)) })
                else -> Originals.Input.File(File(item.getString("file")), item.getBoolean("needsOcr"))
            }
            val id = item.getString("id")
            id to Entry(
                DocumentId(id), item.getString("title"), item.optString("project").takeIf { it.isNotEmpty() },
                Originals.Prepared(input, kind), item.optBoolean("ocrStarted"),
            )
        }
    }

    private fun save(entries: Map<String, Entry>) {
        val array = JSONArray()
        for (entry in entries.values) {
            val item = JSONObject()
                .put("id", entry.id.raw).put("title", entry.title)
                .put("kind", entry.source.kind.name).put("ocrStarted", entry.ocrStarted)
            entry.projectId?.let { item.put("project", it) }
            when (val input = entry.source.input) {
                is Originals.Input.Text -> item.put("text", input.text)
                is Originals.Input.File -> item.put("file", input.file.path).put("needsOcr", input.needsOcr)
                is Originals.Input.Images -> item.put("images", JSONArray().apply { input.files.forEach { put(it.path) } })
            }
            array.put(item)
        }
        file.parentFile?.mkdirs()
        Originals.writeAtomically(file) {
            it.write(array.toString().toByteArray())
            true
        }
    }
}
