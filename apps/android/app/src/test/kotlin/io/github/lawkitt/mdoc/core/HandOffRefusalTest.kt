package io.github.lawkitt.mdoc.core

import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.TextRange
import uniffi.mdoc_ffi.Vault

/**
 * Copy for AI and Share refusals in plain words (#1209 decisions 13 and
 * 14; Vault & Projects item 38) against the sessions on the JVM, over the
 * desktop build of the core (`target/android/host`): each refusal is the
 * app's own, with its step, before the tap and at it, and none carries the
 * core's words. Every way out still reads `Vault.textForAi`. The twin of
 * iOS's `HandOffRefusalTests`.
 */
class HandOffRefusalTest {
    private lateinit var root: File
    private lateinit var vault: Vault
    private lateinit var documents: Documents

    private object Quiet : JobObserver {
        override fun stage(stage: JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    @Before
    fun open() {
        root = Files.createTempDirectory("mdoc-refusal").toFile()
        val storage = MdocStorage(
            root = File(root, "mdoc"), noBackup = File(root, "no-backup"), vault = File(root, "vault"),
            intake = File(root, "vault-intake"), viewing = File(root, "vault-original-view"),
        ).prepare()
        vault = Vault.open(storage.vault.path, ByteArray(32) { 7 })
        // No models: below the detector floor, names marked by hand.
        documents = Documents(vault, storage, models = null)
    }

    @After
    fun close() {
        vault.close()
        root.deleteRecursively()
    }

    /** Copy for AI's refusal, or null when it copies. */
    private fun Documents.Session.refusal(): HandOffRefusal? = try {
        copyForAi()
        null
    } catch (error: AppError.HandOffRefused) {
        // The core's words never ride along.
        for (word in CORE_WORDS) assertFalse("core text in: ${error.message}", error.message.orEmpty().contains(word))
        error.refusal
    }

    /** A read note in `project`, its email proposed as a scan leaves it. */
    private fun noteWithProposal(project: ProjectId): Documents.Session {
        val text = "Write to ann@example.com today."
        val session = documents.importDocument(ImportSource.Text(text), "Note", project)
        session.recognize(Quiet)
        val start = text.indexOf(EMAIL)
        session.seedProposals(
            listOf(
                uniffi.mdoc_ffi.Candidate(
                    TextRange(start.toULong(), (start + EMAIL.length).toULong()), uniffi.mdoc_ffi.Category.EMAIL, 0.9f,
                ),
            ),
        )
        return session
    }

    /** Done when: names left to decide refuse with Replace N and copy, which then copies. */
    @Test
    fun aProposalLeftWaitsForReplaceAndCopy() {
        val note = noteWithProposal(documents.createProject("Harbour").id)
        val refusal = note.refusal()
        assertEquals(HandOffRefusal.NamesToDecide(names = 1, proposals = 1), refusal)
        assertEquals(HandOffRefusal.Step.ReplaceAndCopy(1), refusal!!.step(findsNames = false))
        assertEquals("the bar and the menu agree with the tap", refusal, HandOffRefusal.before(note.snapshot(), false, note.savedReadiness()))

        // The step: Replace all, as the bar's Replace N, then Copy for AI.
        note.replaceAllPending()
        val text = note.copyForAi().text
        assertEquals(vault.textForAi(note.vaultId!!), text)
        assertFalse(text.contains(EMAIL))
    }

    /**
     * Done when: a Keep of a name the Project knows refuses as a known name
     * written out, before the tap and at it, with Review; replacing the
     * kept mention, which Review opens, makes it go (#1091, 04:13Z answer 1).
     */
    @Test
    fun aKeptKnownNameIsWrittenOutUntilReplaced() {
        val note = noteWithProposal(documents.createProject("Harbour").id)
        note.decide(note.snapshot().candidates.single().id, Decision.Keep, Scope.AllOccurrences)

        assertEquals(SavedReadiness.NamesNotReplaced(1), note.savedReadiness())
        val before = HandOffRefusal.before(note.snapshot(), findsNames = false, saved = note.savedReadiness())
        assertEquals(HandOffRefusal.NamesWrittenOut(1), before)
        assertEquals(HandOffRefusal.Step.Review, before!!.step(findsNames = false))
        assertEquals(HandOffRefusal.NamesWrittenOut(1), note.refusal())

        val kept = note.snapshot().candidates.single { it.state == MentionState.Kept }
        note.decide(kept.id, Decision.Replace, Scope.AllOccurrences)
        assertEquals(SavedReadiness.Ready, note.savedReadiness())
        assertNull(note.refusal())
    }

    /**
     * Done when: a name another document of the Project added after this
     * session opened is written out here; Review's way to it, the document
     * opened again from the Vault, proposes it.
     */
    @Test
    fun aNameAnotherDocumentAddedIsProposedOnceLinedUp() {
        val project = documents.createProject("Harbour").id
        val call = documents.importDocument(ImportSource.Text("Ann Lee called back."), "Call", project)
        call.recognize(Quiet)
        assertNull("nothing known yet: it goes", call.refusal())

        val memo = documents.importDocument(ImportSource.Text("Ann Lee signed."), "Memo", project)
        memo.recognize(Quiet)
        memo.addReplacement(SourceRange(0, 7), Category.Person)

        assertEquals(HandOffRefusal.NamesWrittenOut(1), call.refusal())
        assertEquals(0, call.snapshot().pendingCount)
        val lined = call.lineUpWithProject()
        assertEquals("the name is proposed", 1, lined.pendingCount)
        assertEquals(HandOffRefusal.NamesToDecide(names = 1, proposals = 1), call.refusal())
        call.replaceAllPending()
        assertFalse(call.copyForAi().text.contains("Ann Lee"))
    }

    /** Done when: an import not read yet refuses as not ready, with no step. */
    @Test
    fun anImportNotReadYetRefusesAsNotRead() {
        val note = documents.importDocument(ImportSource.Text("Ann Lee signed."), "Memo", null)
        assertEquals(HandOffRefusal.NotRead, note.refusal())
        assertEquals(HandOffRefusal.NotRead, HandOffRefusal.before(note.snapshot(), findsNames = false, saved = null))
        assertNull(HandOffRefusal.NotRead.step(findsNames = true))
    }

    /**
     * Done when: changed since its review and excluded from AI (stand-ins:
     * only a computer makes them today) refuse at the tap in their own
     * words; changed since its review has Find names at the detector tier
     * and no step below the floor, excluded from AI none at all.
     */
    @Test
    fun theComputersStatesRefuseInTheirOwnWords() {
        val note = documents.importDocument(ImportSource.Text("Nothing to hide here."), "Memo", null)
        note.recognize(Quiet)
        assertNull("below the floor, nothing marked: Ready", note.refusal())

        documents.readiness = { SavedReadiness.ChangedSinceReview }
        assertEquals(HandOffRefusal.ChangedSinceReview, note.refusal())
        assertEquals(HandOffRefusal.ChangedSinceReview, HandOffRefusal.before(note.snapshot(), false, note.savedReadiness()))
        assertEquals(HandOffRefusal.Step.FindNames, HandOffRefusal.ChangedSinceReview.step(findsNames = true))
        assertNull(HandOffRefusal.ChangedSinceReview.step(findsNames = false))

        documents.readiness = { SavedReadiness.ExcludedFromAi }
        assertEquals(HandOffRefusal.ExcludedFromAi, note.refusal())
        assertNull(HandOffRefusal.ExcludedFromAi.step(findsNames = true))

        documents.readiness = null
        assertNull(note.refusal())
    }

    /**
     * Done when: a record the tap's save can't bring up to date refuses as
     * not checked yet, and a read that fails where the Vault saw nothing
     * wrong refuses as "couldn't be prepared"; neither has a step below
     * the floor, and neither carries the core's words.
     */
    @Test
    fun theTapsOwnRefusalsAreInPlainWordsToo() {
        val letter = documents.importDocument(ImportSource.Text("Dear Maya Bennett, your claim is ready."), "Letter", null)
        letter.recognize(Quiet)
        letter.addReplacement(SourceRange(5, 17), Category.Person)

        documents.readiness = { SavedReadiness.NotReviewed }
        assertNull("no refusal before the tap below the floor", HandOffRefusal.before(letter.snapshot(), false, letter.savedReadiness()))
        assertEquals(HandOffRefusal.NotChecked, letter.refusal())
        assertNull(HandOffRefusal.NotChecked.step(findsNames = false))

        // The Vault refuses a kept known name; the stand-in says Ready.
        val note = noteWithProposal(documents.createProject("Harbour").id)
        note.decide(note.snapshot().candidates.single().id, Decision.Keep, Scope.AllOccurrences)
        documents.readiness = { SavedReadiness.Ready }
        assertEquals(HandOffRefusal.Failed, note.refusal())
        assertNull(HandOffRefusal.Failed.step(findsNames = true))
        documents.readiness = null
    }

    /**
     * Done when: a saved "not reviewed" while the review is done is no
     * refusal before the tap; at the detector tier a document never
     * scanned waits for Find names, and below the floor the bar and the
     * menu agree that it goes.
     */
    @Test
    fun theTierDecidesWhetherAnUnscannedDocumentGoes() {
        val note = documents.importDocument(ImportSource.Text("Nothing to hide here."), "Memo", null)
        note.recognize(Quiet)
        val snapshot = note.snapshot()
        assertEquals(DocumentPhase.Unscanned, snapshot.phase)
        assertEquals(HandOffRefusal.NotScanned, HandOffRefusal.before(snapshot, findsNames = true, saved = SavedReadiness.NotReviewed))
        assertEquals(HandOffRefusal.Step.FindNames, HandOffRefusal.NotScanned.step(findsNames = true))
        assertNull(HandOffRefusal.before(snapshot, findsNames = false, saved = SavedReadiness.NotReviewed))
        assertNull(HandOffRefusal.before(snapshot, findsNames = false, saved = note.savedReadiness()))
        assertEquals(HandOffRefusal.NotChecked, HandOffRefusal.of(SavedReadiness.NotReviewed))

        // Names marked by hand, never scanned: done for the review, but at
        // the detector tier the Vault waits for Find names, and so does the bar.
        note.addReplacement(SourceRange(0, 7), Category.Person)
        val marked = note.snapshot()
        assertEquals(DocumentPhase.Ready, marked.phase)
        assertEquals(HandOffRefusal.NotScanned, HandOffRefusal.before(marked, findsNames = true, saved = SavedReadiness.NotReviewed))
        assertNull(HandOffRefusal.before(marked, findsNames = false, saved = note.savedReadiness()))
    }

    private companion object {
        const val EMAIL = "ann@example.com"

        /** The core's refusal wording: `not ready for AI (NamesNotReplaced(1))` and its kin. */
        val CORE_WORDS = listOf("not ready for AI", "NamesNotReplaced", "NotReviewed", "ChangedSinceReview", "ExcludedFromAi", "Something went wrong")
    }
}
