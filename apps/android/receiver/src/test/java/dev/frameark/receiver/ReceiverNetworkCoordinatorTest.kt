package dev.frameark.receiver

import org.junit.Assert.assertEquals
import org.junit.Test

class ReceiverNetworkCoordinatorTest {
    @Test
    fun snapshots_endpoint_after_native_load() {
        val certificate = byteArrayOf(7, 8, 9)
        val native = native(
            networkPortProvider = { 42_424 },
            networkPairingCodeProvider = { 123_456 },
            networkCertificateProvider = { certificate },
        )
        native.load()
        val coordinator = ReceiverNetworkCoordinator(native)

        assertEquals(
            ReceiverNetworkCoordinator.State.Ready(
                FrameArkNative.NetworkInfo(42_424, 123_456, certificate),
            ),
            coordinator.refresh(),
        )
    }

    @Test
    fun rejects_missing_or_invalid_endpoint_without_exposing_native_errors() {
        val native = native(
            networkPortProvider = { 0 },
            networkPairingCodeProvider = { 1_000_000 },
            networkCertificateProvider = { byteArrayOf(1) },
        )
        native.load()
        val coordinator = ReceiverNetworkCoordinator(native)

        assertEquals(
            ReceiverNetworkCoordinator.State.Failed(
                ReceiverNetworkCoordinator.FailureReason.ENDPOINT_UNAVAILABLE,
            ),
            coordinator.refresh(),
        )
    }

    @Test
    fun stop_clears_endpoint_material() {
        val native = native(
            networkPortProvider = { 42_424 },
            networkPairingCodeProvider = { 123_456 },
            networkCertificateProvider = { byteArrayOf(1) },
        )
        native.load()
        val coordinator = ReceiverNetworkCoordinator(native)
        coordinator.refresh()

        coordinator.stop()

        assertEquals(ReceiverNetworkCoordinator.State.Stopped, coordinator.state())
    }

    private fun native(
        networkPortProvider: () -> Int,
        networkPairingCodeProvider: () -> Int,
        networkCertificateProvider: () -> ByteArray?,
    ): FrameArkNative = FrameArkNative(
        libraryLoader = {},
        versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
        startProvider = { 0 },
        stopProvider = { 0 },
        runningProvider = { 1 },
        networkPortProvider = networkPortProvider,
        networkPairingCodeProvider = networkPairingCodeProvider,
        networkCertificateProvider = networkCertificateProvider,
    )
}
