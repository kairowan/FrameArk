package dev.frameark.receiver

/** OS output geometry; never modifies negotiated media or session policy. */
data class VideoDisplaySize(val width: Int, val height: Int) {
    init { require(width in 1..8192 && height in 1..8192) }
    fun fit(viewWidth: Int, viewHeight: Int): Pair<Float, Float> {
        require(viewWidth > 0 && viewHeight > 0)
        val scale = minOf(viewWidth.toDouble() / width, viewHeight.toDouble() / height)
        return (width * scale / viewWidth).toFloat() to (height * scale / viewHeight).toFloat()
    }
}
