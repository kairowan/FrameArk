package dev.frameark.sender

/** Rust owns discovery records/cache. This adapter only manages foreground UI/OS resources. */
interface DiscoveryBackend {
    fun discoveryStart(): Long
    fun discoveryStatus(id: Long): Int
    fun discoveryTickets(id: Long): Array<String>?
    fun discoveryStop(id: Long)
}

data class DiscoveryView(val state: Int, val tickets: List<String> = emptyList())

/** All calls on the UI thread. Never reconnect, persist, or authorize a selected candidate. */
class DiscoveryController(
    private val backend: DiscoveryBackend,
    private val acquire: () -> Unit,
    private val release: () -> Unit,
    private val render: (DiscoveryView) -> Unit,
) {
    private var id = 0L
    private var locked = false
    fun start() {
        stop()
        try {
            // Release is attempted even when the platform acquisition partially fails.
            locked = true
            acquire()
            id = backend.discoveryStart()
            check(id > 0)
            render(DiscoveryView(1))
        } catch (_: Exception) { fail() }
    }
    fun poll(): Boolean {
        if (id <= 0) return false
        try {
            val state = backend.discoveryStatus(id)
            check(state == 1 || state == 2)
            val tickets = checkNotNull(backend.discoveryTickets(id)).toList()
            check(tickets.size <= 32)
            if (state == 2) unlock()
            render(DiscoveryView(state, tickets))
            if (state == 2 && tickets.isEmpty()) { stop(false); return false }
            return true
        } catch (_: Exception) { fail(); return false }
    }
    fun stop(clear: Boolean = true) {
        val previous = id
        id = 0
        try { if (previous > 0) backend.discoveryStop(previous) }
        catch (_: Exception) { /* Rust's scan deadline still bounds its worker. */ }
        finally { unlock(); if (clear) render(DiscoveryView(0)) }
    }
    private fun unlock() {
        if (locked) {
            locked = false
            try { release() } catch (_: Exception) { /* Platform may have already released it. */ }
        }
    }
    private fun fail() { stop(false); render(DiscoveryView(-1)) }
}
