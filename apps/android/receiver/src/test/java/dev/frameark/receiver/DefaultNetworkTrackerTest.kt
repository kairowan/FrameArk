package dev.frameark.receiver

import org.junit.Assert.*
import org.junit.Test

class DefaultNetworkTrackerTest {
    @Test fun initial_callback_does_not_restart_a_new_listener() {
        val tracker = DefaultNetworkTracker("wifi")
        repeat(3) { assertFalse(tracker.available("wifi")) }
        assertTrue(tracker.available("mobile"))
        assertFalse(tracker.lost("wifi"))
        assertTrue(tracker.lost("mobile"))
        assertFalse(tracker.lost("mobile"))
        assertTrue(tracker.available("wifi"))
    }

    @Test fun first_network_after_offline_is_a_change() {
        val tracker = DefaultNetworkTracker<String>(null)
        assertFalse(tracker.lost("old"))
        assertTrue(tracker.available("wifi"))
    }
}
