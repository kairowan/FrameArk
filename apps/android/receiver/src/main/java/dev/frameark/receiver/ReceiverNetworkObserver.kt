package dev.frameark.receiver

/**
 * Debounces Android connectivity callbacks into one bounded Rust-owned
 * receiver recovery request. It deliberately carries no network/session
 * state and does not open sockets.
 */
class ReceiverNetworkObserver(
    private val register: (Listener) -> Unit,
    private val unregister: (Listener) -> Unit,
    private val onNetworkChanged: () -> Unit,
) : AutoCloseable {
    interface Listener {
        fun onAvailable()

        fun onLost()

        fun onCapabilitiesChanged()

        fun onLinkPropertiesChanged()
    }

    private val lock = Any()
    private var started = false
    private var recoveryPending = false
    private val listener = object : Listener {
        override fun onAvailable() = changed()

        override fun onLost() = changed()

        override fun onCapabilitiesChanged() = changed()

        override fun onLinkPropertiesChanged() = changed()
    }

    /** Registers the platform callback once; returns false if registration fails. */
    fun start(): Boolean = synchronized(lock) {
        if (started) return true
        val registered = runCatching { register(listener) }.isSuccess
        if (registered) started = true
        registered
    }

    /** Marks the current recovery attempt complete so a later change can retry. */
    fun markRecoveryComplete() = synchronized(lock) {
        recoveryPending = false
    }

    /** Unregisters the callback and forgets pending notifications. */
    fun stop() = synchronized(lock) {
        if (started) runCatching { unregister(listener) }
        started = false
        recoveryPending = false
    }

    override fun close() = stop()

    private fun changed() {
        val notify = synchronized(lock) {
            if (!started || recoveryPending) {
                false
            } else {
                recoveryPending = true
                true
            }
        }
        if (notify) {
            runCatching { onNetworkChanged() }
                .onFailure { markRecoveryComplete() }
        }
    }
}
