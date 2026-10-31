package io.github.lawkitt.mdoc

import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.doubleClick
import androidx.compose.ui.test.performTouchInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.OriginalSource
import io.github.lawkitt.mdoc.core.SourceKind
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** The Original page (ADR 0085): a PDF's pages beside, or instead of, its text. */
@RunWith(AndroidJUnit4::class)
class OriginalTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun aPdfShowsItsOriginalPage() {
        val app = core.app
        val folder = File(app.core.storage.dir(MdocStorage.Area.Originals), "pdf-test").apply { mkdirs() }
        val pdf = File(folder, "english.pdf")
        InstrumentationRegistry.getInstrumentation().context.assets.open("english.pdf").use { input -> pdf.outputStream().use(input::copyTo) }
        val title = "Scan ${System.nanoTime()}"
        val session = app.core.documents.importDocument(ImportSource.File(pdf, SourceKind.Pdf), title, CaseChoice.New(title))
        assertTrue(session.snapshot().original is OriginalSource.Pdf)

        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.waitUntil(10_000) { compose.onAllNodes(hasText(title)).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText(title).performClick()
        compose.waitUntil(10_000) { compose.onAllNodes(hasText("Original")).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Original").performClick()
        compose.waitUntil(10_000) {
            compose.onAllNodes(hasContentDescription("Original page 1 of 1")).fetchSemanticsNodes().isNotEmpty()
        }
        // A double tap zooms in to twice the size.
        val page = compose.onNode(hasContentDescription("Original page 1 of 1"))
        // Its left edge, which zooming about the centre moves off screen (bounds are clipped; position is not).
        val before = page.fetchSemanticsNode().positionInRoot.x
        page.performTouchInput { doubleClick() }
        compose.waitForIdle()
        val after = compose.onNode(hasContentDescription("Original page 1 of 1")).fetchSemanticsNode().positionInRoot.x
        assertTrue("zoomed in: left edge $before -> $after", after < before - 100f)
    }
}
