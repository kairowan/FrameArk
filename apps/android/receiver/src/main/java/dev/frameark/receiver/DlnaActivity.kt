package dev.frameark.receiver

import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.graphics.Matrix
import android.graphics.Color
import android.graphics.SurfaceTexture
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Surface
import android.view.TextureView
import android.view.View
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.ScrollView
import android.widget.FrameLayout
import android.widget.ImageView
import java.net.Inet4Address
import java.net.NetworkInterface

/** Explicit, foreground-only Experimental LAN renderer UI. */
class DlnaActivity : Activity() {
    private val handler = Handler(Looper.getMainLooper())
    private lateinit var status: TextView
    private lateinit var video: TextureView
    private lateinit var images: DlnaImageTarget
    private var surface: Surface? = null
    private var nativeReady = false
    private val refresh = object : Runnable {
        override fun run() {
            val url = if (nativeReady) DlnaNative.descriptionUrl() else null
            status.text = when {
                !nativeReady -> getString(R.string.dlna_native_unavailable)
                DlnaOutputOwner.failure != 0 -> getString(R.string.dlna_failed, DlnaOutputOwner.failure)
                url == null -> getString(R.string.dlna_stopped)
                else -> getString(R.string.dlna_ready, url) + "\n" + (DlnaOutputOwner.output?.diagnostic() ?: "")
            }
            DlnaOutputOwner.output?.videoDisplaySize()?.let { size ->
                if (video.width > 0 && video.height > 0) {
                    val (x, y) = size.fit(video.width, video.height)
                    video.setTransform(Matrix().apply { setScale(x, y, video.width / 2f, video.height / 2f) })
                }
            }
            handler.postDelayed(this, 250)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        nativeReady = FrameArkNative().load() is FrameArkNative.LoadResult.Loaded
        status = TextView(this).apply { setPadding(24, 8, 24, 8); textSize = 12f; maxLines = 3 }
        video = TextureView(this).apply {
            tag = "dlna-video"
            surfaceTextureListener = object : TextureView.SurfaceTextureListener {
                override fun onSurfaceTextureAvailable(texture: SurfaceTexture, width: Int, height: Int) {
                    surface = Surface(texture)
                    DlnaOutputOwner.surface = surface
                }
                override fun onSurfaceTextureSizeChanged(texture: SurfaceTexture, width: Int, height: Int) = Unit
                override fun onSurfaceTextureUpdated(texture: SurfaceTexture) = Unit
                override fun onSurfaceTextureDestroyed(texture: SurfaceTexture): Boolean { stopRenderer(); releaseSurface(); return true }
            }
        }
        val picture = ImageView(this).apply {
            tag = "dlna-picture"
            scaleType = ImageView.ScaleType.FIT_CENTER
            setBackgroundColor(Color.BLACK)
            visibility = View.GONE
            contentDescription = getString(R.string.dlna_image_description)
        }
        images = DlnaImageTarget(picture)
        DlnaOutputOwner.images = images
        val start = Button(this).apply {
            text = getString(R.string.dlna_start)
            filterTouchesWhenObscured = true
            isEnabled = nativeReady
            setOnClickListener { chooseInterface() }
        }
        val stop = Button(this).apply { text = getString(R.string.dlna_stop); setOnClickListener { stopRenderer() } }
        val diagnostics = Button(this).apply {
            text = getString(R.string.dlna_diagnostics)
            setOnClickListener {
                val details = TextView(this@DlnaActivity).apply {
                    text = (DlnaOutputOwner.output?.diagnostic() ?: "") + "\n" + (if (nativeReady) DlnaNative.diagnostic() else null).orEmpty()
                    setPadding(24, 16, 24, 16)
                }
                AlertDialog.Builder(this@DlnaActivity).setTitle(R.string.dlna_diagnostics)
                    .setView(ScrollView(this@DlnaActivity).apply { addView(details) })
                    .setPositiveButton(android.R.string.ok, null).show()
            }
        }
        setContentView(LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(TextView(this@DlnaActivity).apply { text = getString(R.string.dlna_summary); textSize = 14f; maxLines = 3; setPadding(24, 8, 24, 8) })
            addView(LinearLayout(this@DlnaActivity).apply {
                for (button in listOf(start, stop, diagnostics)) addView(button, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
            })
            addView(status)
            addView(FrameLayout(this@DlnaActivity).apply {
                addView(video, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
                addView(picture, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
            }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f))
        })
    }

    private fun chooseInterface() {
        if (surface?.isValid != true) return
        val addresses = runCatching {
            NetworkInterface.getNetworkInterfaces().toList().filter { it.isUp && !it.isLoopback }
                .flatMap { it.inetAddresses.toList() }.filterIsInstance<Inet4Address>()
                .filter { it.isSiteLocalAddress }.mapNotNull { it.hostAddress }.distinct().take(16)
        }.getOrDefault(emptyList())
        if (addresses.isEmpty()) { status.text = getString(R.string.dlna_no_network); return }
        AlertDialog.Builder(this).setTitle(R.string.dlna_select_network)
            .setItems(addresses.toTypedArray()) { _, index ->
                AlertDialog.Builder(this).setTitle(R.string.dlna_title).setMessage(R.string.dlna_warning)
                    .setPositiveButton(R.string.dlna_start) { _, _ ->
                        DlnaOutputOwner.failure = 0
                        startForegroundService(Intent(this, DlnaReceiverService::class.java).putExtra(DlnaReceiverService.EXTRA_BIND, addresses[index]))
                    }.setNegativeButton(android.R.string.cancel, null).show()
            }.setNegativeButton(android.R.string.cancel, null).show()
    }

    private fun stopRenderer() {
        // A finishing Activity must not stop a newer page's output owner.
        if (DlnaOutputOwner.images === images) stopService(Intent(this, DlnaReceiverService::class.java))
    }
    private fun releaseSurface() {
        if (DlnaOutputOwner.surface === surface) DlnaOutputOwner.surface = null
        surface?.release()
        surface = null
    }

    override fun onResume() { super.onResume(); handler.post(refresh) }
    override fun onPause() { handler.removeCallbacksAndMessages(null); stopRenderer(); super.onPause() }
    override fun onDestroy() {
        handler.removeCallbacksAndMessages(null)
        stopRenderer()
        images.close()
        if (DlnaOutputOwner.images === images) DlnaOutputOwner.images = null
        releaseSurface()
        super.onDestroy()
    }
}
