package io.github.lawkitt.mdoc.document

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
import io.github.lawkitt.mdoc.core.MentionState
import io.github.lawkitt.mdoc.core.Scope
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.jobs.CancelReason
import io.github.lawkitt.mdoc.jobs.JobKind
import io.github.lawkitt.mdoc.jobs.JobOutcome
import io.github.lawkitt.mdoc.jobs.Jobs
import io.github.lawkitt.mdoc.jobs.RunningJob
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The header's one primary action, following the document's state. */
sealed interface PrimaryAction {
    data class Recognize(val resume: Boolean) : PrimaryAction

    /** A file that needs no recognition stopped importing before its text was read (ADR 0088). */
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
)

/**
 * One document screen: the core's latest snapshot and view state only;
 * every change goes through the session and comes back as a new snapshot.
 */
class DocumentViewModel(
    private val documents: Documents,
    private val session: Documents.Session,
    private val jobs: Jobs,
    /** Above the detector floor; below it there is no Find names (ADR 0088). */
    val findsNames: Boolean,
) : ViewModel() {
    private val mutable = MutableStateFlow(DocumentState(session.snapshot()))
    val state: StateFlow<DocumentState> = mutable.asStateFlow()

    val running: StateFlow<RunningJob?> = jobs.current
    val interrupted = jobs.interrupted

    /**
     * The id a job of this document carries: its saved path once it has one,
     * which is the id it has after mdoc restarts, so an interrupted job still
     * finds its document.
     */
    private val jobId get() = session.path?.let(::DocumentId) ?: session.id

    /** Whether a job or interruption belongs to this document. */
    fun isMine(id: DocumentId?): Boolean = id != null && (id == session.id || id.raw == session.path)

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

    fun recognize(title: String) = viewModelScope.launch {
        mutable.update { it.copy(needsSetup = null) }
        finish(JobKind.Recognize, jobs.run(JobKind.Recognize, jobId, title) { session.recognize(it) })
    }

    fun findNames(title: String) = viewModelScope.launch {
        mutable.update { it.copy(needsSetup = null) }
        val outcome = jobs.run(JobKind.Scan, jobId, title) { session.scan(it) }
        finish(JobKind.Scan, outcome)
        // A scan that finds nothing says so, rather than going quietly to Share (ADR 0087).
        if (outcome is JobOutcome.Finished && outcome.value.candidates.isEmpty()) {
            mutable.update { it.copy(notice = io.github.lawkitt.mdoc.R.string.no_names_found) }
        }
    }

    fun cancelJob() = jobs.cancel(CancelReason.User)

    private fun finish(kind: JobKind, outcome: JobOutcome<DocumentSnapshot>) {
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

    fun decide(candidate: CandidateId, decision: Decision, scope: Scope, openNext: Boolean = false) = change {
        val next = session.decide(candidate, decision, scope)
        val following = if (openNext) nextProposal(next.candidates, candidate) else null
        mutable.update { it.copy(snapshot = next, reviewing = following) }
    }

    fun replaceAll() = change { mutable.update { it.copy(snapshot = session.replaceAllPending()) } }

    fun startManualReplace(range: SourceRange, text: String) = mutable.update { it.copy(manual = range to text) }

    fun cancelManualReplace() = mutable.update { it.copy(manual = null) }

    fun replaceSelection(category: Category) {
        val (range, _) = mutable.value.manual ?: return
        change { mutable.update { it.copy(snapshot = session.addReplacement(range, category), manual = null) } }
    }

    fun showIdentities(show: Boolean) = mutable.update { it.copy(showsIdentities = show) }

    // Hand-off

    /** Copy for AI's text, or why it is not ready. */
    suspend fun handOff(): HandOff? = withContext(Dispatchers.IO) {
        runCatching { session.copyForAi() }.onFailure(::show).getOrNull()
    }

    fun noticeShown() = mutable.update { it.copy(notice = null) }

    fun notice(text: Int) = mutable.update { it.copy(notice = text) }

    fun showReplySheet(show: Boolean) = mutable.update { it.copy(showsReplySheet = show) }

    fun restoreReply(reply: String) = change {
        val restored = session.restoreReply(reply)
        mutable.update { it.copy(snapshot = restored, showsReplySheet = false, showsReply = true) }
    }

    fun showReply(show: Boolean) = mutable.update { it.copy(showsReply = show) }

    fun clearReply() = change { mutable.update { it.copy(snapshot = session.clearReply(), showsReply = false) } }

    fun dismissError() = mutable.update { it.copy(error = null) }

    fun refresh() = viewModelScope.launch(Dispatchers.IO) {
        runCatching { session.snapshot() }.onSuccess { snapshot -> mutable.update { it.copy(snapshot = snapshot) } }
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
        /** The next proposal after `current` in reading order, wrapping round. */
        fun nextProposal(candidates: List<Candidate>, current: CandidateId): CandidateId? {
            val proposals = candidates.filter { it.state == MentionState.Proposed }.sortedBy { it.range.start }
            if (proposals.isEmpty()) return null
            val at = candidates.firstOrNull { it.id == current }?.range?.start ?: 0
            return (proposals.firstOrNull { it.range.start > at } ?: proposals.first()).id
        }
    }
}
