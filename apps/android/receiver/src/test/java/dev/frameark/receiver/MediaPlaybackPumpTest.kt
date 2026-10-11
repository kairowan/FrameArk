package dev.frameark.receiver

import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MediaPlaybackPumpTest {
    private fun frame(kind: FrameArkNative.MediaKind, pts: Long): FrameArkNative.MediaFrame =
        FrameArkNative.MediaFrame(kind, pts, kind == FrameArkNative.MediaKind.VIDEO, byteArrayOf(1))

    private fun envelope(frame: FrameArkNative.MediaFrame): ByteArray = ByteBuffer.allocate(20)
        .order(ByteOrder.BIG_ENDIAN)
        .put("FAMF".toByteArray())
        .put(1)
        .put(if (frame.kind == FrameArkNative.MediaKind.VIDEO) 1 else 2)
        .put(if (frame.keyframe) 1 else 0)
        .putLong(frame.pts)
        .putInt(frame.payload.size)
        .put(frame.payload)
        .array()

    @Test
    fun drains_frames_in_native_order_and_routes_by_kind() {
        val frames = ArrayDeque(
            listOf(
                frame(FrameArkNative.MediaKind.VIDEO, 1),
                frame(FrameArkNative.MediaKind.AUDIO, 2),
            ),
        )
        val video = mutableListOf<Long>()
        val audio = mutableListOf<Long>()
        val native = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            pollMediaProvider = { frames.removeFirstOrNull()?.let(::envelope) },
            videoConfigProvider = { null },
            audioConfigProvider = { null },
        )
        val pump = MediaPlaybackPump(native, { video += it.pts }, { audio += it.pts })

        assertTrue(pump.start())
        assertEquals(2, pump.drainOnce())
        assertEquals(listOf(1L), video)
        assertEquals(listOf(2L), audio)
        assertEquals(0, pump.drainOnce())
    }

    @Test
    fun stop_and_native_unavailability_prevent_polling() {
        var polls = 0
        val native = FrameArkNative(
            libraryLoader = { throw UnsatisfiedLinkError("missing") },
            pollMediaProvider = { polls += 1; envelope(frame(FrameArkNative.MediaKind.VIDEO, 1)) },
        )
        val pump = MediaPlaybackPump(native, {}, {})

        assertFalse(pump.start())
        assertEquals(0, pump.drainOnce())
        assertEquals(0, polls)
        pump.stop()
        assertFalse(pump.isActive())
    }

    @Test
    fun drain_limit_bounds_each_dispatch_tick() {
        val frames = ArrayDeque((0L until 5L).map { frame(FrameArkNative.MediaKind.VIDEO, it) })
        val native = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            pollMediaProvider = { frames.removeFirstOrNull()?.let(::envelope) },
            videoConfigProvider = { null },
            audioConfigProvider = { null },
        )
        val pump = MediaPlaybackPump(native, {}, {}, maxFramesPerDrain = 2)

        assertTrue(pump.start())
        assertEquals(2, pump.drainOnce())
        assertEquals(2, pump.drainOnce())
        assertEquals(1, pump.drainOnce())
        assertEquals(0, pump.drainOnce())
    }

    @Test(expected = IllegalArgumentException::class)
    fun drain_limit_rejects_unbounded_values() {
        MediaPlaybackPump(
            FrameArkNative(libraryLoader = {}, versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION }),
            {},
            {},
            maxFramesPerDrain = MediaPlaybackPump.MAX_FRAMES_PER_DRAIN + 1,
        )
    }

    @Test
    fun dispatches_native_track_configuration_before_media_frames() {
        val videoConfig = ByteBuffer.allocate(24)
            .order(ByteOrder.BIG_ENDIAN)
            .put("FAMC".toByteArray())
            .put(1).put(1).put(1).put(0)
            .putInt(1_280).putInt(720).putInt(30).putInt(1)
            .array()
        val audioConfig = ByteBuffer.allocate(16)
            .order(ByteOrder.BIG_ENDIAN)
            .put("FAMC".toByteArray())
            .put(1).put(2).put(4).put(0)
            .putInt(48_000).putInt(2)
            .array()
        var video: FrameArkNative.VideoConfig? = null
        var audio: FrameArkNative.AudioConfig? = null
        val native = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            pollMediaProvider = { null },
            videoConfigProvider = { videoConfig },
            audioConfigProvider = { audioConfig },
        )
        val pump = MediaPlaybackPump(
            native,
            videoConsumer = {},
            audioConsumer = {},
            videoConfigConsumer = { video = it },
            audioConfigConsumer = { audio = it },
        )

        assertTrue(pump.start())
        assertEquals(0, pump.drainOnce())
        assertEquals(FrameArkNative.MediaCodecKind.H264, video?.codec)
        assertEquals(1_280, video?.width)
        assertEquals(FrameArkNative.MediaCodecKind.OPUS, audio?.codec)
        assertEquals(48_000, audio?.sampleRate)
    }
}
