package dev.frameark.receiver

import org.junit.Assert.*
import org.junit.Test

class VideoDisplaySizeTest {
    @Test fun fitsBothOrientationsWithoutStretchingOrCropping() {
        assertEquals(1f to 1f, VideoDisplaySize(1280, 720).fit(1920, 1080))
        val landscape = VideoDisplaySize(1920, 1080).fit(1080, 1920)
        assertEquals(1f, landscape.first, 0.0001f)
        assertEquals(0.31640625f, landscape.second, 0.0001f)
        val portrait = VideoDisplaySize(1080, 1920).fit(1920, 1080)
        assertEquals(landscape.first, portrait.second)
        assertEquals(landscape.second, portrait.first)
        val huge = VideoDisplaySize(8192, 1).fit(Int.MAX_VALUE, Int.MAX_VALUE)
        assertTrue(huge.first.isFinite() && huge.second.isFinite())
        assertTrue(huge.first in 0f..1f && huge.second in 0f..1f)
    }
    @Test fun rejectsUnknownAndUnboundedGeometry() {
        for (action in listOf<() -> Unit>(
            { VideoDisplaySize(0, 1) }, { VideoDisplaySize(1, 8193) },
            { VideoDisplaySize(1, 1).fit(0, 100) }, { VideoDisplaySize(1, 1).fit(100, -1) },
        )) { try { action(); fail("invalid geometry accepted") } catch (_: IllegalArgumentException) {} }
    }
}
