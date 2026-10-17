@file:Suppress("DEPRECATION")

package dev.frameark.receiver

import android.content.Intent
import android.graphics.Bitmap
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.test.InstrumentationTestCase
import android.widget.Button
import android.widget.CheckBox
import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** Real platform encoders -> pinned QUIC -> Rust queue -> codecs -> outputs. */
@Suppress("DEPRECATION")
class NativePlaybackTest : InstrumentationTestCase() {
    fun testDisplayPollingDoesNotWaitForCodecOwner() {
        val workers = Executors.newFixedThreadPool(2)
        val held = java.util.concurrent.CountDownLatch(1)
        val release = java.util.concurrent.CountDownLatch(1)
        val owner = workers.submit {
            synchronized(ReceiverPlaybackOutput) {
                held.countDown()
                check(release.await(3, TimeUnit.SECONDS))
            }
        }
        try {
            assertTrue(held.await(1, TimeUnit.SECONDS))
            val read = workers.submit<String> {
                ReceiverPlaybackOutput.videoDisplaySize()
                ReceiverPlaybackOutput.videoConfigurationCount()
                ReceiverPlaybackOutput.videoFormatCount()
                ReceiverPlaybackOutput.diagnostic()
            }
            assertTrue("display polling must not wait for MediaCodec/Surface work", read.get(500, TimeUnit.MILLISECONDS).startsWith("Video submitted:"))
        } finally {
            release.countDown()
            owner.get(1, TimeUnit.SECONDS)
            workers.shutdownNow()
            assertTrue(workers.awaitTermination(1, TimeUnit.SECONDS))
        }
    }

    fun testTransientAudioInputBackpressureKeepsDecoderAlive() {
        var blocked = true
        var inputCalls = 0
        var pcm = 0
        val renderer = AudioDecoderRenderer(onPcm = { pcm++ }, inputIndex = { decoder, timeout ->
            assertEquals("codec input polling never blocks the playback loop", 0L, timeout)
            inputCalls++
            if (blocked) MediaCodec.INFO_TRY_AGAIN_LATER else decoder.dequeueInputBuffer(timeout)
        })
        try {
            renderer.configure(FrameArkNative.AudioConfig(FrameArkNative.MediaCodecKind.AAC, 48_000, 2))
            val encoded = encodeAudio()
            val frame = FrameArkNative.MediaFrame(FrameArkNative.MediaKind.AUDIO, 0, false, encoded)
            repeat(3) { assertFalse(renderer.render(frame)) }
            assertEquals(3, inputCalls)
            assertTrue("temporary input unavailability retains the configured decoder", renderer.isConfigured())
            blocked = false
            var submitted = 0
            val deadline = System.nanoTime() + 3_000_000_000L
            while ((submitted < 12 || pcm < 8) && System.nanoTime() < deadline) {
                if (submitted < 12 && renderer.render(frame.copy(pts = submitted * 1024_000_000L / 48_000))) submitted++
                renderer.drain()
                Thread.sleep(5)
            }
            assertEquals(12, submitted)
            assertTrue("real AAC decode/PCM output recovers after input backpressure", pcm >= 8)
        } finally { renderer.reset() }
        assertFalse(renderer.isConfigured())
    }

    fun testTransientVideoInputBackpressureKeepsDecoderAlive() {
        val texture = android.graphics.SurfaceTexture(false)
        val surface = android.view.Surface(texture)
        var blocked = true
        val renderer = VideoDecoderRenderer(inputIndex = { decoder, timeout ->
            assertEquals(0L, timeout)
            if (blocked) MediaCodec.INFO_TRY_AGAIN_LATER else decoder.dequeueInputBuffer(timeout)
        })
        try {
            renderer.configure(VideoTrackConfig(320, 240, 30, 1), surface)
            val encoded = encodeVideo()
            repeat(3) { assertFalse(renderer.render(encoded, 0, true)) }
            assertTrue(renderer.isConfigured())
            blocked = false
            var accepted = false
            val deadline = System.nanoTime() + 3_000_000_000L
            while (!accepted && System.nanoTime() < deadline) {
                accepted = renderer.render(encoded, 0, true)
                if (!accepted) Thread.sleep(5)
            }
            assertTrue("the same H.264 access unit can enter the real decoder after retry", accepted)
        } finally { renderer.reset(); surface.release(); texture.release() }
        assertFalse(renderer.isConfigured())
    }

    fun testLoopbackRendersVideoAndWritesDecodedAudio() {
        val context = instrumentation.targetContext
        val activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val native = FrameArkNative()
        assertTrue(native.load() is FrameArkNative.LoadResult.Loaded)
        assertTrue(native.setPeerProofRequired(false))
        val settings = PlaybackAudioSettings(context)
        val originalLevel = settings.read()
        val sender = Executors.newSingleThreadExecutor()
        var primaryFailure: Throwable? = null
        try {
            assertTrue(ReceiverPlaybackOutput.setAudioLevel(PlaybackAudioLevel(50)))
            instrumentation.runOnMainSync {
                activity.window.decorView.findViewWithTag<PlaybackAudioPanel>("audio-level-panel").refresh()
            }
            instrumentation.waitForIdleSync()
            Thread.sleep(500) // Wait for this test's TextureView surface, not network retry.
            context.startService(Intent(context, FrameArkReceiverService::class.java).setAction(ReceiverServicePolicy.ACTION_START))
            val deadline = System.nanoTime() + 5_000_000_000L
            while (!native.isReceiverRunning() && System.nanoTime() < deadline) Thread.sleep(20)
            assertTrue(native.isReceiverRunning())
            val beforeVideo = ReceiverPlaybackOutput.renderedVideoFrames()
            val beforeAudio = ReceiverPlaybackOutput.renderedAudioBuffers()
            val video = encodeVideo()
            val audio = encodeAudio()
            // Allow initial connectivity registration to finish before pairing.
            instrumentation.waitForIdleSync()
            assertNotNull("listener after initial network callbacks", native.networkInfo())
            val probe = PlaybackNetworkProbe()
            val stream = sender.submit<Int> { probe.send(video, audio) }
            fun awaitMoreAudio() {
                val target = ReceiverPlaybackOutput.renderedAudioBuffers() + 10
                val timeout = System.nanoTime() + 10_000_000_000L
                while (ReceiverPlaybackOutput.renderedAudioBuffers() < target && System.nanoTime() < timeout && !stream.isDone) {
                    Thread.sleep(10)
                }
                if (ReceiverPlaybackOutput.renderedAudioBuffers() < target) {
                    val result = if (stream.isDone) stream.get(0, TimeUnit.MILLISECONDS).toString() else "pending"
                    fail("decoded audio progresses during gain changes; sender=$result; ${probe.diagnostic()}; ${ReceiverPlaybackOutput.diagnostic()}")
                }
                assertFalse("outputs still live", ReceiverPlaybackOutput.outputsReleased())
            }
            awaitMoreAudio()
            instrumentation.runOnMainSync {
                activity.window.decorView.findViewWithTag<CheckBox>("audio-level-mute").performClick()
            }
            assertEquals(PlaybackAudioLevel(50, true), ReceiverPlaybackOutput.audioLevel())
            awaitMoreAudio() // Muting still consumes decoded PCM instead of retaining stale audio.
            instrumentation.runOnMainSync {
                activity.window.decorView.findViewWithTag<Button>("audio-level-up").performClick()
            }
            assertEquals(PlaybackAudioLevel(60, true), ReceiverPlaybackOutput.audioLevel())
            awaitMoreAudio()
            instrumentation.runOnMainSync {
                activity.window.decorView.findViewWithTag<CheckBox>("audio-level-mute").performClick()
            }
            assertEquals(PlaybackAudioLevel(60), ReceiverPlaybackOutput.audioLevel())
            val result = stream.get(10, TimeUnit.SECONDS)
            assertEquals("FANP ${probe.diagnostic()}; ${ReceiverPlaybackOutput.diagnostic()}", 0, result)
            val drainDeadline = System.nanoTime() + 5_000_000_000L
            while (!ReceiverPlaybackOutput.outputsReleased() && System.nanoTime() < drainDeadline) Thread.sleep(20)
            instrumentation.waitForIdleSync() // Rendered callbacks run on the main Looper.
            assertTrue(ReceiverPlaybackOutput.diagnostic(), ReceiverPlaybackOutput.renderedVideoFrames() - beforeVideo >= 80)
            assertTrue(ReceiverPlaybackOutput.diagnostic(), ReceiverPlaybackOutput.renderedAudioBuffers() - beforeAudio >= 100)
            assertTrue("Stop releases both codec outputs", ReceiverPlaybackOutput.outputsReleased())
            assertEquals("Stop preserves local level", PlaybackAudioLevel(60), ReceiverPlaybackOutput.audioLevel())
            instrumentation.sendStatus(0, android.os.Bundle().apply {
                putString("stream", "\n${ReceiverPlaybackOutput.diagnostic()}\n")
            })
            val screenshot = instrumentation.uiAutomation.takeScreenshot()
            val output = File(context.getExternalFilesDir(null), "native-playback.png")
            output.outputStream().use { screenshot.compress(Bitmap.CompressFormat.PNG, 100, it) }
            screenshot.recycle()
        } catch (failure: Throwable) {
            primaryFailure = failure
            throw failure
        } finally {
            // An assertion must not skip the remaining cleanup or replace the
            // original playback fault with only a secondary teardown failure.
            var cleanupFailure: Throwable? = null
            fun cleanup(action: () -> Unit) {
                try { action() } catch (failure: Throwable) {
                    val first = primaryFailure ?: cleanupFailure
                    if (first != null) first.addSuppressed(failure) else cleanupFailure = failure
                }
            }
            cleanup { context.stopService(Intent(context, FrameArkReceiverService::class.java)) }
            cleanup {
                val stopDeadline = System.nanoTime() + 5_000_000_000L
                while (native.isReceiverRunning() && System.nanoTime() < stopDeadline) Thread.sleep(20)
                assertFalse("receiver listener released", native.isReceiverRunning())
            }
            cleanup { sender.shutdownNow(); assertTrue("probe worker released", sender.awaitTermination(10, TimeUnit.SECONDS)) }
            cleanup { instrumentation.runOnMainSync { activity.finish() } }
            cleanup { ReceiverPlaybackOutput.setAudioLevel(originalLevel) }
            cleanup { settings.save(originalLevel) }
            if (primaryFailure == null) cleanupFailure?.let { throw it }
        }
    }

    private fun encodeVideo(): ByteArray {
        val format = MediaFormat.createVideoFormat("video/avc", 320, 240).apply {
            setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatYUV420Flexible)
            setInteger(MediaFormat.KEY_BIT_RATE, 500_000)
            setInteger(MediaFormat.KEY_FRAME_RATE, 30)
            setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1)
        }
        val raw = ByteArray(320 * 240 * 3 / 2) { if (it < 320 * 240) 180.toByte() else 128.toByte() }
        return encode(format, raw, prependConfig = true)
    }

    private fun encodeAudio(): ByteArray {
        val format = MediaFormat.createAudioFormat("audio/mp4a-latm", 48_000, 2).apply {
            setInteger(MediaFormat.KEY_AAC_PROFILE, MediaCodecInfo.CodecProfileLevel.AACObjectLC)
            setInteger(MediaFormat.KEY_BIT_RATE, 128_000)
        }
        return encode(format, ByteArray(1024 * 2 * 2), prependConfig = false)
    }

    private fun encode(format: MediaFormat, raw: ByteArray, prependConfig: Boolean): ByteArray {
        val codec = MediaCodec.createEncoderByType(requireNotNull(format.getString(MediaFormat.KEY_MIME)))
        try {
            codec.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            codec.start()
            var config = ByteArray(0)
            val info = MediaCodec.BufferInfo()
            var inputs = 0
            val deadline = System.nanoTime() + 5_000_000_000L
            while (System.nanoTime() < deadline) {
                if (inputs < 3) {
                    val index = codec.dequeueInputBuffer(10_000)
                    if (index >= 0) {
                        val buffer = requireNotNull(codec.getInputBuffer(index))
                        buffer.clear(); buffer.put(raw)
                        codec.queueInputBuffer(index, 0, raw.size, inputs * 33_333L, 0)
                        inputs++
                    }
                }
                val index = codec.dequeueOutputBuffer(info, 10_000)
                if (index >= 0) {
                    val buffer = requireNotNull(codec.getOutputBuffer(index))
                    buffer.position(info.offset); buffer.limit(info.offset + info.size)
                    val bytes = ByteArray(info.size); buffer.get(bytes)
                    codec.releaseOutputBuffer(index, false)
                    if (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0) config = bytes
                    else if (bytes.isNotEmpty()) return if (prependConfig) config + bytes else bytes
                }
            }
            error("platform encoder produced no sample")
        } finally { runCatching { codec.stop() }; codec.release() }
    }
}
