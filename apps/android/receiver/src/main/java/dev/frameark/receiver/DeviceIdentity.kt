package dev.frameark.receiver

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.MessageDigest
import java.security.spec.ECGenParameterSpec

/** Redaction-safe public identity returned to the Rust trust boundary. */
data class DeviceIdentity(
    val alias: String,
    val publicKeyFingerprint: String,
)

/** Stable, platform-neutral validation and fingerprint formatting rules. */
object DeviceIdentityPolicy {
    const val DEFAULT_ALIAS = "frameark-device"
    private const val MAX_ALIAS_BYTES = 64

    fun validateAlias(alias: String) {
        require(alias.isNotBlank()) { "identity alias must not be blank" }
        require(alias.toByteArray(Charsets.UTF_8).size <= MAX_ALIAS_BYTES) {
            "identity alias is too long"
        }
        require(alias.all { it.isLowerCase() || it.isDigit() || it == '-' || it == '_' || it == '.' }) {
            "identity alias contains unsupported characters"
        }
    }

    fun fingerprint(publicKey: ByteArray): String {
        require(publicKey.isNotEmpty()) { "public key must not be empty" }
        return MessageDigest.getInstance("SHA-256")
            .digest(publicKey)
            .joinToString(":") { byte -> "%02x".format(byte) }
    }
}

/**
 * Android Keystore-backed identity. Private key material never leaves the
 * Keystore; only a SHA-256 fingerprint of the certificate public key is
 * returned for Rust pairing/trust records.
 */
class AndroidDeviceIdentityStore(
    private val alias: String = DeviceIdentityPolicy.DEFAULT_ALIAS,
) {
    init {
        DeviceIdentityPolicy.validateAlias(alias)
    }

    fun getOrCreate(): DeviceIdentity {
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        if (!keyStore.containsAlias(alias)) {
            val generator = KeyPairGenerator.getInstance(
                KeyProperties.KEY_ALGORITHM_EC,
                "AndroidKeyStore",
            )
            generator.initialize(
                KeyGenParameterSpec.Builder(
                    alias,
                    KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY,
                )
                    .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                    .setDigests(KeyProperties.DIGEST_SHA256)
                    .setUserAuthenticationRequired(false)
                    .build(),
            )
            generator.generateKeyPair()
        }
        val certificate = keyStore.getCertificate(alias)
            ?: error("Android Keystore identity certificate is unavailable")
        return DeviceIdentity(alias, DeviceIdentityPolicy.fingerprint(certificate.publicKey.encoded))
    }
}
