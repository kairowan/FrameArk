package dev.frameark.sender

import android.app.Activity
import android.os.Bundle
import android.text.InputFilter
import android.text.InputType
import android.view.View
import android.view.WindowManager
import android.widget.*
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

class MainActivity : Activity() {
    private val native = SenderNative()
    private val cancelled = AtomicBoolean()
    private val handle = AtomicLong()
    private var worker: Thread? = null
    private lateinit var status: TextView
    private lateinit var start: Button

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        val loaded = native.load()
        val gate = SubmissionGate()
        val ticket = EditText(this).apply {
            hint = getString(R.string.ticket_hint)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            filters = arrayOf(InputFilter.LengthFilter(8256))
            isSaveEnabled = false
            importantForAutofill = View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS
            maxLines = 4
        }
        val pin = EditText(this).apply {
            hint = getString(R.string.pin_hint)
            inputType = InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_VARIATION_PASSWORD
            filters = arrayOf(InputFilter.LengthFilter(6))
            isSaveEnabled = false
            importantForAutofill = View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS
        }
        val summary = TextView(this)
        val confirm = CheckBox(this).apply { text = getString(R.string.confirm); isSaveEnabled = false; filterTouchesWhenObscured = true }
        val fullHd = CheckBox(this).apply { text = getString(R.string.full_hd); isChecked = true }
        val inspect = Button(this).apply {
            text = getString(R.string.inspect)
            isEnabled = loaded
            setOnClickListener {
                val value = ticket.text.toString()
                val result = native.summary(value)
                gate.inspect(value, result != null)
                confirm.isChecked = false
                summary.text = result ?: getString(R.string.invalid)
            }
        }
        status = TextView(this).apply { tag = "sender-status"; if (!loaded) text = getString(R.string.unavailable) }
        start = Button(this).apply {
            text = getString(R.string.start)
            isEnabled = loaded
            filterTouchesWhenObscured = true
            setOnClickListener {
                val value = ticket.text.toString()
                val code = pin.text.toString()
                if (worker?.isAlive == true || !gate.permits(value, code, confirm.isChecked)) {
                    status.text = getString(R.string.invalid)
                    return@setOnClickListener
                }
                val width = if (fullHd.isChecked) 1920 else 1280
                val height = if (fullHd.isChecked) 1080 else 720
                pin.text.clear()
                confirm.isChecked = false
                cancelled.set(false)
                isEnabled = false
                window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                status.text = getString(R.string.connecting)
                worker = Thread({ send(value, code, width, height) }, "frameark-encoder").also { it.start() }
            }
        }
        val stop = Button(this).apply { text = getString(R.string.stop); setOnClickListener { cancel() } }
        val content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(24, 32, 24, 24)
            addView(TextView(this@MainActivity).apply { text = getString(R.string.title); textSize = 24f })
            addView(TextView(this@MainActivity).apply { text = getString(R.string.instructions) })
            listOf(ticket, inspect, summary, confirm, pin, fullHd, start, stop, status).forEach(::addView)
        }
        setContentView(ScrollView(this).apply { addView(content) })
    }

    private fun send(ticket: String, pin: String, width: Int, height: Int) {
        var id = 0L
        var result = getString(R.string.cancelled)
        try {
            if (cancelled.get()) return
            id = native.start(ticket, pin, width, height)
            check(id > 0) { "setup:$id" }
            handle.set(id)
            if (cancelled.get()) native.cancel(id)
            val deadline = System.nanoTime() + 20_000_000_000L
            while (!cancelled.get() && native.status(id) == 1 && System.nanoTime() < deadline) Thread.sleep(10)
            if (cancelled.get()) return
            check(native.status(id) == 2) { "connect:${native.status(id)}" }
            ReferenceEncoder().run(width, height, 30, cancelled, { payload, pts, video, key ->
                val budget = System.nanoTime() + 1_000_000_000L
                var code: Int
                do {
                    if (cancelled.get()) throw InterruptedException()
                    code = native.submit(id, payload, pts, video, key)
                    if (code == 1) Thread.sleep(2)
                } while (code == 1 && System.nanoTime() < budget)
                check(code == 0) { "queue:${native.status(id)}" }
            }, { video, audio -> runOnUiThread { status.text = getString(R.string.progress, video, audio) } })
            if (cancelled.get()) return
            val end = System.nanoTime() + 7_000_000_000L
            while (native.finish(id) == 1 && System.nanoTime() < end && !cancelled.get()) Thread.sleep(5)
            while (native.status(id) == 2 && System.nanoTime() < end && !cancelled.get()) Thread.sleep(10)
            check(native.status(id) == 3) { "finish:${native.status(id)}" }
            result = getString(R.string.complete)
        } catch (failure: Exception) {
            // Source location only, never exception messages (credentials/addresses/media).
            val location = failure.stackTrace.firstOrNull { it.className.startsWith("dev.frameark.sender.") }
            result = getString(R.string.failed, "${location?.fileName}:${location?.lineNumber}; native=${if (id > 0) native.status(id) else id}")
        } finally {
            if (id > 0 && native.status(id) != 3) native.cancel(id)
            handle.compareAndSet(id, 0)
            val message = if (cancelled.get()) getString(R.string.cancelled) else result
            runOnUiThread {
                if (!isDestroyed) {
                    status.text = message
                    start.isEnabled = true
                    window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                }
            }
        }
    }

    private fun cancel() { cancelled.set(true); handle.get().takeIf { it > 0 }?.let(native::cancel) }
    override fun onPause() { cancel(); super.onPause() }
}
