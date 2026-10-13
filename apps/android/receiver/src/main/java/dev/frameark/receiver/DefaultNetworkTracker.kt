package dev.frameark.receiver

/** Filters default-network identity changes, not routine capability callbacks. */
class DefaultNetworkTracker<T>(initial: T?) {
    private var current = initial

    @Synchronized fun available(network: T): Boolean {
        if (current == network) return false
        current = network
        return true
    }

    @Synchronized fun lost(network: T): Boolean {
        if (current != network) return false
        current = null
        return true
    }
}
