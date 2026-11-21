package io.github.lawkitt.mdoc.core

import java.io.File

/**
 * A camera scan in progress: its clean pages, in a `scanning-` stage under
 * `originals/` ([Originals.newScanStage]), and the raw shot the camera is
 * writing ([CameraCapture]). It belongs to the app, not to Home's
 * ViewModel, so a destroyed activity keeps it; Home also saves it in its
 * SavedStateHandle, so a process stopped while the camera is open gets it
 * back. Used on the main thread.
 */
class ScanSession(
    /** The camera's folder, swept once a process starts; null in tests that take no photos. */
    private val captureFolder: File? = null,
) {
    /** The clean pages so far, in order. */
    var pages: List<File> = emptyList()

    /** The raw shot the camera is writing, until Home reads it. */
    var capture: File? = null

    /** A shot is being cleaned. */
    var processing = false

    private var swept = false

    /** Deletes the shots no scan waits for, once a process. */
    fun sweep() {
        if (swept) return
        swept = true
        captureFolder?.let { CameraCapture.sweep(it, capture) }
    }
}
