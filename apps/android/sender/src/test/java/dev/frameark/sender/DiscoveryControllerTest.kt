package dev.frameark.sender

import org.junit.Assert.*
import org.junit.Test

class DiscoveryControllerTest {
    private class Backend : DiscoveryBackend {
        var next = 1L
        var state = 1
        var values: Array<String>? = arrayOf("candidate")
        val stopped = mutableListOf<Long>()
        override fun discoveryStart() = next++
        override fun discoveryStatus(id: Long) = state
        override fun discoveryTickets(id: Long) = values
        override fun discoveryStop(id: Long) { stopped += id }
    }
    @Test fun completionExpiryCancellationAndRestartReleaseResources() {
        val backend = Backend(); var locks = 0; var view = DiscoveryView(0)
        val ui = DiscoveryController(backend, { locks++ }, { locks-- }, { view = it })
        ui.start(); assertEquals(1, locks); assertTrue(ui.poll()); assertEquals(listOf("candidate"), view.tickets)
        backend.state = 2; assertTrue(ui.poll()); assertEquals(0, locks)
        backend.values = emptyArray(); assertFalse(ui.poll()); assertEquals(2, view.state)
        assertEquals(listOf(1L), backend.stopped)
        ui.start(); assertEquals(1, locks); ui.stop(); ui.stop()
        assertEquals(0, locks); assertEquals(0, view.state); assertFalse(ui.poll())
        assertEquals(listOf(1L, 2L), backend.stopped)
    }
    @Test fun errorsAndPartialAcquisitionClearCandidatesAndRelease() {
        for ((state, values) in listOf(-1 to arrayOf("x"), 0 to arrayOf("x"), 1 to null, 1 to Array(33) { "x" })) {
            val backend = Backend().apply { this.state = state; this.values = values }
            var locks = 0; var view = DiscoveryView(0)
            val ui = DiscoveryController(backend, { locks++ }, { locks-- }, { view = it })
            ui.start(); assertFalse(ui.poll()); assertEquals(-1, view.state)
            assertTrue(view.tickets.isEmpty()); assertEquals(0, locks); assertEquals(listOf(1L), backend.stopped)
        }
        var releases = 0
        val ui = DiscoveryController(Backend(), { throw SecurityException() }, { releases++ }, {})
        ui.start(); assertEquals(1, releases); assertFalse(ui.poll()); ui.stop(); assertEquals(1, releases)
    }
    @Test fun selectionNeverProvidesAuthorizationAndChangesInvalidateOldApproval() {
        val gate = SubmissionGate()
        gate.inspect("old", true)
        assertTrue(gate.permits("old", "123456", true))
        gate.inspect("discovered", true)
        assertFalse(gate.permits("discovered", "123456", false))
        assertFalse(gate.permits("old", "123456", true))
        assertFalse(gate.permits("discovered", "", true))
    }
}
