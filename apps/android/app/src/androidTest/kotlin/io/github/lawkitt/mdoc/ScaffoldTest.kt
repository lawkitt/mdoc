package io.github.lawkitt.mdoc

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.AppCore
import io.github.lawkitt.mdoc.storage.KeystoreVaultKeyStore
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.DocumentSession

/**
 * The app on the core, on an Android image: the scaffold's checks.
 * Home listing the library is HomeDeleteTest's first half, and a core that
 * fails to open is SetupFixesTest's.
 */
@RunWith(AndroidJUnit4::class)
class ScaffoldTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun vaultKeyIsMadeOnceAndUnwrapsAgain() {
        val file = File(context.noBackupFilesDir, "test-case-key.bin").apply { delete() }
        val alias = "mdoc-case-key-test"
        val first = KeystoreVaultKeyStore(file, alias).key()
        val again = KeystoreVaultKeyStore(file, alias).key()
        assertEquals(32, first.size)
        assertArrayEquals(first, again)
        assertTrue("the stored key is wrapped, not the key itself", !file.readBytes().contentEquals(first))
        file.delete()
    }

    @Test
    fun vaultStoresADocumentUnderTheKeystoreKey() {
        val storage = MdocStorage.app(context, folder = "scaffold-test").prepare()
        val key = storage.vaultKeyFile
        try {
            val core = AppCore.make(
                context,
                storage = storage,
                keys = KeystoreVaultKeyStore(key, "mdoc-case-key-test"),
            )
            DocumentSession().use { session ->
                session.importText("Letter to Maya Bennett.", "Letter")
                session.saveIntoVault(core.vault)
            }
            assertEquals(listOf("Letter"), core.vault.documents().map { it.title })
            // Unfiled: saving makes no Project.
            assertEquals(emptyList<String>(), core.vault.projects().map { it.name })
        } finally {
            storage.folders.forEach { it.deleteRecursively() }
            key.delete()
        }
    }
}
