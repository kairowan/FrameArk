package dev.frameark.sender

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.spec.ECGenParameterSpec
import java.util.concurrent.atomic.AtomicBoolean

/** Opaque OS key handle. Never serialized, logged, exported, or sent through an Intent. */
class SenderIdentity private constructor(private val key: PrivateKey, publicKey: ByteArray) {
    private val spki = publicKey.copyOf()
    fun publicKeySpki(): ByteArray = spki.copyOf()
    fun sign(transcript: ByteArray): ByteArray {
        require(transcript.size in 1..256)
        check(signing.compareAndSet(false, true))
        try {
            return Signature.getInstance("SHA256withECDSA").run {
                initSign(key)
                update(transcript)
                sign().also { check(it.size in 8..72) }
            }
        } finally { signing.set(false) }
    }
    override fun toString(): String = "SenderIdentity([REDACTED])"

    companion object {
        private const val ALIAS = "frameark-sender-p256-v1"
        private val signing = AtomicBoolean()
        /** Call only off the UI thread. Existing invalid keys fail; never silently rotate. */
        @Synchronized fun load(): SenderIdentity {
            val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
            if (!store.containsAlias(ALIAS)) {
                KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, "AndroidKeyStore").apply {
                    initialize(KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_SIGN)
                        .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                        .setDigests(KeyProperties.DIGEST_SHA256).build())
                }.generateKeyPair()
            }
            val key = store.getKey(ALIAS, null) as PrivateKey
            check(key.encoded == null && key.algorithm == "EC")
            return SenderIdentity(key, store.getCertificate(ALIAS).publicKey.encoded)
        }
    }
}

/** At most one OS key load per process, including Activity recreation. */
object SenderIdentityLoader {
    private val busy = AtomicBoolean()
    fun load(callback: (SenderIdentity?) -> Unit): Boolean {
        if (!busy.compareAndSet(false, true)) return false
        Thread({
            val identity = try { SenderIdentity.load() } catch (_: Exception) { null }
            busy.set(false)
            callback(identity)
        }, "frameark-sender-key").start()
        return true
    }
}

/** Called only on the existing producer worker, never on the UI or Rust executor. */
fun signPending(native: SenderNative, id: Long, identity: SenderIdentity?) {
    val transcript = native.pollSigningRequest(id) ?: return
    checkNotNull(identity)
    check(native.submitSignature(id, identity.sign(transcript)) == 0)
}
