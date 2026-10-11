package dev.frameark.receiver

import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Small lifecycle owner for the Rust bridge.
 *
 * Protocol state remains in Rust. This class only loads the native library and
 * verifies that its exported ABI matches the Kotlin shell.
 */
class FrameArkNative(
    private val libraryLoader: (String) -> Unit = System::loadLibrary,
    versionProvider: (() -> Int)? = null,
    startProvider: (() -> Int)? = null,
    stopProvider: (() -> Int)? = null,
    runningProvider: (() -> Int)? = null,
    submitVideoProvider: ((ByteArray, Long, Boolean) -> Int)? = null,
    submitAudioProvider: ((ByteArray, Long) -> Int)? = null,
    pollMediaProvider: (() -> ByteArray?)? = null,
    networkPortProvider: (() -> Int)? = null,
    networkPairingCodeProvider: (() -> Int)? = null,
    networkCertificateProvider: (() -> ByteArray?)? = null,
    videoConfigProvider: (() -> ByteArray?)? = null,
    audioConfigProvider: (() -> ByteArray?)? = null,
    identityProvider: ((String, String) -> Int)? = null,
    trustPeerProvider: ((String, String, String) -> Int)? = null,
    revokePeerProvider: ((String) -> Int)? = null,
    peerTrustedProvider: ((String, String) -> Int)? = null,
    trustSnapshotExportProvider: (() -> ByteArray?)? = null,
    trustSnapshotImportProvider: ((ByteArray) -> Int)? = null,
    pairingApprovalRequiredProvider: ((Boolean) -> Int)? = null,
    pairingApprovalProvider: (() -> Int)? = null,
) {
    companion object {
        const val LIBRARY_NAME = "frameark_ffi"
        const val EXPECTED_ABI_VERSION = 1
        const val MAX_MEDIA_FRAME_BYTES = 4 * 1024 * 1024
        const val MAX_TRUST_SNAPSHOT_BYTES = 64 * 1024
        private const val MEDIA_FRAME_HEADER_BYTES = 19
        private const val MEDIA_CONFIG_HEADER_BYTES = 8
        private val MEDIA_FRAME_MAGIC = byteArrayOf('F'.code.toByte(), 'A'.code.toByte(), 'M'.code.toByte(), 'F'.code.toByte())
        private val MEDIA_CONFIG_MAGIC = byteArrayOf('F'.code.toByte(), 'A'.code.toByte(), 'M'.code.toByte(), 'C'.code.toByte())
    }

    private val bridgeVersionProvider: () -> Int = versionProvider ?: { nativeVersion() }
    private val bridgeStartProvider: () -> Int = startProvider ?: { nativeStartReceiver() }
    private val bridgeStopProvider: () -> Int = stopProvider ?: { nativeStopReceiver() }
    private val bridgeRunningProvider: () -> Int = runningProvider ?: { nativeReceiverRunning() }
    private val bridgeSubmitVideoProvider: (ByteArray, Long, Boolean) -> Int =
        submitVideoProvider ?: { payload, pts, keyframe -> nativeSubmitVideoFrame(payload, pts, keyframe) }
    private val bridgeSubmitAudioProvider: (ByteArray, Long) -> Int =
        submitAudioProvider ?: { payload, pts -> nativeSubmitAudioFrame(payload, pts) }
    private val bridgePollMediaProvider: () -> ByteArray? =
        pollMediaProvider ?: { nativePollMediaFrame() }
    private val bridgeNetworkPortProvider: () -> Int =
        networkPortProvider ?: { nativeReceiverPort() }
    private val bridgeNetworkPairingCodeProvider: () -> Int =
        networkPairingCodeProvider ?: { nativeReceiverPairingCode() }
    private val bridgeNetworkCertificateProvider: () -> ByteArray? =
        networkCertificateProvider ?: { nativeReceiverCertificate() }
    private val bridgeVideoConfigProvider: () -> ByteArray? =
        videoConfigProvider ?: { nativePollVideoConfig() }
    private val bridgeAudioConfigProvider: () -> ByteArray? =
        audioConfigProvider ?: { nativePollAudioConfig() }
    private val bridgeIdentityProvider: (String, String) -> Int =
        identityProvider ?: { alias, fingerprint -> nativeSetDeviceIdentity(alias, fingerprint) }
    private val bridgeTrustPeerProvider: (String, String, String) -> Int =
        trustPeerProvider ?: { deviceId, fingerprint, label -> nativeTrustPeer(deviceId, fingerprint, label) }
    private val bridgeRevokePeerProvider: (String) -> Int =
        revokePeerProvider ?: { deviceId -> nativeRevokePeer(deviceId) }
    private val bridgePeerTrustedProvider: (String, String) -> Int =
        peerTrustedProvider ?: { deviceId, fingerprint -> nativeIsPeerTrusted(deviceId, fingerprint) }
    private val bridgeTrustSnapshotExportProvider: () -> ByteArray? =
        trustSnapshotExportProvider ?: { nativeExportTrustSnapshot() }
    private val bridgeTrustSnapshotImportProvider: (ByteArray) -> Int =
        trustSnapshotImportProvider ?: { snapshot -> nativeImportTrustSnapshot(snapshot) }
    private val bridgePairingApprovalRequiredProvider: (Boolean) -> Int =
        pairingApprovalRequiredProvider ?: { required -> nativeSetPairingApprovalRequired(required) }
    private val bridgePairingApprovalProvider: () -> Int =
        pairingApprovalProvider ?: { nativeApproveNextPairing() }
    private var loadedAbiVersion: Int? = null

    /** Result of one native-library load attempt. */
    sealed interface LoadResult {
        /** The library loaded and reported the expected ABI. */
        data class Loaded(val abiVersion: Int) : LoadResult

        /** No compatible native library was available in this APK. */
        data class Unavailable(val reason: String) : LoadResult

        /** A library was found, but its ABI does not match this shell. */
        data class Incompatible(val expected: Int, val actual: Int) : LoadResult
    }

    /** Result of forwarding one Android service lifecycle operation to Rust. */
    sealed interface ReceiverResult {
        /** Rust accepted a newly started lifecycle. */
        data object Started : ReceiverResult

        /** Rust already had an active lifecycle. */
        data object AlreadyStarted : ReceiverResult

        /** Rust accepted an idempotent stop. */
        data object Stopped : ReceiverResult

        /** The native lifecycle rejected the operation. */
        data class Failed(val code: Int) : ReceiverResult

        /** The ABI-compatible native library is not available. */
        data class Unavailable(val reason: String) : ReceiverResult
    }

    /** Media kind encoded in one native `FAMF` bridge envelope. */
    enum class MediaKind {
        VIDEO,
        AUDIO,
    }

    /** Codec negotiated by Rust for one platform media track. */
    enum class MediaCodecKind {
        H264,
        H265,
        AAC,
        OPUS,
        PCM,
        ALAC,
        UNKNOWN,
    }

    /** Bounded video configuration emitted before encoded video frames. */
    data class VideoConfig(
        val codec: MediaCodecKind,
        val width: Int,
        val height: Int,
        val frameRateNumerator: Int,
        val frameRateDenominator: Int,
    )

    /** Bounded audio configuration emitted before encoded audio frames. */
    data class AudioConfig(
        val codec: MediaCodecKind,
        val sampleRate: Int,
        val channels: Int,
    )

    /** Public identity presented by a sender for explicit user approval. */
    data class TrustedPeer(
        val deviceId: String,
        val fingerprint: String,
        val label: String,
    )

    /** One bounded encoded frame delivered from Rust to an Android renderer. */
    data class MediaFrame(
        val kind: MediaKind,
        val pts: Long,
        val keyframe: Boolean,
        val payload: ByteArray,
    )

    /** Ephemeral FANP listener material for an explicit in-app pairing flow. */
    data class NetworkInfo(
        val port: Int,
        val pairingCode: Int,
        val certificateDer: ByteArray,
    )

    /** Result of submitting encoded media to the bounded Rust queue. */
    sealed interface MediaSubmitResult {
        /** Rust accepted the frame. */
        data object Accepted : MediaSubmitResult

        /** Rust queue is full and the sender should apply backpressure. */
        data object QueueFull : MediaSubmitResult

        /** Rust rejected the frame or could not acquire its state lock. */
        data class Failed(val code: Int) : MediaSubmitResult

        /** The ABI-compatible native library is not available. */
        data class Unavailable(val reason: String) : MediaSubmitResult
    }

    /** Loads and verifies the Rust library exactly once for this bridge instance. */
    fun load(): LoadResult {
        loadedAbiVersion?.let { return LoadResult.Loaded(it) }
        return try {
            libraryLoader(LIBRARY_NAME)
            val actual = bridgeVersionProvider()
            if (actual != EXPECTED_ABI_VERSION) {
                LoadResult.Incompatible(EXPECTED_ABI_VERSION, actual)
            } else {
                loadedAbiVersion = actual
                LoadResult.Loaded(actual)
            }
        } catch (_: UnsatisfiedLinkError) {
            LoadResult.Unavailable("native library is unavailable")
        }
    }

    /** Returns whether a compatible library has already been loaded. */
    fun isLoaded(): Boolean = loadedAbiVersion != null

    /** Installs the validated public identity before Rust can expose a network endpoint. */
    fun setDeviceIdentity(identity: DeviceIdentity): Boolean {
        when (load()) {
            is LoadResult.Loaded -> Unit
            else -> return false
        }
        return runCatching {
            DeviceIdentityPolicy.validateAlias(identity.alias)
            DeviceIdentityPolicy.validateFingerprint(identity.publicKeyFingerprint)
            bridgeIdentityProvider(identity.alias, identity.publicKeyFingerprint) == 0
        }.getOrDefault(false)
    }

    /** Adds or refreshes an explicitly approved peer in Rust's trust registry. */
    fun trustPeer(peer: TrustedPeer): Boolean {
        when (load()) {
            is LoadResult.Loaded -> Unit
            else -> return false
        }
        return runCatching {
            DeviceIdentityPolicy.validateFingerprint(peer.fingerprint)
            require(peer.label.isNotBlank() && peer.label.length <= 128) {
                "trusted peer label is invalid"
            }
            bridgeTrustPeerProvider(peer.deviceId, peer.fingerprint, peer.label) == 0
        }.getOrDefault(false)
    }

    /** Explicitly revokes a peer; Rust rejects unknown records. */
    fun revokePeer(deviceId: String): Boolean =
        isLoaded() && deviceId.isNotBlank() && bridgeRevokePeerProvider(deviceId) == 0

    /** Checks the Rust-owned trust record without exposing registry contents. */
    fun isPeerTrusted(deviceId: String, fingerprint: String): Boolean {
        if (!isLoaded()) return false
        return runCatching {
            DeviceIdentityPolicy.validateFingerprint(fingerprint)
            bridgePeerTrustedProvider(deviceId, fingerprint) == 1
        }.getOrDefault(false)
    }

    /** Exports Rust's bounded public trust registry for encrypted platform storage. */
    fun exportTrustSnapshot(): ByteArray? {
        when (load()) {
            is LoadResult.Loaded -> Unit
            else -> return null
        }
        return bridgeTrustSnapshotExportProvider().takeIf { it != null && it.size <= MAX_TRUST_SNAPSHOT_BYTES }
    }

    /** Imports an encrypted-store-decoded FTR1 snapshot before receiver startup. */
    fun importTrustSnapshot(snapshot: ByteArray): Boolean {
        when (load()) {
            is LoadResult.Loaded -> Unit
            else -> return false
        }
        if (snapshot.isEmpty() || snapshot.size > MAX_TRUST_SNAPSHOT_BYTES) return false
        return bridgeTrustSnapshotImportProvider(snapshot) == 0
    }

    /** Enables or disables the optional one-shot post-PIN approval gate. */
    fun setPairingApprovalRequired(required: Boolean): Boolean {
        when (load()) {
            is LoadResult.Loaded -> Unit
            else -> return false
        }
        return bridgePairingApprovalRequiredProvider(required) == 0
    }

    /** Arms exactly one upcoming pairing when the optional gate is enabled. */
    fun approveNextPairing(): Boolean =
        isLoaded() && bridgePairingApprovalProvider() == 0

    /** Starts the Rust-owned lifecycle after verifying the ABI. */
    fun startReceiver(): ReceiverResult {
        when (val result = load()) {
            is LoadResult.Loaded -> Unit
            is LoadResult.Incompatible ->
                return ReceiverResult.Unavailable("native ABI mismatch")

            is LoadResult.Unavailable -> return ReceiverResult.Unavailable(result.reason)
        }
        return when (val code = bridgeStartProvider()) {
            0 -> ReceiverResult.Started
            1 -> ReceiverResult.AlreadyStarted
            else -> ReceiverResult.Failed(code)
        }
    }

    /** Stops the Rust-owned lifecycle and releases its protocol session. */
    fun stopReceiver(): ReceiverResult {
        if (!isLoaded()) {
            return ReceiverResult.Unavailable("native library is unavailable")
        }
        return when (val code = bridgeStopProvider()) {
            0 -> ReceiverResult.Stopped
            else -> ReceiverResult.Failed(code)
        }
    }

    /** Returns the native lifecycle flag, or false when the bridge is absent. */
    fun isReceiverRunning(): Boolean = isLoaded() && bridgeRunningProvider() == 1

    /** Returns the current ephemeral FANP endpoint and pinned certificate. */
    fun networkInfo(): NetworkInfo? {
        if (!isLoaded()) return null
        val port = bridgeNetworkPortProvider()
        val pairingCode = bridgeNetworkPairingCodeProvider()
        val certificate = bridgeNetworkCertificateProvider() ?: return null
        if (port !in 1..65_535 || pairingCode !in 0..999_999 || certificate.isEmpty()) return null
        return NetworkInfo(port, pairingCode, certificate)
    }

    /** Polls the newest negotiated video configuration, if one is available. */
    fun pollVideoConfig(): VideoConfig? = if (isLoaded()) {
        decodeVideoConfig(bridgeVideoConfigProvider())
    } else {
        null
    }

    /** Polls the newest negotiated audio configuration, if one is available. */
    fun pollAudioConfig(): AudioConfig? = if (isLoaded()) {
        decodeAudioConfig(bridgeAudioConfigProvider())
    } else {
        null
    }

    /** Submits one bounded encoded video access unit to Rust. */
    fun submitVideoFrame(payload: ByteArray, pts: Long, keyframe: Boolean): MediaSubmitResult {
        return submitMedia(payload) { bridgeSubmitVideoProvider(payload, pts, keyframe) }
    }

    /** Submits one bounded encoded audio access unit to Rust. */
    fun submitAudioFrame(payload: ByteArray, pts: Long): MediaSubmitResult {
        return submitMedia(payload) { bridgeSubmitAudioProvider(payload, pts) }
    }

    /** Polls the oldest Rust-owned media frame, or null when the queue is empty. */
    fun pollMediaFrame(): MediaFrame? {
        if (!isLoaded()) return null
        return decodeMediaFrame(bridgePollMediaProvider())
    }

    private fun submitMedia(payload: ByteArray, submit: () -> Int): MediaSubmitResult {
        if (payload.isEmpty() || payload.size > MAX_MEDIA_FRAME_BYTES) {
            return MediaSubmitResult.Failed(-3)
        }
        when (val result = load()) {
            is LoadResult.Loaded -> Unit
            is LoadResult.Incompatible -> return MediaSubmitResult.Unavailable("native ABI mismatch")
            is LoadResult.Unavailable -> return MediaSubmitResult.Unavailable(result.reason)
        }
        return when (val code = submit()) {
            0 -> MediaSubmitResult.Accepted
            1 -> MediaSubmitResult.QueueFull
            else -> MediaSubmitResult.Failed(code)
        }
    }

    private fun decodeMediaFrame(envelope: ByteArray?): MediaFrame? {
        if (envelope == null || envelope.size < MEDIA_FRAME_HEADER_BYTES) return null
        if (!envelope.copyOfRange(0, MEDIA_FRAME_MAGIC.size).contentEquals(MEDIA_FRAME_MAGIC)) return null
        if (envelope[4].toInt() != 1) return null
        val kind = when (envelope[5].toInt()) {
            1 -> MediaKind.VIDEO
            2 -> MediaKind.AUDIO
            else -> return null
        }
        val keyframe = when (envelope[6].toInt()) {
            0 -> false
            1 -> true
            else -> return null
        }
        if (kind == MediaKind.AUDIO && keyframe) return null
        val buffer = ByteBuffer.wrap(envelope).order(ByteOrder.BIG_ENDIAN)
        val pts = buffer.getLong(7)
        val payloadLength = buffer.getInt(15).toLong()
        if (payloadLength <= 0 || payloadLength > MAX_MEDIA_FRAME_BYTES) return null
        val payloadEnd = MEDIA_FRAME_HEADER_BYTES.toLong() + payloadLength
        if (payloadEnd != envelope.size.toLong()) return null
        return MediaFrame(
            kind = kind,
            pts = pts,
            keyframe = keyframe,
            payload = envelope.copyOfRange(MEDIA_FRAME_HEADER_BYTES, payloadEnd.toInt()),
        )
    }

    private fun decodeVideoConfig(envelope: ByteArray?): VideoConfig? {
        if (envelope == null || envelope.size != 24 || !hasConfigHeader(envelope, 1)) return null
        val buffer = ByteBuffer.wrap(envelope).order(ByteOrder.BIG_ENDIAN)
        val codec = decodeCodec(envelope[6].toInt())
        val width = buffer.getInt(MEDIA_CONFIG_HEADER_BYTES)
        val height = buffer.getInt(MEDIA_CONFIG_HEADER_BYTES + 4)
        val numerator = buffer.getInt(MEDIA_CONFIG_HEADER_BYTES + 8)
        val denominator = buffer.getInt(MEDIA_CONFIG_HEADER_BYTES + 12)
        if (width !in 1..7_680 || height !in 1..7_680 ||
            numerator !in 1..240_000 || denominator !in 1..240_000
        ) return null
        return VideoConfig(codec, width, height, numerator, denominator)
    }

    private fun decodeAudioConfig(envelope: ByteArray?): AudioConfig? {
        if (envelope == null || envelope.size != 16 || !hasConfigHeader(envelope, 2)) return null
        val buffer = ByteBuffer.wrap(envelope).order(ByteOrder.BIG_ENDIAN)
        val codec = decodeCodec(envelope[6].toInt())
        val sampleRate = buffer.getInt(MEDIA_CONFIG_HEADER_BYTES)
        val channels = buffer.getInt(MEDIA_CONFIG_HEADER_BYTES + 4)
        if (sampleRate !in 8_000..192_000 || channels !in 1..8) return null
        return AudioConfig(codec, sampleRate, channels)
    }

    private fun hasConfigHeader(envelope: ByteArray, expectedKind: Int): Boolean =
        envelope.copyOfRange(0, MEDIA_CONFIG_MAGIC.size).contentEquals(MEDIA_CONFIG_MAGIC) &&
            envelope[4].toInt() == 1 && envelope[5].toInt() == expectedKind && envelope[7].toInt() == 0

    private fun decodeCodec(value: Int): MediaCodecKind = when (value) {
        1 -> MediaCodecKind.H264
        2 -> MediaCodecKind.H265
        3 -> MediaCodecKind.AAC
        4 -> MediaCodecKind.OPUS
        5 -> MediaCodecKind.PCM
        6 -> MediaCodecKind.ALAC
        else -> MediaCodecKind.UNKNOWN
    }

    private external fun nativeVersion(): Int

    private external fun nativeSetDeviceIdentity(alias: String, fingerprint: String): Int

    private external fun nativeTrustPeer(deviceId: String, fingerprint: String, label: String): Int

    private external fun nativeRevokePeer(deviceId: String): Int

    private external fun nativeIsPeerTrusted(deviceId: String, fingerprint: String): Int

    private external fun nativeExportTrustSnapshot(): ByteArray?

    private external fun nativeImportTrustSnapshot(snapshot: ByteArray): Int

    private external fun nativeSetPairingApprovalRequired(required: Boolean): Int

    private external fun nativeApproveNextPairing(): Int

    private external fun nativeStartReceiver(): Int

    private external fun nativeStopReceiver(): Int

    private external fun nativeReceiverRunning(): Int

    private external fun nativeReceiverPort(): Int

    private external fun nativeReceiverPairingCode(): Int

    private external fun nativeReceiverCertificate(): ByteArray?

    private external fun nativeSubmitVideoFrame(payload: ByteArray, pts: Long, keyframe: Boolean): Int

    private external fun nativeSubmitAudioFrame(payload: ByteArray, pts: Long): Int

    private external fun nativePollMediaFrame(): ByteArray?

    private external fun nativePollVideoConfig(): ByteArray?

    private external fun nativePollAudioConfig(): ByteArray?
}
