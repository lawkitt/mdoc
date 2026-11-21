package io.github.lawkitt.mdoc.core

import java.io.File
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * A camera scan in progress: its clean pages, in a `scanning-` stage under
 * `originals/` ([Originals.newScanStage]), and the raw shot the camera is
 * writing ([CameraCapture]). It belongs to the app, not to Home's
 * ViewModel, and so does the job that cleans a returned shot: a destroyed
 * activity loses neither. Home saves its [state] in its SavedStateHandle,
 * so a process stopped while the camera is open, or while a shot is being
 * cleaned, gets it back; a shot's raw file stays until that saved state
 * holds its clean page ([saved]). Used on the main thread.
 */
class ScanSession(
    /** The camera's folder, swept once a process starts; null in tests that take no photos. */
    private val captureFolder: File? = null,
    private val scope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
) {
    data class State(
        /** The clean pages so far, in order. */
        val pages: List<File> = emptyList(),
        /** The raw shot the camera is writing, or that is being cleaned. */
        val capture: File? = null,
        /** The camera returned with [capture]: it is being cleaned. */
        val returned: Boolean = false,
        /** Why the last shot was not kept, until Home shows it. */
        val refused: String? = null,
    )

    private val mutable = MutableStateFlow(State())
    val state: StateFlow<State> = mutable.asStateFlow()

    /** Raw shots whose clean page is in [state], deleted once it is saved. */
    private val spent = mutableListOf<File>()
    private var swept = false

    /**
     * A scan a stopped process left in a saved state, when this process has
     * none: a shot the camera had returned is cleaned again.
     */
    fun restore(saved: State, originals: () -> Originals) {
        if (mutable.value.pages.isNotEmpty() || mutable.value.capture != null) return
        val pages = saved.pages.filter { it.isFile }
        val capture = saved.capture?.takeIf { it.isFile }
        mutable.value = State(pages, capture, returned = false)
        if (capture != null && saved.returned) clean(true, originals)
    }

    /** Deletes the shots no scan waits for, once a process. */
    fun sweep() {
        if (swept) return
        swept = true
        captureFolder?.let { CameraCapture.sweep(it, mutable.value.capture) }
    }

    /** The camera is about to write `capture`. */
    fun begin(capture: File) = mutable.update { it.copy(capture = capture, returned = false) }

    /**
     * The camera returned: a `taken` shot is cleaned into the scan's stage.
     * The job is the session's, so it ends whatever happens to Home.
     */
    fun clean(taken: Boolean, originals: () -> Originals) {
        val current = mutable.value
        // Nothing to clean, or already being cleaned.
        val raw = current.capture?.takeIf { !current.returned } ?: return
        if (!taken) {
            mutable.update { it.copy(capture = null, returned = false) }
            raw.delete()
            return
        }
        mutable.update { it.copy(returned = true) }
        scope.launch {
            val previous = mutable.value.pages
            var stage: File? = null
            val result = try {
                Result.success(
                    withContext(Dispatchers.IO) {
                        val size = raw.length()
                        if (size <= 0) return@withContext null
                        check(size <= MAX_IMAGE_BYTES) { "This image is too large to import." }
                        val bytes = raw.readBytes()
                        val kept = originals()
                        val folder = previous.firstOrNull()?.parentFile ?: kept.newScanStage().also { stage = it }
                        kept.stageImage(File(folder, "page-${previous.size + 1}.jpg"), bytes)
                    },
                )
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                Result.failure(error)
            }
            val page = result.getOrNull()
            if (page == null) {
                stage?.deleteRecursively()
                raw.delete()
            } else {
                spent += raw
            }
            mutable.update {
                it.copy(
                    pages = if (page != null) previous + page else previous,
                    capture = null,
                    returned = false,
                    refused = result.exceptionOrNull()?.let { error -> error.message ?: error.toString() } ?: it.refused,
                )
            }
        }
    }

    /** Home saved [state]: the raw shots whose pages it holds can go. */
    fun saved() {
        spent.forEach(File::delete)
        spent.clear()
    }

    fun refusalShown() = mutable.update { it.copy(refused = null) }

    /** The finished scan's pages, the session empty again. */
    fun finish(): List<File> {
        val pages = mutable.value.pages
        mutable.update { it.copy(pages = emptyList()) }
        return pages
    }

    private companion object {
        const val MAX_IMAGE_BYTES = 256L * 1024 * 1024
    }
}
