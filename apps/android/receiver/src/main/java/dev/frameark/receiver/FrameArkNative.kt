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
) {
    companion object {
        const val LIBRARY_NAME = "frameark_ffi"
        const val EXPECTED_ABI_VERSION = 1
        const val MAX_MEDIA_FRAME_BYTES = 4 * 1024 * 1024
        private const val MEDIA_FRAME_HEADER_BYTES = 19
        private val MEDIA_FRAME_MAGIC = byteArrayOf('F'.code.toByte(), 'A'.code.toByte(), 'M'.code.toByte(), 'F'.code.toByte())
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

    /** One bounded encoded frame delivered from Rust to an Android renderer. */
    data class MediaFrame(
        val kind: MediaKind,
        val pts: Long,
        val keyframe: Boolean,
        val payload: ByteArray,
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

    private external fun nativeVersion(): Int

    private external fun nativeStartReceiver(): Int

    private external fun nativeStopReceiver(): Int

    private external fun nativeReceiverRunning(): Int

    private external fun nativeSubmitVideoFrame(payload: ByteArray, pts: Long, keyframe: Boolean): Int

    private external fun nativeSubmitAudioFrame(payload: ByteArray, pts: Long): Int

    private external fun nativePollMediaFrame(): ByteArray?
}
