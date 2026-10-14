package dev.frameark.receiver

import android.media.MediaCodec
import android.media.MediaFormat
import android.os.Handler
import android.os.Looper
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
    private val onRendered: () -> Unit = {},
    private val onFormat: (Int, Int) -> Unit = { _, _ -> },
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
            decoder.setOnFrameRenderedListener({ _, _, _ -> onRendered() }, Handler(Looper.getMainLooper()))
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
        drain(decoder)
        val inputIndex = decoder.dequeueInputBuffer(INPUT_TIMEOUT_US)
        require(inputIndex >= 0) { "video decoder input is backpressured" }
        val input = decoder.getInputBuffer(inputIndex) ?: error("video input buffer unavailable")
        input.clear()
        require(data.size <= input.remaining()) { "encoded video exceeds decoder input capacity" }
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

    fun drain() { codec?.let(::drain) }

    @Suppress("DEPRECATION")
    private fun drain(decoder: MediaCodec) {
        val info = MediaCodec.BufferInfo()
        repeat(16) {
            when (val outputIndex = decoder.dequeueOutputBuffer(info, 0)) {
                MediaCodec.INFO_TRY_AGAIN_LATER -> return
                MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                    val format = decoder.outputFormat
                    val width = format.getInteger(MediaFormat.KEY_WIDTH)
                    val height = format.getInteger(MediaFormat.KEY_HEIGHT)
                    require(width in 1..8192 && height in 1..8192)
                    fun value(key: String, fallback: Int) = if (format.containsKey(key)) format.getInteger(key) else fallback
                    val left = value("crop-left", 0)
                    val top = value("crop-top", 0)
                    val right = value("crop-right", width - 1)
                    val bottom = value("crop-bottom", height - 1)
                    require(left in 0 until width && right in left until width && top in 0 until height && bottom in top until height)
                    onFormat(right - left + 1, bottom - top + 1)
                }
                MediaCodec.INFO_OUTPUT_BUFFERS_CHANGED -> return@repeat
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
