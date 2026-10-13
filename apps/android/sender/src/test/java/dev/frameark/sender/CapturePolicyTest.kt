package dev.frameark.sender

import org.junit.Assert.*
import org.junit.Test

class CapturePolicyTest {
    @Test fun staleServiceCannotReplaceCurrentDiagnostics() {
        val old = CaptureStatus.begin()
        val current = CaptureStatus.begin()
        CaptureStatus.end(old, "old failure")
        CaptureStatus.publish(old, CaptureStatus.Snapshot(phase = "stale"))
        assertTrue(CaptureStatus.snapshot.active)
        CaptureStatus.publish(current, CaptureStatus.Snapshot(true, "sharing", 25))
        CaptureStatus.end(current, "")
        assertEquals(25, CaptureStatus.snapshot.video)
        assertFalse(CaptureStatus.snapshot.active)
    }
    @Test fun geometryFitsBothOrientationsWithoutUpscaling() {
        assertEquals(CaptureOptions(1920, 1080, 60, false), CaptureOptions.fit(3840, 2160, true, 60, false))
        assertEquals(CaptureOptions(1080, 1920, 30, true), CaptureOptions.fit(1080, 1920, true, 30, true))
        assertEquals(CaptureOptions(720, 1280, 30, false), CaptureOptions.fit(1080, 1920, false, 30, false))
        assertEquals(CaptureOptions(640, 480, 30, false), CaptureOptions.fit(641, 481, true, 30, false))
    }
    @Test fun invalidCaptureBoundsFailBeforeGrant() {
        for (action in listOf<() -> Unit>(
            { CaptureOptions(1920, 1920, 30, false) }, { CaptureOptions(1081, 1920, 30, false) },
            { CaptureOptions(1280, 720, 120, false) }, { CaptureOptions(1280, 720, 30, false, 3601) },
            { CaptureOptions.fit(0, 1920, true, 30, false) }, { CaptureOptions.fit(16, 16384, false, 30, false) },
        )) { try { action(); fail("invalid bounds accepted") } catch (_: IllegalArgumentException) {} }
    }
    @Test fun handoffIsSingleUseExpiringAndRedacted() {
        var now = 1L
        val handoff = CaptureHandoff { now }
        val request = CaptureRequest("secret-ticket", "123456", CaptureOptions(1280, 720, 30, false))
        assertFalse(request.toString().contains("secret-ticket"))
        assertFalse(request.toString().contains("123456"))
        val old = handoff.put(request)
        val id = handoff.put(request)
        assertNull(handoff.take(old))
        assertSame(request, handoff.take(id))
        assertNull(handoff.take(id))
        val expired = handoff.put(request)
        now += 15_000_000_001L
        assertNull(handoff.take(expired))
        val discarded = handoff.put(request)
        handoff.discard(discarded)
        assertNull(handoff.take(discarded))
        val backwards = handoff.put(request)
        now--
        assertNull(handoff.take(backwards))
    }
}
