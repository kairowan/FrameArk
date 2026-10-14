package dev.frameark.sender

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.widget.Button
import android.widget.CheckBox
import android.widget.LinearLayout
import android.widget.TextView

/** Identity is opt-in each Activity session. Changing mode invalidates PIN/confirmation. */
class SenderIdentityPanel(context: Context, native: SenderNative, loaded: Boolean, changed: () -> Unit) : LinearLayout(context) {
    private var identity: SenderIdentity? = null
    private val handler = Handler(Looper.getMainLooper())
    private var disposed = false
    private var loading = false
    private val enabled = CheckBox(context).apply {
        text = context.getString(R.string.identity_use)
        isSaveEnabled = false
        isEnabled = false
        filterTouchesWhenObscured = true
        setOnCheckedChangeListener { _, _ -> changed() }
    }
    init {
        orientation = VERTICAL
        val summary = TextView(context).apply { text = context.getString(R.string.identity_explanation); tag = "identity-summary" }
        val load = Button(context).apply { text = context.getString(R.string.identity_load); isEnabled = loaded; filterTouchesWhenObscured = true }
        load.setOnClickListener {
            // Reload may fail or return a changed OS key. Never leave a checked
            // mode with no key, or reuse a PIN/confirmation for a replacement.
            enabled.isChecked = false
            enabled.isEnabled = false
            identity = null
            changed()
            loading = true
            load.isEnabled = false
            val started = SenderIdentityLoader.load { value ->
                val description = runCatching { value?.let { native.identitySummary(it.publicKeySpki()) } }.getOrNull()
                handler.post {
                    if (!disposed) {
                        loading = false
                        identity = value.takeIf { description != null }
                        summary.text = description ?: context.getString(R.string.identity_failed)
                        enabled.isEnabled = identity != null
                        load.isEnabled = loaded
                    }
                }
            }
            if (!started) { loading = false; load.isEnabled = loaded; summary.text = context.getString(R.string.identity_failed) }
        }
        addView(summary); addView(load); addView(enabled)
    }
    fun selected(): SenderIdentity? = if (enabled.isChecked) checkNotNull(identity) else null
    fun isReady(): Boolean = !loading && !disposed
    fun dispose() { disposed = true; identity = null; handler.removeCallbacksAndMessages(null) }
}
