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
import org.json.JSONArray
import org.json.JSONObject

/**
 * A camera scan in progress: its clean pages, in a `scanning-` stage under
 * `originals/` ([Originals.newScanStage]), and the raw shot the camera is
 * writing ([CameraCapture]). It belongs to the app, not to Home's
 * ViewModel, and so does the job that cleans a returned shot: a destroyed
 * activity loses neither.
 *
 * Its state is also kept in a record of the app's own (`no_backup/scan.json`),
 * written whole before a raw shot is deleted: a stopped activity's saved
 * state is not written again while it is stopped, so a page cleaned
 * meanwhile is found only there. Home's saved state says whether a scan is
 * being restored at all; the record says where it stands. Used on the main
 * thread.
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
        /**
         * Counts every change: the record and Home's saved state each say
         * which change they hold, and a restore takes the later one.
         */
        val sequence: Long = 0,
    ) {
        val empty: Boolean get() = pages.isEmpty() && capture == null
    }

    private val mutable = MutableStateFlow(State())
    val state: StateFlow<State> = mutable.asStateFlow()

    private val record: File? = captureFolder?.let { File(it.parentFile, "scan.json") }
    private var swept = false

    /**
     * A scan a stopped process left, when this process has none: the
     * record where it stands, for a `saved` state that holds a scan; a shot
     * the camera had returned is cleaned again. Without a saved scan the
     * record is an abandoned one's, and goes.
     */
    fun restore(saved: State, originals: () -> Originals) {
        if (!mutable.value.empty) return
        if (saved.empty) {
            record?.delete()
            return
        }
        // The later of the two: a record a failed write left behind is
        // older than the saved state that followed it.
        val restored = readRecord()?.takeIf { it.sequence >= saved.sequence } ?: saved
        val pages = restored.pages.filter { it.isFile }
        val capture = restored.capture?.takeIf { it.isFile }
        mutable.value = State(pages, capture, returned = false, sequence = restored.sequence)
        commit(mutable.value)
        if (capture != null && restored.returned) clean(true, originals)
    }

    /** Deletes the shots no scan waits for, once a process. */
    fun sweep() {
        if (swept) return
        swept = true
        captureFolder?.let { folder -> CameraCapture.sweep(folder, mutable.value.capture) }
    }

    /** The camera is about to write `capture`. */
    fun begin(capture: File) = commit(mutable.value.copy(capture = capture, returned = false))

    /**
     * The camera returned: a `taken` shot is cleaned into the scan's stage.
     * The job is the session's, so it ends whatever happens to Home. The
     * clean page is recorded before the raw shot is deleted.
     */
    fun clean(taken: Boolean, originals: () -> Originals) {
        val current = mutable.value
        // Nothing to clean, or already being cleaned.
        val raw = current.capture?.takeIf { !current.returned } ?: return
        if (!taken) {
            commit(current.copy(capture = null, returned = false))
            raw.delete()
            return
        }
        commit(current.copy(returned = true))
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
            if (page == null) stage?.deleteRecursively()
            val recorded = commit(
                mutable.value.copy(
                    pages = if (page != null) previous + page else previous,
                    capture = null,
                    returned = false,
                    refused = result.exceptionOrNull()?.let { error -> error.message ?: error.toString() }
                        ?: mutable.value.refused,
                ),
            )
            if (recorded) {
                raw.delete()
            } else {
                // The raw shot stays (outside `originals/`, swept at the next
                // start) while nothing on disk says its page was kept.
                mutable.update { it.copy(refused = NOT_RECORDED) }
            }
        }
    }

    fun refusalShown() = mutable.update { it.copy(refused = null) }

    /** The finished scan's pages, the session empty again. */
    fun finish(): List<File> {
        val pages = mutable.value.pages
        commit(mutable.value.copy(pages = emptyList()))
        return pages
    }

    /** `next`, one change later, recorded and then shown; whether the record holds it. */
    private fun commit(next: State): Boolean {
        val numbered = next.copy(sequence = mutable.value.sequence.coerceAtLeast(next.sequence) + 1)
        val recorded = persist(numbered)
        mutable.value = numbered
        return recorded
    }

    private fun persist(state: State): Boolean {
        val record = record ?: return true
        if (state.empty) return !record.exists() || record.delete()
        val json = JSONObject()
            .put("pages", JSONArray().apply { state.pages.forEach { put(it.path) } })
            .put("capture", state.capture?.path)
            .put("returned", state.returned)
            .put("sequence", state.sequence)
        return runCatching {
            Originals.writeAtomically(record) {
                it.write(json.toString().toByteArray())
                true
            }
        }.isSuccess
    }

    private fun readRecord(): State? = record?.takeIf { it.isFile }?.let { file ->
        runCatching {
            val json = JSONObject(file.readText())
            val pages = json.getJSONArray("pages")
            State(
                pages = (0 until pages.length()).map { File(pages.getString(it)) },
                capture = json.optString("capture").takeIf { it.isNotEmpty() && it != "null" }?.let(::File),
                returned = json.optBoolean("returned"),
                sequence = json.optLong("sequence"),
            )
        }.getOrNull()
    }

    private companion object {
        const val MAX_IMAGE_BYTES = 256L * 1024 * 1024
        const val NOT_RECORDED = "mdoc could not save this page. Take it again."
    }
}
