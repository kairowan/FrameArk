package dev.frameark.receiver

import android.graphics.Bitmap
import android.os.Handler
import android.os.Looper
import android.view.View
import android.widget.ImageView

/** Main-thread display ownership. Stale worker/runtime callbacks cannot replace a newer picture. */
internal class DlnaImageTarget(private val view: ImageView) : AutoCloseable {
    private val main = Handler(Looper.getMainLooper())
    private var owner = 0L
    private var displayed: Bitmap? = null
    private var open = true

    fun present(token: Long, bitmap: Bitmap?, current: () -> Boolean, done: (Boolean) -> Unit) {
        main.post {
            val accepted = open && view.isAttachedToWindow && current()
            if (accepted) {
                owner = token
                displayed = bitmap
                view.setImageBitmap(bitmap)
                view.visibility = View.VISIBLE // null is an opaque black stop/prepared screen.
            }
            done(accepted)
        }
    }

    fun video(token: Long, current: () -> Boolean, done: (Boolean) -> Unit) {
        main.post {
            val accepted = open && view.isAttachedToWindow && current()
            if (accepted) {
                owner = token
                displayed = null
                view.setImageDrawable(null)
                view.visibility = View.GONE
            }
            done(accepted)
        }
    }

    fun retire(token: Long, bitmap: Bitmap?) {
        main.post {
            if (owner == token && (bitmap == null || displayed === bitmap)) {
                displayed = null
                view.setImageDrawable(null)
                view.visibility = View.VISIBLE
            }
            // A RenderThread display list can still hold the previously published bitmap.
            // Drop our references; let Android reclaim it after all rendering owners release it.
        }
    }

    override fun close() {
        check(Looper.myLooper() == Looper.getMainLooper())
        open = false
        displayed = null
        view.setImageDrawable(null)
        // Output ownership retains the prepared bitmap until its worker retires it.
    }
}
