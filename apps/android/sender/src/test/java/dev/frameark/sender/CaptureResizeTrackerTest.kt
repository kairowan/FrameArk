package dev.frameark.sender

import org.junit.Assert.*
import org.junit.Test

class CaptureResizeTrackerTest {
    @Test fun coalescesStormAndPreservesPresetAudioFpsAndDeadline() {
        var now = 1L
        val tracker = CaptureResizeTracker { now }
        val current = CaptureOptions.fit(640, 480, true, 60, true).copy(seconds = 90)
        repeat(1000) { assertTrue(tracker.update(1920, 1080)); now += 1_000_000 }
        // Duplicate callbacks do not postpone a settled update indefinitely.
        val next = tracker.take(current)!!
        assertEquals(CaptureOptions(1920, 1080, 60, true, 90, true), next)
        assertNull(tracker.take(next))
        tracker.update(1920, 1080)
        now += 249_000_000
        tracker.update(1080, 1920)
        now += 249_000_000
        assertNull(tracker.take(next))
        now += 1_000_000
        assertEquals(CaptureOptions(1080, 1920, 60, true, 90, true), tracker.take(next))
    }

    @Test fun unchangedInvalidAndBackwardsClockDoNotRebuild() {
        var now = 100L
        val tracker = CaptureResizeTracker { now }
        val current = CaptureOptions(720, 1280, 30, false)
        assertFalse(tracker.update(0, 1920))
        assertFalse(tracker.update(16385, 1920))
        assertNull(tracker.take(current))
        tracker.update(1080, 1920)
        now--
        assertNull(tracker.take(current))
        now += 250_000_001L
        assertNull(tracker.take(current)) // Same 720p encoded geometry.
        tracker.update(1920, 1080)
        now += 250_000_000L
        assertEquals(CaptureOptions(1280, 720, 30, false), tracker.take(current))
    }
}
