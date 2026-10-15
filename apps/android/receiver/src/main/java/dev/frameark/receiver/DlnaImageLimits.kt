package dev.frameark.receiver

/** OS image-decoder allocation bounds, not URL authorization or protocol state. */
internal object DlnaImageLimits {
    const val MAX_ENCODED_BYTES = 16 * 1024 * 1024
    const val MAX_DECODED_PIXELS = 4 * 1024 * 1024
    const val MAX_DECODED_EDGE = 4096

    fun length(value: Long): Int {
        require(value in 1L..MAX_ENCODED_BYTES.toLong())
        return value.toInt()
    }

    fun sampleSize(width: Int, height: Int): Int {
        require(width in 1..32768 && height in 1..32768)
        require(width.toLong() * height <= 100_000_000L)
        var sample = 1
        while (true) {
            val w = (width + sample - 1) / sample
            val h = (height + sample - 1) / sample
            if (w <= MAX_DECODED_EDGE && h <= MAX_DECODED_EDGE && w.toLong() * h <= MAX_DECODED_PIXELS) return sample
            sample *= 2
        }
    }

    fun mime(kind: Long): String = when (kind) {
        1L -> "image/jpeg"
        2L -> "image/png"
        else -> error("Unsupported still image kind")
    }

    fun hasSignature(bytes: ByteArray, kind: Long): Boolean = when (kind) {
        1L -> bytes.size >= 3 && bytes[0] == 0xff.toByte() && bytes[1] == 0xd8.toByte() && bytes[2] == 0xff.toByte()
        2L -> bytes.size >= 8 && bytes.take(8) == listOf(137, 80, 78, 71, 13, 10, 26, 10).map(Int::toByte)
        else -> false
    }
}
