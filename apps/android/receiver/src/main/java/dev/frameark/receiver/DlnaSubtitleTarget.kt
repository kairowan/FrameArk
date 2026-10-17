package dev.frameark.receiver

import android.os.Handler
import android.os.Looper
import android.widget.TextView

/** Plain-text main-thread output only. Rust owns parsing, track selection and timing. */
internal class DlnaSubtitleTarget(private val view: TextView) : AutoCloseable {
    private val main = Handler(Looper.getMainLooper())
    private var owner = 0L
    private var open = true

    fun present(token: Long, text: String, current: () -> Boolean) {
        main.post {
            if (open && view.isAttachedToWindow && current()) {
                owner = token
                view.text = text // Never Html.fromHtml, WebView, links or remote font/image loading.
            }
        }
    }

    fun retire(token: Long) {
        main.post { if (owner == token) view.text = "" }
    }

    override fun close() {
        check(Looper.myLooper() == Looper.getMainLooper())
        open = false
        view.text = ""
    }
}
