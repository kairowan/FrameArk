package dev.frameark.receiver

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.io.FileOutputStream
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.SecretKey
import javax.crypto.KeyGenerator
import javax.crypto.spec.GCMParameterSpec

/**
 * Encrypts the Rust-owned FTR1 trust snapshot before writing it to app-private
 * storage. Rust remains authoritative for snapshot validation and trust
 * semantics; this adapter only owns Android Keystore and file lifecycle.
 */
class AndroidTrustSnapshotStore(
    private val file: File,
    private val native: FrameArkNative,
    keyProvider: (() -> SecretKey)? = null,
    private val random: SecureRandom = SecureRandom(),
) {
    private val keyProvider: () -> SecretKey = keyProvider ?: { loadOrCreateKey() }
    companion object {
        private val MAGIC = byteArrayOf('F'.code.toByte(), 'A'.code.toByte(), 'T'.code.toByte(), 'S'.code.toByte(), 1)
        private const val NONCE_BYTES = 12
        private const val TAG_BITS = 128
        private const val MAX_STORED_BYTES = FrameArkNative.MAX_TRUST_SNAPSHOT_BYTES + 64
        private const val KEY_ALIAS = "frameark.trust.snapshot.v1"
    }

    /** Restores a snapshot when present; missing storage is an empty state. */
    fun restore(): Boolean {
        if (!file.exists()) return true
        val stored = runCatching { file.readBytes() }.getOrNull() ?: return false
        if (stored.size !in MAGIC.size + NONCE_BYTES + 1..MAX_STORED_BYTES ||
            !stored.copyOfRange(0, MAGIC.size).contentEquals(MAGIC)
        ) return false
        val nonce = stored.copyOfRange(MAGIC.size, MAGIC.size + NONCE_BYTES)
        val ciphertext = stored.copyOfRange(MAGIC.size + NONCE_BYTES, stored.size)
        val plaintext = runCatching {
            Cipher.getInstance("AES/GCM/NoPadding").run {
                init(Cipher.DECRYPT_MODE, keyProvider(), GCMParameterSpec(TAG_BITS, nonce))
                doFinal(ciphertext)
            }
        }.getOrNull() ?: return false
        return native.importTrustSnapshot(plaintext)
    }

    /** Encrypts the current Rust snapshot and atomically replaces the file. */
    fun persist(): Boolean {
        val plaintext = native.exportTrustSnapshot() ?: return false
        if (plaintext.isEmpty() || plaintext.size > FrameArkNative.MAX_TRUST_SNAPSHOT_BYTES) return false
        val nonce = ByteArray(NONCE_BYTES).also(random::nextBytes)
        val ciphertext = runCatching {
            Cipher.getInstance("AES/GCM/NoPadding").run {
                init(Cipher.ENCRYPT_MODE, keyProvider(), GCMParameterSpec(TAG_BITS, nonce))
                doFinal(plaintext)
            }
        }.getOrNull() ?: return false
        val encoded = MAGIC + nonce + ciphertext
        if (encoded.size > MAX_STORED_BYTES) return false
        val temporary = File(file.parentFile ?: return false, ".${file.name}.tmp")
        return runCatching {
            FileOutputStream(temporary).use { output ->
                output.write(encoded)
                output.fd.sync()
            }
            Files.move(
                temporary.toPath(),
                file.toPath(),
                StandardCopyOption.ATOMIC_MOVE,
                StandardCopyOption.REPLACE_EXISTING,
            )
            true
        }.getOrElse {
            temporary.delete()
            false
        }
    }

    private fun loadOrCreateKey(): SecretKey {
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (keyStore.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(
                KeyGenParameterSpec.Builder(
                    KEY_ALIAS,
                    KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                )
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setRandomizedEncryptionRequired(true)
                    .build(),
            )
            generateKey()
        }
    }
}
