package dev.frameark.receiver

/**
 * Attaches the Rust-owned FANP endpoint to the Android service lifecycle.
 *
 * The coordinator never opens a socket or copies protocol state. It only
 * snapshots the ephemeral endpoint exposed by the JNI bridge after native
 * startup, so the UI/service can present an explicit pairing affordance.
 */
class ReceiverNetworkCoordinator(
    private val native: FrameArkNative,
    private val onStateChanged: (State) -> Unit = {},
) : AutoCloseable {
    sealed interface State {
        data object Stopped : State

        data class Ready(val info: FrameArkNative.NetworkInfo) : State

        data class Failed(val reason: FailureReason) : State
    }

    enum class FailureReason {
        NATIVE_UNAVAILABLE,
        ENDPOINT_UNAVAILABLE,
    }

    private val lock = Any()
    private var state: State = State.Stopped

    /** Returns the last redaction-safe endpoint state. */
    fun state(): State = synchronized(lock) { state }

    /** Snapshots the endpoint after the native lifecycle has started. */
    fun refresh(): State = synchronized(lock) {
        if (!native.isLoaded()) {
            return transitionLocked(State.Failed(FailureReason.NATIVE_UNAVAILABLE))
        }
        val info = native.networkInfo()
            ?: return transitionLocked(State.Failed(FailureReason.ENDPOINT_UNAVAILABLE))
        transitionLocked(State.Ready(info))
    }

    /** Clears endpoint material when the native lifecycle stops. */
    fun stop() = synchronized(lock) {
        transitionLocked(State.Stopped)
    }

    override fun close() {
        stop()
    }

    private fun transitionLocked(next: State): State {
        state = next
        runCatching { onStateChanged(next) }
        return next
    }
}
