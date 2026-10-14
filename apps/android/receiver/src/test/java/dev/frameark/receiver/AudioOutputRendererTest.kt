package dev.frameark.receiver

import org.junit.Assert.*
import org.junit.Test

class AudioOutputRendererTest {
    private class Track : PcmOutputTrack {
        val calls = mutableListOf<String>()
        var gainFailure = false
        var playFailure = false
        var closeFailure = false
        override fun setGain(gain: Float) {
            calls += "gain:$gain"
            if (gainFailure) error("test gain failure")
        }
        override fun play() { calls += "play"; if (playFailure) error("test play failure") }
        override fun write(bytes: ByteArray, offset: Int, size: Int): Int { calls += "write:$size"; return size }
        override fun close() { calls += "close"; if (closeFailure) error("test close failure") }
    }
    private val config = PcmTrackConfig(48_000, 2)

    @Test fun gain_is_applied_before_play_and_retained_through_reset() {
        val tracks = mutableListOf<Track>()
        val output = AudioOutputRenderer(trackFactory = { Track().also(tracks::add) })
        output.setGain(0f)
        output.configure(config)
        assertEquals(listOf("gain:0.0", "play"), tracks[0].calls)
        output.setGain(0.25f)
        assertTrue(output.isConfigured())
        assertEquals(listOf("gain:0.0", "play", "gain:0.25"), tracks[0].calls)
        output.reset()
        output.reset()
        output.configure(config)
        assertEquals(listOf("gain:0.25", "play"), tracks[1].calls)
        assertEquals(1, tracks[0].calls.count { it == "close" })
        output.reset()
    }

    @Test fun muted_audio_continues_consuming_pcm_instead_of_accumulating_backlog() {
        val track = Track()
        var completed = 0
        val output = AudioOutputRenderer({ track }, { completed++ })
        output.setGain(0f)
        output.configure(config)
        repeat(100) { output.render(ByteArray(4096)) }
        assertEquals(100, completed)
        assertEquals(100, track.calls.count { it == "write:4096" })
        assertTrue(output.isConfigured())
        output.reset()
    }

    @Test fun initial_gain_failure_releases_track_before_play() {
        val track = Track().apply { gainFailure = true }
        val output = AudioOutputRenderer({ track })
        assertThrows(IllegalStateException::class.java) { output.configure(config) }
        assertEquals(listOf("gain:0.5", "close"), track.calls)
        assertFalse(output.isConfigured())
        output.reset()
        assertEquals(1, track.calls.count { it == "close" })
    }

    @Test fun failed_live_mute_releases_output_and_next_track_starts_silent() {
        val tracks = mutableListOf<Track>()
        val output = AudioOutputRenderer({ Track().also(tracks::add) })
        output.configure(config)
        tracks[0].gainFailure = true
        assertThrows(IllegalStateException::class.java) { output.setGain(0f) }
        assertFalse(output.isConfigured())
        assertEquals(1, tracks[0].calls.count { it == "close" })
        output.configure(config)
        assertEquals(listOf("gain:0.0", "play"), tracks[1].calls)
        output.reset()
    }

    @Test fun invalid_gain_never_touches_a_live_track() {
        val track = Track()
        val output = AudioOutputRenderer({ track })
        output.configure(config)
        for (invalid in listOf(-0.01f, 1.01f, Float.NaN, Float.POSITIVE_INFINITY, Float.NEGATIVE_INFINITY)) {
            assertThrows(IllegalArgumentException::class.java) { output.setGain(invalid) }
        }
        assertEquals(listOf("gain:0.5", "play"), track.calls)
        assertTrue(output.isConfigured())
        output.reset()
    }

    @Test fun play_failure_releases_prepared_track_once() {
        val track = Track().apply { playFailure = true }
        val output = AudioOutputRenderer({ track })
        assertThrows(IllegalStateException::class.java) { output.configure(config) }
        output.reset()
        assertEquals(listOf("gain:0.5", "play", "close"), track.calls)
    }

    @Test fun release_failure_does_not_retain_a_stale_output() {
        val track = Track().apply { closeFailure = true }
        val output = AudioOutputRenderer({ track })
        output.configure(config)
        assertThrows(IllegalStateException::class.java) { output.reset() }
        assertFalse(output.isConfigured())
        output.reset()
        assertEquals(1, track.calls.count { it == "close" })
    }
}
