package dev.frameark.receiver

import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class VerifiedPeerProofCoordinatorTest {
    private fun envelope(requestId: Long): ByteArray {
        val id = "sender-proof"
        val key = ByteArray(32) { it.toByte() }
        return ByteBuffer.allocate(14 + id.length + key.size)
            .order(ByteOrder.BIG_ENDIAN)
            .put(byteArrayOf('F'.code.toByte(), 'A'.code.toByte(), 'P'.code.toByte(), 'R'.code.toByte()))
            .put(1)
            .putLong(requestId)
            .put(id.length.toByte())
            .put(id.toByteArray())
            .put(key)
            .array()
    }

    private inner class Fixture {
        val events = ArrayDeque<ByteArray>()
        val pending = mutableSetOf<Long>()
        val decisions = mutableListOf<Pair<Long, Boolean>>()
        var polls = 0
        val native = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            pollPeerProofProvider = { polls++; events.removeFirstOrNull() },
            peerProofPendingProvider = { if (it in pending) 1 else 0 },
            peerProofDecisionProvider = { requestId, approved ->
                decisions += requestId to approved
                if (pending.remove(requestId)) 0 else -2
            },
            trustPeerPublicKeyProvider = { _, _, _ -> error("session approval must not enroll") },
            trustPeerProvider = { _, _, _ -> error("session approval must not enroll") },
        )
        init { native.load() }
        val coordinator = VerifiedPeerProofCoordinator(native)
        fun enqueue(id: Long) { events.addLast(envelope(id)); pending.add(id) }
    }

    @Test
    fun renders_fingerprint_and_forwards_only_explicit_one_session_decision() {
        val fixture = Fixture().apply { enqueue(7) }
        val prompt = fixture.coordinator.refresh()!!
        assertEquals("sender-proof", prompt.deviceId)
        assertEquals(DeviceIdentityPolicy.fingerprint(ByteArray(32) { it.toByte() }), prompt.fingerprint)
        assertTrue(fixture.decisions.isEmpty())
        assertEquals(prompt, fixture.coordinator.refresh())
        assertEquals(1, fixture.polls)
        assertTrue(fixture.coordinator.decide(prompt.requestId, approved = true))
        assertEquals(listOf(7L to true), fixture.decisions)
        assertFalse(fixture.coordinator.decide(prompt.requestId, approved = true))
        assertNull(fixture.coordinator.refresh())
    }

    @Test
    fun removes_expired_prompt_and_does_not_block_new_requests() {
        val fixture = Fixture().apply { enqueue(7) }
        fixture.coordinator.refresh()
        fixture.pending.clear() // Rust timeout, stop or cancellation.
        assertNull(fixture.coordinator.refresh())
        fixture.enqueue(8)
        assertEquals(8L, fixture.coordinator.refresh()?.requestId)
        assertFalse(fixture.coordinator.decide(7, approved = true))
        assertTrue(fixture.decisions.isEmpty())
        assertTrue(fixture.coordinator.decide(8, approved = false))
        assertEquals(listOf(8L to false), fixture.decisions)
    }

    @Test
    fun rechecks_expiry_at_click_and_clears_the_prompt_after_failure() {
        val fixture = Fixture().apply { enqueue(7) }
        fixture.coordinator.refresh()
        fixture.pending.clear()
        assertFalse(fixture.coordinator.decide(7, approved = true))
        assertFalse(fixture.coordinator.decide(7, approved = true))
        assertEquals(listOf(7L to true), fixture.decisions)
        assertNull(fixture.coordinator.refresh())
    }

    @Test
    fun rejects_displayed_request_once_on_pause_without_enrolling() {
        val fixture = Fixture().apply { enqueue(7); enqueue(8) }
        fixture.coordinator.refresh()
        fixture.coordinator.dismiss()
        fixture.coordinator.dismiss()
        assertEquals(listOf(7L to false), fixture.decisions)
        assertFalse(fixture.coordinator.decide(7, approved = true))
        assertEquals(8L, VerifiedPeerProofCoordinator(fixture.native).refresh()?.requestId)
    }

    @Test
    fun does_not_display_events_that_expired_before_poll() {
        val fixture = Fixture().apply { enqueue(7); pending.clear() }
        assertNull(fixture.coordinator.refresh())
        assertFalse(fixture.coordinator.decide(7, approved = true))
        assertTrue(fixture.decisions.isEmpty())
    }

    @Test
    fun unavailable_bridge_does_not_poll_or_authorize() {
        val native = FrameArkNative(libraryLoader = { throw UnsatisfiedLinkError("unavailable") })
        native.load()
        val coordinator = VerifiedPeerProofCoordinator(native)
        assertNull(coordinator.refresh())
        assertFalse(coordinator.decide(7, approved = true))
        coordinator.dismiss()
    }
}
