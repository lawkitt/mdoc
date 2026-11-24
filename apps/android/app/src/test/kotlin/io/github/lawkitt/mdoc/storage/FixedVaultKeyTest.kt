package io.github.lawkitt.mdoc.storage

import org.junit.Assert.assertThrows
import org.junit.Test

/**
 * The fixed test key lives in the androidTest source set: the JVM tests
 * build against the app's own classes, so this fails once it moves back.
 */
class FixedVaultKeyTest {
    @Test
    fun theAppHasNoFixedVaultKey() {
        assertThrows(ClassNotFoundException::class.java) {
            Class.forName("io.github.lawkitt.mdoc.storage.FixedVaultKeyStore")
        }
    }
}
