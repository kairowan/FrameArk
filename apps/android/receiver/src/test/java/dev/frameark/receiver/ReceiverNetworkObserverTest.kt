package dev.frameark.receiver

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ReceiverNetworkObserverTest {
    @Test
    fun coalesces_network_events_until_recovery_completes() {
        var registered: ReceiverNetworkObserver.Listener? = null
        var changes = 0
        val observer = ReceiverNetworkObserver(
            register = { registered = it },
            unregister = { registered = null },
            onNetworkChanged = { changes += 1 },
        )

        assertTrue(observer.start())
        assertTrue(observer.start())
        registered?.onLost()
        registered?.onAvailable()
        registered?.onCapabilitiesChanged()
        assertEquals(1, changes)

        observer.markRecoveryComplete()
        registered?.onLinkPropertiesChanged()
        assertEquals(2, changes)
    }

    @Test
    fun stop_unregisters_and_ignores_late_callbacks() {
        var listener: ReceiverNetworkObserver.Listener? = null
        var unregisters = 0
        var changes = 0
        val observer = ReceiverNetworkObserver(
            register = { listener = it },
            unregister = { unregisters += 1 },
            onNetworkChanged = { changes += 1 },
        )

        assertTrue(observer.start())
        observer.stop()
        listener?.onAvailable()
        assertEquals(1, unregisters)
        assertEquals(0, changes)
        assertTrue(observer.start())
    }

    @Test
    fun registration_failure_does_not_mark_observer_started() {
        var attempts = 0
        var changes = 0
        val observer = ReceiverNetworkObserver(
            register = { attempts += 1; error("permission unavailable") },
            unregister = {},
            onNetworkChanged = { changes += 1 },
        )

        assertFalse(observer.start())
        assertFalse(observer.start())
        assertEquals(2, attempts)
        assertEquals(0, changes)
    }

    @Test
    fun callback_failure_releases_the_debounce_for_a_later_attempt() {
        var listener: ReceiverNetworkObserver.Listener? = null
        var attempts = 0
        val observer = ReceiverNetworkObserver(
            register = { listener = it },
            unregister = {},
            onNetworkChanged = {
                attempts += 1
                error("recovery failed")
            },
        )

        assertTrue(observer.start())
        listener?.onLost()
        listener?.onAvailable()
        assertEquals(2, attempts)
    }
}
