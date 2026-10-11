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
        require(alias.all { it in 'a'..'z' || it in '0'..'9' || it == '-' || it == '_' || it == '.' }) {
            "identity alias contains unsupported characters"
        }
    }

    fun fingerprint(publicKey: ByteArray): String {
        require(publicKey.isNotEmpty()) { "public key must not be empty" }
        return MessageDigest.getInstance("SHA-256")
            .digest(publicKey)
            .joinToString(":") { byte -> "%02x".format(byte) }
    }

    fun validateFingerprint(fingerprint: String) {
        require(fingerprint.length == 95) { "identity fingerprint must be SHA-256 text" }
        require(fingerprint.split(':').size == 32) {
            "identity fingerprint must contain 32 octets"
        }
        require(fingerprint.split(':').all { it.length == 2 && it.all { character -> character in "0123456789abcdef" } }) {
            "identity fingerprint must use lowercase hexadecimal text"
        }
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
