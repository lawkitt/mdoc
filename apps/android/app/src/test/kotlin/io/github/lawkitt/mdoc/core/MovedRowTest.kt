package io.github.lawkitt.mdoc.core

import io.github.lawkitt.mdoc.home.HomeViewModel
import io.github.lawkitt.mdoc.home.StatusLine
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
import uniffi.mdoc_ffi.CoreException
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.TextRange
import uniffi.mdoc_ffi.Vault

/**
 * #1292, the maintainer's (2): a Ready document moved out of a deleted
 * Project, or into another one, is not Ready on its row until its next
 * save, as the Vault and the document have it ("Moved · open it to
 * share"); inside, the hand-off saves it Ready again and goes, a name it
 * never marked still as written. One whose save does not come out Ready still
 * refuses at the tap. Against the sessions on the JVM, over the desktop
 * build of the core (`target/android/host`).
 */
class MovedRowTest {
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
        root = Files.createTempDirectory("mdoc-moved-row").toFile()
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

    /** `text` read and saved in `project`, every name in `replace` proposed as a scan would and replaced. */
    private fun reviewed(text: String, project: ProjectId?, replace: List<String>): Documents.Session {
        val session = documents.importDocument(ImportSource.Text(text), "Letter", project)
        session.recognize(Quiet)
        session.seedProposals(
            replace.map { name ->
                val start = text.indexOf(name)
                uniffi.mdoc_ffi.Candidate(
                    TextRange(start.toULong(), (start + name.length).toULong()), uniffi.mdoc_ffi.Category.PERSON, 0.9f,
                )
            },
        )
        for (name in replace) session.decide(session.snapshot().candidates.single { it.original == name }.id, Decision.Replace, Scope.AllOccurrences)
        return session
    }

    /** Its row, from this session's library, or from one that holds no session (`fresh`). */
    private fun row(session: Documents.Session, fresh: Boolean = false): DocumentSummary {
        val library = if (fresh) Documents(vault, storage, models = null) else documents
        return library.snapshot().documents.single { it.vaultId == session.vaultId }
    }

    /** Done when: `phase()` of a record scanned with nothing proposed but not saved Ready is not Ready. */
    @Test
    fun aScannedRecordNotSavedReadyIsNotReady() {
        assertEquals(DocumentPhase.Ready, Documents.phase(scanned = true, candidates = 1, proposed = 0))
        assertEquals(DocumentPhase.Reviewing, Documents.phase(scanned = true, candidates = 1, proposed = 0, savedReady = false))
        assertEquals(DocumentPhase.Reviewing, Documents.phase(scanned = true, candidates = 2, proposed = 1, savedReady = false))
        // Never scanned, names marked by hand below the floor: as before.
        assertEquals(DocumentPhase.Ready, Documents.phase(scanned = false, candidates = 1, proposed = 0, savedReady = false))
        assertEquals(DocumentPhase.Unscanned, Documents.phase(scanned = false, candidates = 0, proposed = 0, savedReady = false))
        val moved = DocumentSummary(DocumentId("a"), "Letter", null, SourceKind.Text, DocumentPhase.Reviewing, 0, 0L, 1)
        assertEquals(StatusLine.MovedToShare, HomeViewModel.status(moved, canFindNames = true))
        assertEquals(StatusLine.MovedToShare, HomeViewModel.status(moved, canFindNames = false))
        assertEquals(StatusLine.ToReview(2), HomeViewModel.status(moved.copy(pendingCount = 2), canFindNames = true))
    }

    /**
     * Done when (#1292): moved out of a deleted Project, a Ready document's
     * row and its record agree it is not Ready, held by a session or not;
     * inside nothing refuses before the tap, and Copy for AI saves it Ready
     * again and copies the Vault's text, the name it never marked as written.
     */
    @Test
    fun aDocumentMovedOutOfADeletedProjectIsReadyAgainAtTheTap() {
        val project = documents.createProject("Lease").id
        val letter = reviewed("Ann Lee met Bob Hart.", project, replace = listOf("Ann Lee"))
        assertEquals(SavedReadiness.Ready, letter.savedReadiness())
        assertEquals(StatusLine.ReadyToShare, HomeViewModel.status(row(letter), canFindNames = true))

        documents.deleteProject(project, ProjectDeletion.MoveToUnfiled)
        for (fresh in listOf(false, true)) {
            val moved = row(letter, fresh)
            assertNull(moved.projectId)
            assertEquals("held by a session: ${!fresh}", DocumentPhase.Reviewing, moved.phase)
            assertEquals(StatusLine.MovedToShare, HomeViewModel.status(moved, canFindNames = true))
        }
        assertEquals("what the Vault says", SavedReadiness.NotReviewed, letter.savedReadiness())
        val inside = letter.snapshot()
        assertTrue(inside.scanned)
        assertEquals(0, inside.pendingCount)
        assertNull("the bar and ⋯", HandOffRefusal.before(inside, findsNames = true, saved = letter.savedReadiness()))
        assertNull(HandOffRefusal.before(inside, findsNames = false, saved = letter.savedReadiness()))

        val copied = letter.copyForAi().text
        assertEquals(vault.textForAi(letter.vaultId!!), copied)
        assertTrue(copied, copied.contains("met Bob Hart."))
        assertFalse(copied, copied.contains("Ann Lee"))
        assertEquals(SavedReadiness.Ready, letter.savedReadiness())
        assertEquals(StatusLine.ReadyToShare, HomeViewModel.status(row(letter), canFindNames = true))
        assertEquals(StatusLine.ReadyToShare, HomeViewModel.status(row(letter, fresh = true), canFindNames = true))
    }

    /**
     * Done when (#1292): moved into a Project that knows a name it left as
     * written, the name is proposed there, and the tap refuses: its save does not
     * come out Ready, and the Vault's own check refuses its text.
     */
    @Test
    fun aMovedDocumentWhoseSaveIsNotReadyStillRefuses() {
        val lease = documents.createProject("Lease").id
        reviewed("Bob Hart sued.", lease, replace = listOf("Bob Hart"))
        val letter = reviewed("Ann Lee met Bob Hart.", null, replace = listOf("Ann Lee"))
        assertEquals(SavedReadiness.Ready, letter.savedReadiness())

        documents.moveDocument(letter.id, MoveTarget.Existing(lease))
        assertEquals(StatusLine.ToReview(1), HomeViewModel.status(row(letter), canFindNames = true))
        val refusal = HandOffRefusal.NamesToDecide(names = 1, proposals = 1)
        assertEquals(refusal, HandOffRefusal.before(letter.snapshot(), findsNames = true, saved = letter.savedReadiness()))
        val refused = runCatching { letter.copyForAi() }.exceptionOrNull() as? AppError.HandOffRefused
        assertEquals(refusal, refused?.refusal)
        assertTrue(runCatching { vault.textForAi(letter.vaultId!!) }.exceptionOrNull() is CoreException)
        assertEquals(SavedReadiness.NotReviewed, letter.savedReadiness())
    }
}
