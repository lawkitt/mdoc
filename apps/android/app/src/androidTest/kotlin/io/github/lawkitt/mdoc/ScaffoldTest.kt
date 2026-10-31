package io.github.lawkitt.mdoc

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.activity.compose.setContent
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import io.github.lawkitt.mdoc.home.HomeScreen
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.AppCore
import io.github.lawkitt.mdoc.storage.KeystoreCaseKeyStore
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.DocumentSession

/** The app on the core, on an Android image: the scaffold's checks (ADR 0089). */
@RunWith(AndroidJUnit4::class)
class ScaffoldTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun homeOpensOnTheCore() {
        // The app's own library is empty on a fresh install; it shows once read.
        compose.waitUntil(10_000) {
            compose.onAllNodesWithText("No documents yet").fetchSemanticsNodes().isNotEmpty()
        }
    }

    @Test
    fun aLibraryThatFailsToOpenSaysSo() {
        compose.activity.setContent {
            HomeScreen(rows = emptyList(), canFindNames = true, error = "The stored case key is damaged.")
        }
        compose.onNodeWithText("mdoc could not open its documents").assertExists()
        compose.onNodeWithText("The stored case key is damaged.").assertExists()
        compose.onNodeWithText("No documents yet").assertDoesNotExist()
    }

    @Test
    fun caseKeyIsMadeOnceAndUnwrapsAgain() {
        val file = File(context.noBackupFilesDir, "test-case-key.bin").apply { delete() }
        val alias = "mdoc-case-key-test"
        val first = KeystoreCaseKeyStore(file, alias).key()
        val again = KeystoreCaseKeyStore(file, alias).key()
        assertEquals(32, first.size)
        assertArrayEquals(first, again)
        assertTrue("the stored key is wrapped, not the key itself", !file.readBytes().contentEquals(first))
        file.delete()
    }

    @Test
    fun libraryStoresADocumentUnderTheKeystoreKey() {
        val storage = MdocStorage.app(context, folder = "scaffold-test").prepare()
        try {
            val core = AppCore.make(
                context,
                storage = storage,
                keys = KeystoreCaseKeyStore(storage.caseKeyFile, "mdoc-case-key-test"),
            )
            DocumentSession().use { session ->
                session.importText("Letter to Maya Bennett.", "Letter")
                session.save(core.library)
            }
            assertEquals(listOf("Letter"), core.library.documents().map { it.title })
            assertEquals(1, core.library.cases().cases.size)
        } finally {
            storage.root.deleteRecursively()
            storage.caseKeyFile.delete()
        }
    }
}
