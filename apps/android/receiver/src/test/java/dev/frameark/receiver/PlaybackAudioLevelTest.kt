package dev.frameark.receiver

import org.junit.Assert.*
import org.junit.Test

class PlaybackAudioLevelTest {
    @Test fun validates_level_and_never_amplifies() {
        assertEquals(0.5f, PlaybackAudioLevel().gain, 0f)
        for (level in 0..100) {
            assertEquals(level / 100f, PlaybackAudioLevel(level).gain, 0f)
            assertEquals(0f, PlaybackAudioLevel(level, true).gain, 0f)
        }
        for (bad in listOf(Int.MIN_VALUE, -1, 101, Int.MAX_VALUE)) {
            assertThrows(IllegalArgumentException::class.java) { PlaybackAudioLevel(bad) }
        }
    }

    @Test fun steps_clamp_without_overflow_or_unmuting() {
        val muted = PlaybackAudioLevel(40, true)
        assertEquals(PlaybackAudioLevel(50, true), muted.stepped(10))
        assertEquals(PlaybackAudioLevel(100, true), muted.stepped(Int.MAX_VALUE))
        assertEquals(PlaybackAudioLevel(0, true), muted.stepped(Int.MIN_VALUE))
        assertEquals(40, muted.copy(muted = false).percent)
    }
}
