package io.github.lawkitt.mdoc

import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.jobs.JobKind
import java.util.concurrent.CountDownLatch
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** One job at a time (ADR 0006): a download asked for while another job runs says so, as on iOS. */
@RunWith(AndroidJUnit4::class)
class JobSlotTest {
    @get:Rule
    val core = TestCore()

    @Test
    fun aDownloadWhileAnotherJobRunsSaysSo() = runBlocking<Unit> {
        val app = core.app
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        ActivityScenario.launch(MainActivity::class.java).use {
            // On screen: a job's foreground service starts only from a visible app.
            val release = CountDownLatch(1)
            val started = CountDownLatch(1)
            val other = app.jobs.start(JobKind.Recognize, null, "Another document") {
                started.countDown()
                release.await()
            }!!
            started.await()
            app.setup.refresh()
            instrumentation.runOnMainSync { app.setup.download() }
            val state = withTimeout(10_000) { app.setup.state.first { it.error != null } }
            assertEquals("Another job is running. Try again when it finishes.", state.error)
            release.countDown()
            other.await()
        }
    }
}
