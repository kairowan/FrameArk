package dev.frameark.receiver

import java.nio.file.Files
import javax.crypto.spec.SecretKeySpec
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class VerifiedPeerApprovalQueueTest {
    @Test
    fun proof_requests_are_copied_refreshed_and_enrolled_once() {
        var now = 1_000L
        var trustCalls = 0
        val fixture = fixture(
            running = { false },
            trust = { trustCalls += 1; 0 },
        )
        val queue = VerifiedPeerApprovalQueue(fixture.controller, nowMillis = { now })
        val key = ByteArray(32) { it.toByte() }
        val peer = FrameArkNative.VerifiedPeerKey("sender-1", key, "Phone")
        try {
            assertEquals(VerifiedPeerApprovalQueue.EnqueueResult.Queued, queue.enqueueProof(peer))
            key[0] = 99
            assertEquals(0, queue.pending()[0].peer.publicKey[0].toInt())
            assertEquals(
                VerifiedPeerApprovalQueue.EnqueueResult.Refreshed,
                queue.enqueueProof(FrameArkNative.VerifiedPeerKey("sender-1", ByteArray(32) { it.toByte() }, "Phone")),
            )
            assertEquals(
                VerifiedPeerApprovalQueue.EnqueueResult.IdentityChanged,
                queue.enqueueProof(FrameArkNative.VerifiedPeerKey("sender-1", ByteArray(32) { (it + 1).toByte() }, "Phone")),
            )
            assertEquals(VerifiedPeerApprovalQueue.DecisionResult.Enrolled, queue.approve("sender-1"))
            assertEquals(1, trustCalls)
            assertTrue(queue.pending().isEmpty())
        } finally {
            fixture.close()
        }
    }

    @Test
    fun expired_requests_cannot_be_approved_or_rejected() {
        var now = 10_000L
        val fixture = fixture(running = { false }, trust = { 0 })
        val queue = VerifiedPeerApprovalQueue(
            fixture.controller,
            nowMillis = { now },
            ttlMillis = VerifiedPeerApprovalQueue.MIN_TTL_MILLIS,
        )
        try {
            assertEquals(
                VerifiedPeerApprovalQueue.EnqueueResult.Queued,
                queue.enqueueProof(FrameArkNative.VerifiedPeerKey("sender-1", ByteArray(32), "Phone")),
            )
            now += VerifiedPeerApprovalQueue.MIN_TTL_MILLIS
            assertEquals(VerifiedPeerApprovalQueue.DecisionResult.Expired, queue.approve("sender-1"))
            assertEquals(VerifiedPeerApprovalQueue.DecisionResult.Unknown, queue.reject("sender-1"))
            assertTrue(queue.pending().isEmpty())
        } finally {
            fixture.close()
        }
    }

    @Test
    fun capacity_and_receiver_running_are_explicit() {
        val now = 20_000L
        var running = true
        val fixture = fixture(running = { running }, trust = { 0 })
        val queue = VerifiedPeerApprovalQueue(fixture.controller, nowMillis = { now }, maxPending = 2)
        try {
            repeat(2) { index ->
                assertEquals(
                    VerifiedPeerApprovalQueue.EnqueueResult.Queued,
                    queue.enqueueProof(
                        FrameArkNative.VerifiedPeerKey("sender-$index", ByteArray(32), "Phone $index"),
                    ),
                )
            }
            assertEquals(
                VerifiedPeerApprovalQueue.EnqueueResult.CapacityReached,
                queue.enqueueProof(FrameArkNative.VerifiedPeerKey("sender-3", ByteArray(32), "Phone 3")),
            )
            assertEquals(VerifiedPeerApprovalQueue.DecisionResult.ReceiverRunning, queue.approve("sender-0"))
            running = false
            assertEquals(VerifiedPeerApprovalQueue.DecisionResult.Enrolled, queue.approve("sender-0"))
            assertFalse(queue.pending().any { it.peer.deviceId == "sender-0" })
        } finally {
            fixture.close()
        }
    }

    private fun fixture(
        running: () -> Boolean,
        trust: () -> Int,
    ): Fixture {
        val snapshot = byteArrayOf('F'.code.toByte(), 'T'.code.toByte(), 'R'.code.toByte(), '1'.code.toByte(), 0, 1, 0, 1)
        val native = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            runningProvider = { if (running()) 1 else 0 },
            trustPeerPublicKeyProvider = { _, _, _ -> trust() },
            trustSnapshotExportProvider = { snapshot },
            trustSnapshotImportProvider = { 0 },
        )
        val file = Files.createTempFile("frameark-proof-queue", ".bin").toFile().also { it.delete() }
        val store = AndroidTrustSnapshotStore(
            file,
            native,
            keyProvider = { SecretKeySpec(ByteArray(32) { it.toByte() }, "AES") },
        )
        native.load()
        return Fixture(TrustSettingsController(native, store), file)
    }

    private data class Fixture(
        val controller: TrustSettingsController,
        val file: java.io.File,
    ) {
        fun close() {
            assertTrue(file.delete() || !file.exists())
        }
    }
}
