package dev.frameark.receiver

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack

/**
 * PCM output sink for the platform audio decoder.
 *
 * Opus/AAC decoding remains a negotiated platform or Rust backend concern;
 * this class deliberately accepts decoded PCM only and owns AudioTrack's
 * start/stop/release lifecycle.
 */
class AudioOutputRenderer(
    private val trackFactory: (PcmTrackConfig) -> AudioTrack = ::createTrack,
    private val onWritten: () -> Unit = {},
) {
    private var track: AudioTrack? = null
    private val pending = PcmWriteQueue()

    /** Opens and starts an audio output for the negotiated PCM format. */
    fun configure(config: PcmTrackConfig) {
        reset()
        val output = trackFactory(config)
        try { output.play(); track = output } catch (error: Throwable) {
            output.release()
            throw error
        }
    }

    /** Writes one interleaved PCM16 buffer, applying explicit backpressure. */
    fun render(pcm16: ByteArray) {
        check(track != null) { "audio output is not configured" }
        pending.append(pcm16)
        drain()
    }

    fun drain() {
        if (track == null) return
        val output = track ?: error("audio output is not configured")
        val completed = pending.drain { data, offset, length ->
            output.write(data, offset, length, AudioTrack.WRITE_NON_BLOCKING)
        }
        repeat(completed) { onWritten() }
    }

    /** Stops, flushes, and releases the output. */
    fun reset() {
        pending.clear()
        val output = track ?: return
        try {
            output.pause()
            output.flush()
            output.stop()
        } catch (_: IllegalStateException) {
            // Release still needs to run when setup or playback failed.
        } finally {
            output.release()
            track = null
        }
    }

    /** Whether an AudioTrack is active. */
    fun isConfigured(): Boolean = track != null

    private companion object {
        fun createTrack(config: PcmTrackConfig): AudioTrack {
            val channelMask = when (config.channels) {
                1 -> AudioFormat.CHANNEL_OUT_MONO
                2 -> AudioFormat.CHANNEL_OUT_STEREO
                else -> throw IllegalArgumentException("only mono/stereo output is enabled")
            }
            val format = AudioFormat.Builder()
                .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                .setSampleRate(config.sampleRate)
                .setChannelMask(channelMask)
                .build()
            val minimumBuffer = AudioTrack.getMinBufferSize(config.sampleRate, channelMask, format.encoding)
            require(minimumBuffer > 0) { "audio device rejected the PCM format" }
            return AudioTrack.Builder()
                .setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_MEDIA)
                        .setContentType(AudioAttributes.CONTENT_TYPE_MOVIE)
                        .build(),
                )
                .setAudioFormat(format)
                .setBufferSizeInBytes(minimumBuffer.coerceAtLeast(config.sampleRate / 2))
                .setTransferMode(AudioTrack.MODE_STREAM)
                .build()
        }
    }
}
