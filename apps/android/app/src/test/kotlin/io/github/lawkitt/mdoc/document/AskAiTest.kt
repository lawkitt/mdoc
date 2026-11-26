package io.github.lawkitt.mdoc.document

import android.content.Intent
import android.service.chooser.ChooserResult
import io.github.lawkitt.mdoc.core.AppError
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.Documents
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.Vault

/**
 * Ask AI (Milestone 2 item 20, as iOS's item 18) over the desktop build of
 * the core (`target/android/host`): its send intent carries the Vault's
 * text for AI and the masked file name, nothing else, never notes or an
 * original; and "Paste reply?" waits for an app to take the text and for
 * mdoc's return.
 */
class AskAiTest {
    private lateinit var root: File
    private lateinit var vault: Vault
    private lateinit var documents: Documents

    private object Quiet : JobObserver {
        override fun stage(stage: JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    @Before
    fun open() {
        root = Files.createTempDirectory("mdoc-ask-ai").toFile()
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

    private val harbour by lazy { documents.createProject("Harbour").id }

    /** `text` read into Harbour as `title`, with `name` replaced. */
    private fun reviewed(text: String, name: String, title: String): Documents.Session {
        val session = documents.importDocument(ImportSource.Text(text), title, harbour)
        session.recognize(Quiet)
        val start = text.indexOf(name)
        session.addReplacement(SourceRange(start, start + name.length), Category.Person)
        // Below the floor: "I checked the names myself" (item 48).
        session.confirmNamesChecked()
        return session
    }

    /** A letter titled by its client's name; Ann Lee is no name until another document makes her one. */
    private fun letter() = reviewed("Dear Maya Bennett, Ann Lee signed your claim.", "Maya Bennett", "Maya Bennett claim")

    /**
     * Done when: Ask AI's intent holds the text and title only, the Vault's
     * `text_for_ai` and `share_file_name` without `.md`, as Share as text's;
     * neither the client's name its title holds nor the notes.
     */
    @Test
    fun askAiCarriesOnlyTheTextForAiAndTheMaskedName() {
        val letter = letter()
        letter.setNotes("Ring the Zanzibar branch on Monday.")
        letter.saveNotes()
        val sent = SharedText.of(letter.copyForAi()).extras
        val id = letter.vaultId!!
        assertEquals(setOf(Intent.EXTRA_TEXT, Intent.EXTRA_TITLE), sent.keys)
        assertEquals(vault.textForAi(id), sent[Intent.EXTRA_TEXT])
        assertEquals(vault.shareFileName(id).removeSuffix(".md"), sent[Intent.EXTRA_TITLE])
        for (value in sent.values) {
            assertFalse("an original left: $value", value.contains("Maya") || value.contains("Bennett"))
            assertFalse("the notes left: $value", value.contains("Zanzibar"))
        }
        // What Share as text sends, byte for byte.
        val handOff = letter.copyForAi()
        assertEquals(SharedText.of(handOff.text, handOff.fileName).extras, sent)
    }

    /** Done when: not Ready (a name another document gave the Project), there is nothing to send. */
    @Test
    fun askAiIsRefusedUntilReady() {
        val letter = letter()
        reviewed("Ann Lee called.", "Ann Lee", "Memo")
        assertThrows(AppError::class.java) { letter.copyForAi() }
    }

    /** Done when: back in front after an app took the text, "Paste reply?" shows; not before. */
    @Test
    fun theBannerShowsOnReturnFromTheChosenApp() {
        val chosen = AskAiWait.chosen(AskAiWait.asked(), inFront = false)
        assertEquals(ReplyOffer.AwaitingReturn, chosen)
        assertEquals(ReplyOffer.Offered, AskAiWait.resumed(chosen))
        assertEquals("it stays until restored or closed", ReplyOffer.Offered, AskAiWait.resumed(ReplyOffer.Offered))
    }

    /** Done when: a choice the chooser reports once mdoc is back shows the banner at once. */
    @Test
    fun aChoiceReportedAfterTheReturnShowsItAtOnce() {
        assertEquals(ReplyOffer.Offered, AskAiWait.chosen(AskAiWait.asked(), inFront = true))
    }

    /** Done when: the chooser's own Copy and Edit (Android 15+) are no app taking the text; a pick is. */
    @Test
    fun theChoosersCopyAndEditAreNoChoice() {
        assertTrue("before Android 15 a report is a pick", AskAiWait.pickedAnApp(null))
        assertTrue(AskAiWait.pickedAnApp(ChooserResult.CHOOSER_RESULT_SELECTED_COMPONENT))
        for (type in listOf(ChooserResult.CHOOSER_RESULT_COPY, ChooserResult.CHOOSER_RESULT_EDIT, ChooserResult.CHOOSER_RESULT_UNKNOWN)) {
            assertFalse("type $type", AskAiWait.pickedAnApp(type))
        }
    }

    /** Done when: a chooser closed without a choice, or Share's, offers nothing. */
    @Test
    fun noBannerAfterACancelOrAPlainShare() {
        assertEquals("cancelled: back in front, nothing offered", ReplyOffer.Sharing, AskAiWait.resumed(AskAiWait.asked()))
        assertNull("nothing asked", AskAiWait.resumed(null))
        assertNull("Share's chooser reports nothing to Ask AI", AskAiWait.chosen(null, inFront = true))
    }
}
