package io.github.lawkitt.mdoc.core

import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.SharedFrom
import uniffi.mdoc_ffi.Vault

/**
 * Document notes and Promote to Document (#1091 Decision 3) through the
 * app's `Documents`, over the desktop build of the core
 * (`target/android/host`): notes save with the document and never leave
 * with Copy for AI or Share; a saved reply promotes into its place, with
 * the way back to it.
 */
class NotesPromoteTest {
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
        root = Files.createTempDirectory("mdoc-notes").toFile()
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

    /** A read document, Unfiled or in `project`, with `name` replaced. */
    private fun reviewed(text: String, name: String, title: String, project: ProjectId? = null): Documents.Session {
        val session = documents.importDocument(ImportSource.Text(text), title, project)
        session.recognize(Quiet)
        val start = text.indexOf(name)
        session.addReplacement(SourceRange(start, start + name.length), Category.Person)
        return session
    }

    private fun letter() = reviewed("Dear Maya Bennett, your claim is ready.", "Maya Bennett", "Letter")

    /** The notes of `session`'s document as a later run reads them from the Vault. */
    private fun storedNotes(session: Documents.Session): String =
        Documents(vault, storage, models = null).session(DocumentId(session.vaultId!!)).notes()

    private val notes = "Ring the Zanzibar branch on Monday."

    /** Notes saved once asked, sealed with the document: a later run reads them back. */
    @Test
    fun notesSaveWithTheDocumentAndComeBack() {
        val letter = letter()
        letter.setNotes(notes)
        assertEquals("held at once", notes, letter.notes())
        assertEquals("not saved until asked", "", storedNotes(letter))
        letter.saveNotes()
        assertEquals(notes, storedNotes(letter))
    }

    /** Copy for AI, Share and Share as Markdown file all send its hand-off: the notes are in none of it. */
    @Test
    fun notesNeverLeaveWithCopyForAiOrShare() {
        val letter = letter()
        letter.setNotes(notes)
        letter.saveNotes()
        val handOff = letter.copyForAi()
        assertTrue("the text goes", handOff.text.contains("PERSON_1"))
        assertFalse("the notes stay: ${handOff.text}", handOff.text.contains("Zanzibar"))
        assertFalse(handOff.fileName.contains("Zanzibar"))
        assertFalse("nor are they in the snapshot", letter.snapshot().markdown.contains("Zanzibar"))
    }

    /** Notes typed in the last pause are saved before a move opens the document again. */
    @Test
    fun aMoveKeepsNotesNotSavedYet() {
        val letter = letter()
        letter.setNotes(notes)
        documents.moveDocument(letter.id, MoveTarget.NewProject("Riverside"))
        assertEquals("the session opened again with them", notes, letter.notes())
        assertEquals(notes, storedNotes(letter))
    }

    /** Notes typed in the last pause are saved before a Project's delete moves the document to Unfiled. */
    @Test
    fun aProjectDeleteKeepsNotesNotSavedYet() {
        val harbour = documents.createProject("Harbour").id
        val lease = reviewed("Ann Lee signed the lease.", "Ann Lee", "Lease", harbour)
        lease.setNotes(notes)
        documents.deleteProject(harbour, ProjectDeletion.MoveToUnfiled)
        assertNull("in Unfiled", lease.snapshot().projectId)
        assertEquals("the session opened again with them", notes, lease.notes())
        assertEquals(notes, storedNotes(lease))
    }

    /**
     * A Project's reply promotes into the Project: its text restored, its
     * names proposed again, the reply kept, and the way back to it, which
     * shows the reply deleted once it is.
     */
    @Test
    fun aProjectsReplyPromotesWithTheWayBack() {
        val harbour = documents.createProject("Harbour").id
        reviewed("Ann Lee signed the lease.", "Ann Lee", "Lease", harbour)
        val reply = vault.saveReply(SharedFrom.Project(harbour.raw), "Reply to Lease", null, "Call PERSON_1 back.")

        val promoted = documents.session(documents.promoteReply(reply.id))
        val snapshot = promoted.snapshot()
        assertEquals("in the reply's Project", harbour, snapshot.projectId)
        assertTrue("real names back: ${snapshot.markdown}", snapshot.markdown.contains("Call Ann Lee back."))
        assertEquals("not reviewed", DocumentPhase.Reviewing, snapshot.phase)
        assertEquals(PromotedFrom.Reply(SavedReplyRow(reply.id, reply.title, reply.created.toLong() * 1000)), snapshot.promotedFrom)
        assertTrue("the reply stays", documents.projectReplies(harbour).any { it.id == reply.id })
        assertTrue("listed", documents.snapshot().documents.any { it.id == promoted.id && it.projectId == harbour })

        vault.deleteReply(reply.id)
        assertEquals(PromotedFrom.DeletedReply, promoted.snapshot().promotedFrom)
        assertNull(documents.replyInfo(reply.id))
    }

    /** An Unfiled document's reply promotes into Unfiled. */
    @Test
    fun anUnfiledRepliesPromotesIntoUnfiled() {
        val letter = letter()
        val reply = vault.saveReply(SharedFrom.Document(letter.vaultId!!), "Reply to Letter", null, "Thank PERSON_1.")
        val promoted = documents.session(documents.promoteReply(reply.id)).snapshot()
        assertNull("Unfiled", promoted.projectId)
        assertTrue(promoted.markdown.contains("Thank Maya Bennett."))
        assertNull("an ordinary document has no way back", letter.snapshot().promotedFrom)
    }

    /** Promote saves what is pending first: notes typed in the last pause are kept. */
    @Test
    fun promoteSavesPendingNotesFirst() {
        val letter = letter()
        val reply = vault.saveReply(SharedFrom.Document(letter.vaultId!!), "Reply to Letter", null, "Thank PERSON_1.")
        letter.setNotes(notes)
        documents.promoteReply(reply.id)
        assertEquals(notes, storedNotes(letter))
    }
}
