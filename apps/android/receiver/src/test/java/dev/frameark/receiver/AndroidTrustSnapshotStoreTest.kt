package dev.frameark.receiver

import java.io.File
import java.nio.file.Files
import javax.crypto.spec.SecretKeySpec
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidTrustSnapshotStoreTest {
    @Test
    fun persists_an_encrypted_snapshot_and_restores_through_rust_bridge() {
        val snapshot = byteArrayOf('F'.code.toByte(), 'T'.code.toByte(), 'R'.code.toByte(), '1'.code.toByte(), 0, 1, 0, 1)
        var restored: ByteArray? = null
        val key = SecretKeySpec(ByteArray(32) { it.toByte() }, "AES")
        val exporting = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            trustSnapshotExportProvider = { snapshot },
        )
        val importing = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            trustSnapshotImportProvider = { value -> restored = value; 0 },
        )
        val file = Files.createTempFile("frameark-trust", ".bin").toFile().also { it.delete() }
        try {
            assertTrue(AndroidTrustSnapshotStore(file, exporting, keyProvider = { key }).persist())
            assertTrue(file.readBytes().isNotEmpty())
            assertFalse(file.readBytes().contains('F'.code.toByte()) && file.readBytes().contains('T'.code.toByte()) && file.readBytes().contains('R'.code.toByte()) && file.readBytes().contains('1'.code.toByte()))
            assertTrue(AndroidTrustSnapshotStore(file, importing, keyProvider = { key }).restore())
            assertTrue(snapshot.contentEquals(restored))
        } finally {
            file.delete()
        }
    }

    @Test
    fun rejects_tampering_without_forwarding_plaintext_to_rust() {
        val key = SecretKeySpec(ByteArray(32) { (it + 1).toByte() }, "AES")
        var imports = 0
        val exporting = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            trustSnapshotExportProvider = { byteArrayOf('F'.code.toByte(), 'T'.code.toByte(), 'R'.code.toByte(), '1'.code.toByte(), 0, 1, 0, 1) },
        )
        val importing = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            trustSnapshotImportProvider = { imports += 1; 0 },
        )
        val file = Files.createTempFile("frameark-trust-tamper", ".bin").toFile().also { it.delete() }
        try {
            assertTrue(AndroidTrustSnapshotStore(file, exporting, keyProvider = { key }).persist())
            val bytes = file.readBytes()
            bytes[bytes.lastIndex] = (bytes.last().toInt() xor 0x01).toByte()
            file.writeBytes(bytes)
            assertFalse(AndroidTrustSnapshotStore(file, importing, keyProvider = { key }).restore())
            assertTrue(imports == 0)
        } finally {
            file.delete()
        }
    }
}
