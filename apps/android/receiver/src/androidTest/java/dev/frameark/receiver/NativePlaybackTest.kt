@file:Suppress("DEPRECATION")

package dev.frameark.receiver

import android.content.Intent
import android.graphics.Bitmap
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.test.InstrumentationTestCase
import java.io.File

/** Real platform encoders -> pinned QUIC -> Rust queue -> codecs -> outputs. */
@Suppress("DEPRECATION")
class NativePlaybackTest : InstrumentationTestCase() {
    fun testLoopbackRendersVideoAndWritesDecodedAudio() {
        val context = instrumentation.targetContext
        val activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val native = FrameArkNative()
        assertTrue(native.load() is FrameArkNative.LoadResult.Loaded)
        assertTrue(native.setPeerProofRequired(false))
        try {
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
            val result = probe.send(video, audio)
            assertEquals("FANP ${probe.diagnostic()}; ${ReceiverPlaybackOutput.diagnostic()}", 0, result)
            val drainDeadline = System.nanoTime() + 5_000_000_000L
            while (!ReceiverPlaybackOutput.outputsReleased() && System.nanoTime() < drainDeadline) Thread.sleep(20)
            instrumentation.waitForIdleSync() // Rendered callbacks run on the main Looper.
            assertTrue(ReceiverPlaybackOutput.diagnostic(), ReceiverPlaybackOutput.renderedVideoFrames() - beforeVideo >= 80)
            assertTrue(ReceiverPlaybackOutput.diagnostic(), ReceiverPlaybackOutput.renderedAudioBuffers() - beforeAudio >= 100)
            assertTrue("Stop releases both codec outputs", ReceiverPlaybackOutput.outputsReleased())
            instrumentation.sendStatus(0, android.os.Bundle().apply {
                putString("stream", "\n${ReceiverPlaybackOutput.diagnostic()}\n")
            })
            val screenshot = instrumentation.uiAutomation.takeScreenshot()
            val output = File(context.getExternalFilesDir(null), "native-playback.png")
            output.outputStream().use { screenshot.compress(Bitmap.CompressFormat.PNG, 100, it) }
            screenshot.recycle()
        } finally {
            context.stopService(Intent(context, FrameArkReceiverService::class.java))
            val stopDeadline = System.nanoTime() + 5_000_000_000L
            while (native.isReceiverRunning() && System.nanoTime() < stopDeadline) Thread.sleep(20)
            assertFalse("receiver listener released", native.isReceiverRunning())
            instrumentation.runOnMainSync { activity.finish() }
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
