package dev.frameark.receiver

/**
 * Thin UI-facing adapter for explicit trust mutations. Rust remains the
 * authority for identity, key rotation, capacity, and revoke semantics; this
 * class only coordinates persistence and refuses mutations during streaming.
 */
class TrustSettingsController(
    private val native: FrameArkNative,
    private val snapshotStore: AndroidTrustSnapshotStore,
) {
    sealed interface MutationResult {
        data object Applied : MutationResult

        data class Rejected(val reason: Reason) : MutationResult
    }

    enum class Reason {
        RECEIVER_RUNNING,
        INVALID_PEER,
        PERSISTENCE_FAILED,
    }

    fun approve(peer: FrameArkNative.TrustedPeer): MutationResult {
        return approveMutation { native.trustPeer(peer) }
    }

    /**
     * Persists a proof-verified raw key only after this explicit UI approval.
     * The native bridge still owns canonical fingerprinting and key-rotation
     * policy; this controller owns the encrypted snapshot rollback boundary.
     */
    fun approve(peer: FrameArkNative.VerifiedPeerKey): MutationResult {
        return approveMutation { native.trustPeerPublicKey(peer) }
    }

    private fun approveMutation(apply: () -> Boolean): MutationResult {
        if (native.isReceiverRunning()) return MutationResult.Rejected(Reason.RECEIVER_RUNNING)
        val previous = native.exportTrustSnapshot()
            ?: return MutationResult.Rejected(Reason.PERSISTENCE_FAILED)
        if (!apply()) return MutationResult.Rejected(Reason.INVALID_PEER)
        if (snapshotStore.persist()) return MutationResult.Applied
        native.importTrustSnapshot(previous)
        return MutationResult.Rejected(Reason.PERSISTENCE_FAILED)
    }

    fun revoke(deviceId: String): MutationResult {
        if (native.isReceiverRunning()) return MutationResult.Rejected(Reason.RECEIVER_RUNNING)
        val previous = native.exportTrustSnapshot()
            ?: return MutationResult.Rejected(Reason.PERSISTENCE_FAILED)
        if (!native.revokePeer(deviceId)) return MutationResult.Rejected(Reason.INVALID_PEER)
        if (snapshotStore.persist()) return MutationResult.Applied
        native.importTrustSnapshot(previous)
        return MutationResult.Rejected(Reason.PERSISTENCE_FAILED)
    }
}
