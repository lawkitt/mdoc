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
        /** The Project chosen at import; `null`: Unfiled. */
        private val chosenProject: String?,
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
            // Started in a Project: it joins it before its first save.
            if (chosenProject != null && vaultId == null) core { ffi.joinProject(vault, chosenProject) }
            synchronized(this) {
                this.source = null
                ocrStarted = false
            }
            save()
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

        /** Remove reply: the reply the pane shows is deleted from the Vault. */
        fun clearReply(): DocumentSnapshot {
            val shown = snapshot().reply?.id ?: return snapshot()
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
        private fun save() {
            if (discarded) throw AppError.Cancelled()
            ffi.setNoNameFinder(findsNoNames())
            val saved = core { ffi.saveIntoVault(vault) }
            val first = synchronized(this) {
                val first = vaultId == null
                vaultId = saved.id
                projectId = saved.project?.id
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
    is CoreException.Case -> detail
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

    /** Forgets an import, and its received files go. */
    @Synchronized
    fun remove(id: DocumentId) {
        val entries = load()
        val entry = entries[id.raw] ?: return
        entry.source.file?.let(owner)?.deleteRecursively()
        save(entries - id.raw)
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
