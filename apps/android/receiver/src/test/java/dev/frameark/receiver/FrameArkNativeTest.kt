package dev.frameark.receiver

import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertArrayEquals

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class FrameArkNativeTest {
    @Test
    fun forwards_keystore_identity_only_after_abi_load() {
        val calls = mutableListOf<Pair<String, String>>()
        val fingerprint = List(32) { "ab" }.joinToString(":")
        val bridge = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            identityProvider = { alias, value -> calls += alias to value; 0 },
        )

        assertTrue(
            bridge.setDeviceIdentity(DeviceIdentity("frameark-device", fingerprint)),
        )
        assertEquals(listOf("frameark-device" to fingerprint), calls)
    }

    @Test
    fun rejects_malformed_identity_before_crossing_the_bridge() {
        var calls = 0
        val bridge = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            identityProvider = { _, _ -> calls += 1; 0 },
        )

        assertFalse(bridge.setDeviceIdentity(DeviceIdentity("INVALID", "00")))
        assertEquals(0, calls)
    }

    @Test
    fun identity_fails_closed_for_unavailable_abi_or_native_rejection() {
        val identity = DeviceIdentity("frameark-device", List(32) { "ab" }.joinToString(":"))
        var calls = 0
        val incompatible = FrameArkNative(
            libraryLoader = {},
            versionProvider = { 99 },
            identityProvider = { _, _ -> calls += 1; 0 },
        )
        assertFalse(incompatible.setDeviceIdentity(identity))
        assertEquals(0, calls)

        val rejecting = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            identityProvider = { _, _ -> -2 },
        )
        assertFalse(rejecting.setDeviceIdentity(identity))
        val missing = FrameArkNative(libraryLoader = { throw UnsatisfiedLinkError("missing") })
        assertFalse(missing.setDeviceIdentity(identity))
    }

    @Test
    fun forwards_explicit_peer_trust_and_revoke_to_rust() {
        val fingerprint = List(32) { "cd" }.joinToString(":")
        val calls = mutableListOf<String>()
        val bridge = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            trustPeerProvider = { id, value, label ->
                calls += "trust:$id:$value:$label"
                0
            },
            revokePeerProvider = { id -> calls += "revoke:$id"; 0 },
            peerTrustedProvider = { id, value ->
                calls += "check:$id:$value"
                1
            },
        )
        val peer = FrameArkNative.TrustedPeer("sender-1", fingerprint, "Living room")

        assertTrue(bridge.trustPeer(peer))
        assertTrue(bridge.isPeerTrusted(peer.deviceId, peer.fingerprint))
        assertTrue(bridge.revokePeer(peer.deviceId))
        assertEquals(
            listOf(
                "trust:sender-1:$fingerprint:Living room",
                "check:sender-1:$fingerprint",
                "revoke:sender-1",
            ),
            calls,
        )
    }

    @Test
    fun proof_key_trust_bridge_requires_bounded_key_and_forwards_raw_key() {
        var forwarded: Triple<String, ByteArray, String>? = null
        val bridge = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            trustPeerPublicKeyProvider = { deviceId, publicKey, label ->
                forwarded = Triple(deviceId, publicKey, label)
                0
            },
        )
        val publicKey = ByteArray(32) { it.toByte() }
        assertTrue(
            bridge.trustPeerPublicKey(
                FrameArkNative.VerifiedPeerKey("sender-1", publicKey, "Phone"),
            ),
        )
        assertEquals("sender-1", forwarded?.first)
        assertEquals(publicKey.toList(), forwarded?.second?.toList())
        assertEquals("Phone", forwarded?.third)
        assertFalse(
            bridge.trustPeerPublicKey(
                FrameArkNative.VerifiedPeerKey("sender-1", ByteArray(31), "Phone"),
            ),
        )
    }

    @Test
    fun rejects_malformed_peer_trust_before_crossing_jni() {
        var trustCalls = 0
        val bridge = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            trustPeerProvider = { _, _, _ -> trustCalls += 1; 0 },
        )

        assertFalse(bridge.trustPeer(FrameArkNative.TrustedPeer("sender-1", "00", "Sender")))
        assertFalse(bridge.trustPeer(FrameArkNative.TrustedPeer("sender-1", List(32) { "aa" }.joinToString(":"), "\n")))
        assertEquals(0, trustCalls)
    }

    @Test
    fun forwards_optional_one_shot_pairing_approval_gate() {
        val calls = mutableListOf<String>()
        val bridge = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            pairingApprovalRequiredProvider = { required ->
                calls += "required:$required"
                0
            },
            pairingApprovalProvider = {
                calls += "approve"
                0
            },
        )

        assertTrue(bridge.setPairingApprovalRequired(true))
        assertTrue(bridge.approveNextPairing())
        assertEquals(listOf("required:true", "approve"), calls)
    }

    @Test
    fun pairing_approval_gate_fails_closed_without_a_loaded_native_bridge() {
        val bridge = FrameArkNative(libraryLoader = { throw UnsatisfiedLinkError("missing") })

        assertFalse(bridge.setPairingApprovalRequired(true))
        assertFalse(bridge.approveNextPairing())
    }

    @Test
    fun decodes_bounded_rust_verified_peer_proof_event_and_forwards_decision() {
        val deviceId = "sender-proof"
        val requestId = 42L
        val key = ByteArray(32) { it.toByte() }
        val envelope = ByteBuffer.allocate(14 + deviceId.length + key.size)
            .order(ByteOrder.BIG_ENDIAN)
            .put(byteArrayOf('F'.code.toByte(), 'A'.code.toByte(), 'P'.code.toByte(), 'R'.code.toByte()))
            .put(1)
            .putLong(requestId)
            .put(deviceId.length.toByte())
            .put(deviceId.toByteArray())
            .put(key)
            .array()
        var decision: Pair<Long, Boolean>? = null
        val bridge = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            peerProofRequiredProvider = { 0 },
            pollPeerProofProvider = { envelope },
            peerProofDecisionProvider = { id, approved -> decision = id to approved; 0 },
        )

        assertTrue(bridge.setPeerProofRequired(true))
        val event = bridge.pollPeerProof()
        assertEquals(requestId, event?.requestId)
        assertEquals(deviceId, event?.peer?.deviceId)
        assertEquals(key.toList(), event?.peer?.publicKey?.toList())
        assertEquals("FANP sender proof", event?.peer?.label)
        assertTrue(bridge.decidePeerProof(requestId, true))
        assertEquals(requestId to true, decision)
    }

    @Test
    fun rejects_malformed_peer_proof_event_and_invalid_decision_id() {
        val bridge = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            pollPeerProofProvider = { byteArrayOf('F'.code.toByte(), 'A'.code.toByte()) },
            peerProofDecisionProvider = { _, _ -> 0 },
        )

        assertNull(bridge.pollPeerProof())
        assertFalse(bridge.decidePeerProof(0, true))
    }

    @Test
    fun reports_a_loaded_matching_abi() {
        var requestedLibrary = ""
        val bridge = FrameArkNative(
            libraryLoader = { requestedLibrary = it },
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
        )

        assertEquals(
            FrameArkNative.LoadResult.Loaded(FrameArkNative.EXPECTED_ABI_VERSION),
            bridge.load(),
        )
        assertEquals(FrameArkNative.LIBRARY_NAME, requestedLibrary)
        assertTrue(bridge.isLoaded())
    }

    @Test
    fun reports_an_incompatible_abi_without_marking_loaded() {
        val bridge = FrameArkNative(libraryLoader = {}, versionProvider = { 99 })

        assertEquals(
            FrameArkNative.LoadResult.Incompatible(
                FrameArkNative.EXPECTED_ABI_VERSION,
                99,
            ),
            bridge.load(),
        )
        assertFalse(bridge.isLoaded())
    }

    @Test
    fun reports_missing_native_library_without_crashing() {
        val bridge = FrameArkNative(libraryLoader = { throw UnsatisfiedLinkError("missing") })

        assertEquals(
            FrameArkNative.LoadResult.Unavailable("native library is unavailable"),
            bridge.load(),
        )
        assertFalse(bridge.isLoaded())
    }

    @Test
    fun forwards_receiver_lifecycle_after_abi_load() {
        var starts = 0
        var stops = 0
        val bridge = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            startProvider = { starts += 1; 0 },
            stopProvider = { stops += 1; 0 },
            runningProvider = { 1 },
        )

        assertEquals(FrameArkNative.ReceiverResult.Started, bridge.startReceiver())
        assertEquals(FrameArkNative.ReceiverResult.Stopped, bridge.stopReceiver())
        assertEquals(1, starts)
        assertEquals(1, stops)
        assertTrue(bridge.isReceiverRunning())
    }

    @Test
    fun does_not_forward_lifecycle_when_abi_is_incompatible() {
        var starts = 0
        val bridge = FrameArkNative(
            libraryLoader = {},
            versionProvider = { 99 },
            startProvider = { starts += 1; 0 },
        )

        assertEquals(
            FrameArkNative.ReceiverResult.Unavailable("native ABI mismatch"),
            bridge.startReceiver(),
        )
        assertEquals(0, starts)
    }

    @Test
    fun maps_media_backpressure_and_decodes_bounded_envelope() {
        val envelope = ByteBuffer.allocate(22)
            .order(ByteOrder.BIG_ENDIAN)
            .put("FAMF".toByteArray())
            .put(1)
            .put(1)
            .put(1)
            .putLong(90_000L)
            .putInt(3)
            .put(byteArrayOf(1, 2, 3))
            .array()
        val bridge = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            submitVideoProvider = { _, _, _ -> 1 },
            pollMediaProvider = { envelope },
        )

        assertEquals(
            FrameArkNative.MediaSubmitResult.QueueFull,
            bridge.submitVideoFrame(byteArrayOf(9), 90_000, keyframe = true),
        )
        val frame = bridge.pollMediaFrame()
        assertEquals(FrameArkNative.MediaKind.VIDEO, frame?.kind)
        assertEquals(90_000L, frame?.pts)
        assertTrue(frame?.keyframe == true)
        assertArrayEquals(byteArrayOf(1, 2, 3), frame?.payload)
    }

    @Test
    fun rejects_empty_media_before_loading_native_library() {
        var loaded = false
        val bridge = FrameArkNative(
            libraryLoader = { loaded = true },
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
        )

        assertEquals(
            FrameArkNative.MediaSubmitResult.Failed(-3),
            bridge.submitAudioFrame(byteArrayOf(), 0),
        )
        assertFalse(loaded)
    }

    @Test
    fun decodes_bounded_video_and_audio_config_envelopes() {
        val video = ByteBuffer.allocate(24)
            .order(ByteOrder.BIG_ENDIAN)
            .put("FAMC".toByteArray())
            .put(1)
            .put(1)
            .put(1)
            .put(0)
            .putInt(1_920)
            .putInt(1_080)
            .putInt(60)
            .putInt(1)
            .array()
        val audio = ByteBuffer.allocate(16)
            .order(ByteOrder.BIG_ENDIAN)
            .put("FAMC".toByteArray())
            .put(1)
            .put(2)
            .put(4)
            .put(0)
            .putInt(48_000)
            .putInt(2)
            .array()
        val bridge = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            videoConfigProvider = { video },
            audioConfigProvider = { audio },
        )

        assertTrue(bridge.load() is FrameArkNative.LoadResult.Loaded)
        assertEquals(
            FrameArkNative.VideoConfig(
                FrameArkNative.MediaCodecKind.H264,
                1_920,
                1_080,
                60,
                1,
            ),
            bridge.pollVideoConfig(),
        )
        assertEquals(
            FrameArkNative.AudioConfig(
                FrameArkNative.MediaCodecKind.OPUS,
                48_000,
                2,
            ),
            bridge.pollAudioConfig(),
        )
    }

    @Test
    fun rejects_malformed_media_config_without_exposing_unbounded_values() {
        val malformed = ByteBuffer.allocate(24)
            .order(ByteOrder.BIG_ENDIAN)
            .put("FAMC".toByteArray())
            .put(1)
            .put(1)
            .put(1)
            .put(0)
            .putInt(20_000)
            .putInt(1_080)
            .putInt(60)
            .putInt(1)
            .array()
        val bridge = FrameArkNative(
            libraryLoader = {},
            versionProvider = { FrameArkNative.EXPECTED_ABI_VERSION },
            videoConfigProvider = { malformed },
        )
        bridge.load()

        assertNull(bridge.pollVideoConfig())
    }
}
