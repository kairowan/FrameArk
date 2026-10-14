@file:Suppress("DEPRECATION")

package dev.frameark.receiver

import android.test.InstrumentationTestCase

/** Real JNI listener with injected notifications; not a Wi-Fi/decoder test. */
class ReceiverNetworkContinuityTest : InstrumentationTestCase() {
    fun testRouteNotificationsKeepRealListenerAndExplicitStopStillReleasesIt() {
        val native = FrameArkNative()
        assertTrue(native.load() is FrameArkNative.LoadResult.Loaded)
        assertFalse("isolated test starts without an active receiver", native.isReceiverRunning())
        assertTrue(native.setDeviceIdentity(AndroidDeviceIdentityStore().getOrCreate()))
        val playbackCalls = mutableListOf<String>()
        val loop = object : PlaybackController {
            override fun start(): Boolean { playbackCalls += "start"; return true }
            override fun stop() { playbackCalls += "stop" }
            override fun close() { playbackCalls += "close" }
        }
        val network = ReceiverNetworkCoordinator(native)
        val coordinator = ReceiverSessionCoordinator(native, loop, network = network)
        var listener: ReceiverNetworkObserver.Listener? = null
        var changes = 0
        val observer = ReceiverNetworkObserver(
            register = { listener = it },
            unregister = {},
            onNetworkChanged = { assertTrue(coordinator.onNetworkChanged()); changes++ },
        )
        try {
            assertTrue(coordinator.start())
            val before = requireNotNull(native.networkInfo())
            assertTrue(observer.start())
            repeat(20) {
                listener!!.onLost()
                listener!!.onAvailable() // Coalesced while the handler is pending.
                observer.markChangeHandled()
                listener!!.onAvailable()
                observer.markChangeHandled()
            }
            assertEquals(40, changes)
            val after = requireNotNull(native.networkInfo())
            assertEquals(before.port, after.port)
            assertEquals(before.pairingCode, after.pairingCode)
            assertTrue(before.certificateDer.contentEquals(after.certificateDer))
            assertEquals(listOf("start"), playbackCalls)
            assertTrue(native.isReceiverRunning())

            observer.stop()
            coordinator.stop()
            listener!!.onAvailable() // A stale platform callback must not restart.
            assertFalse(coordinator.onNetworkChanged())
            assertFalse(native.isReceiverRunning())
            assertNull(native.networkInfo())
            assertEquals(40, changes)
            assertEquals(listOf("start", "stop"), playbackCalls)

            assertTrue(coordinator.start())
            val restarted = requireNotNull(native.networkInfo())
            assertFalse("only explicit restart rotates the ephemeral certificate",
                before.certificateDer.contentEquals(restarted.certificateDer))
        } finally {
            observer.close()
            coordinator.close()
        }
        assertFalse(native.isReceiverRunning())
    }
}
