package io.github.lawkitt.mdoc.storage

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import java.io.File
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Where the 32-byte key sealing saved cases comes from (ADR 0085, 0089). */
fun interface CaseKeyStore {
    /** The key, made and stored the first time. */
    fun key(): ByteArray
}

/**
 * The case key as 32 random bytes wrapped by an Android Keystore AES key
 * (ADR 0089): the wrapping key never leaves the Keystore (StrongBox when the
 * phone has one), works only while the phone is unlocked and needs no
 * fingerprint; the wrapped bytes live in no-backup storage, so a backup or a
 * new phone never has them.
 */
class KeystoreCaseKeyStore(
    private val file: File,
    private val alias: String = "mdoc-case-key",
) : CaseKeyStore {

    @Synchronized
    override fun key(): ByteArray {
        if (file.exists()) return unwrap(file.readBytes())
        val key = ByteArray(KEY_BYTES).also { SecureRandom().nextBytes(it) }
        file.parentFile?.mkdirs()
        val temp = File(file.path + ".new")
        temp.writeBytes(wrap(key))
        check(temp.renameTo(file)) { "mdoc could not store its case key." }
        return key
    }

    private fun wrap(key: ByteArray): ByteArray {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, wrappingKey())
        return cipher.iv + cipher.doFinal(key)
    }

    private fun unwrap(stored: ByteArray): ByteArray {
        check(stored.size > IV_BYTES) { "mdoc's stored case key is damaged." }
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(
            Cipher.DECRYPT_MODE,
            wrappingKey(),
            GCMParameterSpec(TAG_BITS, stored, 0, IV_BYTES),
        )
        val key = cipher.doFinal(stored, IV_BYTES, stored.size - IV_BYTES)
        check(key.size == KEY_BYTES) { "mdoc's stored case key is damaged." }
        return key
    }

    private fun wrappingKey(): SecretKey {
        val keystore = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (keystore.getKey(alias, null) as? SecretKey)?.let { return it }
        return try {
            generate(strongBox = true)
        } catch (_: StrongBoxUnavailableException) {
            generate(strongBox = false)
        }
    }

    private fun generate(strongBox: Boolean): SecretKey {
        val spec = KeyGenParameterSpec.Builder(
            alias,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setUnlockedDeviceRequired(true)
            .setIsStrongBoxBacked(strongBox)
            .build()
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
            .apply { init(spec) }
            .generateKey()
    }

    private companion object {
        const val KEYSTORE = "AndroidKeyStore"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val KEY_BYTES = 32
        const val IV_BYTES = 12
        const val TAG_BITS = 128
    }
}

/** A fixed key for tests and previews. */
class FixedCaseKeyStore(private val bytes: ByteArray = ByteArray(32) { 42 }) : CaseKeyStore {
    override fun key(): ByteArray = bytes.copyOf()
}
