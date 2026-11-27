package io.github.lawkitt.mdoc.ui

import android.os.Handler
import android.os.Looper
import io.github.lawkitt.mdoc.BuildConfig
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * State a screen collects, written on the main thread only (#1332).
 *
 * A write resumes the screen's collector where it is made when the
 * collector's dispatcher runs it in place, as Compose's test dispatcher
 * does: a write on a worker thread then recomposed the screen there, which
 * builds a dialog without a Looper or corrupts the slot table. Work runs off
 * the main thread and its result is written back on it; a callback from a
 * core or system thread goes through [onMain]. In a debug build a write off
 * the main thread fails at once ([checkMain]).
 */
class UiState<T>(initial: T) {
    private val flow = MutableStateFlow(initial)

    var value: T
        get() = flow.value
        set(next) {
            checkMain()
            flow.value = next
        }

    fun update(transform: (T) -> T) {
        checkMain()
        flow.update(transform)
    }

    fun compareAndSet(expect: T, update: T): Boolean {
        checkMain()
        return flow.compareAndSet(expect, update)
    }

    fun asStateFlow(): StateFlow<T> = flow.asStateFlow()
}

/** The main looper; null off Android (the JVM unit tests). */
private val mainLooper: Looper? = runCatching { Looper.getMainLooper() }.getOrNull()

private val mainHandler: Handler? by lazy { mainLooper?.let(::Handler) }

/** On the main thread, or off Android. */
fun onMainThread(): Boolean = mainLooper?.isCurrentThread ?: true

/**
 * Debug builds: fails when UI state is written off the main thread
 * (#1332), so a test finds the write rather than a crash in Compose later.
 */
fun checkMain() {
    if (BuildConfig.DEBUG) {
        check(onMainThread()) { "UI state written on ${Thread.currentThread().name}, not the main thread (#1332)" }
    }
}

/** Runs [write] on the main thread: at once when already there, else posted. */
fun onMain(write: () -> Unit) {
    val handler = mainHandler
    if (onMainThread() || handler == null) write() else handler.post(write)
}
