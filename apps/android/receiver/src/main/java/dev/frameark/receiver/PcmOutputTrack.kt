package dev.frameark.receiver

/** Platform output seam; production delegates to one Android AudioTrack. */
interface PcmOutputTrack : AutoCloseable {
    fun setGain(gain: Float)
    fun play()
    fun write(bytes: ByteArray, offset: Int, size: Int): Int
}
