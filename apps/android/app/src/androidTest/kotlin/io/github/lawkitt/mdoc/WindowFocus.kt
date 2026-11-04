package io.github.lawkitt.mdoc

import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.view.inspector.WindowInspector
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.fail

/**
 * Android shows the clipboard only to the app whose window has input focus
 * (Android 10 and later). The CI emulator can lose that focus to a system
 * window, most likely a "System UI isn't responding" dialog under load or a
 * shade or keyguard: every clipboard test in the run then reads null at once
 * while the rest pass (#545). Clipboard tests call this first. It waits until
 * one of this app's windows (an activity's, or a sheet's dialog) has focus,
 * closing system dialogs and the keyguard while none does, and fails naming
 * the window that holds focus.
 */
fun awaitWindowFocus(timeoutMs: Long = 15_000) {
    val instrumentation = InstrumentationRegistry.getInstrumentation()
    val deadline = SystemClock.uptimeMillis() + timeoutMs
    var closed = false
    while (true) {
        var focused = false
        // Never waitForIdleSync here: a screen with a running animation never idles.
        instrumentation.runOnMainSync {
            focused = WindowInspector.getGlobalWindowViews().any { it.hasWindowFocus() }
        }
        if (focused) return
        if (SystemClock.uptimeMillis() > deadline) {
            val holder = shell("dumpsys window displays")
                .lineSequence()
                .map { it.trim() }
                .filter { it.startsWith("mCurrentFocus") || it.startsWith("mFocusedApp") }
                .distinct()
                .joinToString("; ")
            fail("No window of the app got focus, so it cannot read the clipboard: ${holder.ifEmpty { "no focus reported" }}")
        }
        if (!closed) {
            System.err.println("awaitWindowFocus: no window of the app has focus; closing system dialogs")
            closed = true
        }
        shell("am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS")
        shell("wm dismiss-keyguard")
        SystemClock.sleep(250)
    }
}

/** Runs a shell command as the shell user and returns its output. */
private fun shell(command: String): String {
    val pipe = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)
    return ParcelFileDescriptor.AutoCloseInputStream(pipe).use { it.readBytes().decodeToString() }
}
