package dev.frameark.receiver

/**
 * Owns the Android-side hand-off between a Rust-verified sender proof and an
 * explicit user trust decision.
 *
 * Rust remains responsible for challenge/signature verification and the
 * persistent trust registry. This queue only retains a bounded public-key
 * request until the user approves or rejects it. No private key or proof
 * material is stored here.
 */
class VerifiedPeerApprovalQueue(
    private val trustSettings: TrustSettingsController,
    private val nowMillis: () -> Long = { System.currentTimeMillis() },
    private val maxPending: Int = MAX_PENDING,
    private val ttlMillis: Long = REQUEST_TTL_MILLIS,
) {
    init {
        require(maxPending in 1..MAX_PENDING) { "pending approval capacity is invalid" }
        require(ttlMillis in MIN_TTL_MILLIS..MAX_TTL_MILLIS) { "pending approval TTL is invalid" }
    }

    data class Pending(
        val peer: FrameArkNative.VerifiedPeerKey,
        val expiresAtMillis: Long,
    )

    sealed interface EnqueueResult {
        data object Queued : EnqueueResult

        data object Refreshed : EnqueueResult

        data object CapacityReached : EnqueueResult

        data object Invalid : EnqueueResult

        data object IdentityChanged : EnqueueResult
    }

    sealed interface DecisionResult {
        data object Enrolled : DecisionResult

        data object Rejected : DecisionResult

        data object Expired : DecisionResult

        data object Unknown : DecisionResult

        data object ReceiverRunning : DecisionResult

        data object Invalid : DecisionResult

        data object PersistenceFailed : DecisionResult
    }

    private val pending = LinkedHashMap<String, Pending>()

    /**
     * Enqueues a sender key only after the Rust transport has verified its
     * proof. The key is copied so callers cannot mutate an approval request.
     */
    fun enqueueProof(peer: FrameArkNative.VerifiedPeerKey): EnqueueResult {
        val now = nowMillis()
        expire(now)
        if (!isValid(peer)) return EnqueueResult.Invalid
        val copy = peer.copy(publicKey = peer.publicKey.copyOf())
        val previous = pending[peer.deviceId]
        if (previous != null) {
            if (!previous.peer.publicKey.contentEquals(peer.publicKey)) {
                return EnqueueResult.IdentityChanged
            }
            pending[peer.deviceId] = Pending(copy, expiry(now))
            return EnqueueResult.Refreshed
        }
        if (pending.size >= maxPending) return EnqueueResult.CapacityReached
        pending[peer.deviceId] = Pending(copy, expiry(now))
        return EnqueueResult.Queued
    }

    /** Returns a defensive snapshot and removes expired requests first. */
    fun pending(now: Long = nowMillis()): List<Pending> {
        expire(now)
        return pending.values.map { it.copy(peer = it.peer.copy(publicKey = it.peer.publicKey.copyOf())) }
    }

    /** Approves one request; persistence failure keeps it available for retry. */
    fun approve(deviceId: String, now: Long = nowMillis()): DecisionResult {
        if (pending[deviceId]?.expiresAtMillis?.let { it <= now } == true) {
            pending.remove(deviceId)
            expire(now)
            return DecisionResult.Expired
        }
        expire(now)
        val request = pending[deviceId] ?: return DecisionResult.Unknown
        return when (val result = trustSettings.approve(request.peer)) {
            TrustSettingsController.MutationResult.Applied -> {
                pending.remove(deviceId)
                DecisionResult.Enrolled
            }

            is TrustSettingsController.MutationResult.Rejected -> when (result.reason) {
                TrustSettingsController.Reason.RECEIVER_RUNNING -> DecisionResult.ReceiverRunning
                TrustSettingsController.Reason.INVALID_PEER -> DecisionResult.Invalid
                TrustSettingsController.Reason.PERSISTENCE_FAILED -> DecisionResult.PersistenceFailed
            }
        }
    }

    /** Rejects one request without crossing into Rust trust state. */
    fun reject(deviceId: String, now: Long = nowMillis()): DecisionResult {
        if (pending[deviceId]?.expiresAtMillis?.let { it <= now } == true) {
            pending.remove(deviceId)
            expire(now)
            return DecisionResult.Expired
        }
        expire(now)
        return if (pending.remove(deviceId) != null) DecisionResult.Rejected else DecisionResult.Unknown
    }

    /** Removes expired requests and returns the number removed. */
    fun expire(now: Long = nowMillis()): Int {
        val expired = pending.entries
            .filter { it.value.expiresAtMillis <= now }
            .map { it.key }
        expired.forEach(pending::remove)
        return expired.size
    }

    private fun expiry(now: Long): Long = now.saturatingAdd(ttlMillis)

    private fun isValid(peer: FrameArkNative.VerifiedPeerKey): Boolean =
        peer.deviceId.isNotBlank() &&
            peer.deviceId.length <= MAX_ID_LENGTH &&
            peer.publicKey.size == PUBLIC_KEY_BYTES &&
            peer.label.isNotBlank() &&
            peer.label.length <= MAX_LABEL_LENGTH

    private fun Long.saturatingAdd(value: Long): Long =
        if (Long.MAX_VALUE - this < value) Long.MAX_VALUE else this + value

    companion object {
        const val MAX_PENDING = 16
        const val PUBLIC_KEY_BYTES = 32
        const val MAX_ID_LENGTH = 128
        const val MAX_LABEL_LENGTH = 128
        const val MIN_TTL_MILLIS = 1_000L
        const val REQUEST_TTL_MILLIS = 60_000L
        const val MAX_TTL_MILLIS = 5 * 60_000L
    }
}
