package dev.frameark.receiver

/**
 * Bounded Android-side poll/dispatch loop for Rust-owned encoded media.
 *
 * This class deliberately does not decode or own a Surface/AudioTrack. The
 * supplied consumers integrate those platform APIs and must apply their own
 * codec and output backpressure. Rust remains responsible for the queue's
 * ordering, bounds, and lifecycle.
 */
class MediaPlaybackPump(
    private val native: FrameArkNative,
    private val videoConsumer: (FrameArkNative.MediaFrame) -> Unit,
    private val audioConsumer: (FrameArkNative.MediaFrame) -> Unit,
    maxFramesPerDrain: Int = DEFAULT_MAX_FRAMES_PER_DRAIN,
    private val videoConfigConsumer: (FrameArkNative.VideoConfig) -> Unit = {},
    private val audioConfigConsumer: (FrameArkNative.AudioConfig) -> Unit = {},
    private val eventConsumer: ((FrameArkNative.PlaybackEvent) -> Boolean)? = null,
    private val drainOutput: () -> Unit = {},
    private val resetOutput: () -> Unit = {},
    private val nanoTime: () -> Long = System::nanoTime,
) {
    companion object {
        const val DEFAULT_MAX_FRAMES_PER_DRAIN = 8
        const val MAX_FRAMES_PER_DRAIN = 32
        // OS input retry budget, not a second media clock or a network timeout.
        const val INPUT_RETRY_BUDGET_NS = 500_000_000L
    }

    private val drainLimit = maxFramesPerDrain.also {
        require(it in 1..MAX_FRAMES_PER_DRAIN) { "drain limit is outside the bounded range" }
    }
    private var active = false
    private var pending: FrameArkNative.PlaybackEvent? = null
    private var pendingSince = 0L

    /** Loads the ABI-compatible bridge and enables polling. */
    fun start(): Boolean {
        if (active) return true
        active = when (native.load()) {
            is FrameArkNative.LoadResult.Loaded -> true
            else -> false
        }
        return active
    }

    /** Stops polling; queued frames remain owned by Rust until its lifecycle stops. */
    fun stop() {
        active = false
        pending = null
        pendingSince = 0L
        resetOutput()
    }

    /** Whether this pump will poll the native queue. */
    fun isActive(): Boolean = active

    /**
     * Dispatches at most the configured number of frames and returns the count
     * dispatched. A null poll or an inactive pump returns zero.
     */
    fun drainOnce(): Int {
        if (!active) return 0
        eventConsumer?.let { consume ->
            var count = 0
            while (count < drainLimit) {
                val event = pending ?: native.pollPlaybackEvent()?.also {
                    pending = it
                    pendingSince = nanoTime()
                } ?: break
                check(nanoTime() - pendingSince < INPUT_RETRY_BUDGET_NS) { "codec input retry deadline expired" }
                // Retain just this event and yield. No later config/frame/reset
                // may overtake it; no busy wait or extra encoded-frame queue.
                if (!consume(event)) break
                pending = null
                count++
            }
            drainOutput()
            return count
        }
        native.pollVideoConfig()?.let(videoConfigConsumer)
        native.pollAudioConfig()?.let(audioConfigConsumer)
        var dispatched = 0
        for (index in 0 until drainLimit) {
            val frame = native.pollMediaFrame() ?: break
            when (frame.kind) {
                FrameArkNative.MediaKind.VIDEO -> videoConsumer(frame)
                FrameArkNative.MediaKind.AUDIO -> audioConsumer(frame)
            }
            dispatched += 1
        }
        return dispatched
    }
}
