package dev.frameark.receiver

import android.media.AudioFormat
import android.media.MediaCodec
import android.media.MediaFormat
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Platform decoder for the documented FANP stereo Opus / AAC-LC subset. */
class AudioDecoderRenderer(private val onPcm: () -> Unit = {}) {
    private var codec: MediaCodec? = null
    private val output = AudioOutputRenderer(onWritten = onPcm)

    fun setGain(value: Float) = output.setGain(value)

    fun configure(config: FrameArkNative.AudioConfig) {
        reset()
        require(config.channels in 1..2) { "only mono/stereo output is supported" }
        val format = when (config.codec) {
            FrameArkNative.MediaCodecKind.OPUS -> {
                require(config.sampleRate == 48_000) { "Opus requires 48 kHz output" }
                MediaFormat.createAudioFormat("audio/opus", 48_000, config.channels).apply {
                    // Mapping family 0, no pre-skip or gain: this is a live raw-AU
                    // contract, not arbitrary Ogg Opus playback.
                    val header = ByteBuffer.allocate(19).order(ByteOrder.LITTLE_ENDIAN)
                        .put("OpusHead".toByteArray()).put(1).put(config.channels.toByte())
                        .putShort(0).putInt(48_000).putShort(0).put(0).array()
                    setByteBuffer("csd-0", ByteBuffer.wrap(header))
                    setByteBuffer("csd-1", ByteBuffer.wrap(ByteArray(8)))
                    setByteBuffer("csd-2", ByteBuffer.allocate(8).order(ByteOrder.nativeOrder()).putLong(80_000_000).apply { flip() })
                }
            }
            FrameArkNative.MediaCodecKind.AAC -> {
                val rates = listOf(96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000)
                val index = rates.indexOf(config.sampleRate)
                require(index >= 0) { "unsupported AAC sample rate" }
                val asc = (2 shl 11) or (index shl 7) or (config.channels shl 3)
                MediaFormat.createAudioFormat("audio/mp4a-latm", config.sampleRate, config.channels).apply {
                    setByteBuffer("csd-0", ByteBuffer.wrap(byteArrayOf((asc shr 8).toByte(), asc.toByte())))
                }
            }
            else -> error("unsupported network audio codec")
        }
        format.setInteger(MediaFormat.KEY_PCM_ENCODING, AudioFormat.ENCODING_PCM_16BIT)
        val decoder = MediaCodec.createDecoderByType(requireNotNull(format.getString(MediaFormat.KEY_MIME)))
        try {
            decoder.configure(format, null, null, 0)
            decoder.start()
            codec = decoder
        } catch (error: Throwable) { decoder.release(); throw error }
    }

    fun render(frame: FrameArkNative.MediaFrame) {
        val decoder = codec ?: error("audio decoder not configured")
        drain()
        val index = decoder.dequeueInputBuffer(20_000)
        check(index >= 0) { "audio decoder backpressure" }
        val input = requireNotNull(decoder.getInputBuffer(index))
        input.clear()
        require(frame.payload.size <= input.remaining()) { "encoded audio exceeds input capacity" }
        input.put(frame.payload)
        decoder.queueInputBuffer(index, 0, frame.payload.size, frame.pts, 0)
        drain()
    }

    fun drain() {
        output.drain()
        val decoder = codec ?: return
        val info = MediaCodec.BufferInfo()
        repeat(16) {
            val index = decoder.dequeueOutputBuffer(info, 0)
            when {
                index == MediaCodec.INFO_TRY_AGAIN_LATER -> return
                index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                    val format = decoder.outputFormat
                    require(!format.containsKey(MediaFormat.KEY_PCM_ENCODING) ||
                        format.getInteger(MediaFormat.KEY_PCM_ENCODING) == AudioFormat.ENCODING_PCM_16BIT)
                    output.configure(PcmTrackConfig(format.getInteger(MediaFormat.KEY_SAMPLE_RATE), format.getInteger(MediaFormat.KEY_CHANNEL_COUNT)))
                }
                index >= 0 -> try {
                    if (info.size > 0) {
                        require(info.size <= 256 * 1024)
                        val bytes = requireNotNull(decoder.getOutputBuffer(index))
                        bytes.position(info.offset)
                        bytes.limit(info.offset + info.size)
                        val pcm = ByteArray(info.size)
                        bytes.get(pcm)
                        output.render(pcm)
                    }
                } finally { decoder.releaseOutputBuffer(index, false) }
            }
        }
    }

    fun reset() {
        val decoder = codec
        codec = null
        try {
            if (decoder != null) try { decoder.stop() } finally { decoder.release() }
        } finally { output.reset() }
    }

    fun isConfigured(): Boolean = codec != null
}
