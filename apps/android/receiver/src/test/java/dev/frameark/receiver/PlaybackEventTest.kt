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
            eventConsumer = { seen += it }, drainOutput = { drains++ }, resetOutput = { resets++ })
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
}
