package dev.frameark.receiver

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ReceiverMulticastLockCoordinatorTest {
    @Test
    fun acquires_once_and_releases_once_across_repeated_lifecycle_calls() {
        var acquisitions = 0
        var releases = 0
        val coordinator = ReceiverMulticastLockCoordinator(
            acquireLock = { acquisitions += 1 },
            releaseLock = { releases += 1 },
        )

        assertTrue(coordinator.acquire())
        assertTrue(coordinator.acquire())
        assertTrue(coordinator.isHeld())
        coordinator.release()
        coordinator.release()

        assertFalse(coordinator.isHeld())
        assertTrue(acquisitions == 1)
        assertTrue(releases == 1)
    }

    @Test
    fun treats_platform_acquisition_failure_as_best_effort() {
        val coordinator = ReceiverMulticastLockCoordinator(
            acquireLock = { error("multicast unavailable") },
            releaseLock = { error("release must not run") },
        )

        assertFalse(coordinator.acquire())
        assertFalse(coordinator.isHeld())
    }
}
