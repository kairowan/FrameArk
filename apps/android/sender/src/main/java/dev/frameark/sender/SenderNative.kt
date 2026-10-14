package dev.frameark.sender

/** Rust owns trust parsing, sessions, deadlines, timebases and bounded media queues. */
class SenderNative : DiscoveryBackend {
    override external fun discoveryStart(): Long
    override external fun discoveryStatus(id: Long): Int
    override external fun discoveryTickets(id: Long): Array<String>?
    override external fun discoveryStop(id: Long)
    fun load(): Boolean = runCatching { System.loadLibrary("frameark_ffi"); version() == 1 }.getOrDefault(false)
    private external fun version(): Int
    external fun summary(ticket: String): String?
    /** Positive generation handle or negative setup error. No automatic retry. */
    external fun start(ticket: String, pin: String, width: Int, height: Int): Long
    external fun startCapture(ticket: String, pin: String, width: Int, height: Int, fps: Int, audio: Boolean): Long
    external fun identitySummary(spki: ByteArray): String?
    external fun startWithIdentity(ticket: String, pin: String, width: Int, height: Int, fps: Int, audio: Boolean, reference: Boolean, spki: ByteArray): Long
    external fun pollSigningRequest(id: Long): ByteArray?
    external fun submitSignature(id: Long, signature: ByteArray): Int
    /** 1 connecting, 2 streaming, 3 finished, 4 cancelled, negative failure stage. */
    external fun status(id: Long): Int
    /** 0 accepted, 1 bounded backpressure, negative invalid/stale/not-streaming. */
    external fun submit(id: Long, payload: ByteArray, ptsUs: Long, video: Boolean, keyframe: Boolean): Int
    external fun finish(id: Long): Int
    /** Nonblocking; Rust drops in-progress transport within its cancellation poll. */
    external fun cancel(id: Long)
}
