package dev.frameark.receiver

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
) {
    companion object {
        const val LIBRARY_NAME = "frameark_ffi"
        const val EXPECTED_ABI_VERSION = 1
    }

    private val bridgeVersionProvider: () -> Int = versionProvider ?: { nativeVersion() }
    private val bridgeStartProvider: () -> Int = startProvider ?: { nativeStartReceiver() }
    private val bridgeStopProvider: () -> Int = stopProvider ?: { nativeStopReceiver() }
    private val bridgeRunningProvider: () -> Int = runningProvider ?: { nativeReceiverRunning() }
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

    private external fun nativeVersion(): Int

    private external fun nativeStartReceiver(): Int

    private external fun nativeStopReceiver(): Int

    private external fun nativeReceiverRunning(): Int
}
