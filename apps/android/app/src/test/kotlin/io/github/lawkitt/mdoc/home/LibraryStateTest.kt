package io.github.lawkitt.mdoc.home

import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.DocumentPhase
import io.github.lawkitt.mdoc.core.DocumentSummary
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.ProjectColour
import io.github.lawkitt.mdoc.core.ProjectId
import io.github.lawkitt.mdoc.core.ProjectNameRow
import io.github.lawkitt.mdoc.core.ProjectSummary
import io.github.lawkitt.mdoc.core.SourceKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** What the sidebar, Home and the pages read from the library's state (#1091 Decisions 28, 33, 35), and where Back goes. */
class LibraryStateTest {
    private val ellison = ProjectSummary(ProjectId("e"), "Ellison", ProjectColour.Blue)
    private val harbour = ProjectSummary(ProjectId("h"), "Harbour", ProjectColour.Green)

    private fun document(id: String, title: String, project: ProjectId?, phase: DocumentPhase, updated: Long) =
        DocumentSummary(DocumentId(id), title, project, SourceKind.Text, phase, if (phase == DocumentPhase.Reviewing) 2 else 0, updated)

    private val state = HomeState(
        documents = listOf(
            document("1", "Lease", ellison.id, DocumentPhase.Reviewing, 30),
            document("2", "Note", null, DocumentPhase.Ready, 20),
            document("3", "Schedule", null, DocumentPhase.Reviewing, 10),
            document("4", "Tenancy", harbour.id, DocumentPhase.Ready, 5),
        ),
        projects = (1..7).map { ProjectSummary(ProjectId("p$it"), "Project $it", ProjectColour.Red) } + listOf(ellison, harbour),
        loaded = true,
    )

    @Test
    fun theSidebarShowsFiveProjectsInTheVaultsOrder() {
        assertEquals((1..5).map { "Project $it" }, state.topProjects.map { it.name })
    }

    @Test
    fun toReviewAndContinueAreTheDocumentsWithNamesLeft() {
        assertEquals(listOf("Lease", "Schedule"), state.toReview.map { it.title })
        assertEquals("Lease", state.continueDocument?.title)
        assertNull(state.copy(documents = state.documents.filter { it.phase != DocumentPhase.Reviewing }).continueDocument)
    }

    @Test
    fun unfiledAndAProjectsDocuments() {
        assertEquals(listOf("Note", "Schedule"), state.unfiled.map { it.title })
        assertEquals(listOf("Lease"), state.documentsIn(ellison.id).map { it.title })
    }

    @Test
    fun aDeleteWaitingOnItsUndoHidesItsDocuments() {
        val deleting = state.copy(pendingDelete = PendingDelete(listOf(DocumentId("2"), DocumentId("3")), null, 1))
        assertEquals(emptyList<DocumentSummary>(), deleting.unfiled)
        assertEquals(listOf("Lease"), deleting.toReview.map { it.title })
        assertEquals(4, deleting.documents.size)
    }

    @Test
    fun searchMatchesTitlesAndProjectNames() {
        val (documents, projects) = state.search(" harb ")
        assertEquals(listOf("Tenancy"), documents.map { it.title })
        assertEquals(listOf("Harbour"), projects.map { it.name })
        assertEquals(listOf("Lease"), state.search("LEASE").first.map { it.title })
        assertTrue(state.search("  ").first.isEmpty())
    }

    @Test
    fun onlyAFileOrASinglePhotoIsComparedWithWhatIsInMdoc() {
        val file = java.io.File("lease.pdf")
        assertEquals(file, HomeViewModel.sourceFile(ImportSource.File(file, SourceKind.Pdf)))
        assertEquals(file, HomeViewModel.sourceFile(ImportSource.Pages(listOf(file), SourceKind.Image)))
        assertNull("photos picked together", HomeViewModel.sourceFile(ImportSource.Pages(listOf(file, file), SourceKind.Image)))
        assertNull("camera pages", HomeViewModel.sourceFile(ImportSource.Pages(listOf(file), SourceKind.Scan)))
        assertNull("a paste", HomeViewModel.sourceFile(ImportSource.Text("Ann Lee")))
    }

    @Test
    fun aNewProjectsNameIsNumberedWhenTaken() {
        assertEquals("Riverside", HomeViewModel.unusedName("Riverside", listOf("Ellison")))
        assertEquals("Riverside 2", HomeViewModel.unusedName("Riverside", listOf("Riverside")))
        assertEquals("Riverside 3", HomeViewModel.unusedName("Riverside", listOf("Riverside", "Riverside 2")))
    }

    @Test
    fun namesGroupByCategoryAndMatchAliasOrWording() {
        val names = listOf(
            ProjectNameRow("ORG_1", Category.Organization, listOf("Harrow & Finch")),
            ProjectNameRow("PERSON_2", Category.Person, listOf("Ann Lee")),
            ProjectNameRow("PERSON_1", Category.Person, listOf("Maya Bennett", "Ms Bennett")),
        )
        val groups = HomeViewModel.names(names, "")
        assertEquals(listOf(Category.Person, Category.Organization), groups.map { it.first })
        assertEquals(listOf("PERSON_1", "PERSON_2"), groups.first().second.map { it.alias })
        assertEquals(listOf("PERSON_1"), HomeViewModel.names(names, "ms benn").flatMap { it.second }.map { it.alias })
        assertEquals(listOf("ORG_1"), HomeViewModel.names(names, "org_").flatMap { it.second }.map { it.alias })
        assertEquals(listOf("Ann"), HomeViewModel.wordings(listOf("Ann", "Lee"), "an"))
    }

    @Test
    fun backGoesHomeFromEveryPageAndLeavesFromHome() {
        val pages = listOf(Destination.AllProjects, Destination.Unfiled, Destination.AllDocuments, Destination.Project(ellison.id))
        pages.forEach { assertEquals(Destination.Home, it.back) }
        assertNull(Destination.Home.back)
        assertEquals(ellison.id, Destination.Project(ellison.id).project)
        assertNull(Destination.Unfiled.project)
    }

    /**
     * A document's moves (#1091 Decision 9): Move to Project… while another
     * Project exists, Move to New Project…, Move to Unfiled from a Project;
     * none for an import not read yet.
     */
    @Test
    fun aDocumentsMenuOffersTheMovesThatGoSomewhere() {
        val both = listOf(ellison, harbour)
        assertEquals(
            listOf(MoveCommand.ToProject, MoveCommand.ToNewProject),
            HomeViewModel.moveCommands(null, saved = true, both),
        )
        assertEquals(
            listOf(MoveCommand.ToProject, MoveCommand.ToNewProject, MoveCommand.ToUnfiled),
            HomeViewModel.moveCommands(ellison.id, saved = true, both),
        )
        assertEquals(
            "its own Project is the only one",
            listOf(MoveCommand.ToNewProject, MoveCommand.ToUnfiled),
            HomeViewModel.moveCommands(ellison.id, saved = true, listOf(ellison)),
        )
        assertEquals(listOf(MoveCommand.ToNewProject), HomeViewModel.moveCommands(null, saved = true, emptyList()))
        assertTrue(HomeViewModel.moveCommands(ellison.id, saved = false, both).isEmpty())
    }

    @Test
    fun aDestinationIsKeptAsOneString() {
        (listOf(Destination.Home, Destination.AllProjects, Destination.Unfiled, Destination.AllDocuments, Destination.Project(ellison.id))).forEach {
            assertEquals(it, Destination.decode(Destination.encode(it)))
        }
    }
}
