package dev.frameark.receiver

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class FrameArkNativeNetworkInfoTest {
    @Test
    fun network_info_is_unavailable_before_the_native_receiver_starts() {
        val native = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            startProvider = { 0 },
            stopProvider = { 0 },
            runningProvider = { 0 },
        )
        assertNull(native.networkInfo())
    }

    @Test
    fun network_info_exposes_bounded_pairing_material_after_load() {
        val certificate = byteArrayOf(1, 2, 3)
        val native = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            startProvider = { 0 },
            stopProvider = { 0 },
            runningProvider = { 1 },
            networkPortProvider = { 42_424 },
            networkPairingCodeProvider = { 123_456 },
            networkCertificateProvider = { certificate },
        )

        native.load()

        assertEquals(
            FrameArkNative.NetworkInfo(42_424, 123_456, certificate),
            native.networkInfo(),
        )
    }
}
