package io.github.lawkitt.mdoc.document

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.lawkitt.mdoc.core.AppError
import io.github.lawkitt.mdoc.core.Candidate
import io.github.lawkitt.mdoc.core.CandidateId
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.Decision
import io.github.lawkitt.mdoc.core.DocumentPhase
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.DocumentSnapshot
import io.github.lawkitt.mdoc.core.Documents
import io.github.lawkitt.mdoc.core.HandOff
import io.github.lawkitt.mdoc.core.HandOffRefusal
import io.github.lawkitt.mdoc.core.MentionState
import io.github.lawkitt.mdoc.core.PromotedFrom
import io.github.lawkitt.mdoc.core.SavedReadiness
import io.github.lawkitt.mdoc.core.Scope
import io.github.lawkitt.mdoc.core.SelectionAction
import io.github.lawkitt.mdoc.core.SelectionCopy
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.home.ShownReply
import io.github.lawkitt.mdoc.jobs.CancelReason
import io.github.lawkitt.mdoc.jobs.JobKind
import io.github.lawkitt.mdoc.jobs.JobOutcome
import io.github.lawkitt.mdoc.jobs.Jobs
import io.github.lawkitt.mdoc.jobs.RunningJob
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The header's one primary action, following the document's state. */
sealed interface PrimaryAction {
    data class Recognize(val resume: Boolean) : PrimaryAction

    /** A file that needs no recognition stopped importing before its text was read. */
    data object FinishImport : PrimaryAction

    data object FindNames : PrimaryAction

    data class Running(val job: RunningJob) : PrimaryAction

    /** Another document's job holds the one slot. */
    data object Waiting : PrimaryAction

    data class ReplaceAll(val count: Int) : PrimaryAction

    data object HandOff : PrimaryAction
}

data class DocumentState(
    val snapshot: DocumentSnapshot,
    /** The candidate whose sheet is open. */
    val reviewing: CandidateId? = null,
    /** The selection Replace… was chosen for, with its text. */
    val manual: Pair<SourceRange, String>? = null,
    val showsIdentities: Boolean = false,
    val showsReplySheet: Boolean = false,
    /** Showing the restored reply rather than the document. */
    val showsReply: Boolean = false,
    val error: String? = null,
    /** A short confirmation such as "Copied for AI". */
    val notice: Int? = null,
    /** Recognition or Find names needs a model that is not downloaded. */
    val needsSetup: JobKind? = null,
    /** A locked DOCX or PDF waits for its password: the password card shows. */
    val passwordPrompt: PasswordPrompt? = null,
    /** Notes: the sheet with the document's private notes. */
    val showsNotes: Boolean = false,
    /** The saved reply a promoted document came from, shown restored (From AI reply). */
    val fromReply: ShownReply? = null,
    /** Where Ask AI's wait for the AI app's answer stands; null when nothing waits. */
    val replyOffer: ReplyOffer? = null,
    /** What the Vault says of the document as saved, for this snapshot; null until asked. */
    val saved: SavedReadiness? = null,
    /** Copy for AI or Share refused on a tap: its dialog shows (#1209). */
    val refusal: HandOffRefusal? = null,
    /** The hand-off last tapped, which "I checked the names myself" goes on with (item 48). */
    val tapped: HandOffKind? = null,
) {
    /** "Paste reply?" shows, back from the AI app Ask AI shared to. */
    val showsReplyBanner: Boolean get() = replyOffer == ReplyOffer.Offered
}

/** The ways out "I checked the names myself" goes on with: the one tapped (item 48). */
enum class HandOffKind { Copy, Share, ShareFile, AskAi }

enum class PasswordPrompt {
    Needed,

    /** The last password didn't unlock it. */
    Wrong,
}

/**
 * One document screen: the core's latest snapshot and view state only;
 * every change goes through the session and comes back as a new snapshot.
 */
class DocumentViewModel(
    private val documents: Documents,
    private val session: Documents.Session,
    private val jobs: Jobs,
    /** Above the detector floor, as setup reads it now; below it there is no Find names. */
    private val detectorTier: () -> Boolean,
    /** Keeps Ask AI's wait when Android ends mdoc while the AI app is in front. */
    private val savedState: SavedStateHandle = SavedStateHandle(),
) : ViewModel() {
    /**
     * Above the detector floor; below it there is no Find names. Read
     * again after each scan and on resume: a scan's first timing of the
     * name finder can put the phone below the floor.
     */
    @Volatile var findsNames: Boolean = detectorTier()
        private set

    private val mutable = MutableStateFlow(
        DocumentState(session.snapshot(), replyOffer = savedState.get<String>(REPLY_OFFER)?.let(ReplyOffer::valueOf)),
    )
    val state: StateFlow<DocumentState> = mutable.asStateFlow()

    val running: StateFlow<RunningJob?> = jobs.current
    val interrupted = jobs.interrupted

    /**
     * The id a job of this document carries: its Vault id once it has one,
     * which is the id it has after mdoc restarts, so an interrupted job still
     * finds its document.
     */
    private val jobId get() = session.vaultId?.let(::DocumentId) ?: session.id

    /** Saved in the Vault: it moves (an import not read yet does not). */
    val saved: Boolean get() = session.vaultId != null

    /** Whether a job or interruption belongs to this document. */
    fun isMine(id: DocumentId?): Boolean = id != null && (id == session.id || id.raw == session.vaultId)

    fun primaryAction(state: DocumentState, job: RunningJob?): PrimaryAction {
        val snapshot = state.snapshot
        if (job != null && isMine(job.documentId)) return PrimaryAction.Running(job)
        val phase = snapshot.phase
        return when {
            // Nothing to find below the floor: marking by selection, then the hand-off.
            phase == DocumentPhase.Unscanned && !findsNames ->
                if (snapshot.pendingCount > 0) PrimaryAction.ReplaceAll(snapshot.pendingCount) else PrimaryAction.HandOff
            (phase == DocumentPhase.Recognizing || phase == DocumentPhase.Unscanned) && job != null -> PrimaryAction.Waiting
            phase == DocumentPhase.Recognizing && !snapshot.kind.needsRecognition -> PrimaryAction.FinishImport
            phase == DocumentPhase.Recognizing -> PrimaryAction.Recognize(snapshot.resumesOcr)
            phase == DocumentPhase.Unscanned -> PrimaryAction.FindNames
            phase == DocumentPhase.Reviewing -> PrimaryAction.ReplaceAll(snapshot.pendingCount)
            else -> PrimaryAction.HandOff
        }
    }

    // Long jobs

    fun recognize() = recognize(password = null)

    /** Unlock on the password card: the import runs again with [password], for this one call. */
    fun unlock(password: String) {
        if (password.isNotEmpty()) recognize(password)
    }

    private fun recognize(password: String?) = viewModelScope.launch {
        mutable.update { it.copy(needsSetup = null) }
        val outcome = jobs.run(JobKind.Recognize, jobId) { session.recognize(it, password) }
        when ((outcome as? JobOutcome.Failed)?.error) {
            is AppError.NeedsPassword -> mutable.update { it.copy(passwordPrompt = PasswordPrompt.Needed) }
            is AppError.WrongPassword -> mutable.update { it.copy(passwordPrompt = PasswordPrompt.Wrong) }
            else -> {
                // A locked scan its password opened can need the OCR model:
                // the setup card takes the password card's place, which asks
                // again once the model is in.
                val opened = outcome is JobOutcome.Finished ||
                    (outcome as? JobOutcome.Failed)?.error is AppError.ModelMissing
                if (opened) mutable.update { it.copy(passwordPrompt = null) }
                finish(JobKind.Recognize, outcome)
            }
        }
    }

    /** Minutes a scan takes on this phone, when more than about one. */
    fun scanEstimate(): Int? = if (findsNames) session.scanEstimate() else null

    fun findNames() = viewModelScope.launch {
        mutable.update { it.copy(needsSetup = null) }
        val outcome = jobs.run(JobKind.Scan, jobId) { session.scan(it) }
        finish(JobKind.Scan, outcome)
        // A scan that finds nothing says so, rather than going quietly to Share.
        if (outcome is JobOutcome.Finished && outcome.value.candidates.isEmpty()) {
            mutable.update { it.copy(notice = io.github.lawkitt.mdoc.R.string.no_names_found) }
        }
    }

    fun cancelJob() = jobs.cancel(CancelReason.User)

    private fun finish(kind: JobKind, outcome: JobOutcome<DocumentSnapshot>) {
        if (kind == JobKind.Scan) findsNames = detectorTier()
        when (outcome) {
            is JobOutcome.Finished -> mutable.update { it.copy(snapshot = outcome.value) }
            is JobOutcome.Failed -> when (outcome.error) {
                is AppError.ModelMissing -> mutable.update { it.copy(needsSetup = kind) }
                else -> show(outcome.error)
            }
            is JobOutcome.Cancelled, JobOutcome.Busy -> refresh()
        }
    }

    fun skipSetup() = mutable.update { it.copy(needsSetup = null) }

    // Review

    fun open(candidate: CandidateId) = mutable.update { it.copy(reviewing = candidate) }

    fun closeCandidate() = mutable.update { it.copy(reviewing = null) }

    fun decide(candidate: CandidateId, decision: Decision, scope: Scope, openNext: Boolean = false): Job {
        // Picked on the snapshot shown: refused if a move opened the document again since.
        val seen = mutable.value.snapshot.opened
        return change {
            val next = session.decide(candidate, decision, scope, seen)
            val following = if (openNext) nextProposal(next.candidates, candidate) else null
            mutable.update { it.copy(snapshot = next, reviewing = following) }
        }
    }

    fun replaceAll() = change { mutable.update { it.copy(snapshot = session.replaceAllPending()) } }

    fun startManualReplace(range: SourceRange, text: String) = mutable.update { it.copy(manual = range to text) }

    fun cancelManualReplace() = mutable.update { it.copy(manual = null) }

    // A selection copied by hand (#1209): the user's own copy, never refused.

    /** The selection's copy menu, from what the core finds in it; Copy with names alone when the check fails. */
    suspend fun selectionMenu(range: SourceRange): List<SelectionAction> {
        val seen = mutable.value.snapshot.opened
        val check = withContext(Dispatchers.IO) { runCatching { session.checkSelection(range, seen) }.getOrNull() }
        return SelectionAction.menu(check)
    }

    /** What to put on the clipboard for `action` on `range`, which read `shown`, and whether it is marked; null, with why shown, on failure. */
    suspend fun copySelection(range: SourceRange, shown: String, action: SelectionAction): SelectionCopy? {
        // Selected in the snapshot shown: refused if a move opened the document again since.
        val seen = mutable.value.snapshot.opened
        return withContext(Dispatchers.IO) {
            runCatching { session.copySelection(range, shown, action, seen) }
                .onSuccess { copied -> copied.snapshot?.let { next -> mutable.update { it.copy(snapshot = next) } } }
                .onFailure(::show)
                .getOrNull()
        }
    }

    fun replaceSelection(category: Category) {
        val (range, _) = mutable.value.manual ?: return
        // Selected in the snapshot shown: refused if a move opened the document again since.
        val seen = mutable.value.snapshot.opened
        change { mutable.update { it.copy(snapshot = session.addReplacement(range, category, seen), manual = null) } }
    }

    fun showIdentities(show: Boolean) = mutable.update { it.copy(showsIdentities = show) }

    // Hand-off

    /** Copy for AI's text; null when refused, and the refusal's dialog shows with its step. */
    suspend fun handOff(): HandOff? = withContext(Dispatchers.IO) {
        runCatching { session.copyForAi() }.onFailure(::refused).getOrNull()
    }

    // Refusals in plain words (#1209 decisions 13 and 14)

    /**
     * Why Copy for AI and Share wait now, before a tap; null when they go.
     * The bar and the menu read this one answer, so they agree, below the
     * floor too.
     */
    fun handOffRefusal(state: DocumentState): HandOffRefusal? = HandOffRefusal.before(state.snapshot, findsNames, state.saved)

    /** The step that resolves `refusal` on this phone. */
    fun step(refusal: HandOffRefusal): HandOffRefusal.Step? = refusal.step(findsNames)

    /** Readiness checks started, the latest last; before [init], which starts one at once. */
    private val checks = AtomicInteger()

    init {
        // Each new snapshot (a save, a scan, a move, a refresh): what the
        // Vault says of it now. A name another document added to the
        // Project, or a Keep of one it knows, shows here before a tap.
        viewModelScope.launch {
            state.map { it.snapshot }.distinctUntilChanged().collectLatest { snapshot ->
                mutable.update { if (it.snapshot == snapshot) it.copy(saved = null) else it }
                checkSaved(snapshot)
            }
        }
    }

    /**
     * Asks the Vault, off the main thread, about `snapshot`; kept while it
     * is still the one shown, and only from the latest check: one started
     * earlier and answered later says nothing of the Project as it is now.
     */
    private suspend fun checkSaved(snapshot: DocumentSnapshot = mutable.value.snapshot) {
        val check = checks.incrementAndGet()
        val saved = withContext(Dispatchers.IO) { session.savedReadiness() }
        mutable.update { if (it.snapshot == snapshot && check == checks.get()) it.copy(saved = saved) else it }
    }

    /** A hand-off that failed: a refusal shows with its step; anything else as before. */
    private fun refused(error: Throwable) {
        if (error !is AppError.HandOffRefused) return show(error)
        mutable.update { it.copy(refusal = error.refusal) }
        // What the tap found, the bar and the menu show too.
        viewModelScope.launch { checkSaved() }
    }

    fun dismissRefusal() = mutable.update { it.copy(refusal = null) }

    /**
     * Replace N and copy: the pending proposals replaced as the bar's
     * Replace N does, then Copy for AI's text, through the same check;
     * null when either refuses, and why shows.
     */
    suspend fun replaceAndHandOff(): HandOff? {
        mutable.update { it.copy(refusal = null) }
        // A failed replace or save is a refusal in plain words too, never the core's.
        val replaced = withContext(Dispatchers.IO) { runCatching { session.replaceAllPending() } }
            .onFailure { refused(AppError.HandOffRefused(HandOffRefusal.Failed)) }.getOrNull() ?: return null
        mutable.update { it.copy(snapshot = replaced) }
        return handOff()
    }

    /** A hand-off tapped: kept here, so the screen made again still knows it. */
    fun tapped(kind: HandOffKind) = mutable.update { it.copy(tapped = kind) }

    /**
     * "I checked the names myself" (item 48): said of the text the
     * question was asked of, and saved; the hand-off tapped, which then
     * goes on, or null. Text changed since asks again; a save that fails
     * is a refusal in plain words.
     */
    suspend fun confirmNamesChecked(): HandOffKind? {
        mutable.update { it.copy(refusal = null) }
        val checked = withContext(Dispatchers.IO) { runCatching { session.confirmNamesChecked() } }
            .onFailure { refused(it as? AppError.HandOffRefused ?: AppError.HandOffRefused(HandOffRefusal.Failed)) }
            .getOrNull() ?: return null
        mutable.update { it.copy(snapshot = checked) }
        return mutable.value.tapped
    }

    /**
     * Review: the next proposal's sheet; else a kept mention's, a known
     * name written out, which Replace resolves; else the document opened
     * again from the Vault, so a name another document of its Project
     * added since is proposed, and its sheet opens; else its names.
     */
    fun review() {
        mutable.update { it.copy(refusal = null) }
        val snapshot = mutable.value.snapshot
        val open = firstProposal(snapshot) ?: snapshot.candidates.filter { it.state == MentionState.Kept }.minByOrNull { it.range.start }?.id
        if (open != null) return open(open)
        viewModelScope.launch(Dispatchers.IO) {
            runCatching {
                val lined = session.lineUpWithProject()
                val first = firstProposal(lined)
                mutable.update { it.copy(snapshot = lined, reviewing = first, showsIdentities = first == null && lined.identities.isNotEmpty()) }
            }.onFailure { refused(it as? AppError.HandOffRefused ?: AppError.HandOffRefused(HandOffRefusal.Failed)) }
        }
    }

    fun noticeShown() = mutable.update { it.copy(notice = null) }

    fun notice(text: Int) = mutable.update { it.copy(notice = text) }

    fun showReplySheet(show: Boolean) = mutable.update { it.copy(showsReplySheet = show) }

    fun restoreReply(reply: String) = change {
        val restored = session.restoreReply(reply)
        // The answer Ask AI waited for is in, however it came; a restore
        // that fails keeps the banner.
        mutable.update { it.copy(snapshot = restored, showsReplySheet = false, showsReply = true) }
        viewModelScope.launch { offer { null } }
    }

    // Ask AI (#1164 decision 11, Milestone 2 item 20)

    /**
     * Ask AI: Copy for AI's hand-off, as Share as text sends it, with the
     * wait for the answer armed; null, with why shown, until Ready, and
     * then nothing waits.
     */
    suspend fun askAi(): HandOff? = handOff()?.also { offer { AskAiWait.asked() } }

    /** Ask AI's chooser started an app; `inFront`: mdoc is resumed. */
    fun askAiChosen(inFront: Boolean) = offer { AskAiWait.chosen(it, inFront) }

    /** mdoc is in front again: back from the AI app, the banner shows. */
    fun askAiReturned() = offer(AskAiWait::resumed)

    /**
     * Ask AI's wait moves on, and is kept for a restart. When the banner
     * comes up, an open candidate's sheet or Replace… gives way to it: the
     * user is back for the reply.
     */
    private fun offer(next: (ReplyOffer?) -> ReplyOffer?) {
        mutable.update { state ->
            val offer = next(state.replyOffer)
            val shown = offer == ReplyOffer.Offered && state.replyOffer != ReplyOffer.Offered
            if (shown) state.copy(replyOffer = offer, reviewing = null, manual = null) else state.copy(replyOffer = offer)
        }
        savedState[REPLY_OFFER] = mutable.value.replyOffer?.name
    }

    /** × on the banner, or the preview turned off: nothing waits; Paste in the bar stays. */
    fun dismissReplyBanner() = offer { null }

    fun showReply(show: Boolean) = mutable.update { it.copy(showsReply = show) }

    /** Remove reply: the one shown, by its id, never a newer one saved meanwhile. */
    fun clearReply(shown: String) = change { mutable.update { it.copy(snapshot = session.clearReply(shown), showsReply = false) } }

    // Notes (#1091 Decision 3)

    /**
     * One notes call at a time, in the order typed. Its scope outlives the
     * view model, so what was typed is saved when the document closes.
     */
    private val notesCalls = Dispatchers.IO.limitedParallelism(1)
    private val notesScope = CoroutineScope(SupervisorJob() + notesCalls)
    private var notesSave: Job? = null

    fun showNotes() = mutable.update { it.copy(showsNotes = true) }

    /** The document's notes as the core holds them, once what was typed before is in; null when they can't be read. */
    suspend fun notes(): String? = withContext(notesCalls) { runCatching { session.notes() }.onFailure(::show).getOrNull() }

    /** Typed in Notes: the core holds it at once, and it is saved once typing pauses. */
    fun setNotes(notes: String) {
        notesScope.launch { runCatching { session.setNotes(notes) }.onFailure(::show) }
        notesSave?.cancel()
        notesSave = notesScope.launch {
            // timing: typing in Notes pauses this long before its save, as iOS's (ADR 0104, #1091)
            delay(NotesPause.millis)
            saveNotesNow()
        }
    }

    /** Notes closed: what was typed is saved now, not after the pause. */
    fun closeNotes() {
        mutable.update { it.copy(showsNotes = false) }
        notesSave?.cancel()
        notesScope.launch { saveNotesNow() }
    }

    private fun saveNotesNow() {
        runCatching { session.saveNotes() }.onFailure(::show)
    }

    // Promote to Document (#1091 Decision 3)

    /**
     * Promote to Document of saved reply `reply`: a new document in the
     * reply's place, which `onOpen` opens; the reply stays. After the notes
     * typed here are in, as Promote saves what is pending first.
     */
    fun promote(reply: String, onOpen: (DocumentId) -> Unit) {
        // A second tap while one runs makes no second document.
        if (promoting) return
        promoting = true
        mutable.update { it.copy(fromReply = null) }
        viewModelScope.launch {
            withContext(notesCalls) { runCatching { documents.promoteReply(reply) } }
                .onSuccess(onOpen)
                .onFailure(::show)
            promoting = false
        }
    }

    /** A Promote to Document is running; on the main thread. */
    private var promoting = false

    /** From AI reply: the reply this document was promoted from, restored as it restores now. */
    fun showFromReply() {
        val from = (mutable.value.snapshot.promotedFrom as? PromotedFrom.Reply)?.reply ?: return
        mutable.update { it.copy(fromReply = ShownReply(from)) }
        viewModelScope.launch {
            val restored = withContext(Dispatchers.IO) { runCatching { documents.restoreSavedReply(from.id) }.getOrNull() }
            mutable.update { if (it.fromReply?.reply != from) it else it.copy(fromReply = ShownReply(from, restored, restored == null)) }
        }
    }

    fun hideFromReply() = mutable.update { it.copy(fromReply = null) }

    fun dismissError() = mutable.update { it.copy(error = null) }

    fun refresh() = viewModelScope.launch(Dispatchers.IO) {
        runCatching { session.snapshot() }.onSuccess { snapshot -> mutable.update { it.copy(snapshot = snapshot) } }
    }

    /**
     * Back on screen: a reply Restore with mdoc saved meanwhile comes in,
     * whether or not Home's refresh took it in first.
     */
    fun resumed() = viewModelScope.launch(Dispatchers.IO) {
        documents.refresh()
        findsNames = detectorTier()
        runCatching { session.snapshot() }.onSuccess { snapshot -> mutable.update { it.copy(snapshot = snapshot) } }
        // Another document may have changed the Project's names meanwhile.
        checkSaved()
    }

    /** The document closed: its decrypted original goes (#1091 Decision 24). */
    override fun onCleared() {
        session.closeOriginal()
        // Notes typed in the last pause are saved as the document closes.
        notesSave?.cancel()
        notesScope.launch { runCatching { session.saveNotes() } }
    }

    fun delete(onDeleted: () -> Unit) = viewModelScope.launch {
        // A job on this document stops first, so it cannot save it again.
        if (isMine(jobs.current.value?.documentId)) {
            jobs.cancel(CancelReason.User)
            jobs.current.first { it == null }
        }
        withContext(Dispatchers.IO) { runCatching { documents.delete(session.id) } }
            .onSuccess { onDeleted() }
            .onFailure(::show)
    }

    private fun change(work: () -> Unit) = viewModelScope.launch(Dispatchers.IO) {
        runCatching(work).onFailure(::show)
    }

    private fun show(error: Throwable) {
        mutable.update { it.copy(error = error.message ?: error.toString()) }
    }

    private companion object {
        /** Ask AI's wait in the saved state: a [ReplyOffer]'s name. */
        const val REPLY_OFFER = "replyOffer"

        /** The first proposal in reading order. */
        fun firstProposal(snapshot: DocumentSnapshot): CandidateId? =
            snapshot.candidates.filter { it.state == MentionState.Proposed }.minByOrNull { it.range.start }?.id

        /** The next proposal after `current` in reading order, wrapping round. */
        fun nextProposal(candidates: List<Candidate>, current: CandidateId): CandidateId? {
            val proposals = candidates.filter { it.state == MentionState.Proposed }.sortedBy { it.range.start }
            if (proposals.isEmpty()) return null
            val at = candidates.firstOrNull { it.id == current }?.range?.start ?: 0
            return (proposals.firstOrNull { it.range.start > at } ?: proposals.first()).id
        }
    }
}

/** How long typing in Notes pauses before its save: iOS's typing pause. The tests lengthen it. */
internal object NotesPause {
    @Volatile var millis: Long = 1000L
}
