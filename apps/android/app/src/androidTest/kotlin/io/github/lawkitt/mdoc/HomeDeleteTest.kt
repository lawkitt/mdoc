package io.github.lawkitt.mdoc

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.lawkitt.mdoc.core.ImportSource
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * A document is deleted from a list with a long press, as iOS's swipe
 * does: it leaves the list while a snackbar offers Undo, and is deleted
 * once the snackbar ends.
 */
@RunWith(AndroidJUnit4::class)
class HomeDeleteTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    private fun stored(title: String) = core.app.core.documents.snapshot().documents.any { it.title == title }

    private fun deleteFromTheList(title: String) {
        compose.waitForText(title)
        compose.shownText(title).performTouchInput { longClick() }
        compose.waitForShown(hasText("Delete") and hasClickAction())
        compose.shown(hasText("Delete") and hasClickAction()).performClick()
        compose.waitUntilGone(title)
        compose.waitForText("Deleted $title")
    }

    @Test
    fun aLongPressDeletesWithUndo() {
        val title = "Delete me ${System.nanoTime()}"
        core.app.core.documents.importDocument(ImportSource.Text("Dear Maya."), title)
        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.sidebar("All Documents")

        deleteFromTheList(title)
        assertTrue("kept while Undo is offered", stored(title))
        compose.shownText("Undo").performClick()
        compose.waitForText(title)
        assertTrue(stored(title))

        deleteFromTheList(title)
        // The snackbar ends: it is deleted.
        compose.waitUntil(20_000) { !stored(title) }
        assertFalse(stored(title))
    }
}
