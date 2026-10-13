package dev.frameark.receiver

/** Owns bounded PCM copies while a non-blocking AudioTrack is backpressured. */
class PcmWriteQueue {
    private val buffers = ArrayDeque<ByteArray>()
    private var offset = 0
    private var bytes = 0

    fun append(pcm: ByteArray) {
        require(pcm.isNotEmpty() && pcm.size <= 256 * 1024 - bytes && buffers.size < 64) {
            "PCM output queue exhausted"
        }
        buffers.addLast(pcm.copyOf())
        bytes += pcm.size
    }

    fun drain(write: (ByteArray, Int, Int) -> Int): Int {
        var completed = 0
        repeat(16) {
            val buffer = buffers.firstOrNull() ?: return completed
            val remaining = buffer.size - offset
            val count = write(buffer, offset, remaining)
            require(count in 0..remaining) { "audio output rejected PCM data" }
            if (count == 0) return completed
            offset += count
            if (offset == buffer.size) {
                buffers.removeFirst()
                bytes -= buffer.size
                offset = 0
                completed++
            }
        }
        return completed
    }

    fun clear() { buffers.clear(); offset = 0; bytes = 0 }
}
