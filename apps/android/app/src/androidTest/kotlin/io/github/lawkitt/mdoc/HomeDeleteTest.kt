package io.github.lawkitt.mdoc

import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.ImportSource
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** A document is deleted from Home with a long press, as iOS's swipe does. */
@RunWith(AndroidJUnit4::class)
class HomeDeleteTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun aLongPressDeletes() {
        val title = "Delete me ${System.nanoTime()}"
        core.app.core.documents.importDocument(ImportSource.Text("Dear Maya."), title, CaseChoice.New(title))
        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.waitUntil(10_000) { compose.onAllNodes(hasText(title)).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText(title).performTouchInput { longClick() }
        compose.waitUntil(5_000) { compose.onAllNodes(hasText("Delete")).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Delete").performClick()
        compose.waitUntil(10_000) { compose.onAllNodes(hasText(title)).fetchSemanticsNodes().isEmpty() }
        assertTrue(core.app.core.documents.snapshot().documents.none { it.title == title })
    }
}
