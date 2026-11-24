package io.github.lawkitt.mdoc.core

import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import java.nio.file.Files
import kotlin.concurrent.thread
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.mdoc_ffi.CoreException
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.Vault

/**
 * Delete Project against the sessions open on the JVM, over the desktop
 * build of the core (`target/android/host`): an import whose first save
 * shows its Vault id as the delete runs, and a move that stops part way
 * (item 13's review, #1091).
 */
class ProjectDeleteRaceTest {
    private lateinit var root: File
    private lateinit var vault: Vault
    private lateinit var documents: Documents

    private object Quiet : JobObserver {
        override fun stage(stage: JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    private val letterText = "Dear Maya Bennett, your claim is ready."

    @Before
    fun open() {
        root = Files.createTempDirectory("mdoc-project-delete").toFile()
        val storage = MdocStorage(
            root = File(root, "mdoc"), noBackup = File(root, "no-backup"), vault = File(root, "vault"),
            intake = File(root, "vault-intake"), viewing = File(root, "vault-original-view"),
        ).prepare()
        vault = Vault.open(storage.vault.path, ByteArray(32) { 7 })
        documents = Documents(vault, storage, models = null)
    }

    @After
    fun close() {
        vault.close()
        root.deleteRecursively()
    }

    /** Replaces "Maya Bennett", which saves the document again. */
    private fun Documents.Session.replaceMaya() {
        val start = letterText.indexOf("Maya Bennett")
        addReplacement(SourceRange(start, start + "Maya Bennett".length), Category.Person)
    }

    /**
     * Runs `delete` on its own thread the moment the import's first save
     * shows its Vault id, while that save has not ended, and waits for it.
     */
    private fun deletingAsTheFirstSaveShows(delete: () -> Unit): () -> Throwable? {
        var outcome: Result<Unit>? = null
        documents.firstSaveShown = {
            documents.firstSaveShown = null
            val deleting = thread { outcome = runCatching(delete) }
            deleting.join(60_000)
            check(!deleting.isAlive) { "the delete waited on the save it raced" }
        }
        return { checkNotNull(outcome) { "the first save never showed" }.exceptionOrNull() }
    }

    /** Move to Unfiled as an import's first save ends: it is found, and opens again in Unfiled. */
    @Test
    fun aMoveAsAnImportsFirstSaveShowsFindsTheImport() {
        val project = documents.createProject("Ellison")
        val session = documents.importDocument(ImportSource.Text(letterText), "Letter", project.id)
        val failure = deletingAsTheFirstSaveShows { documents.deleteProject(project.id, ProjectDeletion.MoveToUnfiled) }
        session.recognize(Quiet)
        assertNull(failure())
        val vaultId = session.vaultId!!
        assertNull("in Unfiled in the Vault", vault.document(vaultId).projectId)
        assertNull("its session opened again in Unfiled", session.snapshot().projectId)
        // It saves again, in Unfiled.
        session.replaceMaya()
        assertNull(vault.document(vaultId).projectId)
        assertEquals(listOf(session.id), documents.snapshot().documents.map { it.id })
    }

    /** Delete them as an import's first save ends: its session goes with the document, and never saves again. */
    @Test
    fun aDeleteAsAnImportsFirstSaveShowsTakesItsSession() {
        val project = documents.createProject("Ellison")
        val session = documents.importDocument(ImportSource.Text(letterText), "Letter", project.id)
        val failure = deletingAsTheFirstSaveShows { documents.deleteProject(project.id, ProjectDeletion.DeleteDocuments) }
        session.recognize(Quiet)
        assertNull(failure())
        assertTrue("nothing is listed", documents.snapshot().documents.isEmpty())
        assertTrue(vault.documents().isEmpty())
        assertThrows(AppError.Cancelled::class.java) { session.replaceMaya() }
        assertTrue("nothing saved it back", vault.documents().isEmpty())
    }

    /**
     * A move to Unfiled that stops part way: the document that moved
     * opens again in Unfiled and saves there, the one left stays in the
     * Project, and the error still comes.
     */
    @Test
    fun aMoveThatStopsPartWayOpensWhatMovedAgain() {
        val project = documents.createProject("Ellison")
        val moved = documents.importDocument(ImportSource.Text(letterText), "Letter", project.id)
        moved.recognize(Quiet)
        val left = documents.importDocument(ImportSource.Text(letterText), "Lease", project.id)
        left.recognize(Quiet)
        documents.deleteInVault = { _, _ ->
            vault.moveDocument(moved.vaultId!!, null)
            // Seen only if the one left is opened again from the Vault.
            vault.renameDocument(left.vaultId!!, "Renamed behind its session")
            throw CoreException.Io("the disk is full")
        }
        val error = assertThrows(AppError.Failed::class.java) {
            documents.deleteProject(project.id, ProjectDeletion.MoveToUnfiled)
        }
        assertEquals("the disk is full", error.message)
        assertNull("the moved one shows Unfiled", moved.snapshot().projectId)
        assertEquals("the one left stays", project.id, left.snapshot().projectId)
        assertEquals("the one left was not opened again: its job runs on", "Lease", left.snapshot().title)
        documents.deleteInVault = { id, how -> vault.deleteProject(id, how) }
        // The moved one saves, in Unfiled.
        moved.replaceMaya()
        assertNull(vault.document(moved.vaultId!!).projectId)
        assertEquals(project.id.raw, vault.document(left.vaultId!!).projectId)
        assertFalse(vault.projects().isEmpty())
    }

    /**
     * An import started into a Project while its delete runs, after the
     * delete counted its imports and members: it lands in Unfiled, and
     * neither the delete nor a stale session takes it.
     */
    @Test
    fun anImportStartedWhileItsProjectIsDeletedLandsInUnfiled() {
        for (how in listOf(ProjectDeletion.DeleteDocuments, ProjectDeletion.MoveToUnfiled)) {
            val project = documents.createProject("Ellison $how")
            var late: Documents.Session? = null
            documents.deleteInVault = { id, kind ->
                late = documents.importDocument(ImportSource.Text(letterText), "Late $how", project.id).also { it.recognize(Quiet) }
                vault.deleteProject(id, kind)
            }
            documents.deleteProject(project.id, how)
            val session = checkNotNull(late)
            assertNull("$how: it shows Unfiled", session.snapshot().projectId)
            assertNull("$how: it is in Unfiled in the Vault", vault.document(session.vaultId!!).projectId)
            session.replaceMaya()
            assertTrue("$how: it is listed", documents.snapshot().documents.any { it.id == session.id })
        }
    }
}
