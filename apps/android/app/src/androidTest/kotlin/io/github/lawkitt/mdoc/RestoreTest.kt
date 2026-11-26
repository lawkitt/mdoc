package io.github.lawkitt.mdoc

import android.content.Intent
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.Documents
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.ProjectId
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.restore.RestoreActivity
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.ReplyOwner

/**
 * Restore with mdoc on the Vault: a reply shared as text restores with the
 * names of the place shared from last, and Save reply keeps it through the
 * Restore handle as that place's AI Reply, which the app then shows,
 * restored.
 */
@RunWith(AndroidJUnit4::class)
class RestoreTest {
    @get:Rule
    val core = TestCore()

    @get:Rule
    val compose = createEmptyComposeRule()

    private val app get() = core.app

    private object Quiet : JobObserver {
        override fun stage(stage: JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    /** A letter with Maya Bennett replaced, shared for AI, in `project` or Unfiled. */
    private fun shared(title: String, project: ProjectId? = null): Documents.Session {
        val letter = "Dear Maya Bennett, your claim is ready."
        val session = app.core.documents.importDocument(ImportSource.Text(letter), title, project)
        session.recognize(Quiet)
        val start = letter.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + 12), Category.Person)
        // Below the floor: "I checked the names myself" (item 48).
        session.confirmNamesChecked()
        // Copy for AI records the place as shared last.
        session.copyForAi()
        return session
    }

    private fun share(reply: String) = Intent(app, RestoreActivity::class.java)
        .setAction(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, reply)

    @Test
    fun aSharedReplyRestoresWithTheLastSharedDocument() {
        val title = "Restore ${System.nanoTime()}"
        val session = shared(title)
        // Another document, newer, shared from nowhere: the default stays the shared one.
        app.core.documents.importDocument(ImportSource.Text("A note."), "Newer").recognize(Quiet)

        ActivityScenario.launch<RestoreActivity>(share("Thank you, PERSON_1. The claim is filed.")).use {
            compose.waitUntil(15_000) { compose.onAllNodes(hasText("1 value restored")).fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText(title).assertExists()
            compose.onNodeWithText("Save reply").performClick()
            compose.waitUntil(10_000) { compose.onAllNodes(hasText("Saved")).fetchSemanticsNodes().isNotEmpty() }
        }
        // Saved through Restore's own handle: the app takes it in, an Unfiled document's.
        app.core.documents.refresh()
        val vaultId = checkNotNull(session.vaultId)
        assertEquals(
            listOf(ReplyOwner.Document(vaultId)),
            app.core.vault.documentReplies(vaultId).map { it.owner },
        )
        assertEquals("Thank you, Maya Bennett. The claim is filed.", session.snapshot().reply?.restored?.text)
    }

    @Test
    fun aReplyToADocumentInAProjectIsTheProjects() {
        val project = app.core.vault.createProject("Harrow ${System.nanoTime()}")
        val session = shared("Letter ${System.nanoTime()}", ProjectId(project.id))
        assertEquals(project.id, session.projectId)

        ActivityScenario.launch<RestoreActivity>(share("PERSON_1 signed.")).use {
            compose.waitUntil(15_000) { compose.onAllNodes(hasText("1 value restored")).fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText(project.name).assertExists()
            compose.onNodeWithText("Save reply").performClick()
            compose.waitUntil(10_000) { compose.onAllNodes(hasText("Saved")).fetchSemanticsNodes().isNotEmpty() }
        }
        app.core.documents.refresh()
        assertEquals(
            listOf(ReplyOwner.Project(project.id)),
            app.core.vault.projectReplies(project.id).map { it.owner },
        )
        assertEquals("Maya Bennett signed.", session.snapshot().reply?.restored?.text)
    }

    /** Remove reply deletes the reply the pane showed, never one Restore saved since. */
    @Test
    fun removeReplyDeletesTheReplyShown() {
        val session = shared("Remove ${System.nanoTime()}")
        val shown = checkNotNull(session.restoreReply("First: PERSON_1.").reply)
        assertEquals("First: Maya Bennett.", shown.restored.text)
        // Restore with mdoc saves another while the pane still shows the first.
        val replies = app.core.replies
        val place = checkNotNull(replies.places().let { it.of(it.default) })
        replies.save("Second: PERSON_1.", place)
        app.core.documents.refresh()

        val after = session.clearReply(checkNotNull(shown.id))
        assertEquals("Second: Maya Bennett.", after.reply?.restored?.text)
        assertEquals(1, app.core.vault.documentReplies(checkNotNull(session.vaultId)).size)
    }

    @Test
    fun aSecondReplyReplacesTheFirst() {
        shared("Second ${System.nanoTime()}")
        fun shareNew(reply: String) = share(reply).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        ActivityScenario.launch<RestoreActivity>(shareNew("First: PERSON_1.")).use { scenario ->
            compose.waitUntil(15_000) { compose.exists(hasText("1 value restored")) }
            // The activity is single-task: a new share arrives through
            // onNewIntent, which pauses it for a moment.
            scenario.onActivity { it.startActivity(shareNew("Second: PERSON_1 and PERSON_9.")) }
            compose.waitUntil(15_000) { compose.exists(hasText("1 unrecognised", substring = true)) }
        }
    }
}
