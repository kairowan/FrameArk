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
    @Test
    fun stop_waits_for_inflight_dispatch_before_resetting_outputs() {
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val stopped = CountDownLatch(1)
        val resets = AtomicInteger()
        val native = FrameArkNative(libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            pollMediaProvider = { envelope(1, 0) }, videoConfigProvider = { null }, audioConfigProvider = { null })
        val pump = MediaPlaybackPump(native, {
            entered.countDown()
            check(release.await(2, TimeUnit.SECONDS))
            assertEquals(0, resets.get())
        }, {}, maxFramesPerDrain = 1, resetOutput = { resets.incrementAndGet() })
        val loop = MediaPlaybackLoop(pump, tickIntervalMs = 1)
        try {
            assertTrue(loop.start())
            assertTrue(entered.await(1, TimeUnit.SECONDS))
            val stopper = Thread { loop.stop(); stopped.countDown() }.apply { start() }
            assertFalse(stopped.await(30, TimeUnit.MILLISECONDS))
            assertEquals(0, resets.get())
            release.countDown()
            assertTrue(stopped.await(1, TimeUnit.SECONDS))
            stopper.join(1_000)
            assertEquals(1, resets.get())
            assertFalse(loop.isRunning())
        } finally { release.countDown(); loop.close() }
    }

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
            videoConfigProvider = { null },
            audioConfigProvider = { null },
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
            videoConfigProvider = { null },
            audioConfigProvider = { null },
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
