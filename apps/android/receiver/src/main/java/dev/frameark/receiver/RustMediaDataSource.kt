package dev.frameark.receiver

import android.media.MediaDataSource
import java.io.IOException
import java.util.concurrent.atomic.AtomicBoolean

/** OS demuxer byte adapter only. Rust owns URL policy, TLS, ranges and cancellation. */
internal class RustMediaDataSource(
    private val sourceId: Long,
    private val access: DlnaSourceAccess = DlnaNative,
) : MediaDataSource() {
    private val closed = AtomicBoolean()

    init { require(sourceId > 0) }

    override fun getSize(): Long {
        if (closed.get()) throw IOException("Media source closed")
        return access.sourceSize(sourceId).takeIf { it > 0 } ?: throw IOException("Media source unavailable")
    }

    override fun readAt(position: Long, buffer: ByteArray, offset: Int, size: Int): Int {
        if (closed.get()) throw IOException("Media source closed")
        if (position < 0 || offset < 0 || size < 0 || offset > buffer.size || size > buffer.size - offset) {
            throw IOException("Invalid media read bounds")
        }
        if (size == 0) return 0
        val requested = size.coerceAtMost(64 * 1024)
        val bytes = access.sourceRead(sourceId, position, requested) ?: throw IOException("Media source read failed")
        if (closed.get() || bytes.size > requested) throw IOException("Media source read invalidated")
        if (bytes.isEmpty()) return -1
        bytes.copyInto(buffer, offset)
        return bytes.size
    }

    override fun close() {
        if (closed.compareAndSet(false, true)) access.sourceClose(sourceId)
    }
}

internal interface DlnaSourceAccess {
    fun sourceSize(id: Long): Long
    fun sourceRead(id: Long, position: Long, size: Int): ByteArray?
    fun sourceClose(id: Long)
}

/** The owning receiver loads and checks the existing FrameArk native library first. */
internal object DlnaNative : DlnaSourceAccess {
    external override fun sourceSize(id: Long): Long
    external override fun sourceRead(id: Long, position: Long, size: Int): ByteArray?
    external override fun sourceClose(id: Long)
    external fun outputToken(): Long
    external fun pollCommand(token: Long): LongArray?
    external fun acknowledge(id: Long, success: Boolean): Boolean
    external fun feedback(id: Long, kind: Int, positionMs: Long, durationMs: Long): Boolean
    external fun subtitleOptions(id: Long): Array<String>?
    external fun selectSubtitle(id: Long, index: Int): Boolean
    external fun subtitleText(id: Long, positionMs: Long): String?
    external fun start(bindAddress: String): Int
    external fun stop()
    external fun descriptionUrl(): String?
    external fun diagnostic(): String?
    // Symbol exists only in the explicitly opted-in instrumentation native build.
    external fun startProbe(): Int
}
