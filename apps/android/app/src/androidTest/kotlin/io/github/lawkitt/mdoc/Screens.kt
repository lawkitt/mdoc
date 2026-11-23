package io.github.lawkitt.mdoc

import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.printToLog

/**
 * On screen: not in the closed sidebar, which a modal drawer keeps composed
 * beside the screen, where its rows repeat titles and names the page shows.
 */
val onScreen = SemanticsMatcher("on screen") { node -> node.boundsInWindow.right > 0f && node.boundsInWindow.width > 0f }

/** The one node on screen matching `matcher`. */
fun ComposeTestRule.shown(matcher: SemanticsMatcher): SemanticsNodeInteraction = onAllNodes(matcher and onScreen).onFirst()

/** The node on screen with `text`. */
fun ComposeTestRule.shownText(text: String, substring: Boolean = false): SemanticsNodeInteraction = shown(hasText(text, substring))

fun ComposeTestRule.waitForShown(matcher: SemanticsMatcher, timeout: Long = 15_000) = try {
    waitUntil(timeout) { onAllNodes(matcher and onScreen).fetchSemanticsNodes().isNotEmpty() }
} catch (error: Throwable) {
    // What the screen showed instead, in logcat and as a shot in /data/local/tmp.
    runCatching { onAllNodes(androidx.compose.ui.test.isRoot()).fetchSemanticsNodes().indices.forEach { onAllNodes(androidx.compose.ui.test.isRoot())[it].printToLog("MDOCTEST") } }
    runCatching {
        val shot = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        java.io.File(androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "mdoc-test-failure.png").outputStream().use {
            shot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)
        }
    }
    throw error
}

fun ComposeTestRule.waitForText(text: String, timeout: Long = 15_000, substring: Boolean = false) =
    waitForShown(hasText(text, substring), timeout)

fun ComposeTestRule.waitUntilGone(text: String, timeout: Long = 15_000) =
    waitUntil(timeout) { onAllNodes(hasText(text) and onScreen).fetchSemanticsNodes().isEmpty() }

/** Opens the sidebar from the page's bar and taps its row `row`. */
fun ComposeTestRule.sidebar(row: String) {
    waitForShown(hasContentDescription("Show sidebar"))
    shown(hasContentDescription("Show sidebar")).performClick()
    waitForShown(hasText(row) and hasClickAction())
    shown(hasText(row) and hasClickAction()).performClick()
    waitForIdle()
}

/** A document opened from All Documents, as a person finds one Home no longer lists. */
fun ComposeTestRule.openDocument(title: String) {
    sidebar("All Documents")
    waitForText(title)
    shownText(title).performClick()
}

/** The system Back, as a person presses it. */
fun ComposeTestRule.back() {
    androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().uiAutomation
        .performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_BACK)
    waitForIdle()
    Thread.sleep(300)
}
