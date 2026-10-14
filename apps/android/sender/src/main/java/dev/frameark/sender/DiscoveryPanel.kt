package dev.frameark.sender

import android.app.Activity
import android.net.wifi.WifiManager
import android.os.Handler
import android.os.Looper
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

/** Foreground-only multicast lock and untrusted candidate list; no network parsing in Kotlin. */
class DiscoveryPanel(
    activity: Activity, native: SenderNative, loaded: Boolean, select: (String) -> Unit,
) : LinearLayout(activity) {
    private val handler = Handler(Looper.getMainLooper())
    private var multicast: WifiManager.MulticastLock? = null
    private val message = TextView(activity).apply { tag = "discovery-status" }
    private val choices = LinearLayout(activity).apply { orientation = VERTICAL; tag = "discovery-choices" }
    private var displayed = emptyList<String>()
    private val controller = DiscoveryController(native, {
        val manager = activity.applicationContext.getSystemService(WifiManager::class.java)
        multicast = manager.createMulticastLock("frameark.sender.discovery").apply { setReferenceCounted(false) }
        multicast?.acquire()
    }, {
        multicast?.let { if (it.isHeld) it.release() }
        multicast = null
    }, { view ->
        message.text = activity.getString(when (view.state) {
            1 -> R.string.discovery_scanning
            2 -> R.string.discovery_complete
            -1 -> R.string.discovery_failed
            else -> R.string.discovery_idle
        }, view.tickets.size)
        if (displayed != view.tickets) {
            displayed = view.tickets
            choices.removeAllViews()
            for (ticket in view.tickets) {
                val summary = native.summary(ticket) ?: continue
                choices.addView(Button(activity).apply {
                    // Numeric endpoint only; remote service names cannot impersonate trusted UI.
                    text = activity.getString(R.string.discovery_candidate, summary.substringBefore('\n'))
                    filterTouchesWhenObscured = true
                    setOnClickListener { select(ticket) }
                })
            }
        }
    })
    private val refresh = object : Runnable {
        override fun run() { if (controller.poll()) handler.postDelayed(this, 500) }
    }
    init {
        orientation = VERTICAL
        addView(TextView(activity).apply { text = activity.getString(R.string.discovery_warning) })
        addView(Button(activity).apply {
            text = activity.getString(R.string.discovery_start); isEnabled = loaded
            setOnClickListener { handler.removeCallbacks(refresh); controller.start(); handler.post(refresh) }
        })
        addView(Button(activity).apply {
            text = activity.getString(R.string.discovery_stop)
            setOnClickListener { stop() }
        })
        addView(message); addView(choices)
    }
    fun stop() { handler.removeCallbacks(refresh); controller.stop() }
}
