package io.github.lawkitt.mdoc.core

import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import java.nio.file.Files
import kotlin.concurrent.thread
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.mdoc_ffi.CoreException
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.Vault

/**
 * Move to Project, Move to New Project and Move to Unfiled with their
 * Undo (#1091 Decision 9) against the sessions open on the JVM, over the
 * desktop build of the core (`target/android/host`): the moved session
 * opens again where it went and saves there, after a move that stops
 * part way too, and a move never lands in a Project its delete has read.
 */
class MoveDocumentTest {
    private lateinit var root: File
    private lateinit var storage: MdocStorage
    private lateinit var vault: Vault
    private lateinit var documents: Documents

    private object Quiet : JobObserver {
        override fun stage(stage: JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    @Before
    fun open() {
        root = Files.createTempDirectory("mdoc-move").toFile()
        storage = MdocStorage(
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

    /** A read document, Unfiled or in `project`, with `name` replaced: PERSON_1 where it lives. */
    private fun reviewed(text: String, name: String, title: String, project: ProjectId? = null): Documents.Session {
        val session = documents.importDocument(ImportSource.Text(text), title, project)
        session.recognize(Quiet)
        val start = text.indexOf(name)
        session.addReplacement(SourceRange(start, start + name.length), Category.Person)
        return session
    }

    /** Harbour, whose PERSON_1 is Ann Lee. */
    private fun harbour(): ProjectId {
        val project = documents.createProject("Harbour").id
        reviewed("Ann Lee signed the lease.", "Ann Lee", "Lease", project)
        return project
    }

    private fun letter() = reviewed("Dear Maya Bennett, your claim is ready.", "Maya Bennett", "Letter")

    private fun Documents.Session.aliases() = snapshot().identities.map { it.alias }

    /**
     * Into a Project and back: the open letter opens again each time, in
     * its new place with the aliases it has there, and saves there.
     */
    @Test
    fun aMoveAndItsUndoOpenTheSessionAgainWhereItWent() {
        val harbour = harbour()
        val letter = letter()
        assertEquals(listOf("PERSON_1"), letter.aliases())

        val move = documents.moveDocument(letter.id, MoveTarget.Existing(harbour))
        assertEquals(letter.id, move.document)
        assertEquals("Harbour", move.to?.name)
        assertNull("it was Unfiled", move.from)
        assertEquals("the session shows its Project", harbour, letter.snapshot().projectId)
        assertEquals("Harbour", letter.snapshot().projectName)
        assertEquals("Ann Lee keeps PERSON_1 there", listOf("PERSON_2"), letter.aliases())
        // It saves in its new place: a session left as it was is refused as stale.
        letter.replaceAllPending()
        assertEquals(harbour.raw, vault.document(letter.vaultId!!).projectId)

        val back = documents.undoMove(move)
        assertNull("back in Unfiled", back.to)
        assertEquals(harbour, back.from)
        assertNull(letter.snapshot().projectId)
        assertEquals(listOf("PERSON_1"), letter.aliases())
        letter.replaceAllPending()
        assertNull(vault.document(letter.vaultId!!).projectId)
    }

    /** Move to New Project makes it, Move to Unfiled takes it out; the place it is in already is refused. */
    @Test
    fun aMoveToANewProjectAndToUnfiled() {
        val letter = letter()
        val move = documents.moveDocument(letter.id, MoveTarget.NewProject("Riverside"))
        val riverside = checkNotNull(move.to)
        assertEquals("Riverside", riverside.name)
        assertEquals(listOf("Riverside"), vault.projects().map { it.name })
        assertEquals(riverside.id, letter.snapshot().projectId)
        assertThrows(AppError.Failed::class.java) { documents.moveDocument(letter.id, MoveTarget.Existing(riverside.id)) }

        val out = documents.moveDocument(letter.id, MoveTarget.Unfiled)
        assertNull(out.to)
        assertEquals(riverside.id, out.from)
        assertNull(letter.snapshot().projectId)
        assertEquals("its names came with it", listOf("PERSON_1"), letter.aliases())
        letter.replaceAllPending()
        assertNull(vault.document(letter.vaultId!!).projectId)
        assertThrows(AppError.Failed::class.java) { documents.moveDocument(letter.id, MoveTarget.Unfiled) }
    }

    /** A document no session holds this run moves by its Vault id, and opens where it went. */
    @Test
    fun aDocumentNotOpenedThisRunMovesByItsVaultId() {
        val harbour = harbour()
        val vaultId = letter().vaultId!!
        val later = Documents(vault, storage, models = null)
        val move = later.moveDocument(DocumentId(vaultId), MoveTarget.Existing(harbour))
        assertEquals(DocumentId(vaultId), move.document)
        assertEquals(harbour.raw, vault.document(vaultId).projectId)
        assertEquals(harbour, later.session(DocumentId(vaultId)).snapshot().projectId)
    }

    /** An import not read yet is refused, in iOS's words, and stays where it was started. */
    @Test
    fun anImportNotReadYetIsRefused() {
        val harbour = harbour()
        val pending = documents.importDocument(ImportSource.Text("Dear Maya Bennett."), "Letter")
        val error = assertThrows(AppError.Failed::class.java) { documents.moveDocument(pending.id, MoveTarget.Existing(harbour)) }
        assertEquals(Documents.NOT_READ_YET, error.message)
        pending.recognize(Quiet)
        assertNull(vault.document(pending.vaultId!!).projectId)
    }

    /**
     * A move that stops part way: one that moved it before the error, and
     * one that wrote it back as it was. Either way the session opens it
     * again as the Vault has it and saves there, and the error comes.
     */
    @Test
    fun aMoveThatStopsPartWayOpensTheSessionAgain() {
        val harbour = harbour()
        val letter = letter()
        documents.moveInVault = { move ->
            move()
            throw CoreException.Io("the disk is full")
        }
        val error = assertThrows(AppError.Failed::class.java) { documents.moveDocument(letter.id, MoveTarget.Existing(harbour)) }
        assertEquals("the disk is full", error.message)
        assertEquals("it moved: the session shows where", harbour, letter.snapshot().projectId)
        letter.replaceAllPending()
        assertEquals(harbour.raw, vault.document(letter.vaultId!!).projectId)

        // Written back where it was, as a new revision.
        documents.moveInVault = { move ->
            vault.undoMove(move().undo)
            throw CoreException.Io("the disk is full")
        }
        assertThrows(AppError.Failed::class.java) { documents.moveDocument(letter.id, MoveTarget.Unfiled) }
        assertEquals(harbour, letter.snapshot().projectId)
        letter.replaceAllPending()
        assertEquals(harbour.raw, vault.document(letter.vaultId!!).projectId)
    }

    /**
     * A move into a Project while its delete runs waits for the delete:
     * it never lands there after the delete read the Project's documents,
     * which would move it to Unfiled behind its session. It is refused,
     * the Project gone, and the letter saves in Unfiled.
     */
    @Test
    fun aMoveIntoAProjectBeingDeletedWaitsForTheDelete() {
        val harbour = harbour()
        val letter = letter()
        var moved: Result<ProjectMove>? = null
        documents.deleteInVault = { id, how ->
            val moving = thread { moved = runCatching { documents.moveDocument(letter.id, MoveTarget.Existing(harbour)) } }
            moving.join(2_000)
            assertTrue("the move waits for the delete", moving.isAlive)
            vault.deleteProject(id, how)
        }
        documents.deleteProject(harbour, ProjectDeletion.MoveToUnfiled)
        val deadline = System.currentTimeMillis() + 30_000
        while (moved == null && System.currentTimeMillis() < deadline) Thread.sleep(20)
        assertTrue("refused: the Project went", checkNotNull(moved) { "the move never ended" }.isFailure)
        assertNull(letter.snapshot().projectId)
        letter.replaceAllPending()
        assertNull(vault.document(letter.vaultId!!).projectId)
        assertTrue(vault.projects().isEmpty())
        // The delete's own documents are in Unfiled too.
        assertEquals(2, vault.unfiled().size)
    }
}
