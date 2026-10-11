package dev.frameark.receiver

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ReceiverSessionCoordinatorTest {
    @Test
    fun startsNativeAndPlaybackExactlyOnce() {
        val calls = Calls()
        val native = fakeNative(calls)
        val loop = FakePlayback(calls, startResult = true)
        val states = mutableListOf<ReceiverSessionCoordinator.State>()
        val coordinator = ReceiverSessionCoordinator(native, loop, states::add)

        assertTrue(coordinator.start())
        assertTrue(coordinator.start())
        assertEquals(ReceiverSessionCoordinator.State.Running, coordinator.state())
        assertEquals(listOf("native-start", "loop-start"), calls.events)
        assertEquals(
            listOf(
                ReceiverSessionCoordinator.State.Starting,
                ReceiverSessionCoordinator.State.Running,
            ),
            states,
        )
    }

    @Test
    fun playbackFailureStopsNativeAndReportsFailure() {
        val calls = Calls()
        val coordinator = ReceiverSessionCoordinator(
            fakeNative(calls),
            FakePlayback(calls, startResult = false),
        )

        assertFalse(coordinator.start())
        assertEquals(
            ReceiverSessionCoordinator.State.Failed(
                ReceiverSessionCoordinator.FailureReason.PLAYBACK_LOOP_UNAVAILABLE,
            ),
            coordinator.state(),
        )
        assertEquals(listOf("native-start", "loop-start", "native-stop"), calls.events)
    }

    @Test
    fun network_endpoint_failure_stops_native_before_playback() {
        val calls = Calls()
        val native = fakeNativeWithNetwork(
            calls,
            port = 0,
            pairingCode = 123_456,
            certificate = byteArrayOf(1),
        )
        val network = ReceiverNetworkCoordinator(native)
        val coordinator = ReceiverSessionCoordinator(
            native,
            FakePlayback(calls, startResult = true),
            network = network,
        )

        assertFalse(coordinator.start())
        assertEquals(
            ReceiverSessionCoordinator.State.Failed(
                ReceiverSessionCoordinator.FailureReason.NETWORK_ENDPOINT_UNAVAILABLE,
            ),
            coordinator.state(),
        )
        assertEquals(ReceiverNetworkCoordinator.State.Stopped, network.state())
        assertEquals(listOf("native-start", "native-stop"), calls.events)
    }

    @Test
    fun stopAndCloseAreIdempotentAndReleaseBothOwners() {
        val calls = Calls()
        val coordinator = ReceiverSessionCoordinator(
            fakeNative(calls),
            FakePlayback(calls, startResult = true),
        )

        assertTrue(coordinator.start())
        coordinator.stop()
        coordinator.stop()
        coordinator.close()
        assertEquals(ReceiverSessionCoordinator.State.Stopped, coordinator.state())
        assertEquals(
            listOf(
                "native-start",
                "loop-start",
                "loop-stop",
                "native-stop",
                "loop-stop",
                "native-stop",
                "loop-close",
                "native-stop",
            ),
            calls.events,
        )
    }

    private fun fakeNative(calls: Calls): FrameArkNative = FrameArkNative(
        libraryLoader = {},
        versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
        startProvider = { calls.events += "native-start"; 0 },
        stopProvider = { calls.events += "native-stop"; 0 },
        runningProvider = { 1 },
    )

    private fun fakeNativeWithNetwork(
        calls: Calls,
        port: Int,
        pairingCode: Int,
        certificate: ByteArray,
    ): FrameArkNative = FrameArkNative(
        libraryLoader = {},
        versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
        startProvider = { calls.events += "native-start"; 0 },
        stopProvider = { calls.events += "native-stop"; 0 },
        runningProvider = { 1 },
        networkPortProvider = { port },
        networkPairingCodeProvider = { pairingCode },
        networkCertificateProvider = { certificate },
    )

    private class FakePlayback(
        private val calls: Calls,
        private val startResult: Boolean,
    ) : PlaybackController {
        override fun start(): Boolean {
            calls.events += "loop-start"
            return startResult
        }

        override fun stop() {
            calls.events += "loop-stop"
        }

        override fun close() {
            calls.events += "loop-close"
        }
    }

    private class Calls {
        val events = mutableListOf<String>()
    }
}
