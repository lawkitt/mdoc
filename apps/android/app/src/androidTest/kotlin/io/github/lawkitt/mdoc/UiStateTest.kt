package io.github.lawkitt.mdoc

import android.os.Looper
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.ui.UiState
import io.github.lawkitt.mdoc.ui.onMain
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * #1332: the state a screen collects is written on the main thread only. A
 * write on a worker thread recomposed the document screen there under
 * Compose's test dispatcher: a dialog built without a Looper
 * (RefusalsTest) and a corrupted slot table (RestoredSelectionTest). In a
 * debug build, the build every instrumented test runs, such a write now
 * fails where it is made.
 */
@RunWith(AndroidJUnit4::class)
class UiStateTest {
    @Test
    fun aWriteOffTheMainThreadFailsWhereItIsMade() = runBlocking {
        val state = runOnMain { UiState(0) }
        runOnMain { state.update { it + 1 } }
        assertEquals(1, state.value)

        val refused = withContext(Dispatchers.IO) { runCatching { state.update { it + 1 } }.exceptionOrNull() }
        assertTrue("$refused", refused is IllegalStateException && refused.message.orEmpty().contains("#1332"))
        val set = withContext(Dispatchers.IO) { runCatching { state.value = 5 }.exceptionOrNull() }
        assertTrue("$set", set is IllegalStateException)
        assertEquals("nothing written off the main thread", 1, state.value)
    }

    @Test
    fun onMainBringsAWorkerThreadsWriteToTheMainThread() = runBlocking {
        val state = runOnMain { UiState(0) }
        val written = CountDownLatch(1)
        var thread: Thread? = null
        withContext(Dispatchers.IO) {
            onMain {
                thread = Thread.currentThread()
                state.update { it + 1 }
                written.countDown()
            }
        }
        assertTrue(written.await(10, TimeUnit.SECONDS))
        assertEquals(Looper.getMainLooper().thread, thread)
        assertEquals(1, state.value)
    }

    private fun <T> runOnMain(body: () -> T): T {
        var result: T? = null
        InstrumentationRegistry.getInstrumentation().runOnMainSync { result = body() }
        @Suppress("UNCHECKED_CAST")
        return result as T
    }
}
