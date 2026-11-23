package io.github.lawkitt.mdoc

import android.content.ClipboardManager
import android.content.Intent
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.printToLog
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.restore.RestoreActivity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Item 12's emulator run, through the screens on an isolated Vault: a
 * pasted document is added (Unfiled), reviewed, shared, reopened after mdoc
 * restarts, and a reply shared to Restore with mdoc is saved and shows in
 * the document, restored. The emulator is below the detector floor, so the
 * name is marked by hand, as a phone below the floor does.
 */
@RunWith(AndroidJUnit4::class)
class VaultFlowTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    private val app get() = core.app
    private val title = "Vault flow ${System.nanoTime()}"
    private val letter = "Dear Maya Bennett, your claim is ready."

    private fun waitFor(text: String) = try {
        compose.waitUntil(15_000) {
            compose.onAllNodes(hasText(text, substring = true)).fetchSemanticsNodes().isNotEmpty()
        }
    } catch (error: Throwable) {
        // What the screens showed instead, in logcat.
        val roots = compose.onAllNodes(androidx.compose.ui.test.isRoot()).fetchSemanticsNodes().size
        repeat(roots) { compose.onAllNodes(androidx.compose.ui.test.isRoot())[it].printToLog("VAULTFLOW") }
        throw error
    }

    @Test
    fun importReviewShareReopenRestore() {
        // Reading is a long job: its notification's permission dialog would cover the screens.
        InstrumentationRegistry.getInstrumentation().uiAutomation
            .grantRuntimePermission(app.packageName, android.Manifest.permission.POST_NOTIFICATIONS)
        // Import: Add › Paste, Unfiled by default.
        compose.onNodeWithContentDescription("Add").performClick()
        compose.onNodeWithText("Paste").performClick()
        waitFor("Paste text")
        compose.onNode(hasSetTextAction() and hasText("Paste or type the text to check")).performTextInput(letter)
        compose.onNode(hasSetTextAction() and hasText("Title")).performTextReplacement(title)
        compose.shownText("Unfiled").assertExists()
        // The sheet's Add, not the button behind it that opened the menu.
        compose.onNode(hasText("Add") and hasClickAction() and !hasContentDescription("Add")).performScrollTo().performClick()
        waitFor("Copy")
        val listed = app.core.documents.snapshot().documents.single()
        assertNull("no import makes a Project", listed.projectId)
        assertEquals(emptyList<String>(), app.core.vault.projects().map { it.name })

        // Review: the name marked by hand.
        val session = app.core.documents.session(listed.id)
        val start = letter.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + 12), Category.Person)
        compose.activityRule.scenario.onActivity { it.recreate() }

        // Share: Copy for AI carries the alias, from the Vault's checked text.
        waitFor("Copy")
        awaitWindowFocus()
        compose.onNodeWithText("Copy").performClick()
        compose.waitForIdle()
        awaitWindowFocus()
        var copied = ""
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            copied = app.getSystemService(ClipboardManager::class.java).primaryClip?.getItemAt(0)?.text?.toString().orEmpty()
        }
        assertTrue(copied, copied.contains("Dear PERSON_1, your claim is ready."))
        assertFalse(copied, copied.contains("Maya Bennett"))
        val vaultId = checkNotNull(session.vaultId)

        // Reopen: mdoc starts again and finds the document by its id, as reviewed.
        app.reopenCore()
        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.sidebar("All Documents")
        waitFor(title)
        assertEquals(vaultId, app.core.documents.snapshot().documents.single().id.raw)

        // Restore: the reply restores with the document shared last and is saved.
        val share = Intent(app, RestoreActivity::class.java).setAction(Intent.ACTION_SEND).setType("text/plain")
            .putExtra(Intent.EXTRA_TEXT, "Thank you, PERSON_1. The claim is filed.")
        ActivityScenario.launch<RestoreActivity>(share).use {
            waitFor("1 value restored")
            compose.onNodeWithText("Save reply").performClick()
            waitFor("Saved")
        }

        // The document shows it, restored.
        compose.shownText(title).performClick()
        waitFor("Reply")
        compose.onAllNodesWithText("Reply").onFirst().performClick()
        waitFor("1 value restored")
        // The reply pane draws the restored text in a TextView: read it from the core.
        assertEquals(
            "Thank you, Maya Bennett. The claim is filed.",
            app.core.documents.session(app.core.documents.snapshot().documents.single().id).snapshot().reply?.restored?.text,
        )
    }
}
