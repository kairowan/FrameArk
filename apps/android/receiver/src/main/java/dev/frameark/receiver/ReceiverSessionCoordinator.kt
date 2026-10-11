package dev.frameark.receiver

/**
 * Coordinates one Android receiver lifecycle without duplicating FANP state.
 *
 * Rust remains the owner of the protocol/session state. This class only
 * sequences the native lifecycle bridge and the Android polling loop, and
 * reports redacted lifecycle outcomes to the Service/UI layer.
 */
class ReceiverSessionCoordinator(
    private val native: FrameArkNative,
    private val playbackLoop: PlaybackController,
    private val onStateChanged: (State) -> Unit = {},
) : AutoCloseable {
    sealed interface State {
        data object Stopped : State

        data object Starting : State

        data object Running : State

        data class Failed(val reason: FailureReason) : State
    }

    enum class FailureReason {
        NATIVE_UNAVAILABLE,
        NATIVE_START_REJECTED,
        PLAYBACK_LOOP_UNAVAILABLE,
        PLAYBACK_LOOP_FAILED,
    }

    private val lock = Any()
    private var state: State = State.Stopped

    /** Returns the last redacted lifecycle state. */
    fun state(): State = synchronized(lock) { state }

    /** Starts native lifecycle and playback polling exactly once. */
    fun start(): Boolean = synchronized(lock) {
        when (state) {
            State.Running, State.Starting -> return true
            is State.Failed -> stopLocked()
            State.Stopped -> Unit
        }
        transitionLocked(State.Starting)
        when (val result = native.startReceiver()) {
            is FrameArkNative.ReceiverResult.Started,
            is FrameArkNative.ReceiverResult.AlreadyStarted,
            -> Unit

            is FrameArkNative.ReceiverResult.Unavailable -> {
                transitionLocked(State.Failed(FailureReason.NATIVE_UNAVAILABLE))
                return false
            }

            is FrameArkNative.ReceiverResult.Failed -> {
                transitionLocked(State.Failed(FailureReason.NATIVE_START_REJECTED))
                return false
            }

            is FrameArkNative.ReceiverResult.Stopped -> {
                transitionLocked(State.Failed(FailureReason.NATIVE_START_REJECTED))
                return false
            }
        }
        if (!playbackLoop.start()) {
            native.stopReceiver()
            transitionLocked(State.Failed(FailureReason.PLAYBACK_LOOP_UNAVAILABLE))
            return false
        }
        transitionLocked(State.Running)
        true
    }

    /** Stops polling and releases the Rust-owned lifecycle, safely repeated. */
    fun stop() = synchronized(lock) { stopLocked() }

    /** Called by the polling loop when a platform consumer fails. */
    fun onPlaybackFailure() = synchronized(lock) {
        playbackLoop.stop()
        native.stopReceiver()
        transitionLocked(State.Failed(FailureReason.PLAYBACK_LOOP_FAILED))
    }

    override fun close() = synchronized(lock) {
        playbackLoop.close()
        native.stopReceiver()
        transitionLocked(State.Stopped)
    }

    private fun stopLocked() {
        playbackLoop.stop()
        native.stopReceiver()
        transitionLocked(State.Stopped)
    }

    private fun transitionLocked(next: State) {
        state = next
        runCatching { onStateChanged(next) }
    }
}

/** Small seam for testing and for future surface/audio pipeline owners. */
interface PlaybackController {
    fun start(): Boolean

    fun stop()

    fun close()
}
