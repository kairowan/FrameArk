package dev.frameark.sender

import java.util.concurrent.atomic.AtomicBoolean

/** OS producer facade. Rust is the only protocol/session state machine. */
class CaptureTransport(private val native: SenderNative, val id: Long, private val stop: AtomicBoolean, private val identity: SenderIdentity? = null) : AutoCloseable {
    fun awaitStreaming() {
        val deadline = System.nanoTime() + if (identity == null) 20_000_000_000L else 65_000_000_000L
        while (!stop.get() && native.status(id) == 1 && System.nanoTime() < deadline) {
            signPending(native, id, identity)
            Thread.sleep(10)
        }
        if (stop.get()) throw InterruptedException()
        check(native.status(id) == 2)
    }

    fun submit(payload: ByteArray, ptsUs: Long, video: Boolean, keyframe: Boolean) {
        val deadline = System.nanoTime() + 1_000_000_000L
        var result: Int
        do {
            result = native.submit(id, payload, ptsUs, video, keyframe)
            if (result == 1) Thread.sleep(2)
        } while (result == 1 && !stop.get() && System.nanoTime() < deadline)
        check(result == 0)
    }

    fun finish() {
        val deadline = System.nanoTime() + 7_000_000_000L
        var result: Int
        do {
            result = native.finish(id)
            if (result == 1) Thread.sleep(2)
        } while (result == 1 && System.nanoTime() < deadline)
        check(result == 0)
        while (native.status(id) == 2 && System.nanoTime() < deadline) Thread.sleep(10)
        check(native.status(id) == 3)
    }
    fun reconfigure(options: CaptureOptions) {
        val deadline = System.nanoTime() + 1_000_000_000L
        var result: Int
        do {
            if (stop.get()) throw InterruptedException()
            result = native.reconfigureVideo(id, options.width, options.height, options.fps)
            if (result == 1) Thread.sleep(2)
        } while (result == 1 && System.nanoTime() < deadline)
        check(result == 0)
    }
    fun stage(): Int = native.status(id)
    override fun close() { if (native.status(id) != 3) native.cancel(id) }
}
