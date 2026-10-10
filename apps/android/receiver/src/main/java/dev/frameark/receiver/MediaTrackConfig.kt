package dev.frameark.receiver

/** Validated H.264 decoder configuration supplied by the Rust media session. */
data class VideoTrackConfig(
    val width: Int,
    val height: Int,
    val frameRateNumerator: Int,
    val frameRateDenominator: Int,
) {
    init {
        require(width > 0 && height > 0) { "video dimensions must be positive" }
        require(frameRateNumerator > 0 && frameRateDenominator > 0) {
            "video frame rate must be positive"
        }
    }

    val frameRate: Float
        get() = frameRateNumerator.toFloat() / frameRateDenominator.toFloat()
}

/** PCM output configuration after the negotiated audio decoder stage. */
data class PcmTrackConfig(
    val sampleRate: Int,
    val channels: Int,
) {
    init {
        require(sampleRate in 8_000..96_000) { "sample rate is outside the supported range" }
        require(channels in 1..8) { "channel count is outside the supported range" }
    }
}
