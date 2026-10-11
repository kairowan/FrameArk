package dev.frameark.receiver

/**
 * Loads the Android Keystore identity once before the receiver lifecycle can
 * expose a network endpoint. The private key remains inside Keystore; only the
 * redaction-safe public identity is retained by this coordinator.
 */
class ReceiverIdentityCoordinator(
    private val loadIdentity: () -> DeviceIdentity,
) {
    sealed interface State {
        data object Uninitialized : State

        data class Ready(val identity: DeviceIdentity) : State

        data class Failed(val reason: FailureReason) : State
    }

    enum class FailureReason {
        KEYSTORE_UNAVAILABLE,
        INVALID_IDENTITY,
    }

    private val lock = Any()
    private var state: State = State.Uninitialized

    /** Returns the last redaction-safe identity state. */
    fun state(): State = synchronized(lock) { state }

    /** Loads and validates the identity exactly once until the process exits. */
    fun initialize(): State = synchronized(lock) {
        if (state !is State.Uninitialized) return state
        state = try {
            val identity = loadIdentity()
            DeviceIdentityPolicy.validateAlias(identity.alias)
            DeviceIdentityPolicy.validateFingerprint(identity.publicKeyFingerprint)
            State.Ready(identity)
        } catch (_: IllegalArgumentException) {
            State.Failed(FailureReason.INVALID_IDENTITY)
        } catch (_: Exception) {
            State.Failed(FailureReason.KEYSTORE_UNAVAILABLE)
        }
        state
    }
}
