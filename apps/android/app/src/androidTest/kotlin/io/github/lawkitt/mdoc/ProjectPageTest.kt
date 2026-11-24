package io.github.lawkitt.mdoc

import androidx.core.content.FileProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.ProjectDeletion
import io.github.lawkitt.mdoc.core.ProjectId
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.home.HomeState
import io.github.lawkitt.mdoc.home.HomeViewModel
import io.github.lawkitt.mdoc.home.PendingAdd
import io.github.lawkitt.mdoc.home.RepeatedImport
import java.io.File
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.SharedFrom

/**
 * A Project's page, Delete Project's two answers, Unfiled's multi-select
 * delete and a repeated import (#1091 Decisions 5, 7, 9, 29–35), on the
 * real core: driven through the view model as the screens drive it, and
 * checked by what the lists, the page and the Vault show. The twin of
 * iOS's `ProjectPageTests` (#1118).
 */
@RunWith(AndroidJUnit4::class)
class ProjectPageTest {
    @get:Rule
    val core = TestCore()

    private val app get() = core.app
    private val documents get() = app.core.documents
    private val vault get() = app.core.vault

    private object Quiet : JobObserver {
        override fun stage(stage: uniffi.mdoc_ffi.JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    private val letterText = "Dear Maya Bennett, your claim is ready."

    /** A Project with one reviewed document, whose name is replaced. */
    private fun project(name: String, title: String = "Letter"): Pair<ProjectId, DocumentId> {
        val project = documents.createProject(name)
        val session = documents.importDocument(ImportSource.Text(letterText), title, project.id)
        session.recognize(Quiet)
        val start = letterText.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + 12), Category.Person)
        return project.id to session.id
    }

    private fun model(): HomeViewModel = runBlocking {
        withContext(Dispatchers.Main) { HomeViewModel({ app.core }, app.jobs) }.also { it.refreshNow() }
    }

    private fun HomeViewModel.now(): HomeState = state.value

    /** An AI reply the Project itself saved, as Restore with mdoc saves one. */
    private fun projectReply(project: ProjectId): String =
        vault.saveReply(SharedFrom.Project(project.raw), "Reply to the Project", null, "Noted, PERSON_1.").id

    // The page

    @Test
    fun aProjectsPageShowsItsDocumentsRepliesAndNames() = runBlocking {
        val (ellison, letter) = project("Ellison")
        val (harbour, _) = project("Harbour", "Tenancy")
        val reply = projectReply(ellison)
        val home = model()
        assertEquals(listOf(letter), home.now().documentsIn(ellison).map { it.id })
        assertEquals(listOf(reply), documents.projectReplies(ellison).map { it.id })
        assertTrue(documents.projectReplies(harbour).isEmpty())
        assertEquals("Noted, Maya Bennett.", home.restoreSavedReply(reply)?.text)

        val details = documents.projectDetails(ellison)
        val name = details.names.single()
        assertEquals("PERSON_1", name.alias)
        assertEquals(listOf("Maya Bennett"), name.originals)
        assertEquals(listOf(Category.Person), HomeViewModel.names(details.names, "").map { it.first })
        assertTrue("found by a wording, in any case", HomeViewModel.names(details.names, "MAYA").flatMap { it.second }.contains(name))
        assertTrue(HomeViewModel.names(details.names, "person_1").flatMap { it.second }.contains(name))
        assertTrue(HomeViewModel.names(details.names, "zzzz-no-such-name").isEmpty())
        assertTrue("the phones add nothing to them", details.always.isEmpty() && details.never.isEmpty())

        // The page reads them through the view model.
        withContext(Dispatchers.Main) { home.loadPage(ellison) }.join()
        val page = home.now().page!!
        assertEquals(ellison, page.project)
        assertEquals(listOf(reply), page.replies.map { it.id })
        assertEquals(details, page.details)
    }

    /**
     * A page load that ends late, its Project left for another, never
     * covers the page shown: Harbour's replies stay when Ellison's slow
     * read ends after them, and a refresh reads Harbour's again.
     */
    @Test
    fun aLatePageLoadNeverCoversThePageShown() = runBlocking {
        val (ellison, _) = project("Ellison")
        val (harbour, _) = project("Harbour", "Tenancy")
        projectReply(ellison)
        val reply = projectReply(harbour)
        val home = model()
        val read = home.readPage
        val slow = CountDownLatch(1)
        val reads = mutableListOf<ProjectId>()
        home.readPage = { id ->
            synchronized(reads) { reads += id }
            if (id == ellison) check(slow.await(30, TimeUnit.SECONDS)) { "Ellison's read was never let go" }
            read(id)
        }
        val late = withContext(Dispatchers.Main) { home.loadPage(ellison) }
        withContext(Dispatchers.Main) { home.loadPage(harbour) }.join()
        assertEquals(harbour, home.now().page?.project)
        slow.countDown()
        late.join()
        val page = home.now().page!!
        assertEquals("the page shown keeps its own", harbour, page.project)
        assertEquals(listOf(reply), page.replies.map { it.id })
        // A refresh reads the page shown again, not the one left.
        synchronized(reads) { reads.clear() }
        home.refreshNow()
        withTimeout(10_000) { while (synchronized(reads) { reads.isEmpty() }) kotlinx.coroutines.delay(20) }
        assertEquals(listOf(harbour), synchronized(reads) { reads.toList() })
    }

    /** New Project: a blank name makes nothing, a taken one is numbered, and what is added from its page lands in it. */
    @Test
    fun aNewProjectIsNamedAndTakesWhatIsAddedThere() = runBlocking {
        val home = model()
        assertNull(home.createProject("   "))
        assertTrue(home.now().projects.isEmpty())
        val riverside = home.createProject(" Riverside ")!!
        assertEquals("Riverside", home.now().project(riverside)?.name)
        val session = documents.importDocument(ImportSource.Text("Ann Lee signed the lease."), "Lease", riverside)
        session.recognize(Quiet)
        home.refreshNow()
        assertEquals(listOf(session.id), home.now().documentsIn(riverside).map { it.id })
        val second = home.createProject("Riverside")!!
        assertEquals("Riverside 2", home.now().project(second)?.name)
        // Renamed: its documents keep their titles.
        withContext(Dispatchers.Main) { home.renameProject(riverside, " Riverside lease ") }.join()
        assertEquals("Riverside lease", vault.project(riverside.raw).name)
        assertEquals("Lease", vault.document(session.vaultId!!).title)
    }

    // Delete Project (#1091 Decision 9)

    /**
     * Move its documents to Unfiled: they land there with their text and
     * names, an open one with them; the Project and its own AI replies go.
     */
    @Test
    fun deletingAProjectCanMoveItsDocumentsToUnfiled() = runBlocking {
        val (ellison, letter) = project("Ellison")
        val (harbour, _) = project("Harbour", "Tenancy")
        val reply = projectReply(ellison)
        val open = documents.session(letter)
        val text = open.snapshot().markdown
        val home = model()
        assertTrue(home.deleteProject(ellison, ProjectDeletion.MoveToUnfiled))
        assertNull(home.now().project(ellison))
        assertEquals(listOf(letter), home.now().unfiled.map { it.id })
        assertEquals("Ellison", home.now().deletedProject)
        assertTrue("the Project's own reply went with it", runCatching { vault.reply(reply) }.isFailure)
        assertNull("the open letter shows it in Unfiled", open.snapshot().projectId)
        assertEquals(text, open.snapshot().markdown)
        assertTrue("its names came with it", open.snapshot().identities.any { it.alias == "PERSON_1" })
        assertNotNull("other Projects stay", home.now().project(harbour))
    }

    /** Delete them: the documents go with the Project; other Projects keep theirs. */
    @Test
    fun deletingAProjectCanDeleteItsDocuments() = runBlocking {
        val (ellison, letter) = project("Ellison")
        val (harbour, tenancy) = project("Harbour", "Tenancy")
        documents.session(letter)
        val home = model()
        assertTrue(home.deleteProject(ellison, ProjectDeletion.DeleteDocuments))
        assertNull(home.now().project(ellison))
        assertTrue(home.now().documents.none { it.id == letter })
        assertEquals(listOf(tenancy), home.now().documentsIn(harbour).map { it.id })
        assertEquals(1, vault.documents().size)
    }

    /** An empty Project goes, and no document with it. */
    @Test
    fun anEmptyProjectIsDeleted() = runBlocking {
        documents.importDocument(ImportSource.Text("A note."), "Note").recognize(Quiet)
        val home = model()
        val empty = home.createProject("Empty")!!
        assertEquals(0, home.now().documentsIn(empty).size)
        assertTrue(home.deleteProject(empty, ProjectDeletion.DeleteDocuments))
        assertNull(home.now().project(empty))
        assertEquals(1, home.now().documents.size)
    }

    /**
     * An import not read yet that was to join the deleted Project lands in
     * Unfiled once it is read, and after a restart too; with Delete them it
     * goes too.
     */
    @Test
    fun anImportNotReadYetFollowsItsProject() = runBlocking {
        val (ellison, _) = project("Ellison")
        val moved = documents.importDocument(ImportSource.Text("Second letter to Maya Bennett."), "Second letter", ellison)
        val home = model()
        assertTrue(home.deleteProject(ellison, ProjectDeletion.MoveToUnfiled))
        assertNull(moved.snapshot().projectId)
        // After a restart, the pending import is Unfiled too.
        app.reopenCore()
        val again = app.core.documents.session(moved.id)
        again.recognize(Quiet)
        assertNull(app.core.vault.document(again.vaultId!!).projectId)

        val harbour = app.core.documents.createProject("Harbour").id
        val deleted = app.core.documents.importDocument(ImportSource.Text("Ann Lee"), "Note", harbour)
        val after = model()
        assertTrue(after.deleteProject(harbour, ProjectDeletion.DeleteDocuments))
        assertTrue(after.now().documents.none { it.id == deleted.id })
        assertTrue("it never saves", runCatching { deleted.recognize(Quiet) }.isFailure)
        assertTrue(app.core.vault.documents().none { it.title == "Note" })
    }

    // Unfiled's multi-select delete (#1091 Decision 5)

    /**
     * Documents selected together leave the lists under one Undo, which
     * brings them all back; once it ends they are deleted, and the others
     * stay.
     */
    @Test
    fun unfiledDeletesSeveralUnderOneUndo() = runBlocking {
        val (ellison, _) = project("Ellison")
        val ids = listOf("One", "Two", "Three").map { title ->
            documents.importDocument(ImportSource.Text("$title Ann Lee"), title).also { it.recognize(Quiet) }.id
        }
        val home = model()
        assertEquals(ids.toSet(), home.now().unfiled.map { it.id }.toSet())

        withContext(Dispatchers.Main) { home.delete(listOf(ids[0], ids[2])) }
        assertEquals(listOf(ids[1]), home.now().unfiled.map { it.id })
        assertEquals(2, home.now().pendingDelete?.ids?.size)
        assertNull("several: no one title", home.now().pendingDelete?.title)
        withContext(Dispatchers.Main) { home.undoDelete() }
        assertEquals("Undo brings both back", ids.toSet(), home.now().unfiled.map { it.id }.toSet())

        withContext(Dispatchers.Main) {
            home.delete(listOf(ids[0], ids[2]))
            home.commitPendingDelete()
        }
        withTimeout(10_000) { while (vault.unfiled().size != 1) kotlinx.coroutines.delay(50) }
        home.refreshNow()
        assertEquals(listOf(ids[1]), home.now().unfiled.map { it.id })
        assertEquals("the Project's document stays", 1, home.now().documentsIn(ellison).size)
    }

    // Repeated imports (#1091 Decision 7)

    private fun share(name: String): android.net.Uri {
        val folder = File(app.cacheDir, "hand-off").apply { mkdirs() }
        val file = File(folder, "${UUID.randomUUID()}-$name")
        InstrumentationRegistry.getInstrumentation().context.assets.open("handmade-running-header.pdf").use { input ->
            file.outputStream().use(input::copyTo)
        }
        return FileProvider.getUriForFile(app, "${app.packageName}.files", file)
    }

    private suspend fun receive(home: HomeViewModel, name: String, into: ProjectId? = null): HomeState {
        withContext(Dispatchers.Main) { home.received(app.contentResolver, listOf(share(name)), null, "Title", into) }
        return withTimeout(15_000) { home.state.first { it.adding != null || it.repeated != null || it.refused != null } }
    }

    /**
     * A file whose bytes are already in mdoc asks first: Open Existing
     * opens that document and keeps no copy; Import Again goes on to the
     * add sheet; Cancel imports nothing.
     */
    @Test
    fun aFileAlreadyInMdocAsksFirst() = runBlocking {
        val home = model()
        val first = receive(home, "lease.pdf")
        assertNull("a new file asks nothing", first.repeated)
        assertTrue((first.adding as PendingAdd.Received).suggestedTitle.endsWith("lease"))
        val id = withContext(Dispatchers.Main) { home.add(null, "Lease") }!!
        documents.session(id).recognize(Quiet)
        home.refreshNow()

        suspend fun repeated(): RepeatedImport {
            val state = receive(home, "lease.pdf")
            val repeated = state.repeated ?: fail("no question: $state") as Nothing
            assertEquals(id, repeated.existing)
            assertTrue(repeated.name, repeated.name.endsWith("lease.pdf"))
            return repeated
        }

        val opened = repeated()
        assertEquals(id, withContext(Dispatchers.Main) { home.openExisting() })
        assertFalse("the copy went", opened.pending.folder.exists())
        assertNull(home.now().repeated)

        val cancelled = repeated()
        withContext(Dispatchers.Main) { home.cancelRepeated() }
        assertNull(home.now().repeated)
        assertNull(home.now().adding)
        assertFalse(cancelled.pending.folder.exists())
        assertEquals(1, vault.documents().size)

        // Import Again goes on to the add sheet, and its Add makes a second document.
        val again = repeated()
        withContext(Dispatchers.Main) { home.importAgain() }
        assertEquals(again.pending, home.now().adding)
        val second = withContext(Dispatchers.Main) { home.add(null, "Lease again") }!!
        documents.session(second).recognize(Quiet)
        assertEquals(listOf("Lease", "Lease again"), vault.documents().map { it.title }.sorted())
    }

    /**
     * What is added lands where it was started: from a Project's page in
     * that Project, from elsewhere, or "Open in mdoc" from another app, in
     * Unfiled; the add sheet starts there.
     */
    @Test
    fun anAddStartsWhereItWasStarted() = runBlocking {
        val riverside = documents.createProject("Riverside").id
        val home = model()
        withContext(Dispatchers.Main) { home.startPaste("Pasted text", riverside) }
        assertEquals(riverside, home.now().adding?.into)
        withContext(Dispatchers.Main) { home.cancelAdd() }
        assertEquals(riverside, receive(home, "lease.pdf", riverside).adding?.into)
        withContext(Dispatchers.Main) { home.cancelAdd() }
        assertNull("Open in mdoc starts nowhere", receive(home, "other.pdf").adding?.into)
        withContext(Dispatchers.Main) { home.cancelAdd() }
    }

    /**
     * Its Project deleted, and mdoc stopped before its pending record said
     * so: the import lands in Unfiled when it is read, not refused.
     */
    @Test
    fun anImportWhoseProjectWentLandsInUnfiled() {
        val riverside = documents.createProject("Riverside").id
        val pending = documents.importDocument(ImportSource.Text("Ann Lee signed the lease."), "Lease", riverside)
        // The Project's delete reached the Vault only.
        vault.deleteProject(riverside.raw, uniffi.mdoc_ffi.ProjectDocuments.MOVE_TO_UNFILED)
        app.reopenCore()
        val again = app.core.documents.session(pending.id)
        again.recognize(Quiet)
        assertNull(app.core.vault.document(again.vaultId!!).projectId)
    }

    /** A paste asks nothing: it keeps no source to compare. */
    @Test
    fun aPasteAsksNothing() = runBlocking {
        val home = model()
        withContext(Dispatchers.Main) { home.startPaste("Pasted text") }
        assertNull(home.now().repeated)
        assertTrue(home.now().adding is PendingAdd.Text)
        withContext(Dispatchers.Main) { home.cancelAdd() }
    }
}
