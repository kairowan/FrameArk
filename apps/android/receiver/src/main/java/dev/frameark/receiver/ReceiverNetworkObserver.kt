package dev.frameark.receiver

/**
 * Coalesces Android connectivity callbacks into one endpoint-view refresh.
 * It deliberately carries no protocol/session state, opens no sockets and
 * does not restart a native listener or report recovery success.
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
    private var changePending = false
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

    /** Allows the next notification after the queued refresh is handled. */
    fun markChangeHandled() = synchronized(lock) {
        changePending = false
    }

    /** Unregisters the callback and forgets pending notifications. */
    fun stop() = synchronized(lock) {
        if (started) runCatching { unregister(listener) }
        started = false
        changePending = false
    }

    override fun close() = stop()

    private fun changed() {
        val notify = synchronized(lock) {
            if (!started || changePending) {
                false
            } else {
                changePending = true
                true
            }
        }
        if (notify) {
            runCatching { onNetworkChanged() }
                .onFailure { markChangeHandled() }
        }
    }
}
