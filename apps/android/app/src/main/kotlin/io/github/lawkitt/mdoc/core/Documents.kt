package io.github.lawkitt.mdoc.core

import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import java.util.UUID
import org.json.JSONArray
import org.json.JSONObject
import uniffi.mdoc_ffi.CoreException
import uniffi.mdoc_ffi.DocumentSession as FfiSession
import uniffi.mdoc_ffi.ImportOptions
import uniffi.mdoc_ffi.JobProgress
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.MdocLibrary
import uniffi.mdoc_ffi.Mention
import uniffi.mdoc_ffi.RestoredReply
import uniffi.mdoc_ffi.ReviewSnapshot
import uniffi.mdoc_ffi.TextRange
import uniffi.mdoc_ffi.markdownStyling
import uniffi.mdoc_ffi.Category as FfiCategory
import uniffi.mdoc_ffi.MentionState as FfiMentionState
import uniffi.mdoc_ffi.Scope as FfiScope

/** Progress and cooperative cancellation for a long core call. */
interface JobObserver {
    fun stage(stage: JobStage)

    /** Runs `handler` once when the job is cancelled (at once if it already is). */
    fun onCancel(handler: () -> Unit)
}

/**
 * Cases and stored documents over the core's `MdocLibrary` (ADR 0085): the
 * twin of the iOS app's `RealLibrary`. It converts the core's records into
 * the view types and keeps which source file a document came from.
 */
class Documents(
    private val ffi: MdocLibrary,
    private val storage: MdocStorage,
    private val models: Models?,
) {
    private val sessions = mutableMapOf<DocumentId, Session>()

    /** A saved document's path → the id this run gave it. */
    private val ids = mutableMapOf<String, DocumentId>()
    private val originals = Originals(storage)
    private val pending = PendingImports(storage, originals::importFolder)

    init {
        // Imports not yet read when mdoc last stopped come back, ready to read again.
        for (entry in pending.all()) {
            sessions[entry.id] = Session(entry.id, entry.title, entry.caseId, entry.source, entry.ocrStarted)
        }
    }

    @Synchronized
    fun snapshot(): LibrarySnapshot {
        val cases = ffi.cases().cases.map { CaseSummary(CaseId(it.id), it.name, caseColour(it.hue)) }
        val documents = mutableListOf<DocumentSummary>()
        val seen = mutableSetOf<DocumentId>()
        for (stored in runCatching { ffi.documents() }.getOrDefault(emptyList())) {
            val session = runCatching { session(stored.path) }.getOrNull() ?: continue
            seen += session.id
            documents += session.summary(stored.modified.toLong() * 1000)
        }
        documents += sessions.values.filter { it.id !in seen }.map { it.summary(it.created) }
        return LibrarySnapshot(
            cases = cases,
            documents = documents.sortedByDescending { it.updated },
            currentCase = ffi.currentCase()?.let { CaseId(it.id) },
        )
    }

    @Synchronized
    fun session(id: DocumentId): Session = sessions[id] ?: session(id.raw)

    /** The session of a saved document, opened in its case the first time. */
    private fun session(path: String): Session {
        ids[path]?.let { id -> sessions[id]?.let { return it } }
        if (!File(path).exists()) throw AppError.NotFound()
        val session = Session(DocumentId(path), "", null, null)
        session.reopen(path)
        sessions[session.id] = session
        ids[path] = session.id
        return session
    }

    /** Stores the source, starts its case and returns its session, ready to read. */
    @Synchronized
    fun importDocument(source: ImportSource, title: String, case: CaseChoice): Session {
        val caseId = when (case) {
            is CaseChoice.Existing -> case.id.raw
            is CaseChoice.New -> core { ffi.createCase(case.name) }.id
        }
        val prepared = originals.prepare(source, title)
        val id = DocumentId("new:${UUID.randomUUID()}")
        pending.add(PendingImports.Entry(id, title, caseId, prepared, ocrStarted = false))
        return Session(id, title, caseId, prepared).also { sessions[id] = it }
    }

    @Synchronized
    fun delete(id: DocumentId) {
        val session = sessions[id] ?: throw AppError.NotFound()
        // An import still being read must not save itself back afterwards.
        session.discard()
        val path = session.path
        val source = session.sourceFile
        if (path != null) {
            core { ffi.deleteDocument(path) }
            ids.remove(path)
            originals.remove(path)
        } else if (source != null) {
            // OCR's kept pages go first, found by the source file's digest.
            core { ffi.discardImport(source.path) }
        }
        sessions.remove(id)
        pending.remove(id)
    }

    /** Restores a reply with one case's mapping; needs no model. */
    fun restore(reply: String, case: CaseId): RestoreResult = restoreResult(core { ffi.restore(case.raw, reply) })

    /** Keeps `reply`, in alias form, as the document's reply (ADR 0078). */
    fun saveReply(reply: String, document: DocumentId) {
        val session = session(document)
        val path = session.path ?: throw AppError.NoCase()
        val case = session.caseId ?: throw AppError.NoCase()
        core { ffi.saveReply(case, path, reply) }
    }

    /** The case of the most recently shared document, for Restore with mdoc. */
    fun currentCase(): CaseId? = ffi.currentCase()?.let { CaseId(it.id) }

    private fun caseName(id: String?): String? = id?.let { runCatching { ffi.caseInfo(it).name }.getOrNull() }

    /** One document's import, OCR, review and hand-off: the twin of the iOS `RealSession`. */
    inner class Session internal constructor(
        val id: DocumentId,
        private var title: String,
        caseId: String?,
        private var source: Originals.Prepared?,
        private var ocrStarted: Boolean = false,
    ) {
        val created = System.currentTimeMillis()
        private val ffi = FfiSession()
        private var review: ReviewSnapshot? = null
        private var scanned = false
        private var kind: SourceKind = source?.kind ?: SourceKind.Text
        var caseId: String? = caseId
            private set
        var path: String? = null
            private set

        internal val sourceFile: File? get() = source?.file

        @Volatile private var discarded = false

        internal fun discard() {
            discarded = true
            ffi.cancel()
        }

        internal fun reopen(path: String) {
            val opened = core { ffi.openDocument(this@Documents.ffi, path) }
            this.path = path
            review = opened
            title = opened.title
            caseId = opened.caseId
            scanned = opened.mentions.isNotEmpty() || originals.scanned(path)
            kind = originals.kind(path)
        }

        internal fun summary(updated: Long): DocumentSummary {
            val current = snapshot()
            return DocumentSummary(
                id, current.title, current.caseId, current.kind, current.phase,
                current.pendingCount, updated, current.candidates.size,
            )
        }

        @Synchronized
        fun snapshot(): DocumentSnapshot {
            val review = review
                ?: return DocumentSnapshot(
                    id, title, caseId?.let(::CaseId), caseName(caseId), kind, DocumentPhase.Recognizing,
                    "", null, emptyList(), emptyList(), null, resumesOcr = ocrStarted,
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
            val phase = when {
                !scanned && review.mentions.isEmpty() -> DocumentPhase.Unscanned
                review.proposed > 0uL -> DocumentPhase.Reviewing
                else -> DocumentPhase.Ready
            }
            val reply = path?.let { path ->
                val case = review.caseId ?: caseId ?: return@let null
                val text = runCatching { this@Documents.ffi.reply(case, path) }.getOrNull()
                    ?.takeIf { it.isNotEmpty() } ?: return@let null
                runCatching {
                    val restored = restoreResult(ffi.restoreReply(text))
                    ReplySnapshot(text, restored, markdownStyling(restored.text))
                }.getOrNull()
            }
            return DocumentSnapshot(
                id, review.title, (review.caseId ?: caseId)?.let(::CaseId), caseName(review.caseId ?: caseId),
                kind, phase, review.text, markdownStyling(review.text), candidates, identities, reply,
                warning = review.warning,
            )
        }

        /** Import and OCR; resumes after the pages already finished. */
        fun recognize(observer: JobObserver): DocumentSnapshot {
            val source = synchronized(this) { source } ?: return snapshot()
            observer.onCancel { ffi.cancel() }
            val progress = Progress(observer)
            observer.stage(JobStage.READING)
            val imported = when (val input = source.input) {
                is Originals.Input.Text -> core { ffi.importText(input.text, title) }
                is Originals.Input.File -> {
                    val ocr = models?.ocrPaths()
                    if (input.needsOcr && ocr == null) throw AppError.ModelMissing()
                    if (ocr != null && (input.needsOcr || source.kind == SourceKind.Pdf) && !ocrStarted) {
                        ocrStarted = true
                        pending.markOcrStarted(id)
                    }
                    core {
                        ffi.importFileIn(
                            this@Documents.ffi, input.file.path, ImportOptions(ocr, nativeTextOnly = false), progress,
                        )
                    }
                }
            }
            caseId?.let { case -> core { ffi.joinCase(this@Documents.ffi, case) } }
            synchronized(this) {
                review = imported
                this.source = null
                ocrStarted = false
            }
            save()
            return snapshot()
        }

        /** Finds candidates with the detector; a cancelled scan reruns from the start. */
        fun scan(observer: JobObserver): DocumentSnapshot {
            val detector = models?.detectorPaths() ?: throw AppError.ModelMissing()
            observer.onCancel { ffi.cancel() }
            val scanned = core { ffi.scan(detector, Progress(observer)) }
            synchronized(this) {
                review = scanned
                this.scanned = true
            }
            save()
            // Kept, so an empty scan still reads "Nothing found" after mdoc restarts (ADR 0087).
            path?.let(originals::markScanned)
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

        /** Copy for AI's text, through the desktop's export path; records the case as shared last. */
        fun copyForAi(): HandOff {
            val current = snapshot()
            if (current.pendingCount > 0) throw AppError.PendingReview(current.pendingCount)
            val text = core { ffi.copyForAi() }
            val fileName = core { ffi.shareFileName() }
            caseId?.let { case -> core { this@Documents.ffi.noteShared(case) } }
            return HandOff(text, fileName)
        }

        /** Restores a reply with this document's case and keeps it as the document's reply. */
        fun restoreReply(reply: String): DocumentSnapshot {
            val case = caseId ?: throw AppError.NoCase()
            val path = path ?: throw AppError.NoCase()
            core { ffi.restoreReply(reply) }
            core { this@Documents.ffi.saveReply(case, path, reply) }
            return snapshot()
        }

        fun clearReply(): DocumentSnapshot {
            val case = caseId ?: throw AppError.NoCase()
            val path = path ?: throw AppError.NoCase()
            core { this@Documents.ffi.saveReply(case, path, "") }
            return snapshot()
        }

        /** A review transition, then the document saved into its case, as desktop autosaves. */
        private fun change(transition: () -> ReviewSnapshot): DocumentSnapshot {
            val changed = core(transition)
            synchronized(this) { review = changed }
            save()
            return snapshot()
        }

        private fun save() {
            if (discarded) throw AppError.Cancelled()
            val saved = core { ffi.save(this@Documents.ffi) }
            synchronized(this) {
                path = saved.path
                caseId = saved.case.id
                review = saved.review
            }
            synchronized(this@Documents) { ids[saved.path] = id }
            originals.record(saved.path, kind, source?.file ?: originalFile)
            pending.remove(id, keepFiles = true)
        }

        private var originalFile: File? = source?.file
    }

    private class Progress(private val observer: JobObserver) : JobProgress {
        override fun stage(stage: JobStage) = observer.stage(stage)
    }

    private companion object {
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
            FfiCategory.PHONE -> Category.Phone
            FfiCategory.ADDRESS -> Category.Address
            FfiCategory.IDENTITY -> Category.Identity
            FfiCategory.TAX -> Category.Tax
            FfiCategory.BANK -> Category.Bank
            FfiCategory.DATE -> Category.Date
            FfiCategory.OTHER -> Category.Other
        }

        fun ffiCategory(category: Category) = when (category) {
            Category.Person -> FfiCategory.PERSON
            Category.Organization -> FfiCategory.ORGANIZATION
            Category.Email -> FfiCategory.EMAIL
            Category.Phone -> FfiCategory.PHONE
            Category.Address -> FfiCategory.ADDRESS
            Category.Identity -> FfiCategory.IDENTITY
            Category.Tax -> FfiCategory.TAX
            Category.Bank -> FfiCategory.BANK
            Category.Date -> FfiCategory.DATE
            Category.Other -> FfiCategory.OTHER
        }

        /** The swatch nearest a case's stored hue (ADR 0077). */
        fun caseColour(hue: UInt): CaseColour {
            val hues = listOf(
                CaseColour.Red to 0.0, CaseColour.Orange to 30.0, CaseColour.Amber to 45.0, CaseColour.Green to 130.0,
                CaseColour.Teal to 175.0, CaseColour.Blue to 215.0, CaseColour.Violet to 270.0, CaseColour.Pink to 330.0,
            )
            val value = (hue % 360u).toDouble()
            return hues.minBy { (_, h) -> minOf(kotlin.math.abs(h - value), 360 - kotlin.math.abs(h - value)) }.first
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
 * survive mdoc stopping (`pending.json` beside the core's folders).
 */
internal class PendingImports(storage: MdocStorage, private val owner: (File) -> File?) {
    data class Entry(
        val id: DocumentId,
        val title: String,
        val caseId: String,
        val source: Originals.Prepared,
        val ocrStarted: Boolean,
    )

    private val file = File(storage.root, "pending.json")

    @Synchronized
    fun all(): List<Entry> = load().values.toList()

    @Synchronized
    fun add(entry: Entry) = save(load() + (entry.id.raw to entry))

    @Synchronized
    fun markOcrStarted(id: DocumentId) {
        val entries = load()
        entries[id.raw]?.let { save(entries + (id.raw to it.copy(ocrStarted = true))) }
    }

    @Synchronized
    fun remove(id: DocumentId, keepFiles: Boolean = false) {
        val entries = load()
        val entry = entries[id.raw] ?: return
        if (!keepFiles) entry.source.file?.let(owner)?.deleteRecursively()
        save(entries - id.raw)
    }

    private fun load(): Map<String, Entry> = runCatching {
        val array = JSONArray(file.readText())
        (0 until array.length()).associate { i ->
            val item = array.getJSONObject(i)
            val kind = SourceKind.valueOf(item.getString("kind"))
            val input = if (item.has("text")) {
                Originals.Input.Text(item.getString("text"))
            } else {
                Originals.Input.File(File(item.getString("file")), item.getBoolean("needsOcr"))
            }
            val id = item.getString("id")
            id to Entry(
                DocumentId(id), item.getString("title"), item.getString("case"),
                Originals.Prepared(input, kind), item.optBoolean("ocrStarted"),
            )
        }
    }.getOrDefault(emptyMap())

    private fun save(entries: Map<String, Entry>) {
        val array = JSONArray()
        for (entry in entries.values) {
            val item = JSONObject()
                .put("id", entry.id.raw).put("title", entry.title).put("case", entry.caseId)
                .put("kind", entry.source.kind.name).put("ocrStarted", entry.ocrStarted)
            when (val input = entry.source.input) {
                is Originals.Input.Text -> item.put("text", input.text)
                is Originals.Input.File -> item.put("file", input.file.path).put("needsOcr", input.needsOcr)
            }
            array.put(item)
        }
        file.parentFile?.mkdirs()
        val temp = File(file.path + ".new")
        temp.writeText(array.toString())
        temp.renameTo(file)
    }
}
