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
) {
    companion object {
        const val DEFAULT_MAX_FRAMES_PER_DRAIN = 8
        const val MAX_FRAMES_PER_DRAIN = 32
    }

    private val drainLimit = maxFramesPerDrain.also {
        require(it in 1..MAX_FRAMES_PER_DRAIN) { "drain limit is outside the bounded range" }
    }
    private var active = false

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
    }

    /** Whether this pump will poll the native queue. */
    fun isActive(): Boolean = active

    /**
     * Dispatches at most the configured number of frames and returns the count
     * dispatched. A null poll or an inactive pump returns zero.
     */
    fun drainOnce(): Int {
        if (!active) return 0
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
