package io.github.lawkitt.mdoc

import androidx.compose.ui.test.assert
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isSelectable
import androidx.compose.ui.test.isSelected
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.ProjectId
import io.github.lawkitt.mdoc.core.SourceRange
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The sidebar, a Project's page, the delete question and Unfiled's Select
 * through the screens (#1091 Decisions 5, 9, 33–35): the twin of iOS's
 * `ProjectPageUITests` (#1118).
 */
@RunWith(AndroidJUnit4::class)
class ProjectScreensTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    private val app get() = core.app
    private val documents get() = app.core.documents

    private object Quiet : JobObserver {
        override fun stage(stage: uniffi.mdoc_ffi.JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    @Before
    fun notifications() {
        // Reading is a long job: its notification's permission dialog would cover the screens.
        InstrumentationRegistry.getInstrumentation().uiAutomation
            .grantRuntimePermission(app.packageName, android.Manifest.permission.POST_NOTIFICATIONS)
    }

    private fun reload() {
        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.waitForShown(hasContentDescription("Show sidebar"))
    }

    private fun seededProject(name: String, vararg titles: String): ProjectId {
        val project = documents.createProject(name).id
        for (title in titles) {
            val text = "Dear Maya Bennett, about $title."
            val session = documents.importDocument(ImportSource.Text(text), title, project)
            session.recognize(Quiet)
            val start = text.indexOf("Maya Bennett")
            session.addReplacement(SourceRange(start, start + 12), Category.Person)
        }
        return project
    }

    private fun tap(text: String) {
        compose.waitForShown(hasText(text) and hasClickAction())
        // Into view first where it is in a list.
        runCatching { compose.shown(hasText(text) and hasClickAction()).performScrollTo() }
        compose.shown(hasText(text) and hasClickAction()).performClick()
        compose.waitForIdle()
    }

    /** New Project opens a page titled with its name, on Documents, with Replies and Names beside it. */
    @Test
    fun aNewProjectOpensItsPage() {
        reload()
        compose.sidebar("New Project")
        compose.waitForText("Documents you add from its page land in it.")
        compose.shown(hasSetTextAction()).performTextInput("Riverside")
        tap("Create")
        compose.waitForText("Riverside")
        compose.waitForText("What you add with + lands in this Project.")
        compose.shown(hasText("Documents") and isSelectable()).assert(isSelected())
        tap("Replies")
        compose.waitForText("No replies yet")
        tap("Names")
        compose.waitForText("No names yet")
        compose.waitForText("Always replace")
        compose.waitForText("Never replace")
        assertEquals(listOf("Riverside"), app.core.vault.projects().map { it.name })
    }

    /** What + adds on a Project's page lands in it: the add sheet starts on the Project. */
    @Test
    fun addingFromAProjectsPageLandsInIt() {
        val project = seededProject("Harrow & Finch")
        reload()
        compose.sidebar("Harrow & Finch")
        compose.waitForText("What you add with + lands in this Project.")
        compose.shown(hasContentDescription("Add")).performClick()
        tap("Paste")
        compose.waitForText("Paste text")
        compose.shown(hasSetTextAction() and hasText("Paste or type the text to check")).performTextInput("Ann Lee signed the lease.")
        // The sheet's Add, not the page's button behind it.
        compose.shown(hasText("Add") and hasClickAction() and !hasContentDescription("Add")).performScrollTo().performClick()
        compose.waitUntil(15_000) { app.core.vault.documents().isNotEmpty() }
        assertEquals(project.raw, app.core.vault.documents().single().projectId)
    }

    /** The page's tabs: its documents, its saved reply restored, and its names. */
    @Test
    fun aProjectsPageShowsItsRepliesAndNames() {
        val project = seededProject("Harrow & Finch", "Letter of engagement")
        app.core.vault.saveReply(uniffi.mdoc_ffi.SharedFrom.Project(project.raw), "Reply to Harrow & Finch", null, "Thank you, PERSON_1.")
        reload()
        compose.sidebar("Harrow & Finch")
        compose.waitForText("Letter of engagement")
        tap("Replies")
        tap("Reply to Harrow & Finch")
        compose.waitForText("1 value restored")
        // The restored text is drawn in a text view: ProjectPageTest reads it from the core.
        compose.back()
        tap("Names")
        compose.waitForText("PERSON_1")
        compose.waitForText("Maya Bennett")
        compose.shown(hasText("Names and wordings")).performTextInput("bennett")
        compose.waitForText("PERSON_1")
        compose.waitUntilGone("Always replace")
    }

    /** Delete Project… with documents: Move its documents to Unfiled lands them there. */
    @Test
    fun deletingAProjectMovesItsDocumentsToUnfiled() {
        seededProject("Harrow & Finch", "Letter of engagement")
        reload()
        compose.sidebar("Harrow & Finch")
        compose.shown(hasContentDescription("More options")).performClick()
        tap("Delete Project…")
        compose.waitForText("Delete “Harrow & Finch”?")
        compose.waitForText("Its document can move to Unfiled, or be deleted with it. Its own AI replies are deleted either way.")
        tap("Move its documents to Unfiled")
        compose.waitForText("Deleted Harrow & Finch")
        // The page gave way to All Projects.
        compose.waitForText("No Projects yet")
        assertTrue(app.core.vault.projects().isEmpty())
        assertNull(app.core.vault.documents().single().projectId)
        compose.sidebar("Unfiled")
        compose.waitForText("Letter of engagement")
    }

    /** Delete Project… with documents: Delete them takes them with it. */
    @Test
    fun deletingAProjectCanDeleteItsDocuments() {
        seededProject("Harrow & Finch", "Letter of engagement", "Schedule")
        seededProject("Ellison", "Tenancy")
        reload()
        compose.sidebar("All Projects")
        tap("Harrow & Finch")
        compose.shown(hasContentDescription("More options")).performClick()
        tap("Delete Project…")
        compose.waitForText("Its 2 documents can move to Unfiled, or be deleted with it. Its own AI replies are deleted either way.")
        tap("Delete them")
        compose.waitForText("Deleted Harrow & Finch")
        assertEquals(listOf("Tenancy"), app.core.vault.documents().map { it.title })
        assertEquals(listOf("Ellison"), app.core.vault.projects().map { it.name })
    }

    /** An empty Project asks plainly. */
    @Test
    fun anEmptyProjectAsksPlainly() {
        documents.createProject("Empty")
        reload()
        compose.sidebar("Empty")
        compose.shown(hasContentDescription("More options")).performClick()
        tap("Delete Project…")
        compose.waitForText("Delete Project?")
        compose.waitForText("“Empty” has no documents. Its names and its AI replies are deleted with it.")
        tap("Delete")
        compose.waitUntil(10_000) { app.core.vault.projects().isEmpty() }
    }

    /** Unfiled's Select: two of three deleted under one snackbar, whose Undo brings both back. */
    @Test
    fun unfiledDeletesSeveralWithUndo() {
        for (title in listOf("One", "Two", "Three")) {
            documents.importDocument(ImportSource.Text("$title Ann Lee"), title).recognize(Quiet)
        }
        reload()
        compose.sidebar("Unfiled")
        tap("Select")
        tap("One")
        tap("Three")
        compose.waitForText("2 selected")
        tap("Delete 2")
        compose.waitForText("Deleted 2 documents")
        compose.waitUntilGone("One")
        compose.waitForText("Two")
        tap("Undo")
        compose.waitForText("One")
        compose.waitForText("Three")
        assertEquals(3, app.core.vault.unfiled().size)

        tap("Select")
        tap("One")
        tap("Three")
        tap("Delete 2")
        compose.waitForText("Deleted 2 documents")
        // The snackbar ends: they are deleted.
        compose.waitUntil(20_000) { app.core.vault.unfiled().size == 1 }
        assertEquals(listOf("Two"), app.core.vault.unfiled().map { it.title })
    }

    /** The sidebar: To review only while names wait, five Projects, then All Projects; a row opens its page. */
    @Test
    fun theSidebarListsFiveProjectsAndWhatWaits() {
        (1..6).forEach { documents.createProject("Project $it") }
        reload()
        compose.shown(hasContentDescription("Show sidebar")).performClick()
        compose.waitForText("All Projects")
        assertTrue(compose.onAllNodes(hasText("To review") and onScreen).fetchSemanticsNodes().isEmpty())
        // Five of the six, with All Projects counting them all.
        compose.waitForText("6", substring = true)
        assertEquals(5, compose.onAllNodes(hasText("Project ", substring = true) and onScreen).fetchSemanticsNodes().size)
        compose.back()
        compose.waitUntil(5_000) { compose.onAllNodes(hasText("All Projects", substring = true) and onScreen).fetchSemanticsNodes().isEmpty() }

        val text = "Dear Maya Bennett."
        val session = documents.importDocument(ImportSource.Text(text), "Proposed letter")
        session.recognize(Quiet)
        session.seedProposals(listOf(uniffi.mdoc_ffi.Candidate(uniffi.mdoc_ffi.TextRange(5uL, 17uL), uniffi.mdoc_ffi.Category.PERSON, 0.9f)))
        reload()
        // Home's Continue, and the sidebar's To review.
        compose.waitForShown(hasContentDescription("Continue Proposed letter", substring = true))
        compose.shown(hasContentDescription("Show sidebar")).performClick()
        compose.waitForText("To review")
        tap("Proposed letter")
        compose.waitForText("Replace", substring = true)

        // Search finds a Project by name and opens its page.
        compose.back()
        compose.shown(hasContentDescription("Show sidebar")).performClick()
        compose.shown(hasSetTextAction()).performTextInput("project 1")
        tap("Project 1")
        compose.waitForText("What you add with + lands in this Project.")
    }
}
