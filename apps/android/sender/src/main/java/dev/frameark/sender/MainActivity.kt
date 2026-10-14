package dev.frameark.sender

import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Point
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
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
    private lateinit var captureStatus: TextView
    private lateinit var discovery: DiscoveryPanel
    private lateinit var identityPanel: SenderIdentityPanel
    private var pendingCapture: CaptureRequest? = null
    private val captureHandler = Handler(Looper.getMainLooper())
    private val refreshCapture = object : Runnable {
        override fun run() {
            val value = CaptureStatus.snapshot
            captureStatus.text = getString(R.string.capture_progress, value.phase, value.seconds, value.video, value.audio, value.nonSilentAudio, value.failure)
            captureHandler.postDelayed(this, 500)
        }
    }

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
        identityPanel = SenderIdentityPanel(this, native, loaded) { pin.text.clear(); confirm.isChecked = false }
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
        discovery = DiscoveryPanel(this, native, loaded) { candidate ->
            ticket.setText(candidate)
            pin.text.clear()
            confirm.isChecked = false
            val result = native.summary(candidate)
            gate.inspect(candidate, result != null)
            summary.text = result ?: getString(R.string.invalid)
        }
        status = TextView(this).apply { tag = "sender-status"; if (!loaded) text = getString(R.string.unavailable) }
        start = Button(this).apply {
            text = getString(R.string.start)
            isEnabled = loaded
            filterTouchesWhenObscured = true
            setOnClickListener {
                val value = ticket.text.toString()
                val code = pin.text.toString()
                if (!identityPanel.isReady() || worker?.isAlive == true || CaptureStatus.snapshot.active || pendingCapture != null || !gate.permits(value, code, confirm.isChecked)) {
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
                val identity = identityPanel.selected()
                worker = Thread({ send(value, code, width, height, identity) }, "frameark-encoder").also { it.start() }
            }
        }
        val stop = Button(this).apply { text = getString(R.string.stop); setOnClickListener { cancel() } }
        val sixty = CheckBox(this).apply { text = getString(R.string.capture_sixty) }
        val playbackAudio = CheckBox(this).apply { text = getString(R.string.capture_audio); isSaveEnabled = false }
        captureStatus = TextView(this).apply { tag = "capture-status" }
        val capture = Button(this).apply {
            text = getString(R.string.capture_start)
            isEnabled = loaded && Build.VERSION.SDK_INT >= 29
            filterTouchesWhenObscured = true
            setOnClickListener {
                val value = ticket.text.toString()
                val code = pin.text.toString()
                if (!identityPanel.isReady() || worker?.isAlive == true || CaptureStatus.snapshot.active || pendingCapture != null || !gate.permits(value, code, confirm.isChecked)) {
                    status.text = getString(R.string.invalid)
                    return@setOnClickListener
                }
                val size = if (Build.VERSION.SDK_INT >= 30) windowManager.maximumWindowMetrics.bounds.let { Point(it.width(), it.height()) }
                    else Point().also { @Suppress("DEPRECATION") windowManager.defaultDisplay.getRealSize(it) }
                val options = runCatching { CaptureOptions.fit(size.x, size.y, fullHd.isChecked, if (sixty.isChecked) 60 else 30, playbackAudio.isChecked) }.getOrNull()
                if (options == null) { status.text = getString(R.string.capture_unsupported); return@setOnClickListener }
                pendingCapture = CaptureRequest(value, code, options, identityPanel.selected())
                pin.text.clear()
                confirm.isChecked = false
                authorizeCapture()
            }
        }
        val captureStop = Button(this).apply {
            text = getString(R.string.capture_stop)
            setOnClickListener {
                pendingCapture = null
                if (CaptureStatus.snapshot.active) startService(Intent(this@MainActivity, ScreenCaptureService::class.java).setAction(ScreenCaptureService.ACTION_STOP))
            }
        }
        val content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(24, 32, 24, 24)
            addView(TextView(this@MainActivity).apply { text = getString(R.string.title); textSize = 24f })
            addView(TextView(this@MainActivity).apply { text = getString(R.string.instructions) })
            addView(discovery)
            addView(identityPanel)
            listOf(ticket, inspect, summary, confirm, pin, fullHd, start, stop, status).forEach(::addView)
            addView(TextView(this@MainActivity).apply { text = getString(R.string.capture_instructions) })
            listOf(sixty, playbackAudio, capture, captureStop, captureStatus).forEach(::addView)
        }
        setContentView(ScrollView(this).apply { addView(content) })
    }

    private fun send(ticket: String, pin: String, width: Int, height: Int, identity: SenderIdentity?) {
        var id = 0L
        var result = getString(R.string.cancelled)
        try {
            if (cancelled.get()) return
            id = if (identity == null) native.start(ticket, pin, width, height)
                else native.startWithIdentity(ticket, pin, width, height, 30, true, true, identity.publicKeySpki())
            check(id > 0) { "setup:$id" }
            handle.set(id)
            if (cancelled.get()) native.cancel(id)
            val deadline = System.nanoTime() + if (identity == null) 20_000_000_000L else 65_000_000_000L
            while (!cancelled.get() && native.status(id) == 1 && System.nanoTime() < deadline) {
                signPending(native, id, identity)
                Thread.sleep(10)
            }
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
    private fun authorizeCapture() {
        val request = pendingCapture ?: return
        if (request.options.audio && checkSelfPermission(android.Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(android.Manifest.permission.RECORD_AUDIO), 41)
            return
        }
        if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 42)
            return
        }
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(getSystemService(MediaProjectionManager::class.java).createScreenCaptureIntent(), 43)
        } catch (_: Exception) { pendingCapture = null; status.text = getString(R.string.capture_unsupported) }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == 41 || requestCode == 42) {
            if (grantResults.size == 1 && grantResults[0] == PackageManager.PERMISSION_GRANTED) authorizeCapture()
            else { pendingCapture = null; status.text = getString(R.string.capture_denied) }
        }
    }

    @Deprecated("Platform activity result bridge")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != 43) return
        val request = pendingCapture
        pendingCapture = null
        if (request == null || resultCode != RESULT_OK || data == null) { status.text = getString(R.string.capture_denied); return }
        val id = ScreenCaptureService.handoff.put(request)
        try {
            startForegroundService(Intent(this, ScreenCaptureService::class.java).setAction(ScreenCaptureService.ACTION_START)
                .putExtra(ScreenCaptureService.EXTRA_HANDOFF, id).putExtra(ScreenCaptureService.EXTRA_GRANT, data))
            status.text = getString(R.string.capture_started)
        } catch (_: Exception) { ScreenCaptureService.handoff.discard(id); status.text = getString(R.string.capture_unsupported) }
    }

    override fun onResume() { super.onResume(); captureHandler.post(refreshCapture) }
    override fun onPause() { discovery.stop(); cancel(); captureHandler.removeCallbacks(refreshCapture); super.onPause() }
    override fun onDestroy() { discovery.stop(); identityPanel.dispose(); pendingCapture = null; captureHandler.removeCallbacks(refreshCapture); super.onDestroy() }
}
