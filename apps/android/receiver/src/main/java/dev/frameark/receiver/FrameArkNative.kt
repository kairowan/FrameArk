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
) {
    companion object {
        const val LIBRARY_NAME = "frameark_ffi"
        const val EXPECTED_ABI_VERSION = 1
    }

    private val bridgeVersionProvider: () -> Int = versionProvider ?: { nativeVersion() }
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

    private external fun nativeVersion(): Int
}
