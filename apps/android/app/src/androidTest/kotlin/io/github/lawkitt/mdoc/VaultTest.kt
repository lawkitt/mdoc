package io.github.lawkitt.mdoc

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.AppError
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.DocumentPhase
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.OriginalData
import io.github.lawkitt.mdoc.core.OriginalSource
import io.github.lawkitt.mdoc.core.ProjectId
import io.github.lawkitt.mdoc.core.SourceKind
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.JobStage

/**
 * The app on the Vault (ROADMAP Vault & Projects item 12, #1091): intake
 * lands in Unfiled or the Project it was started in and makes no Project,
 * documents are known by id across a restart, an original is viewed from
 * the Vault and wiped on close and at launch, and the old library's files
 * stay untouched.
 */
@RunWith(AndroidJUnit4::class)
class VaultTest {
    @get:Rule
    val core = TestCore()

    private val app get() = core.app

    private object Quiet : JobObserver {
        override fun stage(stage: JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    private val letter = "Dear Maya Bennett, your claim is ready."

    @Test
    fun anImportLandsInUnfiledAndMakesNoProject() {
        val session = app.core.documents.importDocument(ImportSource.Text(letter), "Letter")
        session.recognize(Quiet)

        assertEquals(emptyList<String>(), app.core.vault.projects().map { it.name })
        val stored = app.core.vault.unfiled().single()
        assertEquals(stored.id, session.vaultId)
        assertEquals("Letter", stored.title)
        assertNull(session.snapshot().projectId)
        // The list keeps the id this run gave it.
        assertEquals(session.id, app.core.documents.snapshot().documents.single().id)
    }

    @Test
    fun anImportStartedInAProjectLandsInIt() {
        val project = app.core.vault.createProject("Harrow & Finch")
        val session = app.core.documents.importDocument(ImportSource.Text(letter), "Letter", ProjectId(project.id))
        assertEquals(project.name, session.snapshot().projectName)
        session.recognize(Quiet)

        assertEquals(listOf(session.vaultId), app.core.vault.projectPage(project.id).documents.map { it.id })
        assertEquals(ProjectId(project.id), app.core.documents.snapshot().documents.single().projectId)
        assertEquals(1, app.core.vault.projects().size)
    }

    @Test
    fun aDocumentReopensByItsIdAfterARestart() {
        val session = app.core.documents.importDocument(ImportSource.Text(letter), "Letter")
        session.recognize(Quiet)
        val start = letter.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + 12), Category.Person)
        val vaultId = checkNotNull(session.vaultId)

        app.reopenCore()
        val listed = app.core.documents.snapshot().documents.single()
        assertEquals(DocumentId(vaultId), listed.id)
        val reopened = app.core.documents.session(listed.id).snapshot()
        assertEquals("Dear PERSON_1, your claim is ready.", reopened.markdown)
        assertEquals(listOf("Maya Bennett"), reopened.identities.map { it.primaryWording })
    }

    /** Below the name finder's floor, names marked by hand with none left are Ready (#1091 Q4 (b)). */
    @Test
    fun belowTheFloorADocumentReviewedByHandIsReadyToShare() {
        val session = app.core.documents.importDocument(ImportSource.Text(letter), "Letter")
        session.recognize(Quiet)
        val start = letter.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + 12), Category.Person)
        assertTrue(session.copyForAi().text.contains("Dear PERSON_1, your claim is ready."))
        // It stays unscanned: Home still reads what a scan would add.
        assertEquals(DocumentPhase.Ready, session.snapshot().phase)
        assertFalse(app.core.vault.document(checkNotNull(session.vaultId)).scanned)
    }

    @Test
    fun aPdfKeepsItsOriginalInTheVault() {
        val pdf = asset("handmade-running-header.pdf")
        val folder = checkNotNull(pdf.parentFile)
        val session = app.core.documents.importDocument(ImportSource.File(pdf, SourceKind.Pdf), "Running header")
        session.recognize(Quiet)

        // The received file goes at the first save: the Vault keeps the original.
        assertFalse(folder.exists())
        assertTrue(app.core.vault.document(checkNotNull(session.vaultId)).hasOriginal)
        val original = session.snapshot().original as OriginalSource.Pdf
        val stored = original.file as OriginalData.Stored
        // Small: decrypted into memory, with no copy on disk.
        val memory = stored.open() as OriginalData.Decrypted.Memory
        assertArrayEquals(assetBytes("handmade-running-header.pdf"), memory.bytes)
        assertEquals(emptyList<String>(), core.storage.viewing.list().orEmpty().toList())
        session.closeOriginal()
    }

    @Test
    fun aLargeOriginalIsViewedFromACopyWipedOnCloseAndAtLaunch() {
        app.core.documents.originalsInMemory = 0uL
        val session = app.core.documents.importDocument(
            ImportSource.File(asset("handmade-running-header.pdf"), SourceKind.Pdf), "Running header",
        )
        session.recognize(Quiet)
        val stored = (session.snapshot().original as OriginalSource.Pdf).file as OriginalData.Stored
        val copy = (stored.open() as OriginalData.Decrypted.Copy).file
        assertTrue(copy.path.startsWith(core.storage.viewing.path))
        assertArrayEquals(assetBytes("handmade-running-header.pdf"), copy.readBytes())

        session.closeOriginal()
        assertFalse(copy.exists())

        // A copy a stop left behind goes at the next launch.
        val left = File(core.storage.viewing, "left/behind.pdf").apply {
            checkNotNull(parentFile).mkdirs()
            writeText("x")
        }
        app.reopenCore()
        app.core.documents
        assertFalse(left.exists())
    }

    /** No migrator (#1091 Decision 2): the old library's files are never read, written or removed. */
    @Test
    fun theOldLibraryStaysUntouched() {
        val old = mapOf(
            "cases/0123.enc" to "sealed case",
            "documents/Letter.md" to "Dear Maya Bennett.",
            "imports/pages/1.json" to "{}",
            "originals/1b4e28ba-2fa1-11d2-883f-0016cb5d6a57/Letter.pdf" to "%PDF",
            "originals.json" to "{}",
            "pending.json" to "[]",
        )
        old.forEach { (path, text) ->
            File(core.storage.root, path).apply { checkNotNull(parentFile).mkdirs() }.writeText(text)
        }
        app.reopenCore()
        val session = app.core.documents.importDocument(ImportSource.Text(letter), "Letter")
        session.recognize(Quiet)
        app.core.documents.delete(session.id)

        old.forEach { (path, text) -> assertEquals(path, text, File(core.storage.root, path).readText()) }
        val made = core.storage.root.walk().filter { it.isFile }.map { it.relativeTo(core.storage.root).path.replace('\\', '/') }
        assertEquals(old.keys, made.toSet())
    }

    @Test
    fun aDeletedDocumentLeavesTheVault() {
        val session = app.core.documents.importDocument(ImportSource.Text(letter), "Letter")
        session.recognize(Quiet)
        app.core.documents.delete(session.id)
        assertEquals(emptyList<String>(), app.core.vault.documents().map { it.id })
        try {
            app.core.documents.session(DocumentId(checkNotNull(session.vaultId)))
            fail("a deleted document opened")
        } catch (_: AppError.NotFound) {
        }
    }

    private fun assetBytes(name: String) =
        InstrumentationRegistry.getInstrumentation().context.assets.open(name).use { it.readBytes() }

    /** A test asset received into the intake folder, as the add sheet stages it. */
    private fun asset(name: String): File {
        val folder = File(core.storage.dir(MdocStorage.Area.Originals), "vault-test-${System.nanoTime()}").apply { mkdirs() }
        return File(folder, name).apply { writeBytes(assetBytes(name)) }
    }
}
