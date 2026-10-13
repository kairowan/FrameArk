package dev.frameark.receiver

import android.app.Activity
import android.content.Intent
import android.graphics.SurfaceTexture
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Surface
import android.view.TextureView
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

/** Minimal receiver shell used to verify startup and native ABI reporting. */
class MainActivity : Activity() {
    private val proofHandler = Handler(Looper.getMainLooper())
    private var proofRequest: VerifiedPeerProofCoordinator.Prompt? = null
    private lateinit var proofCoordinator: VerifiedPeerProofCoordinator
    private lateinit var proofStatus: TextView
    private lateinit var approveProof: Button
    private lateinit var rejectProof: Button
    private lateinit var playbackStatus: TextView
    private var videoSurface: Surface? = null
    private val pollProof = object : Runnable {
        override fun run() {
            playbackStatus.text = ReceiverPlaybackOutput.diagnostic()
            val prompt = proofCoordinator.refresh()
            if (prompt != proofRequest) {
                proofStatus.text = if (prompt == null) getString(R.string.proof_waiting) else getString(
                    R.string.proof_pending, prompt.deviceId, prompt.fingerprint,
                )
            }
            proofRequest = prompt
            approveProof.isEnabled = prompt != null
            rejectProof.isEnabled = prompt != null
            proofHandler.postDelayed(this, PROOF_POLL_DELAY_MS)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val native = FrameArkNative()
        val trustStore = AndroidTrustSnapshotStore(filesDir.resolve("frameark-trust.fats"), native)
        val trustSettings = TrustSettingsController(native, trustStore)
        proofCoordinator = VerifiedPeerProofCoordinator(native)
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
        val mutationStatus = TextView(this)
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
                dismissProof()
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
        val requirePairingApproval = Button(this).apply {
            text = getString(R.string.pairing_require_approval)
            setOnClickListener {
                mutationStatus.text = if (native.setPairingApprovalRequired(true)) {
                    getString(R.string.pairing_approval_required)
                } else {
                    getString(R.string.pairing_approval_rejected)
                }
            }
        }
        val approveNextPairing = Button(this).apply {
            text = getString(R.string.pairing_approve_next)
            setOnClickListener {
                mutationStatus.text = if (native.approveNextPairing()) {
                    getString(R.string.pairing_approval_armed)
                } else {
                    getString(R.string.pairing_approval_rejected)
                }
            }
        }
        val requireProof = Button(this).apply {
            text = getString(R.string.proof_require)
            setOnClickListener {
                mutationStatus.text = if (native.setPeerProofRequired(true)) {
                    getString(R.string.proof_required)
                } else {
                    getString(R.string.proof_mode_rejected)
                }
            }
        }
        val disableProof = Button(this).apply {
            text = getString(R.string.proof_disable)
            setOnClickListener {
                mutationStatus.text = if (native.setPeerProofRequired(false)) {
                    getString(R.string.proof_disabled)
                } else {
                    getString(R.string.proof_mode_rejected)
                }
            }
        }
        proofStatus = TextView(this).apply {
            text = getString(R.string.proof_waiting)
            setPadding(32, 16, 32, 16)
        }
        approveProof = Button(this).apply {
            text = getString(R.string.proof_approve)
            isEnabled = false
            filterTouchesWhenObscured = true
            setOnClickListener { decideProof(approved = true) }
        }
        rejectProof = Button(this).apply {
            text = getString(R.string.proof_reject)
            isEnabled = false
            filterTouchesWhenObscured = true
            setOnClickListener { decideProof(approved = false) }
        }
        val warning = TextView(this).apply {
            text = getString(R.string.pairing_warning) + "\n" + getString(R.string.trust_warning)
            setPadding(32, 16, 32, 16)
        }
        val peerId = EditText(this).apply { hint = getString(R.string.trust_device_id_hint) }
        val fingerprint = EditText(this).apply { hint = getString(R.string.trust_fingerprint_hint) }
        val label = EditText(this).apply { hint = getString(R.string.trust_label_hint) }
        val approve = Button(this).apply {
            text = getString(R.string.trust_approve)
            setOnClickListener {
                mutationStatus.text = when (
                    trustSettings.approve(
                        FrameArkNative.TrustedPeer(
                            peerId.text.toString(),
                            fingerprint.text.toString(),
                            label.text.toString(),
                        ),
                    )
                ) {
                    TrustSettingsController.MutationResult.Applied -> getString(R.string.trust_applied)
                    is TrustSettingsController.MutationResult.Rejected -> getString(R.string.trust_rejected)
                }
            }
        }
        val revoke = Button(this).apply {
            text = getString(R.string.trust_revoke)
            setOnClickListener {
                mutationStatus.text = when (trustSettings.revoke(peerId.text.toString())) {
                    TrustSettingsController.MutationResult.Applied -> getString(R.string.trust_applied)
                    is TrustSettingsController.MutationResult.Rejected -> getString(R.string.trust_rejected)
                }
            }
        }
        val content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(statusView)
            addView(TextureView(this@MainActivity).apply {
                layoutParams = LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, (200 * resources.displayMetrics.density).toInt())
                surfaceTextureListener = object : TextureView.SurfaceTextureListener {
                    override fun onSurfaceTextureAvailable(texture: SurfaceTexture, width: Int, height: Int) {
                        videoSurface = Surface(texture).also(ReceiverPlaybackOutput::attach)
                    }
                    override fun onSurfaceTextureSizeChanged(texture: SurfaceTexture, width: Int, height: Int) = Unit
                    override fun onSurfaceTextureUpdated(texture: SurfaceTexture) = Unit
                    override fun onSurfaceTextureDestroyed(texture: SurfaceTexture): Boolean {
                        releaseVideoSurface()
                        stopService(Intent(this@MainActivity, FrameArkReceiverService::class.java))
                        return true
                    }
                }
            })
            playbackStatus = TextView(this@MainActivity).apply { text = ReceiverPlaybackOutput.diagnostic() }
            addView(playbackStatus)
            addView(mutationStatus)
            addView(proofStatus)
            addView(approveProof)
            addView(rejectProof)
            addView(start)
            addView(stop)
            addView(refresh)
            addView(requirePairingApproval)
            addView(approveNextPairing)
            addView(requireProof)
            addView(disableProof)
            addView(warning)
            addView(peerId)
            addView(fingerprint)
            addView(label)
            addView(approve)
            addView(revoke)
        }
        setContentView(ScrollView(this).apply { addView(content) })
    }

    override fun onResume() {
        super.onResume()
        proofHandler.post(pollProof)
    }

    override fun onPause() {
        proofHandler.removeCallbacksAndMessages(null)
        dismissProof()
        super.onPause()
    }

    private fun decideProof(approved: Boolean) {
        val prompt = proofRequest ?: return
        val accepted = proofCoordinator.decide(prompt.requestId, approved)
        proofRequest = null
        approveProof.isEnabled = false
        rejectProof.isEnabled = false
        proofStatus.text = getString(
            if (!accepted) R.string.proof_decision_failed else if (approved) {
                R.string.proof_approved
            } else R.string.proof_session_rejected,
        )
    }

    private fun dismissProof() {
        proofCoordinator.dismiss()
        proofRequest = null
        approveProof.isEnabled = false
        rejectProof.isEnabled = false
        proofStatus.text = getString(R.string.proof_waiting)
    }

    override fun onDestroy() {
        proofHandler.removeCallbacksAndMessages(null)
        dismissProof()
        releaseVideoSurface()
        super.onDestroy()
    }

    private fun releaseVideoSurface() {
        videoSurface?.let { surface ->
            try { ReceiverPlaybackOutput.detach(surface) } finally { surface.release() }
        }
        videoSurface = null
    }

    private companion object {
        const val NETWORK_REFRESH_DELAY_MS = 500L
        const val PROOF_POLL_DELAY_MS = 250L
    }
}
