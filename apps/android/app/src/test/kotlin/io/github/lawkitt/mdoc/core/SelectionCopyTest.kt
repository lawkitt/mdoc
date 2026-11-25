package io.github.lawkitt.mdoc.core

import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.mdoc_ffi.Candidate
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.SelectionCheck
import uniffi.mdoc_ffi.TextRange
import uniffi.mdoc_ffi.Vault

/**
 * A selection copied by hand (#1209, Vault & Projects item 42) through the
 * app's `Documents`, over the desktop build of the core
 * (`target/android/host`), below the detector floor as `models = null`
 * is: the menu says what the selection holds, Replace N and copy replaces
 * only its proposals, and a copy is marked unless the core finds it clean.
 */
class SelectionCopyTest {
    private lateinit var root: File
    private lateinit var vault: Vault
    private lateinit var documents: Documents

    private object Quiet : JobObserver {
        override fun stage(stage: JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    @Before
    fun open() {
        root = Files.createTempDirectory("mdoc-selection").toFile()
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

    /** `part` of `text` as a source range; the texts here are ASCII. */
    private fun range(text: String, part: String): SourceRange {
        val start = text.indexOf(part)
        assertTrue("'$part' in '$text'", start >= 0)
        return SourceRange(start, start + part.length)
    }

    /** A note in a new Project, its email proposed as a scan leaves it. */
    private fun note(): Documents.Session {
        val text = "Write to $EMAIL today."
        val project = documents.createProject("Harbour").id
        val session = documents.importDocument(ImportSource.Text(text), "Note", project)
        session.recognize(Quiet)
        val start = text.indexOf(EMAIL)
        session.seedProposals(
            listOf(Candidate(TextRange(start.toULong(), (start + EMAIL.length).toULong()), uniffi.mdoc_ffi.Category.EMAIL, 0.9f)),
        )
        return session
    }

    @Test
    fun theMenuSaysWhatTheSelectionHolds() {
        assertEquals(listOf(SelectionAction.Copy), SelectionAction.menu(SelectionCheck.Clean))
        assertEquals(
            listOf(SelectionAction.ReplaceAndCopy(2), SelectionAction.CopyWithNames),
            SelectionAction.menu(SelectionCheck.NotClean(2uL, false, 0uL)),
        )
        // Still proposals: a name written out or a change since the scan doesn't hide Replace N and copy.
        assertEquals(
            listOf(SelectionAction.ReplaceAndCopy(1), SelectionAction.CopyWithNames),
            SelectionAction.menu(SelectionCheck.NotClean(1uL, true, 1uL)),
        )
        for (alone in listOf(SelectionCheck.NotClean(0uL, true, 0uL), SelectionCheck.NotClean(0uL, false, 1uL), SelectionCheck.NeverScanned, null)) {
            assertEquals("$alone", listOf(SelectionAction.CopyWithNames), SelectionAction.menu(alone))
        }
        assertFalse(SelectionAction.marked(SelectionCheck.Clean))
        for (marked in listOf(SelectionCheck.NotClean(0uL, true, 0uL), SelectionCheck.NotClean(1uL, false, 0uL), SelectionCheck.NeverScanned, null)) {
            assertTrue("$marked", SelectionAction.marked(marked))
        }
    }

    /** Ready below the floor, its client's name replaced by hand: a selection of it is clean and copies unmarked. */
    @Test
    fun aCleanSelectionCopiesAsWrittenUnmarked() {
        val text = "Dear Maya Bennett, your claim is ready."
        val letter = documents.importDocument(ImportSource.Text(text), "Letter")
        letter.recognize(Quiet)
        letter.addReplacement(range(text, "Maya Bennett"), Category.Person)
        val shown = letter.snapshot().markdown
        val alias = letter.snapshot().candidates.single().alias
        val selection = range(shown, "$alias, your claim")

        assertEquals(SelectionCheck.Clean, letter.checkSelection(selection))
        val copied = letter.copySelection(selection, "$alias, your claim", SelectionAction.Copy)
        assertEquals("$alias, your claim", copied.text)
        assertFalse(copied.marked)
    }

    /** A proposal in the selection: Copy with names copies it as written, marked; Replace 1 and copy replaces it, then copies clean. */
    @Test
    fun aProposalOffersReplaceAndCopy() {
        val note = note()
        val shown = note.snapshot().markdown
        val selection = range(shown, "Write to $EMAIL")
        val check = note.checkSelection(selection)
        assertEquals(listOf(SelectionAction.ReplaceAndCopy(1), SelectionAction.CopyWithNames), SelectionAction.menu(check))

        val withNames = note.copySelection(selection, "Write to $EMAIL", SelectionAction.CopyWithNames)
        assertEquals("Write to $EMAIL", withNames.text)
        assertTrue(withNames.marked)
        assertEquals("nothing changed", 1, note.snapshot().pendingCount)

        // Cut into the email: the alias still copies whole.
        val replaced = note.copySelection(range(shown, "to ann@exa"), "to ann@exa", SelectionAction.ReplaceAndCopy(1))
        val alias = note.snapshot().candidates.single().alias
        assertEquals("to $alias", replaced.text)
        assertFalse(replaced.marked)
        assertEquals(0, replaced.snapshot!!.pendingCount)
        assertEquals(0, note.snapshot().pendingCount)
    }

    /** A Keep of a name its Project knows: Copy with names alone, and its copy is marked. */
    @Test
    fun aKeptKnownNameCopiesWithNamesMarked() {
        val note = note()
        note.decide(note.snapshot().candidates.single().id, Decision.Keep, Scope.AllOccurrences)
        val selection = range(note.snapshot().markdown, EMAIL)
        val check = note.checkSelection(selection)
        assertTrue("$check", check is SelectionCheck.NotClean && check.proposals == 0uL && check.knownNames > 0uL)
        assertEquals(listOf(SelectionAction.CopyWithNames), SelectionAction.menu(check))
        val copied = note.copySelection(selection, EMAIL, SelectionAction.CopyWithNames)
        assertEquals(EMAIL, copied.text)
        assertTrue(copied.marked)
    }

    /**
     * A selection drawn before a move opened the document again: Copy with
     * names still copies what was selected, marked; Replace N and copy is
     * refused rather than replace names the user did not select.
     */
    @Test
    fun aSelectionFromBeforeAMoveCopiesMarked() {
        val note = note()
        val seen = note.snapshot().opened
        val selection = range(note.snapshot().markdown, "Write to")
        documents.moveDocument(note.id, MoveTarget.Unfiled)
        val copied = note.copySelection(selection, "Write to", SelectionAction.CopyWithNames, seen)
        assertEquals("Write to", copied.text)
        assertTrue(copied.marked)
        val replaced = runCatching { note.copySelection(selection, "Write to", SelectionAction.ReplaceAndCopy(1), seen) }
        assertTrue("${replaced.exceptionOrNull()}", replaced.exceptionOrNull() is AppError.Failed)
    }

    /**
     * The text changed under a selection since it was drawn (a decision
     * shifted it): Copy with names copies the text the user selected,
     * marked; Replace N and copy is refused and replaces nothing.
     */
    @Test
    fun aSelectionWhoseTextChangedCopiesWhatWasSelected() {
        val note = note()
        val shown = note.snapshot().markdown
        val selection = range(shown, "Write to $EMAIL")
        note.replaceAllPending()
        val copied = note.copySelection(selection, "Write to $EMAIL", SelectionAction.CopyWithNames)
        assertEquals("Write to $EMAIL", copied.text)
        assertTrue(copied.marked)

        val text = "Ann Lee met the client."
        val memo = documents.importDocument(ImportSource.Text(text), "Memo")
        memo.recognize(Quiet)
        val drawn = memo.snapshot().markdown
        // Ann Lee is replaced after "Ann Lee met" was selected: Replace N and copy on it is refused.
        memo.addReplacement(range(text, "Ann Lee"), Category.Person)
        val outcome = runCatching { memo.copySelection(range(drawn, "Ann Lee met"), "Ann Lee met", SelectionAction.ReplaceAndCopy(1)) }
        assertTrue("${outcome.exceptionOrNull()}", outcome.exceptionOrNull() is AppError.Failed)
    }

    private companion object {
        const val EMAIL = "ann@example.com"
    }
}
