package dev.frameark.receiver

import java.nio.ByteBuffer
import org.junit.Assert.*
import org.junit.Test

class PlaybackEventTest {
    private fun event(kind: Int, body: ByteArray = byteArrayOf()) =
        byteArrayOf(70, 65, 80, 69, 1, kind.toByte()) + body

    private fun frame(kind: Int) = ByteBuffer.allocate(20)
        .put("FAMF".toByteArray()).put(1).put(kind.toByte()).put(0)
        .putLong(20_000L).putInt(1).put(7).array()

    private fun native(provider: () -> ByteArray?) = FrameArkNative(
        libraryLoader = {}, versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
        playbackEventProvider = provider,
        pollMediaProvider = { error("legacy queue must not be polled") },
        videoConfigProvider = { error("legacy config must not be polled") },
        audioConfigProvider = { error("legacy config must not be polled") },
    )

    @Test fun ordered_events_drain_outputs_even_after_queue_becomes_empty() {
        val video = ByteBuffer.allocate(24).put("FAMC".toByteArray())
            .put(1).put(1).put(1).put(0).putInt(320).putInt(240).putInt(30).putInt(1).array()
        val audio = ByteBuffer.allocate(16).put("FAMC".toByteArray())
            .put(1).put(2).put(3).put(0).putInt(48_000).putInt(2).array()
        val queue = ArrayDeque(listOf(event(5), event(1, video), event(2, audio), event(3, frame(1)), event(4, frame(2)), event(5)))
        val seen = mutableListOf<FrameArkNative.PlaybackEvent>()
        var drains = 0
        var resets = 0
        val pump = MediaPlaybackPump(native { queue.removeFirstOrNull() }, {}, {},
            eventConsumer = { seen += it; true }, drainOutput = { drains++ }, resetOutput = { resets++ })
        assertTrue(pump.start())
        assertEquals(6, pump.drainOnce())
        assertEquals(0, pump.drainOnce())
        assertEquals(2, drains)
        assertEquals(FrameArkNative.PlaybackEvent.Reset, seen.first())
        assertTrue(seen[1] is FrameArkNative.PlaybackEvent.Video)
        assertTrue(seen[2] is FrameArkNative.PlaybackEvent.Audio)
        assertEquals(20_000L, (seen[3] as FrameArkNative.PlaybackEvent.Frame).frame.pts)
        assertEquals(FrameArkNative.MediaKind.AUDIO, (seen[4] as FrameArkNative.PlaybackEvent.Frame).frame.kind)
        assertEquals(FrameArkNative.PlaybackEvent.Reset, seen.last())
        pump.stop()
        assertEquals(1, resets)
        assertEquals(0, pump.drainOnce())
        assertEquals(2, drains)
    }

    @Test fun malformed_events_fail_closed() {
        val invalid = listOf(byteArrayOf(), event(5) + 0, event(5).apply { this[4] = 2 },
            event(8), event(3, frame(2)), event(1), event(4, frame(1)),
            ByteArray(FrameArkNative.MAX_MEDIA_FRAME_BYTES + 26))
        for (bytes in invalid) {
            val bridge = native { bytes }
            bridge.load()
            assertThrows(RuntimeException::class.java) { bridge.pollPlaybackEvent() }
        }
    }

    @Test fun temporary_backpressure_retains_one_exact_event_and_prevents_overtaking() {
        val queue = ArrayDeque(listOf(event(3, frame(1)), event(4, frame(2)), event(5)))
        var polls = 0
        var drains = 0
        var blocked = true
        val attempts = mutableListOf<FrameArkNative.PlaybackEvent>()
        val accepted = mutableListOf<FrameArkNative.PlaybackEvent>()
        val pump = MediaPlaybackPump(native { polls++; queue.removeFirstOrNull() }, {}, {},
            eventConsumer = {
                attempts += it
                if (blocked) false else { accepted += it; true }
            }, drainOutput = { drains++ })
        assertTrue(pump.start())
        repeat(2) { assertEquals(0, pump.drainOnce()) }
        assertEquals(1, polls)
        assertEquals(2, drains)
        assertSame(attempts[0], attempts[1])
        assertTrue(accepted.isEmpty())
        blocked = false
        assertEquals(3, pump.drainOnce())
        assertSame(attempts[0], accepted[0])
        assertEquals(20_000L, (accepted[0] as FrameArkNative.PlaybackEvent.Frame).frame.pts)
        assertArrayEquals(byteArrayOf(7), (accepted[0] as FrameArkNative.PlaybackEvent.Frame).frame.payload)
        assertEquals(FrameArkNative.MediaKind.AUDIO, (accepted[1] as FrameArkNative.PlaybackEvent.Frame).frame.kind)
        assertEquals(FrameArkNative.PlaybackEvent.Reset, accepted[2])
        assertEquals(3, drains)
        pump.stop()
    }

    @Test fun input_deadline_is_absolute_and_stop_discards_unaccepted_frame() {
        var now = 0L
        var attempts = 0
        var resets = 0
        val queue = ArrayDeque(listOf(event(3, frame(1)), event(5)))
        val pump = MediaPlaybackPump(native { queue.removeFirstOrNull() }, {}, {},
            eventConsumer = { attempts++; it is FrameArkNative.PlaybackEvent.Reset },
            resetOutput = { resets++ }, nanoTime = { now })
        assertTrue(pump.start())
        assertEquals(0, pump.drainOnce())
        now = MediaPlaybackPump.INPUT_RETRY_BUDGET_NS - 1
        assertEquals(0, pump.drainOnce())
        now++
        assertThrows(IllegalStateException::class.java) { pump.drainOnce() }
        assertEquals(2, attempts)
        pump.stop()
        assertEquals(1, resets)
        assertEquals(0, pump.drainOnce())
        assertTrue(pump.start())
        assertEquals(1, pump.drainOnce()) // Reset, never the expired frame.
        assertEquals(3, attempts)
        pump.stop()
    }

    @Test fun accepted_frame_releases_its_retry_budget_for_the_next_frame() {
        var now = 0L
        var acceptFirst = false
        var first: FrameArkNative.PlaybackEvent? = null
        val queue = ArrayDeque(listOf(event(3, frame(1)), event(4, frame(2))))
        val pump = MediaPlaybackPump(native { queue.removeFirstOrNull() }, {}, {},
            eventConsumer = { event ->
                if (first == null) first = event
                event === first && acceptFirst
            }, nanoTime = { now })
        pump.start()
        assertEquals(0, pump.drainOnce())
        now = MediaPlaybackPump.INPUT_RETRY_BUDGET_NS - 1
        acceptFirst = true
        assertEquals(1, pump.drainOnce())
        now += MediaPlaybackPump.INPUT_RETRY_BUDGET_NS - 1
        assertEquals(0, pump.drainOnce())
        now++
        assertThrows(IllegalStateException::class.java) { pump.drainOnce() }
        pump.stop()
    }

    @Test fun consumer_exception_is_not_reclassified_as_retryable_backpressure() {
        var resets = 0
        var calls = 0
        val pump = MediaPlaybackPump(native { event(3, frame(1)) }, {}, {},
            eventConsumer = { calls++; error("decoder failure") }, resetOutput = { resets++ })
        pump.start()
        assertThrows(IllegalStateException::class.java) { pump.drainOnce() }
        pump.stop()
        assertEquals(1, calls)
        assertEquals(1, resets)
        assertEquals(0, pump.drainOnce())
    }
}
