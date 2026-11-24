package io.github.lawkitt.mdoc

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.ProjectId
import io.github.lawkitt.mdoc.core.SourceRange
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.DocumentSession
import uniffi.mdoc_ffi.SharedFrom

/**
 * Document notes and Promote to Document (#1091 Decision 3) through the
 * screens: Notes from the document's ⋯ menu keeps what was typed; Promote
 * from a Project's Replies or the Reply pane opens a new document, whose
 * ⋯ menu leads back to the reply until it is deleted. The twin of iOS's
 * `NotesPromoteUITests`.
 */
@RunWith(AndroidJUnit4::class)
class NotesPromoteTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    private val app get() = core.app
    private val documents get() = app.core.documents
    private val vault get() = app.core.vault

    private object Quiet : JobObserver {
        override fun stage(stage: uniffi.mdoc_ffi.JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    @Before
    fun notifications() {
        InstrumentationRegistry.getInstrumentation().uiAutomation
            .grantRuntimePermission(app.packageName, android.Manifest.permission.POST_NOTIFICATIONS)
    }

    private fun reload() {
        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.waitForShown(hasContentDescription("Show sidebar"))
    }

    /** A read document, Unfiled or in `project`, Maya Bennett replaced. */
    private fun reviewed(title: String, project: ProjectId? = null): DocumentId {
        val text = "Dear Maya Bennett, about $title."
        val session = documents.importDocument(ImportSource.Text(text), title, project)
        session.recognize(Quiet)
        val start = text.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + 12), Category.Person)
        return session.id
    }

    private fun tap(text: String) {
        compose.waitForShown(hasText(text) and hasClickAction())
        compose.shown(hasText(text) and hasClickAction()).performClick()
        compose.waitForIdle()
    }

    private fun moreOptions() {
        compose.waitForShown(hasContentDescription("More options"))
        compose.shown(hasContentDescription("More options")).performClick()
        compose.waitForIdle()
    }

    /** The notes saved in the Vault for `vaultId`, read through a session of its own. */
    private fun storedNotes(vaultId: String): String = DocumentSession().run {
        openFromVault(vault, vaultId)
        notes()
    }

    private fun openNotes() {
        moreOptions()
        tap("Notes")
        compose.waitForShown(hasTestTag("notes-editor"))
    }

    /** Notes typed and closed are in the Vault, and there when Notes opens again, under the line that says they stay. */
    @Test
    fun notesKeepWhatWasTyped() {
        val letter = reviewed("Letter")
        val vaultId = checkNotNull(documents.session(letter).vaultId)
        reload()
        compose.openDocument("Letter")
        openNotes()
        compose.waitForText("Private to this device. Never sent with Copy for AI or Share.")
        compose.shown(hasTestTag("notes-editor")).performTextInput("Ring the bank on Monday.")
        compose.shown(hasTestTag("notes-done")).performClick()
        compose.waitUntil(15_000) { !compose.exists(hasTestTag("notes-editor")) }
        compose.waitUntil(15_000) { storedNotes(vaultId) == "Ring the bank on Monday." }

        openNotes()
        compose.waitForShown(hasTestTag("notes-editor") and hasText("Ring the bank on Monday."))
    }

    /**
     * Promote from a Project's Replies: the new document opens in the
     * Project, saying where it came from; From AI reply shows the reply
     * restored, and once the reply is deleted it is disabled.
     */
    @Test
    fun aPromotedReplyLeadsBackToTheReply() {
        val harbour = documents.createProject("Harbour").id
        reviewed("Lease", harbour)
        val reply = vault.saveReply(SharedFrom.Project(harbour.raw), "Reply to Lease", null, "Call PERSON_1 back.")
        reload()
        compose.sidebar("Harbour")
        tap("Replies")
        tap("Reply to Lease")
        // The restored text is drawn in a text view: the core says what the document holds.
        compose.waitForText("1 value restored")
        tap("Promote to Document")

        compose.waitForShown(hasTestTag("from-reply-line") and hasText("From AI reply"))
        val promoted = vault.documents().single { it.title != "Lease" }
        assertEquals("in the reply's Project", harbour.raw, vault.document(promoted.id).projectId)
        assertTrue("the real name back", documents.session(DocumentId(promoted.id)).snapshot().markdown.contains("Call Maya Bennett back."))

        moreOptions()
        tap("From AI reply")
        compose.waitForText("1 value restored")
        compose.waitForShown(hasText("Promote to Document") and hasClickAction())
        compose.back()
        compose.waitUntil(15_000) { !compose.exists(hasText("1 value restored")) }

        // Deleted elsewhere: the document, opened again, shows it gone.
        vault.deleteReply(reply.id)
        compose.back()
        compose.openDocument(promoted.title)
        compose.waitForShown(hasTestTag("from-reply-line") and hasText("From a deleted AI reply"))
        moreOptions()
        compose.waitForShown(hasTestTag("from-reply"))
        compose.shown(hasTestTag("from-reply")).assertIsNotEnabled()
    }

    /** Promote on the document's own Reply pane opens the new document in its place. */
    @Test
    fun promoteFromTheReplyPane() {
        val letter = reviewed("Letter")
        documents.session(letter).restoreReply("Thank PERSON_1.")
        reload()
        compose.openDocument("Letter")
        tap("Reply")
        compose.waitForText("1 value restored")
        tap("Promote to Document")
        compose.waitForShown(hasTestTag("from-reply-line") and hasText("From AI reply"))
        assertEquals("the letter and the promoted reply", 2, vault.documents().size)
    }
}
