package dev.frameark.receiver

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FrameArkNativeTest {
    @Test
    fun reports_a_loaded_matching_abi() {
        var requestedLibrary = ""
        val bridge = FrameArkNative(
            libraryLoader = { requestedLibrary = it },
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
        )

        assertEquals(
            FrameArkNative.LoadResult.Loaded(FrameArkNative.EXPECTED_ABI_VERSION),
            bridge.load(),
        )
        assertEquals(FrameArkNative.LIBRARY_NAME, requestedLibrary)
        assertTrue(bridge.isLoaded())
    }

    @Test
    fun reports_an_incompatible_abi_without_marking_loaded() {
        val bridge = FrameArkNative(libraryLoader = {}, versionProvider = { 99 })

        assertEquals(
            FrameArkNative.LoadResult.Incompatible(
                FrameArkNative.EXPECTED_ABI_VERSION,
                99,
            ),
            bridge.load(),
        )
        assertFalse(bridge.isLoaded())
    }

    @Test
    fun reports_missing_native_library_without_crashing() {
        val bridge = FrameArkNative(libraryLoader = { throw UnsatisfiedLinkError("missing") })

        assertEquals(
            FrameArkNative.LoadResult.Unavailable("native library is unavailable"),
            bridge.load(),
        )
        assertFalse(bridge.isLoaded())
    }
}
