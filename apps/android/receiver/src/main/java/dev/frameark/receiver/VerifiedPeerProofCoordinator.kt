package dev.frameark.receiver

/**
 * Main-thread UI projection of Rust's verified proof queue. Rust owns expiry,
 * cancellation and admission; no protocol or trust state is duplicated here.
 * A prompt only permits one-session approval, never persistent enrollment.
 */
class VerifiedPeerProofCoordinator(
    private val native: FrameArkNative,
) {
    data class Prompt(val requestId: Long, val deviceId: String, val fingerprint: String)

    private var displayed: Prompt? = null

    /** Refreshes one prompt against Rust, discarding expired/cancelled requests. */
    fun refresh(): Prompt? {
        if (!native.isLoaded()) {
            displayed = null
            return null
        }
        displayed?.let { if (native.isPeerProofPending(it.requestId)) return it }
        displayed = null
        val event = native.pollPeerProof() ?: return null
        if (!native.isPeerProofPending(event.requestId)) return null
        return Prompt(
            event.requestId,
            event.peer.deviceId,
            DeviceIdentityPolicy.fingerprint(event.peer.publicKey),
        ).also { displayed = it }
    }

    /** Accepts a click only for the displayed request; Rust rechecks its lifetime. */
    fun decide(requestId: Long, approved: Boolean): Boolean {
        if (displayed?.requestId != requestId) return false
        displayed = null
        return native.decidePeerProof(requestId, approved)
    }

    /** Leaving the foreground dismisses and rejects the displayed session only. */
    fun dismiss() {
        val prompt = displayed ?: return
        displayed = null
        native.decidePeerProof(prompt.requestId, approved = false)
    }
}
