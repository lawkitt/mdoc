package io.github.lawkitt.mdoc

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.JobStage

/**
 * Every control has a name TalkBack can read (ADR 0085: VoiceOver and
 * TalkBack on every control): no "unlabeled button" on Home or a document.
 */
@RunWith(AndroidJUnit4::class)
class LabelsTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun everyControlIsNamed() {
        val title = "Labels ${System.nanoTime()}"
        val session = core.app.core.documents.importDocument(ImportSource.Text("Dear Maya Bennett."), title, CaseChoice.New(title))
        session.recognize(object : JobObserver {
            override fun stage(stage: JobStage) = Unit

            override fun onCancel(handler: () -> Unit) = Unit
        })
        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.waitUntil(10_000) { compose.onAllNodes(hasText(title)).fetchSemanticsNodes().isNotEmpty() }
        assertNamed("Home")
        compose.onNodeWithText(title).performClick()
        compose.waitUntil(10_000) { compose.onAllNodes(hasText("Text")).fetchSemanticsNodes().isNotEmpty() || compose.onAllNodes(hasText("Copy")).fetchSemanticsNodes().isNotEmpty() }
        assertNamed("the document")
    }

    private fun assertNamed(screen: String) {
        val unnamed = compose.onAllNodes(hasClickAction()).fetchSemanticsNodes().filter { node ->
            val config = node.config
            val text = config.getOrNull(SemanticsProperties.Text).orEmpty().joinToString("")
            val description = config.getOrNull(SemanticsProperties.ContentDescription).orEmpty().joinToString("")
            text.isBlank() && description.isBlank()
        }
        assertTrue("unnamed controls on $screen: ${unnamed.map { it.boundsInRoot }}", unnamed.isEmpty())
    }
}
