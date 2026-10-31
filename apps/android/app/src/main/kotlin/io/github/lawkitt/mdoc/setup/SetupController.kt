package io.github.lawkitt.mdoc.setup

import android.content.Context
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.AppError
import io.github.lawkitt.mdoc.core.Models
import io.github.lawkitt.mdoc.core.SetupSnapshot
import io.github.lawkitt.mdoc.jobs.JobKind
import io.github.lawkitt.mdoc.jobs.JobOutcome
import io.github.lawkitt.mdoc.jobs.Jobs
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import uniffi.mdoc_ffi.ModelId

data class SetupState(
    val snapshot: SetupSnapshot,
    /** Download progress by model, 0 to 1, while it runs. */
    val progress: Map<ModelId, Float> = emptyMap(),
    /** Downloading, but held: no Wi-Fi, and cellular not allowed (ADR 0088). */
    val waitingForWiFi: Boolean = false,
    /** Checking and installing the last file. */
    val installing: Boolean = false,
    /** "Download over cellular?" is showing. */
    val asksCellular: Boolean = false,
    val working: Boolean = false,
    val dismissed: Boolean = false,
    val error: String? = null,
) {
    /** The bytes still to download for what this phone is offered. */
    val missingBytes: Long get() = snapshot.offers.filter { !it.installed }.sumOf { it.bytes }
    val needsSetup: Boolean get() = snapshot.offers.any { !it.installed }

    /** Home's card: the detector tier, something missing, not dismissed (ADR 0088). */
    val showsCard: Boolean get() = snapshot.tier == io.github.lawkitt.mdoc.core.DeviceTier.Detector && needsSetup && !dismissed
}

/**
 * Model setup for the whole app (ADR 0085, 0088, 0089): downloads each
 * pinned file in Kotlin as a long job, Wi-Fi only unless the user says yes
 * to cellular, and hands it to Rust to verify and install. A job that asked
 * for setup runs again once the models are in.
 */
class SetupController(
    private val context: Context,
    private val models: () -> Models,
    private val storage: () -> MdocStorage,
    private val jobs: Jobs,
    private val network: NetworkWatch,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private val prefs = context.getSharedPreferences("setup", Context.MODE_PRIVATE)
    private val mutable = MutableStateFlow(SetupState(models().snapshot(), dismissed = prefs.getBoolean(DISMISSED, false)))
    val state: StateFlow<SetupState> = mutable.asStateFlow()

    @Volatile private var allowsCellular = false

    @Volatile private var cancelled = false
    private var continuation: (() -> Unit)? = null

    fun refresh() = mutable.update { it.copy(snapshot = models().snapshot()) }

    fun dismissCard() {
        prefs.edit().putBoolean(DISMISSED, true).apply()
        mutable.update { it.copy(dismissed = true) }
    }

    /** Download what is missing; on cellular, ask first (ADR 0088). Then run `then`. */
    fun download(then: (() -> Unit)? = null) {
        if (mutable.value.working) return
        continuation = then
        if (network.route.value == Route.Cellular) {
            mutable.update { it.copy(asksCellular = true) }
            return
        }
        begin(allowCellular = false)
    }

    /** "Use cellular" on a download held for Wi-Fi asks the same question. */
    fun askCellular() = mutable.update { it.copy(asksCellular = true) }

    fun answerCellular(useCellular: Boolean) {
        mutable.update { it.copy(asksCellular = false) }
        if (!mutable.value.working) {
            begin(useCellular)
        } else if (useCellular) {
            // The held download goes on over cellular at its next attempt.
            allowsCellular = true
        }
    }

    fun cancel() {
        continuation = null
        cancelled = true
        jobs.cancel()
    }

    private fun begin(allowCellular: Boolean) {
        allowsCellular = allowCellular
        cancelled = false
        mutable.update { it.copy(working = true, error = null) }
        scope.launch {
            val outcome = jobs.run(JobKind.Install, null, context.getString(R.string.downloading_models)) { observer ->
                observer.onCancel { cancelled = true }
                install()
            }
            val next = continuation
            continuation = null
            mutable.update {
                it.copy(
                    snapshot = models().snapshot(), working = false, progress = emptyMap(), waitingForWiFi = false,
                    installing = false, error = (outcome as? JobOutcome.Failed)?.error?.message,
                )
            }
            if (outcome is JobOutcome.Finished && !mutable.value.needsSetup) next?.invoke()
        }
    }

    private fun install() {
        val models = models()
        val downloader = ModelDownloader(
            network = { network.wifi ?: if (allowsCellular) network.cellular else null },
            cancelled = { cancelled },
            waiting = { held -> mutable.update { it.copy(waitingForWiFi = held) } },
        )
        for (offer in models.snapshot().offers.filter { !it.installed }) {
            val files = models.missingFiles(offer.id)
            val total = files.sumOf { it.bytes.toLong() }.coerceAtLeast(1)
            var done = 0L
            mutable.update { it.copy(progress = it.progress + (offer.id to 0f)) }
            for (file in files) {
                val part = File(storage().dir(MdocStorage.Area.Downloads), "${offer.id.name}/${file.path}.part")
                downloader.download(file.url, part) { bytes ->
                    val fraction = (done + bytes).toFloat() / total
                    mutable.update { it.copy(progress = it.progress + (offer.id to fraction)) }
                }
                if (cancelled) throw AppError.Cancelled()
                mutable.update { it.copy(installing = true) }
                models.install(offer.id, file, part)
                part.delete()
                mutable.update { it.copy(installing = false) }
                done += file.bytes.toLong()
            }
        }
    }

    private companion object {
        const val DISMISSED = "cardDismissed"
    }
}
