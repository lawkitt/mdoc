package io.github.lawkitt.mdoc.ui

import android.os.Handler
import android.os.Looper
import io.github.lawkitt.mdoc.BuildConfig
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withContext

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

// Asynchronous, as Dispatchers.Main's: a post made before a coroutine's
// continuation is dispatched runs before it, so a job's last progress never
// lands after the job has ended and cleared it.
private val mainHandler: Handler? by lazy { mainLooper?.let(Handler::createAsync) }

/** On the main thread, or off Android. */
fun isMainThread(): Boolean = mainLooper?.isCurrentThread ?: true

/**
 * Debug builds: fails when UI state is written off the main thread
 * (#1332), so a test finds the write rather than a crash in Compose later.
 */
fun checkMain() {
    if (BuildConfig.DEBUG) {
        check(isMainThread()) { "UI state written on ${Thread.currentThread().name}, not the main thread (#1332)" }
    }
}

/**
 * [body], which may go off the main thread, then back on it whatever the
 * caller's dispatcher: a composition's coroutine resumes after
 * `withContext(Dispatchers.IO)` on the IO thread under Compose's test
 * dispatcher, so a screen's state written after it comes back here (#1332).
 */
suspend fun <T> onMainThread(body: suspend () -> T): T = withContext(Dispatchers.Main.immediate) { body() }

/** Runs [write] on the main thread: at once when already there, else posted. */
fun onMain(write: () -> Unit) {
    val handler = mainHandler
    if (isMainThread() || handler == null) write() else handler.post(write)
}
