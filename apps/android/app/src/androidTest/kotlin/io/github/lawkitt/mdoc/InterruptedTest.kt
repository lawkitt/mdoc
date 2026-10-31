package io.github.lawkitt.mdoc

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.SourceKind
import io.github.lawkitt.mdoc.jobs.CancelReason
import io.github.lawkitt.mdoc.jobs.JobKind
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import java.util.concurrent.CountDownLatch
import kotlinx.coroutines.runBlocking
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * A job Android ends is offered again (ADR 0089): an import not yet read,
 * whose recognition stopped off screen, says so when its document opens.
 */
@RunWith(AndroidJUnit4::class)
class InterruptedTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun anImportWhoseRecognitionStoppedSaysSo() = runBlocking {
        val app = core.app
        val folder = File(app.core.storage.dir(MdocStorage.Area.Originals), "interrupted").apply { mkdirs() }
        val pdf = File(folder, "english.pdf")
        InstrumentationRegistry.getInstrumentation().context.assets.open("english.pdf").use { input -> pdf.outputStream().use(input::copyTo) }
        val title = "Stopped ${System.nanoTime()}"
        val session = app.core.documents.importDocument(ImportSource.File(pdf, SourceKind.Pdf), title, CaseChoice.New(title))

        // A recognition that Android ends while it runs, as when it stops the service.
        val started = CountDownLatch(1)
        val job = app.jobs.start(JobKind.Recognize, session.id, title) { observer ->
            val stop = CountDownLatch(1)
            observer.onCancel { stop.countDown() }
            started.countDown()
            stop.await()
            throw io.github.lawkitt.mdoc.core.AppError.Cancelled()
        }!!
        started.await()
        app.jobs.cancel(CancelReason.Background)
        job.await()

        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.waitUntil(10_000) { compose.onAllNodes(hasText(title)).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText(title).performClick()
        compose.waitUntil(10_000) {
            compose.onAllNodes(hasText("The import stopped when mdoc left the screen", substring = true)).fetchSemanticsNodes().isNotEmpty()
        }
        // On screen, not only in the tree: the text pane once took all the height.
        compose.onNodeWithText("The import stopped when mdoc left the screen", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Finish import").assertIsDisplayed()
        app.jobs.clearInterrupted()
    }
}
