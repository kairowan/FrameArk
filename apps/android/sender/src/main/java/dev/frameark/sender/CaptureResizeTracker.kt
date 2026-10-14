package dev.frameark.sender

/** Bounded OS-event coalescing only; Rust negotiates every actual video update. */
class CaptureResizeTracker(private val clock: () -> Long = System::nanoTime) {
    private data class Size(val width: Int, val height: Int)
    private var pending: Size? = null
    private var changedAt = 0L

    @Synchronized fun update(width: Int, height: Int): Boolean {
        if (width !in 16..16384 || height !in 16..16384) return false
        val size = Size(width, height)
        if (pending != size) { pending = size; changedAt = clock() }
        return true
    }

    @Synchronized fun take(current: CaptureOptions): CaptureOptions? {
        val size = pending ?: return null
        val elapsed = clock() - changedAt
        if (elapsed < 250_000_000L) return null
        pending = null
        val next = current.resized(size.width, size.height)
        return next.takeIf { it.width != current.width || it.height != current.height }
    }
}
