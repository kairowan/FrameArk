package dev.frameark.receiver

import android.view.Surface
import java.util.concurrent.atomic.AtomicLong

/** Process-local OS output ownership only; Rust owns session/order/timing. */
object ReceiverPlaybackOutput {
    private var surface: Surface? = null
    private val videoRendered = AtomicLong()
    private val audioRendered = AtomicLong()
    // UI/TextureView polling must not wait for a codec call holding this
    // object's monitor. Writers remain serialized; readers see published
    // scalar values and an immutable width/height pair (approximate counters).
    @Volatile private var videoSubmitted = 0L
    @Volatile private var videoInputRetries = 0L
    @Volatile private var audioInputRetries = 0L
    @Volatile private var error = false
    @Volatile private var failureLocation = ""
    @Volatile private var videoSize: VideoDisplaySize? = null
    @Volatile private var videoConfigurations = 0L
    @Volatile private var videoFormats = 0L
    private val video = VideoDecoderRenderer(onRendered = { videoRendered.incrementAndGet() }, onFormat = { width, height ->
        videoSize = VideoDisplaySize(width, height)
        videoFormats++
    })
    private val audio = AudioDecoderRenderer(onPcm = { audioRendered.incrementAndGet() })
    private var audioLevel = PlaybackAudioLevel()
    private var audioLevelInitialized = false

    /** Read persisted local settings once, before either Activity or Service can play. */
    @Synchronized fun initializeAudioLevel(value: PlaybackAudioLevel) {
        if (!audioLevelInitialized) {
            audio.setGain(value.gain)
            audioLevel = value
            audioLevelInitialized = true
        }
    }

    @Synchronized fun audioLevel(): PlaybackAudioLevel = audioLevel
    fun videoDisplaySize(): VideoDisplaySize? = videoSize
    fun videoConfigurationCount(): Long = videoConfigurations
    fun videoFormatCount(): Long = videoFormats

    @Synchronized fun setAudioLevel(value: PlaybackAudioLevel): Boolean {
        audioLevelInitialized = true
        return try {
            audio.setGain(value.gain)
            audioLevel = value
            true
        } catch (failure: Exception) {
            // An unsuccessful mute must not leave unknown live output behind.
            audioLevel = value.copy(muted = true)
            recordFailure(failure)
            runCatching { reset() }
            false
        }
    }

    @Synchronized fun attach(value: Surface) { reset(); surface = value }
    /** Idle ticket exchange may leave the listener alive, active playback may not. */
    @Synchronized fun detach(value: Surface): Boolean {
        if (surface !== value) return false
        val active = !outputsReleased()
        try { reset() } finally { surface = null }
        return active
    }

    @Synchronized fun consume(event: FrameArkNative.PlaybackEvent): Boolean {
        try {
            when (event) {
                is FrameArkNative.PlaybackEvent.Video -> {
                    val config = event.config
                    require(config.codec == FrameArkNative.MediaCodecKind.H264)
                    val target = surface?.takeIf { it.isValid } ?: error("video surface unavailable")
                    video.configure(VideoTrackConfig(config.width, config.height, config.frameRateNumerator, config.frameRateDenominator), target)
                    videoConfigurations++
                    videoSize = VideoDisplaySize(config.width, config.height)
                    error = false
                }
                is FrameArkNative.PlaybackEvent.Audio -> { audio.configure(event.config); error = false }
                is FrameArkNative.PlaybackEvent.Frame -> when (event.frame.kind) {
                    FrameArkNative.MediaKind.VIDEO -> {
                        if (!video.render(event.frame.payload, event.frame.pts, event.frame.keyframe)) {
                            videoInputRetries++
                            return false
                        }
                        videoSubmitted++
                    }
                    FrameArkNative.MediaKind.AUDIO -> if (!audio.render(event.frame)) {
                        audioInputRetries++
                        return false
                    }
                }
                FrameArkNative.PlaybackEvent.Reset -> reset()
            }
            return true
        } catch (failure: Throwable) { recordFailure(failure); runCatching { reset() }; throw failure }
    }

    @Synchronized fun drain() {
        try { video.drain(); audio.drain() } catch (failure: Throwable) {
            recordFailure(failure); runCatching { reset() }; throw failure
        }
    }

    @Synchronized fun reset() { try { video.reset() } finally { audio.reset() } }
    fun diagnostic(): String {
        val size = videoSize
        return "Video submitted: $videoSubmitted; rendered: ${videoRendered.get()}; PCM buffers: ${audioRendered.get()}" +
            "\nCodec input retries: video=$videoInputRetries; audio=$audioInputRetries" +
            "\nVideo: ${size?.width ?: 0}x${size?.height ?: 0}; configs: $videoConfigurations; formats: $videoFormats" +
            if (error) "\nPlayback failed ($failureLocation). Stop and restart; inspect codec/surface support." else ""
    }

    private fun recordFailure(failure: Throwable) {
        // Static source location only: never retain/log a decoder's raw message.
        failureLocation = failure.stackTrace.firstOrNull { it.className.startsWith("dev.frameark.receiver.") }
            ?.let { "${it.fileName}:${it.lineNumber}" } ?: "platform output"
        error = true
    }

    fun renderedVideoFrames(): Long = videoRendered.get()
    fun renderedAudioBuffers(): Long = audioRendered.get()
    @Synchronized fun outputsReleased(): Boolean = !video.isConfigured() && !audio.isConfigured()
}
