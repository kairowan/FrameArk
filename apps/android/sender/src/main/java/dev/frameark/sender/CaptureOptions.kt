package dev.frameark.sender

/** Platform capture geometry, not protocol negotiation or session state. */
data class CaptureOptions(val width: Int, val height: Int, val fps: Int, val audio: Boolean, val seconds: Int = 3600) {
    init {
        require(width in 16..1920 && height in 16..1920 && width % 2 == 0 && height % 2 == 0)
        require(width.toLong() * height <= 1920L * 1080 && fps in listOf(30, 60) && seconds in 1..3600)
    }

    companion object {
        fun fit(sourceWidth: Int, sourceHeight: Int, fullHd: Boolean, fps: Int, audio: Boolean): CaptureOptions {
            require(sourceWidth in 16..16384 && sourceHeight in 16..16384)
            val landscape = sourceWidth >= sourceHeight
            val longEdge = if (fullHd) 1920 else 1280
            val shortEdge = if (fullHd) 1080 else 720
            val maxWidth = if (landscape) longEdge else shortEdge
            val maxHeight = if (landscape) shortEdge else longEdge
            val scale = minOf(1.0, maxWidth.toDouble() / sourceWidth, maxHeight.toDouble() / sourceHeight)
            return CaptureOptions((sourceWidth * scale).toInt() / 2 * 2, (sourceHeight * scale).toInt() / 2 * 2, fps, audio)
        }
    }
}

/** Deliberately not a data class: toString must never reveal credentials. */
class CaptureRequest(val ticket: String, val pin: String, val options: CaptureOptions) {
    override fun toString(): String = "CaptureRequest([REDACTED])"
}

/** One-use, process-memory handoff to the non-exported foreground service. */
class CaptureHandoff(private val clock: () -> Long = System::nanoTime) {
    private var next = 0L
    private var pending: Triple<Long, Long, CaptureRequest>? = null
    @Synchronized fun put(request: CaptureRequest): Long {
        val id = ++next
        pending = Triple(id, clock(), request)
        return id
    }
    @Synchronized fun take(id: Long): CaptureRequest? {
        val current = pending ?: return null
        if (current.first != id) return null
        pending = null
        val elapsed = clock() - current.second
        return current.third.takeIf { elapsed in 0..15_000_000_000L }
    }
    @Synchronized fun discard(id: Long) { if (pending?.first == id) pending = null }
}

/** Redacted process-local UI diagnostics; no input, token or media is retained. */
object CaptureStatus {
    data class Snapshot(val active: Boolean = false, val phase: String = "idle", val video: Int = 0, val audio: Int = 0, val nonSilentAudio: Int = 0, val seconds: Long = 0, val failure: String = "")
    @Volatile var snapshot = Snapshot()
        private set
    private var generation = 0L
    @Synchronized fun begin(): Long { generation++; snapshot = Snapshot(active = true, phase = "connecting"); return generation }
    @Synchronized fun publish(id: Long, value: Snapshot) { if (id == generation) snapshot = value }
    @Synchronized fun end(id: Long, failure: String) {
        if (id == generation) snapshot = snapshot.copy(active = false, phase = if (failure.isEmpty()) "stopped" else "failed", failure = failure)
    }
}
