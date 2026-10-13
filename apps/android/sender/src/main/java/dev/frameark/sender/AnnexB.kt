package dev.frameark.sender

/** Reference subset: Annex B only, bounded SPS/PPS repeated on each IDR. */
object AnnexB {
    private fun hasStartCode(bytes: ByteArray): Boolean = bytes.size >= 4 && bytes[0] == 0.toByte() &&
        bytes[1] == 0.toByte() && (bytes[2] == 1.toByte() || (bytes[2] == 0.toByte() && bytes[3] == 1.toByte()))

    fun accessUnit(config: ByteArray, payload: ByteArray, keyframe: Boolean): ByteArray {
        require(hasStartCode(payload) && payload.size <= 4 * 1024 * 1024)
        if (!keyframe) return payload
        require(config.size in 4..65536 && hasStartCode(config))
        require(config.size + payload.size <= 4 * 1024 * 1024)
        return config + payload
    }
}
