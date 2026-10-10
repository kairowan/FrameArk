package dev.frameark.receiver

import org.junit.Assert.assertEquals
import org.junit.Test

class MediaTrackConfigTest {
    @Test
    fun calculates_fractional_video_rate_without_rounding_to_integer() {
        assertEquals(29.97f, VideoTrackConfig(1920, 1080, 30_000, 1_001).frameRate, 0.001f)
    }

    @Test(expected = IllegalArgumentException::class)
    fun rejects_empty_video_dimensions() {
        VideoTrackConfig(0, 1080, 30, 1)
    }

    @Test(expected = IllegalArgumentException::class)
    fun rejects_unbounded_audio_channels() {
        PcmTrackConfig(48_000, 9)
    }
}
