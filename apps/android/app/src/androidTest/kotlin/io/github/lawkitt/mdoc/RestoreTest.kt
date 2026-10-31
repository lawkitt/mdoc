package io.github.lawkitt.mdoc

import android.content.Intent
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.restore.RestoreActivity
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.JobStage

/**
 * Restore with mdoc (ADR 0085, 0089): a reply shared as text restores with
 * the case of the document shared last, and Save to document keeps it as
 * that document's reply (ADR 0078).
 */
@RunWith(AndroidJUnit4::class)
class RestoreTest {
    @get:Rule
    val core = TestCore()

    @get:Rule
    val compose = createEmptyComposeRule()

    private val app get() = core.app

    @Test
    fun aSharedReplyRestoresWithTheLastSharedCase() {
        val title = "Restore ${System.nanoTime()}"
        val letter = "Dear Maya Bennett, your claim is ready."
        val session = app.core.documents.importDocument(ImportSource.Text(letter), title, CaseChoice.New(title))
        session.recognize(object : JobObserver {
            override fun stage(stage: JobStage) = Unit

            override fun onCancel(handler: () -> Unit) = Unit
        })
        val start = letter.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + 12), Category.Person)
        // Copy for AI records the case as shared last.
        session.copyForAi()

        val reply = "Thank you, PERSON_1. The claim is filed."
        val intent = Intent(app, RestoreActivity::class.java)
            .setAction(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, reply)
        ActivityScenario.launch<RestoreActivity>(intent).use {
            compose.waitUntil(15_000) { compose.onAllNodes(hasText("1 value restored")).fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText(title).assertExists()
            compose.onNodeWithText("Save to document").performClick()
            compose.waitUntil(10_000) { compose.onAllNodes(hasText("Saved")).fetchSemanticsNodes().isNotEmpty() }
        }
        assertEquals("Thank you, Maya Bennett. The claim is filed.", session.snapshot().reply?.restored?.text)
    }

    @Test
    fun aSecondReplyReplacesTheFirst() {
        val title = "Second ${System.nanoTime()}"
        val letter = "Dear Maya Bennett, your claim is ready."
        val session = app.core.documents.importDocument(ImportSource.Text(letter), title, CaseChoice.New(title))
        session.recognize(object : JobObserver {
            override fun stage(stage: JobStage) = Unit

            override fun onCancel(handler: () -> Unit) = Unit
        })
        val start = letter.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + 12), Category.Person)
        session.copyForAi()
        fun share(reply: String) = Intent(app, RestoreActivity::class.java)
            .setAction(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, reply)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        ActivityScenario.launch<RestoreActivity>(share("First: PERSON_1.")).use { scenario ->
            compose.waitUntil(15_000) { compose.onAllNodes(hasText("1 value restored")).fetchSemanticsNodes().isNotEmpty() }
            // The activity is single-task: a new share arrives through onNewIntent.
            scenario.onActivity { it.startActivity(share("Second: PERSON_1 and PERSON_9.")) }
            compose.waitUntil(15_000) { compose.onAllNodes(hasText("1 unrecognised", substring = true)).fetchSemanticsNodes().isNotEmpty() }
        }
    }
}
