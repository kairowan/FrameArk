package dev.frameark.receiver

import android.media.MediaCodec
import android.media.MediaFormat
import android.view.Surface

/**
 * Hardware-first H.264 renderer owned by the Android lifecycle.
 *
 * Rust owns the offer, packet ordering, and session state. This class only
 * adapts validated encoded access units to MediaCodec and never retains or
 * releases the caller's Surface.
 */
class VideoDecoderRenderer(
    private val codecFactory: (String) -> MediaCodec = MediaCodec::createDecoderByType,
) {
    private var codec: MediaCodec? = null
    private var configuredSurface: Surface? = null

    /** Configures and starts an H.264 decoder for the supplied output surface. */
    fun configure(config: VideoTrackConfig, surface: Surface) {
        reset()
        val decoder = codecFactory(MIME_H264)
        try {
            val format = MediaFormat.createVideoFormat(MIME_H264, config.width, config.height)
            format.setFloat(MediaFormat.KEY_FRAME_RATE, config.frameRate)
            decoder.configure(format, surface, null, 0)
            decoder.start()
            codec = decoder
            configuredSurface = surface
        } catch (error: Throwable) {
            decoder.release()
            throw error
        }
    }

    /** Queues one encoded access unit and drains all currently available output. */
    fun render(data: ByteArray, presentationTimeUs: Long, keyFrame: Boolean) {
        val decoder = codec ?: error("video decoder is not configured")
        val inputIndex = decoder.dequeueInputBuffer(INPUT_TIMEOUT_US)
        require(inputIndex >= 0) { "video decoder input is backpressured" }
        val input = decoder.getInputBuffer(inputIndex) ?: error("video input buffer unavailable")
        input.clear()
        input.put(data)
        decoder.queueInputBuffer(
            inputIndex,
            0,
            data.size,
            presentationTimeUs,
            if (keyFrame) MediaCodec.BUFFER_FLAG_KEY_FRAME else 0,
        )
        drain(decoder)
    }

    /** Stops and releases decoder state. The caller retains ownership of the Surface. */
    fun reset() {
        val decoder = codec ?: return
        try {
            decoder.stop()
        } catch (_: IllegalStateException) {
            // A failed configure may leave MediaCodec in a non-started state.
        } finally {
            decoder.release()
            codec = null
            configuredSurface = null
        }
    }

    /** Whether a decoder has been configured and started. */
    fun isConfigured(): Boolean = codec != null && configuredSurface != null

    @Suppress("DEPRECATION")
    private fun drain(decoder: MediaCodec) {
        val info = MediaCodec.BufferInfo()
        while (true) {
            when (val outputIndex = decoder.dequeueOutputBuffer(info, 0)) {
                MediaCodec.INFO_TRY_AGAIN_LATER -> return
                MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> continue
                MediaCodec.INFO_OUTPUT_BUFFERS_CHANGED -> continue
                else -> {
                    if (outputIndex >= 0) {
                        decoder.releaseOutputBuffer(outputIndex, true)
                    } else {
                        return
                    }
                }
            }
        }
    }

    private companion object {
        const val MIME_H264 = "video/avc"
        const val INPUT_TIMEOUT_US = 20_000L
    }
}
