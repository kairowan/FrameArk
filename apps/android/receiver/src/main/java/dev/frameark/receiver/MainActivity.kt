package dev.frameark.receiver

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

/** Minimal receiver shell used to verify startup and native ABI reporting. */
class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val native = FrameArkNative()
        val nativeStatus = when (val result = native.load()) {
            is FrameArkNative.LoadResult.Loaded ->
                getString(R.string.native_loaded, result.abiVersion)

            is FrameArkNative.LoadResult.Incompatible ->
                getString(R.string.native_incompatible, result.expected, result.actual)

            is FrameArkNative.LoadResult.Unavailable ->
                getString(R.string.native_unavailable, result.reason)
        }

        val statusView = TextView(this).apply {
            textSize = 20f
            setPadding(32, 32, 32, 32)
        }
        fun refreshStatus() {
            val network = native.networkInfo()
            statusView.text = if (network == null) {
                getString(R.string.receiver_status, nativeStatus)
            } else {
                getString(
                    R.string.receiver_network_info,
                    network.port,
                    network.pairingCode,
                )
            }
        }
        refreshStatus()
        val start = Button(this).apply {
            text = getString(R.string.receiver_start)
            setOnClickListener {
                val intent = Intent(this@MainActivity, FrameArkReceiverService::class.java)
                    .setAction(ReceiverServicePolicy.ACTION_START)
                startService(intent)
                statusView.postDelayed({ refreshStatus() }, NETWORK_REFRESH_DELAY_MS)
            }
        }
        val stop = Button(this).apply {
            text = getString(R.string.receiver_stop)
            setOnClickListener {
                val intent = Intent(this@MainActivity, FrameArkReceiverService::class.java)
                    .setAction(ReceiverServicePolicy.ACTION_STOP)
                startService(intent)
                statusView.postDelayed({ refreshStatus() }, NETWORK_REFRESH_DELAY_MS)
            }
        }
        val refresh = Button(this).apply {
            text = getString(R.string.receiver_refresh)
            setOnClickListener { refreshStatus() }
        }
        setContentView(LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(statusView)
            addView(start)
            addView(stop)
            addView(refresh)
        })
    }

    private companion object {
        const val NETWORK_REFRESH_DELAY_MS = 500L
    }
}
