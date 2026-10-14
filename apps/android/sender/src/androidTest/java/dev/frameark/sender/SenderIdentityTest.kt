@file:Suppress("DEPRECATION")
package dev.frameark.sender

import android.test.InstrumentationTestCase
import dev.frameark.receiver.FrameArkNative
import java.net.Inet4Address
import java.net.NetworkInterface
import java.nio.ByteBuffer
import java.security.MessageDigest

/** Disposable emulator only. Real Keystore -> JNI signing queue -> pinned QUIC
 * -> Rust verification -> explicit decision/trust. No rendering/physical claim. */
class SenderIdentityTest : InstrumentationTestCase() {
    fun testKeystoreProofApprovalRejectionAndTrustedReconnect() {
        val sender = SenderNative()
        assertTrue(sender.load())
        val identity = SenderIdentity.load()
        assertTrue(identity.publicKeySpki().contentEquals(SenderIdentity.load().publicKeySpki()))
        val summary = sender.identitySummary(identity.publicKeySpki())!!
        val deviceId = summary.lineSequence().first()
        val receiver = FrameArkNative()
        val localAddress = NetworkInterface.getNetworkInterfaces().toList().flatMap { it.inetAddresses.toList() }
            .filterIsInstance<Inet4Address>().first { it.isSiteLocalAddress }.hostAddress!!
        assertEquals(0, receiver.nativeSetDeviceIdentity("test-receiver", List(32) { "ab" }.joinToString(":")))
        assertEquals(0, receiver.nativeSetPeerProofRequired(true))
        var verifiedKey: ByteArray? = null
        try {
            for (mode in listOf("approve", "deny", "cancel", "trusted", "revoked", "capture")) {
                if (mode == "trusted") assertEquals(0, receiver.nativeTrustPeerPublicKey(deviceId, verifiedKey!!, "Test-approved sender"))
                if (mode == "revoked") assertEquals(0, receiver.nativeRevokePeer(deviceId))
                assertEquals(0, receiver.nativeStartReceiver())
                var handle = 0L
                try {
                    val ticket = receiver.nativePairingTicket(localAddress)!!
                    val pin = "%06d".format(java.util.Locale.ROOT, receiver.nativeReceiverPairingCode())
                    handle = sender.startWithIdentity(ticket, pin, 1280, 720, if (mode == "capture") 60 else 30,
                        mode != "capture", mode != "capture", identity.publicKeySpki())
                    assertTrue(handle > 0)
                    assertNull(sender.pollSigningRequest(-1))
                    assertTrue(sender.submitSignature(-1, ByteArray(72)) != 0)
                    var proofSeen = false
                    val deadline = System.nanoTime() + 10_000_000_000L
                    while (sender.status(handle) == 1 && System.nanoTime() < deadline) {
                        signPending(sender, handle, identity)
                        receiver.nativePollPeerProof()?.let { event ->
                            assertFalse(proofSeen)
                            assertEquals(2, event[4].toInt())
                            val length = event[13].toInt() and 255
                            assertEquals(deviceId, event.copyOfRange(14, 14 + length).toString(Charsets.US_ASCII))
                            val publicKey = event.copyOfRange(14 + length, event.size)
                            assertEquals(65, publicKey.size)
                            val fingerprint = MessageDigest.getInstance("SHA-256").digest(publicKey).joinToString(":") { "%02x".format(it.toInt() and 255) }
                            assertTrue(summary.endsWith(fingerprint))
                            val request = ByteBuffer.wrap(event).getLong(5)
                            assertEquals(1, sender.status(handle))
                            assertFalse(mode == "trusted")
                            if (mode == "approve") { Thread.sleep(6_000); assertEquals(1, sender.status(handle)) }
                            if (mode == "cancel") sender.cancel(handle)
                            else {
                                assertEquals(0, receiver.nativeDecidePeerProof(request, mode != "deny"))
                                assertTrue(receiver.nativeDecidePeerProof(request, true) != 0)
                            }
                            verifiedKey = publicKey
                            proofSeen = true
                        }
                        Thread.sleep(10)
                    }
                    if (mode == "deny") assertTrue(sender.status(handle) < 0)
                    else if (mode == "cancel") assertEquals(4, sender.status(handle))
                    else assertEquals(2, sender.status(handle))
                    assertEquals(mode != "trusted", proofSeen)
                    assertNull(sender.pollSigningRequest(handle))
                    assertTrue(sender.submitSignature(handle, ByteArray(72)) != 0)
                } finally {
                    if (handle > 0) {
                        sender.cancel(handle)
                        val deadline = System.nanoTime() + 2_000_000_000L
                        while (sender.status(handle) in 1..2 && System.nanoTime() < deadline) Thread.sleep(10)
                    }
                    assertEquals(0, receiver.nativeStopReceiver())
                }
            }
        } finally {
            receiver.nativeStopReceiver()
            receiver.nativeRevokePeer(deviceId)
            receiver.nativeSetPeerProofRequired(false)
        }
    }
}
