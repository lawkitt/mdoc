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
import uniffi.mdoc_ffi.DocumentSession as FfiSession
import uniffi.mdoc_ffi.DocumentSourceKind
import uniffi.mdoc_ffi.FloorReason
import uniffi.mdoc_ffi.ImportOptions
import uniffi.mdoc_ffi.JobProgress
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.Mention
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

    @Synchronized
    fun session(id: DocumentId): Session = sessions[id] ?: stored(id.raw)

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
     * import makes a Project.
     */
    @Synchronized
    fun importDocument(source: ImportSource, title: String, project: ProjectId? = null): Session {
        val prepared = originals.prepare(source, title)
        val id = DocumentId("new:${UUID.randomUUID()}")
        pending.add(PendingImports.Entry(id, title, project?.raw, prepared, ocrStarted = false))
        return Session(id, title, project?.raw, prepared).also { sessions[id] = it }
    }

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
     * Project lands in Unfiled, or is deleted with the rest. The caller
     * stops a job on a document it deletes first.
     */
    fun deleteProject(project: ProjectId, documents: ProjectDeletion) {
        val raw = project.raw
        // The imports that would join it, first; the Documents' lock only to
        // read them: a save holds its session's save lock, then takes this one.
        val imports = synchronized(this) { sessions.values.filter { it.joins(raw) } }
        try {
            // No import's join and first save runs meanwhile, so the members
            // read now are all of them: each import lands in Unfiled, or,
            // saved in the Project before, moves with the rest.
            val open = holdingSaves(imports) {
                val stored = vault.documents().filter { it.projectId == raw }.map { it.id }
                val members = synchronized(this) { stored.map { it to (ids[it] ?: DocumentId(it)) } }
                when (documents) {
                    ProjectDeletion.DeleteDocuments -> {
                        (imports.filter { it.joins(raw) }.map { it.id } + members.map { it.second }).forEach(::delete)
                        core { vault.deleteProject(raw, uniffi.mdoc_ffi.ProjectDocuments.DELETE) }
                        emptyList<Pair<String, Session>>()
                    }
                    ProjectDeletion.MoveToUnfiled -> {
                        // Unfiled in the pending list first: a stop before the
                        // Project's delete leaves it Unfiled in a Project that
                        // stays, which its first save takes as it is.
                        val leaving = imports.filter { it.leaveProject(raw) }
                        leaving.forEach { pending.moveToUnfiled(it.id) }
                        try {
                            core { vault.deleteProject(raw, uniffi.mdoc_ffi.ProjectDocuments.MOVE_TO_UNFILED) }
                        } catch (error: Exception) {
                            leaving.forEach {
                                it.rejoin(raw)
                                pending.rejoin(it.id, raw)
                            }
                            throw error
                        }
                        synchronized(this) { members.mapNotNull { (vaultId, id) -> sessions[id]?.let { vaultId to it } } }
                    }
                }
            }
            // The core rewrote their text: each opens again as it is now.
            for ((vaultId, session) in open) {
                runCatching { session.reopen(vaultId, vault.document(vaultId)) }
            }
        } finally {
            synchronized(projectNames) { projectNames.clear() }
        }
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

        /** Opens a saved document from the Vault: its review as saved, its names lined up with its Project's as they are now. */
        internal fun reopen(vaultId: String, info: DocumentInfo) {
            val opened = core { ffi.openFromVault(vault, vaultId) }
            val stored = storedOriginal(vaultId)
            synchronized(this) {
                this.vaultId = vaultId
                review = opened
                projectId = info.projectId
                kind = sourceKind(info.sourceKind)
                original.close()
                original = stored
            }
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
                    "", null, emptyList(), emptyList(), null, resumesOcr = ocrStarted, original = original,
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
            )
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

        fun decide(candidate: CandidateId, decision: Decision, scope: Scope): DocumentSnapshot {
            val mention = snapshot().candidate(candidate) ?: throw AppError.NotFound()
            val range = TextRange(mention.range.start.toULong(), mention.range.end.toULong())
            val reach = if (scope == Scope.AllOccurrences) FfiScope.ALL else FfiScope.THIS
            return change {
                when (decision) {
                    Decision.Replace -> ffi.replace(range, reach)
                    Decision.Keep -> ffi.keep(range, reach)
                }
            }
        }

        fun replaceAllPending(): DocumentSnapshot = change { ffi.replaceAll() }

        /**
         * Proposals as a scan would give them, for UI tests and the survey on
         * an emulator with no detector (the iOS app's seedForUITesting).
         */
        internal fun seedProposals(candidates: List<uniffi.mdoc_ffi.Candidate>): DocumentSnapshot =
            change { ffi.addCandidates(candidates) }

        /** Replace…: a manual identity for the selected text, replaced in every exact repeat. */
        fun addReplacement(range: SourceRange, category: Category): DocumentSnapshot {
            if (range.length <= 0) throw AppError.EmptySelection()
            val selection = TextRange(range.start.toULong(), range.end.toULong())
            return change {
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
         * Its place is kept for Restore with mdoc.
         */
        fun copyForAi(): HandOff {
            val current = snapshot()
            if (current.pendingCount > 0) throw AppError.PendingReview(current.pendingCount)
            val vaultId = vaultId ?: throw AppError.NotFound()
            val fileName = core { ffi.shareFileName() }
            val text = core { vault.textForAi(vaultId) }
            lastShared.set(projectId?.let { Place.Project(ProjectId(it)) } ?: Place.Unfiled(vaultId))
            return HandOff(text, fileName)
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

        /** A review transition, then the document saved into the Vault, as desktop autosaves. */
        private fun change(transition: () -> ReviewSnapshot): DocumentSnapshot {
            val changed = core(transition)
            synchronized(this) { review = changed }
            save()
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
            val first = synchronized(this) {
                val first = vaultId == null
                vaultId = saved.id
                projectId = saved.project?.id
                // The core holds its Project from now on: the one chosen at
                // import would show it there after a move to Unfiled.
                chosenProject = null
                review = saved.review
                first
            }
            synchronized(this@Documents) { ids[saved.id] = id }
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
