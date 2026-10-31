package io.github.lawkitt.mdoc

import android.content.ClipboardManager
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.SourceRange
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.JobStage

/**
 * The review flow on the core, through the screens (ADR 0085, 0089): open a
 * document, see its replacement, Copy for AI carries the alias and not the
 * original, and a pasted AI reply comes back restored. The twin of the iOS
 * `ReviewFlowUITests`; the emulator is below the detector floor, so the
 * name is marked by Replace… as a phone below the floor does.
 */
@RunWith(AndroidJUnit4::class)
class ReviewFlowTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val app get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as MdocApplication
    private val title = "Flow ${System.nanoTime()}"
    private val letter = "Dear Maya Bennett, your claim is ready."

    @Before
    fun seed() {
        val session = app.core.documents.importDocument(ImportSource.Text(letter), title, CaseChoice.New(title))
        session.recognize(object : JobObserver {
            override fun stage(stage: JobStage) = Unit

            override fun onCancel(handler: () -> Unit) = Unit
        })
        val start = letter.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + "Maya Bennett".length), Category.Person)
    }

    private fun waitFor(text: String) = compose.waitUntil(15_000) {
        compose.onAllNodes(hasText(text, substring = true)).fetchSemanticsNodes().isNotEmpty()
    }

    @Test
    fun openReviewShareRestore() {
        // Home lists it, ready to share.
        compose.activityRule.scenario.onActivity { it.recreate() }
        waitFor(title)
        compose.onNodeWithText(title).performClick()

        // Copy for AI: the alias, never the original.
        waitFor("Copy")
        compose.onNodeWithText("Copy").performClick()
        compose.waitForIdle()
        val clipboard = app.getSystemService(ClipboardManager::class.java)
        var copied = ""
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            copied = clipboard.primaryClip?.getItemAt(0)?.text?.toString().orEmpty()
        }
        assertTrue(copied, copied.contains("PERSON_1"))
        assertFalse(copied, copied.contains("Maya Bennett"))

        // An AI reply in alias form comes back with the name restored.
        compose.onNodeWithText("Paste").performClick()
        waitFor("Paste AI reply")
        compose.onNodeWithText("AI reply").performTextInput("Thank you, PERSON_1. The claim is filed.")
        compose.onNodeWithText("Restore").performClick()
        waitFor("1 values restored")
        // The reply pane draws the restored text in a TextView; read it from the core.
        val document = app.core.documents.snapshot().documents.first { it.title == title }
        val reply = app.core.documents.session(document.id).snapshot().reply
        assertEquals("Thank you, Maya Bennett. The claim is filed.", reply?.restored?.text)
        compose.onAllNodesWithText("Reply")[0].assertExists()
    }
}
