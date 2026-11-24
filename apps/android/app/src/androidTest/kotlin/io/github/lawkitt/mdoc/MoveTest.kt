package io.github.lawkitt.mdoc

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.ProjectId
import io.github.lawkitt.mdoc.core.SourceRange
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.CoreException

/**
 * A document's moves through the screens (#1091 Decision 9): Move to
 * Project… from an Unfiled row and its Undo, Move to New Project… from
 * the document's menu, Move to Unfiled from a Project's page, and a move
 * that stops part way. The twin of iOS's Move to Project and its toast.
 */
@RunWith(AndroidJUnit4::class)
class MoveTest {
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

    /** A read document, Unfiled or in `project`, its name replaced. */
    private fun reviewed(title: String, project: ProjectId? = null): DocumentId {
        val text = "Dear Maya Bennett, about $title."
        val session = documents.importDocument(ImportSource.Text(text), title, project)
        session.recognize(Quiet)
        val start = text.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + 12), Category.Person)
        return session.id
    }

    private fun vaultId(id: DocumentId) = checkNotNull(documents.session(id).vaultId)

    private fun tap(text: String) {
        compose.waitForShown(hasText(text) and hasClickAction())
        compose.shown(hasText(text) and hasClickAction()).performClick()
        compose.waitForIdle()
    }

    /** A long press on a list's row: its menu. */
    private fun menuOf(title: String) {
        compose.waitForText(title)
        compose.shownText(title).performTouchInput { longClick() }
    }

    /** Move to Project… from an Unfiled row, then the snackbar's Undo: the letter is back in Unfiled. */
    @Test
    fun aMoveIntoAProjectAndItsUndo() {
        val harbour = documents.createProject("Harbour").id
        val letter = reviewed("Letter")
        reload()
        compose.sidebar("Unfiled")
        menuOf("Letter")
        tap("Move to Project…")
        compose.waitForText("Move to Project")
        tap("Harbour")
        compose.waitForText("Moved to Harbour")
        assertEquals(harbour.raw, vault.document(vaultId(letter)).projectId)
        compose.waitUntilGone("Letter")
        tap("Undo")
        compose.waitForText("Moved back to Unfiled")
        compose.waitForText("Letter")
        assertNull(vault.document(vaultId(letter)).projectId)
    }

    /** Move to New Project… from the document's own menu: the Project is made with the letter in it. */
    @Test
    fun aMoveToANewProjectFromTheDocument() {
        val letter = reviewed("Letter")
        reload()
        compose.openDocument("Letter")
        compose.waitForShown(hasContentDescription("More options"))
        compose.shown(hasContentDescription("More options")).performClick()
        tap("Move to New Project…")
        compose.waitForText("The document leaves where it is for a new Project. Replies already sent still restore with the names they were sent with.")
        compose.shown(hasSetTextAction()).performTextInput("Riverside")
        tap("Move")
        compose.waitForText("Moved to Riverside")
        assertEquals(listOf("Riverside"), vault.projects().map { it.name })
        assertEquals(vault.projects().single().id, vault.document(vaultId(letter)).projectId)
        // The open letter shows its new place: its menu offers Move to Unfiled.
        compose.shown(hasContentDescription("More options")).performClick()
        compose.waitForText("Move to Unfiled")
    }

    /** Move to Unfiled from a Project's page: the lease leaves the page for Unfiled. */
    @Test
    fun aMoveToUnfiledFromAProjectsPage() {
        val harbour = documents.createProject("Harbour").id
        val lease = reviewed("Lease", harbour)
        reload()
        compose.sidebar("Harbour")
        menuOf("Lease")
        tap("Move to Unfiled")
        compose.waitForText("Moved to Unfiled")
        assertNull(vault.document(vaultId(lease)).projectId)
        compose.waitUntilGone("Lease")
        compose.sidebar("Unfiled")
        compose.waitForText("Lease")
    }

    /**
     * A move from the open letter's menu that stops part way after the
     * Vault moved it: the error shows, no Undo is offered, and the letter
     * still open shows where the Vault has it, with Harbour's Ann Lee
     * proposed in it, and saves its review there.
     */
    @Test
    fun aMoveThatStopsPartWay() {
        val harbour = documents.createProject("Harbour").id
        val lease = documents.importDocument(ImportSource.Text("Ann Lee signed the lease."), "Lease", harbour)
        lease.recognize(Quiet)
        lease.addReplacement(SourceRange(0, 7), Category.Person)
        val text = "Dear Maya Bennett, Ann Lee will call."
        val session = documents.importDocument(ImportSource.Text(text), "Letter")
        session.recognize(Quiet)
        session.addReplacement(SourceRange(5, 17), Category.Person)
        documents.moveInVault = { move ->
            move()
            throw CoreException.Io("the disk is full")
        }
        reload()
        compose.openDocument("Letter")
        compose.waitForShown(hasContentDescription("More options"))
        compose.shown(hasContentDescription("More options")).performClick()
        tap("Move to Project…")
        tap("Harbour")
        compose.waitForText("Something went wrong")
        compose.waitForText("the disk is full")
        tap("OK")
        assertEquals(0, compose.onAllNodes(hasText("Moved to Harbour")).fetchSemanticsNodes().size)
        // The open letter is in Harbour now: Ann Lee is proposed, and its Replace saves there.
        tap("Replace 1")
        compose.waitUntil(15_000) { vault.document(vaultId(session.id)).proposed == 0uL }
        assertEquals(harbour.raw, vault.document(vaultId(session.id)).projectId)
        assertEquals(harbour, session.snapshot().projectId)
        compose.shown(hasContentDescription("More options")).performClick()
        compose.waitForText("Move to Unfiled")
    }
}
