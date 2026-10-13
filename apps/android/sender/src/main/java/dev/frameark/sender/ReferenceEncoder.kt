package dev.frameark.sender

import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.view.Surface
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.sin

/** Real platform encoders, synthetic content only. No capture or microphone permission. */
class ReferenceEncoder {
    fun run(
        width: Int, height: Int, seconds: Int, cancelled: AtomicBoolean,
        submit: (ByteArray, Long, Boolean, Boolean) -> Unit,
        progress: (Int, Int) -> Unit,
    ) {
        require(seconds in 1..30)
        var video: MediaCodec? = null
        var audio: MediaCodec? = null
        var surface: Surface? = null
        val egl = PatternSurface()
        try {
            val videoCodec = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC).also { video = it }
            val videoFormat = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height).apply {
                setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
                setInteger(MediaFormat.KEY_BIT_RATE, if (width == 1920) 5_000_000 else 2_500_000)
                setInteger(MediaFormat.KEY_FRAME_RATE, 30)
                setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1)
                setInteger(MediaFormat.KEY_PROFILE, MediaCodecInfo.CodecProfileLevel.AVCProfileBaseline)
            }
            videoCodec.configure(videoFormat, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            surface = videoCodec.createInputSurface()
            egl.open(surface)
            videoCodec.start()
            val audioCodec = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_AUDIO_AAC).also { audio = it }
            audioCodec.configure(MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_AAC, 48000, 2).apply {
                setInteger(MediaFormat.KEY_AAC_PROFILE, MediaCodecInfo.CodecProfileLevel.AACObjectLC)
                setInteger(MediaFormat.KEY_BIT_RATE, 128000)
                setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 4096)
            }, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            audioCodec.start()

            var config = ByteArray(0)
            var videoCount = 0
            var audioCount = 0
            var videoEos = false
            var audioEos = false
            val info = MediaCodec.BufferInfo()
            fun copy(buffer: ByteBuffer): ByteArray = ByteArray(buffer.remaining()).also { buffer.get(it) }
            fun drain(codec: MediaCodec, isVideo: Boolean) {
                repeat(32) {
                    if (cancelled.get()) throw InterruptedException()
                    when (val index = codec.dequeueOutputBuffer(info, 0)) {
                        MediaCodec.INFO_TRY_AGAIN_LATER -> return
                        MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> if (isVideo) {
                            val format = codec.outputFormat
                            val sps = format.getByteBuffer("csd-0")?.duplicate() ?: error("missing configuration")
                            val pps = format.getByteBuffer("csd-1")?.duplicate()
                            require(sps.remaining() + (pps?.remaining() ?: 0) <= 65536)
                            config = copy(sps) + (pps?.let(::copy) ?: ByteArray(0))
                        }
                        else -> if (index >= 0) {
                            try {
                                val isConfig = info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0
                                if (info.size > 0 && !isConfig && info.presentationTimeUs >= 0) {
                                    require(info.size <= if (isVideo) 4 * 1024 * 1024 else 65536)
                                    val buffer = codec.getOutputBuffer(index)!!.duplicate()
                                    require(info.offset >= 0 && info.size <= buffer.capacity() - info.offset)
                                    buffer.position(info.offset); buffer.limit(info.offset + info.size)
                                    val key = info.flags and MediaCodec.BUFFER_FLAG_KEY_FRAME != 0
                                    val payload = copy(buffer)
                                    submit(if (isVideo) AnnexB.accessUnit(config, payload, key) else payload, info.presentationTimeUs, isVideo, key)
                                    if (isVideo) videoCount++ else audioCount++
                                }
                                if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) {
                                    if (isVideo) videoEos = true else audioEos = true
                                }
                            } finally { codec.releaseOutputBuffer(index, false) }
                        }
                    }
                }
            }

            var audioSamples = 0L
            fun queueAudio(eos: Boolean): Boolean {
                val index = audioCodec.dequeueInputBuffer(0)
                if (index < 0) return false
                val buffer = audioCodec.getInputBuffer(index)!!.order(ByteOrder.LITTLE_ENDIAN)
                buffer.clear()
                if (!eos) {
                    require(buffer.remaining() >= 4096)
                    repeat(1024) { offset ->
                        val sample = audioSamples + offset
                        val amplitude = if (sample % 48000 < 3840) sin(sample * 2.0 * Math.PI * 880 / 48000) * 2500 else 0.0
                        buffer.putShort(amplitude.toInt().toShort()); buffer.putShort(amplitude.toInt().toShort())
                    }
                }
                audioCodec.queueInputBuffer(index, 0, if (eos) 0 else 4096, audioSamples * 1_000_000 / 48000, if (eos) MediaCodec.BUFFER_FLAG_END_OF_STREAM else 0)
                if (!eos) audioSamples += 1024
                return true
            }
            val clock = System.nanoTime()
            repeat(seconds * 30) { frame ->
                val pts = frame * 1_000_000L / 30
                while (System.nanoTime() < clock + pts * 1000) {
                    if (cancelled.get()) throw InterruptedException()
                    drain(videoCodec, true); drain(audioCodec, false)
                    Thread.sleep(1)
                }
                if (cancelled.get()) throw InterruptedException()
                check(System.nanoTime() - clock - pts * 1000 < 2_000_000_000L) { "encoder too slow" }
                egl.draw(width, height, frame, pts)
                val audioDeadline = System.nanoTime() + 1_000_000_000L
                while (audioSamples * 1_000_000 / 48000 <= pts) {
                    if (!queueAudio(false)) Thread.sleep(1)
                    drain(audioCodec, false); drain(videoCodec, true)
                    check(System.nanoTime() < audioDeadline)
                }
                drain(videoCodec, true); drain(audioCodec, false)
                if (frame % 15 == 0) progress(videoCount, audioCount)
            }
            videoCodec.signalEndOfInputStream()
            val deadline = System.nanoTime() + 3_000_000_000L
            var queuedAudioEos = false
            while (!videoEos || !audioEos) {
                if (cancelled.get()) throw InterruptedException()
                if (!queuedAudioEos) queuedAudioEos = queueAudio(true)
                drain(videoCodec, true); drain(audioCodec, false)
                check(System.nanoTime() < deadline)
                Thread.sleep(1)
            }
            check(videoCount > 0 && audioCount > 0)
            progress(videoCount, audioCount)
        } finally {
            egl.close()
            surface?.release()
            // Release even if configure/start failed, without hiding the first failure.
            video?.let { runCatching { it.stop() }; runCatching { it.release() } }
            audio?.let { runCatching { it.stop() }; runCatching { it.release() } }
        }
    }
}
