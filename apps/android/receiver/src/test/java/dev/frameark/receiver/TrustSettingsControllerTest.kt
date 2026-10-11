package dev.frameark.receiver

import java.nio.file.Files
import javax.crypto.spec.SecretKeySpec
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class TrustSettingsControllerTest {
    @Test
    fun approval_and_revoke_persist_only_when_receiver_is_stopped() {
        val fingerprint = List(32) { "ab" }.joinToString(":")
        val snapshot = byteArrayOf('F'.code.toByte(), 'T'.code.toByte(), 'R'.code.toByte(), '1'.code.toByte(), 0, 1, 0, 1)
        var trustCalls = 0
        var revokeCalls = 0
        var running = false
        val native = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            runningProvider = { if (running) 1 else 0 },
            trustPeerProvider = { _, _, _ -> trustCalls += 1; 0 },
            revokePeerProvider = { revokeCalls += 1; 0 },
            trustSnapshotExportProvider = { snapshot },
            trustSnapshotImportProvider = { 0 },
        )
        val file = Files.createTempFile("frameark-trust-controller", ".bin").toFile().also { it.delete() }
        val store = AndroidTrustSnapshotStore(
            file,
            native,
            keyProvider = { SecretKeySpec(ByteArray(32) { it.toByte() }, "AES") },
        )
        val controller = TrustSettingsController(native, store)
        try {
            assertEquals(
                TrustSettingsController.MutationResult.Applied,
                controller.approve(FrameArkNative.TrustedPeer("sender-1", fingerprint, "Phone")),
            )
            assertEquals(
                TrustSettingsController.MutationResult.Applied,
                controller.revoke("sender-1"),
            )
            assertEquals(1, trustCalls)
            assertEquals(1, revokeCalls)
            running = true
            assertEquals(
                TrustSettingsController.MutationResult.Rejected(TrustSettingsController.Reason.RECEIVER_RUNNING),
                controller.revoke("sender-1"),
            )
        } finally {
            assertTrue(file.delete() || !file.exists())
        }
    }
}
