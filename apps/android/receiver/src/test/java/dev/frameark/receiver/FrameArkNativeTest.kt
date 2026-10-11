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
