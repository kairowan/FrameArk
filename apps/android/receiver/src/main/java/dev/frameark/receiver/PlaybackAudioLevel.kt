package dev.frameark.receiver

/** Local output gain only, not FANP authorization or media/session state. */
data class PlaybackAudioLevel(val percent: Int = 50, val muted: Boolean = false) {
    init { require(percent in 0..100) { "audio level outside 0..100" } }
    val gain: Float get() = if (muted) 0f else percent / 100f
    fun stepped(delta: Int): PlaybackAudioLevel =
        copy(percent = (percent.toLong() + delta).coerceIn(0, 100).toInt())
}
