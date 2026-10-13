package dev.frameark.sender

import android.annotation.SuppressLint
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.*
import android.media.projection.MediaProjection
import android.os.Build
import android.view.Surface
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.abs

/** OS capture and encoding only. Access units/timestamps go to the shared Rust sender. */
class ScreenCaptureEncoder {
    @SuppressLint("MissingPermission", "NewApi") // Non-exported service validates API and explicit audio grant.
    fun run(
        projection: MediaProjection, options: CaptureOptions, density: Int, stop: AtomicBoolean,
        submit: (ByteArray, Long, Boolean, Boolean) -> Unit,
        progress: (Int, Int, Int, Long) -> Unit,
    ) {
        check(Build.VERSION.SDK_INT >= 29)
        var video: MediaCodec? = null
        var audio: MediaCodec? = null
        var recorder: AudioRecord? = null
        var surface: Surface? = null
        var display: VirtualDisplay? = null
        try {
            val encoder = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC).also { video = it }
            encoder.configure(MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, options.width, options.height).apply {
                setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
                setInteger(MediaFormat.KEY_BIT_RATE, (if (options.width * options.height > 1280 * 720) 5_000_000 else 2_500_000) * (options.fps / 30))
                setInteger(MediaFormat.KEY_FRAME_RATE, options.fps)
                setFloat(MediaFormat.KEY_MAX_FPS_TO_ENCODER, options.fps.toFloat())
                // Static screens still advance the media stream and its bounded idle timeout.
                setLong(MediaFormat.KEY_REPEAT_PREVIOUS_FRAME_AFTER, 1_000_000L / options.fps)
                setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1)
                setInteger(MediaFormat.KEY_PROFILE, MediaCodecInfo.CodecProfileLevel.AVCProfileBaseline)
            }, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            surface = encoder.createInputSurface()
            encoder.start()
            if (options.audio) {
                val aac = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_AUDIO_AAC).also { audio = it }
                aac.configure(MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_AAC, 48000, 2).apply {
                    setInteger(MediaFormat.KEY_AAC_PROFILE, MediaCodecInfo.CodecProfileLevel.AACObjectLC)
                    setInteger(MediaFormat.KEY_BIT_RATE, 128000)
                    setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 4096)
                }, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
                aac.start()
                val bufferSize = AudioRecord.getMinBufferSize(48000, AudioFormat.CHANNEL_IN_STEREO, AudioFormat.ENCODING_PCM_16BIT)
                require(bufferSize in 1..524288)
                recorder = AudioRecord.Builder()
                    .setAudioFormat(AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT).setSampleRate(48000).setChannelMask(AudioFormat.CHANNEL_IN_STEREO).build())
                    .setBufferSizeInBytes(maxOf(16384, bufferSize * 2))
                    .setAudioPlaybackCaptureConfig(AudioPlaybackCaptureConfiguration.Builder(projection)
                        .addMatchingUsage(AudioAttributes.USAGE_MEDIA).addMatchingUsage(AudioAttributes.USAGE_GAME)
                        .addMatchingUsage(AudioAttributes.USAGE_UNKNOWN).build()).build()
                check(recorder.state == AudioRecord.STATE_INITIALIZED)
            }
            if (stop.get()) return
            val clock = System.nanoTime()
            val audioOriginUs = (System.nanoTime() - clock) / 1000
            recorder?.startRecording()
            recorder?.let { check(it.recordingState == AudioRecord.RECORDSTATE_RECORDING) }
            // Exactly one display per OS consent token. Rotation is fit into this fixed canvas.
            display = projection.createVirtualDisplay("FrameArk capture", options.width, options.height, density,
                DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR, surface, null, null)
            var config = ByteArray(0)
            var videos = 0
            var audios = 0
            var nonSilent = 0
            var lastVideo = -1L
            var lastAudio = -1L
            var nextProgress = 0L
            var lastVideoAt = clock
            var audioSamples = 0L
            var audioInputPts = -1L
            val pcm = ShortArray(2048)
            var pcmSize = 0
            val info = MediaCodec.BufferInfo()
            val timestamp = AudioTimestamp()
            fun copy(buffer: ByteBuffer) = ByteArray(buffer.remaining()).also { buffer.get(it) }
            fun drain(codec: MediaCodec, isVideo: Boolean) {
                repeat(32) {
                    if (stop.get()) return
                    when (val index = codec.dequeueOutputBuffer(info, 0)) {
                        MediaCodec.INFO_TRY_AGAIN_LATER -> return
                        MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> if (isVideo) {
                            val format = codec.outputFormat
                            val sps = requireNotNull(format.getByteBuffer("csd-0")).duplicate()
                            val pps = format.getByteBuffer("csd-1")?.duplicate()
                            require(sps.remaining() + (pps?.remaining() ?: 0) <= 65536)
                            config = copy(sps) + (pps?.let(::copy) ?: ByteArray(0))
                        }
                        else -> if (index >= 0) {
                            try {
                                if (info.size > 0 && info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG == 0) {
                                    val pts = if (isVideo) info.presentationTimeUs - clock / 1000 else info.presentationTimeUs
                                    // Codec priming may precede the capture clock; never send negative PTS.
                                    if (pts >= 0) {
                                        require(pts <= 3_601_000_000L && pts > if (isVideo) lastVideo else lastAudio)
                                        require(info.size <= if (isVideo) 4 * 1024 * 1024 else 65536)
                                        val buffer = requireNotNull(codec.getOutputBuffer(index)).duplicate()
                                        require(info.offset >= 0 && info.size <= buffer.capacity() - info.offset)
                                        buffer.position(info.offset); buffer.limit(info.offset + info.size)
                                        val key = info.flags and MediaCodec.BUFFER_FLAG_KEY_FRAME != 0
                                        val payload = copy(buffer)
                                        submit(if (isVideo) AnnexB.accessUnit(config, payload, key) else payload, pts, isVideo, key)
                                        if (isVideo) { videos++; lastVideo = pts; lastVideoAt = System.nanoTime() }
                                        else { audios++; lastAudio = pts }
                                    }
                                }
                            } finally { codec.releaseOutputBuffer(index, false) }
                        }
                    }
                }
            }
            while (!stop.get() && System.nanoTime() - clock < options.seconds * 1_000_000_000L) {
                val source = recorder
                val aac = audio
                if (source != null && aac != null) {
                    if (pcmSize < pcm.size) {
                        val count = source.read(pcm, pcmSize, pcm.size - pcmSize, AudioRecord.READ_NON_BLOCKING)
                        check(count >= 0 && count % 2 == 0)
                        pcmSize += count
                    }
                    if (pcmSize == pcm.size) {
                        val index = aac.dequeueInputBuffer(0)
                        if (index >= 0) {
                            val input = requireNotNull(aac.getInputBuffer(index)).order(ByteOrder.LITTLE_ENDIAN)
                            input.clear()
                            require(input.remaining() >= pcm.size * 2)
                            pcm.forEach(input::putShort)
                            // Frame position is the clock of captured PCM, not encoder dequeue time.
                            val estimate = if (source.getTimestamp(timestamp, AudioTimestamp.TIMEBASE_MONOTONIC) == AudioRecord.SUCCESS)
                                timestamp.nanoTime / 1000 - clock / 1000 + (audioSamples - timestamp.framePosition) * 1_000_000 / 48000
                            else audioOriginUs + audioSamples * 1_000_000 / 48000
                            val pts = maxOf(0, audioInputPts + 1, estimate)
                            audioInputPts = pts
                            aac.queueInputBuffer(index, 0, pcm.size * 2, pts, 0)
                            if (pcm.any { abs(it.toInt()) > 32 }) nonSilent++
                            audioSamples += pcm.size / 2
                            pcmSize = 0
                        }
                    }
                    drain(aac, false)
                }
                drain(encoder, true)
                val elapsed = System.nanoTime() - clock
                check(System.nanoTime() - lastVideoAt < 8_000_000_000L) // Fail visibly if capture/encoder stalls.
                if (elapsed >= nextProgress) { progress(videos, audios, nonSilent, elapsed / 1_000_000_000); nextProgress = elapsed + 250_000_000L }
                Thread.sleep(2)
            }
            progress(videos, audios, nonSilent, (System.nanoTime() - clock) / 1_000_000_000)
        } finally {
            // Stop OS capture first. Only whole, already encoded access units are drained by Rust.
            display?.let { runCatching { it.release() } }
            recorder?.let { runCatching { it.stop() }; runCatching { it.release() } }
            video?.let { runCatching { it.stop() }; runCatching { it.release() } }
            audio?.let { runCatching { it.stop() }; runCatching { it.release() } }
            surface?.release()
        }
    }
}
