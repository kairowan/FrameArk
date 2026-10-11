package dev.frameark.receiver

import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MediaPlaybackLoopTest {
    private fun envelope(kind: Int, pts: Long): ByteArray = ByteBuffer.allocate(20)
        .order(ByteOrder.BIG_ENDIAN)
        .put("FAMF".toByteArray())
        .put(1)
        .put(kind.toByte())
        .put(if (kind == 1) 1 else 0)
        .putLong(pts)
        .putInt(1)
        .put(1)
        .array()

    @Test
    fun scheduled_loop_dispatches_frames_and_stops_cleanly() {
        val frames = ArrayDeque(listOf(envelope(1, 7), envelope(2, 8)))
        val dispatched = CountDownLatch(2)
        val native = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            pollMediaProvider = { frames.removeFirstOrNull() },
        )
        val pump = MediaPlaybackPump(
            native,
            { dispatched.countDown() },
            { dispatched.countDown() },
        )
        val loop = MediaPlaybackLoop(pump, tickIntervalMs = 1)

        assertTrue(loop.start())
        assertTrue(dispatched.await(1, TimeUnit.SECONDS))
        loop.stop()
        assertFalse(loop.isRunning())
        assertFalse(pump.isActive())
        loop.close()
    }

    @Test
    fun consumer_failure_stops_loop_and_reports_once() {
        val errors = AtomicInteger(0)
        val errorSeen = CountDownLatch(1)
        val native = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            pollMediaProvider = { envelope(1, 9) },
        )
        val loop = MediaPlaybackLoop(
            MediaPlaybackPump(native, { error("decoder rejected frame") }, {}),
            tickIntervalMs = 1,
            onError = {
                errors.incrementAndGet()
                errorSeen.countDown()
            },
        )

        assertTrue(loop.start())
        assertTrue(errorSeen.await(1, TimeUnit.SECONDS))
        Thread.sleep(10)
        assertFalse(loop.isRunning())
        assertEquals(1, errors.get())
        loop.close()
    }

    @Test
    fun unavailable_native_bridge_does_not_create_a_running_loop() {
        val native = FrameArkNative(libraryLoader = { throw UnsatisfiedLinkError("missing") })
        val loop = MediaPlaybackLoop(MediaPlaybackPump(native, {}, {}), tickIntervalMs = 1)

        assertFalse(loop.start())
        assertFalse(loop.isRunning())
        loop.close()
    }

    @Test(expected = IllegalArgumentException::class)
    fun tick_interval_is_bounded() {
        MediaPlaybackLoop(
            MediaPlaybackPump(
                FrameArkNative(libraryLoader = {}, versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION }),
                {},
                {},
            ),
            tickIntervalMs = MediaPlaybackLoop.MAX_TICK_INTERVAL_MS + 1,
        )
    }
}
