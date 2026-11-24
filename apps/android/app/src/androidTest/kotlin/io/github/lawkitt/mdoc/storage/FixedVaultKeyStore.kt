package io.github.lawkitt.mdoc.storage

/**
 * A fixed Vault key for the instrumented tests. It lives in the androidTest
 * source set so that no build of the app can contain it.
 */
class FixedVaultKeyStore(private val bytes: ByteArray = ByteArray(32) { 42 }) : VaultKeyStore {
    override fun key(): ByteArray = bytes.copyOf()
}
