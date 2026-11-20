package io.github.lawkitt.mdoc.storage

/**
 * A fixed case key for the instrumented tests. It lives in the androidTest
 * source set so that no build of the app can contain it.
 */
class FixedCaseKeyStore(private val bytes: ByteArray = ByteArray(32) { 42 }) : CaseKeyStore {
    override fun key(): ByteArray = bytes.copyOf()
}
