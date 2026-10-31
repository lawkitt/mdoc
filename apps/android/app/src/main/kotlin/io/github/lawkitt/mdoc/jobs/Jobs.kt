package io.github.lawkitt.mdoc.jobs

import android.content.Context
import androidx.core.content.ContextCompat
import io.github.lawkitt.mdoc.core.AppError
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.JobObserver
import java.io.File
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import org.json.JSONObject
import uniffi.mdoc_ffi.JobStage

enum class JobKind { Recognize, Scan, Install }

enum class CancelReason {
    User,

    /** Android ended the foreground service (its time limit) or the process. */
    Background,
}

data class RunningJob(val kind: JobKind, val documentId: DocumentId?, val title: String, val stage: JobStage? = null)

/** A job that stopped when mdoc left the screen or was ended: the document offers to resume. */
data class InterruptedJob(val kind: JobKind, val documentId: DocumentId?)

sealed interface JobOutcome<out T> {
    data class Finished<T>(val value: T) : JobOutcome<T>

    data class Cancelled(val reason: CancelReason) : JobOutcome<Nothing>

    data class Failed(val error: Throwable) : JobOutcome<Nothing>

    /** Another job holds the one slot (ADR 0006). */
    data object Busy : JobOutcome<Nothing>
}

/**
 * One model job at a time (ADR 0006), off the main thread, with progress and
 * cooperative cancellation, kept alive by a `dataSync` foreground service
 * with a notification while it runs (ADR 0089). A job starts only from a
 * visible screen, as Android requires of a foreground service; one the
 * system ends, or that dies with the process, is offered again on return.
 */
class Jobs(private val context: Context) {
    private val running = MutableStateFlow<RunningJob?>(null)
    val current: StateFlow<RunningJob?> = running.asStateFlow()

    private val stopped = MutableStateFlow(loadInterrupted())
    val interrupted: StateFlow<InterruptedJob?> = stopped.asStateFlow()

    private val token = AtomicReference<Token?>(null)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /**
     * Runs `work` as the one long job. The job belongs to the app, not the
     * caller: a screen that closes stops waiting, but the job finishes, stops
     * its service and frees the slot.
     */
    suspend fun <T> run(kind: JobKind, documentId: DocumentId?, title: String, work: (JobObserver) -> T): JobOutcome<T> =
        start(kind, documentId, title, work)?.await() ?: JobOutcome.Busy

    fun <T> start(kind: JobKind, documentId: DocumentId?, title: String, work: (JobObserver) -> T): Deferred<JobOutcome<T>>? {
        val job = RunningJob(kind, documentId, title)
        if (!running.compareAndSet(null, job)) return null
        val token = Token { stage -> running.value = running.value?.copy(stage = stage) }
        this.token.set(token)
        if (stopped.value?.documentId == documentId) clearInterrupted()
        return scope.async {
            try {
                // A foreground service starts only while mdoc is on screen
                // (ADR 0089); refused, the job does not run.
                ContextCompat.startForegroundService(context, LongJobService.start(context, title))
                // Written first: a process Android ends mid-job leaves it behind.
                saveInterrupted(InterruptedJob(kind, documentId))
                val result = withContext(Dispatchers.IO) { runCatching { work(token) } }
                val reason = token.reason
                when {
                    result.isSuccess -> {
                        clearInterrupted()
                        JobOutcome.Finished(result.getOrThrow())
                    }
                    reason != null -> {
                        if (reason == CancelReason.Background) stopped.value = InterruptedJob(kind, documentId) else clearInterrupted()
                        JobOutcome.Cancelled(reason)
                    }
                    result.exceptionOrNull() is AppError.Cancelled -> {
                        clearInterrupted()
                        JobOutcome.Cancelled(CancelReason.User)
                    }
                    else -> {
                        clearInterrupted()
                        JobOutcome.Failed(result.exceptionOrNull()!!)
                    }
                }
            } catch (refused: IllegalStateException) {
                JobOutcome.Failed(refused)
            } finally {
                context.stopService(LongJobService.intent(context))
                this@Jobs.token.set(null)
                running.value = null
            }
        }
    }

    /** Cancel from the screen or the notification. */
    fun cancel(reason: CancelReason = CancelReason.User) {
        token.get()?.cancel(reason)
    }

    fun clearInterrupted() {
        stopped.value = null
        marker.delete()
    }

    private val marker get() = File(context.noBackupFilesDir, "interrupted-job.json")

    private fun saveInterrupted(job: InterruptedJob) {
        marker.writeText(JSONObject().put("kind", job.kind.name).put("document", job.documentId?.raw).toString())
    }

    private fun loadInterrupted(): InterruptedJob? = runCatching {
        val item = JSONObject(marker.readText())
        InterruptedJob(JobKind.valueOf(item.getString("kind")), item.optString("document").takeIf { it.isNotEmpty() }?.let(::DocumentId))
    }.getOrNull()

    private class Token(private val onStage: (JobStage) -> Unit) : JobObserver {
        @Volatile var reason: CancelReason? = null
            private set
        private val handlers = mutableListOf<() -> Unit>()

        override fun stage(stage: JobStage) = onStage(stage)

        override fun onCancel(handler: () -> Unit) {
            val now = synchronized(this) {
                if (reason == null) {
                    handlers += handler
                    false
                } else {
                    true
                }
            }
            if (now) handler()
        }

        fun cancel(reason: CancelReason) {
            val run = synchronized(this) {
                if (this.reason != null) return
                this.reason = reason
                handlers.toList().also { handlers.clear() }
            }
            run.forEach { it() }
        }
    }
}
