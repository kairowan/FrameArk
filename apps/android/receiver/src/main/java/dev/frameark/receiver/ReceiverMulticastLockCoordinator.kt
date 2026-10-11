package dev.frameark.receiver

/**
 * Best-effort owner for Android's Wi-Fi multicast lock.
 *
 * mDNS publication remains optional and FANP can run over non-Wi-Fi links, so
 * failure to acquire the platform lock is reported to the caller but does not
 * throw or duplicate the lock on repeated service starts.
 */
class ReceiverMulticastLockCoordinator(
    private val acquireLock: () -> Unit,
    private val releaseLock: () -> Unit,
) : AutoCloseable {
    private val lock = Any()
    private var held = false

    /** Attempts one bounded acquisition and returns whether the lock is held. */
    fun acquire(): Boolean = synchronized(lock) {
        if (held) return true
        return try {
            acquireLock()
            held = true
            true
        } catch (_: Exception) {
            false
        }
    }

    /** Releases the lock exactly once when it was acquired. */
    fun release() = synchronized(lock) {
        if (!held) return
        runCatching { releaseLock() }
        held = false
    }

    fun isHeld(): Boolean = synchronized(lock) { held }

    override fun close() {
        release()
    }
}
