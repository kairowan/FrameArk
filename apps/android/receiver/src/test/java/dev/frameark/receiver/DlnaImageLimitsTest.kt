package dev.frameark.receiver

import org.junit.Assert.*
import org.junit.Test

class DlnaImageLimitsTest {
    @Test fun inputLengthIsBoundedBeforeAllocation() {
        assertEquals(1, DlnaImageLimits.length(1))
        assertEquals(16 * 1024 * 1024, DlnaImageLimits.length(16L * 1024 * 1024))
        for (length in listOf(-1L, 0L, 16L * 1024 * 1024 + 1, Long.MAX_VALUE)) {
            try { DlnaImageLimits.length(length); fail("unbounded input") } catch (_: IllegalArgumentException) { }
        }
    }

    @Test fun cameraImagesAreSampledWithoutOverflowOrUpscaling() {
        assertEquals(1, DlnaImageLimits.sampleSize(1920, 1080))
        assertEquals(2, DlnaImageLimits.sampleSize(4000, 3000))
        for (w in listOf(1, 100, 2048, 4097, 10000, 32768)) for (h in listOf(1, 17, 1024, 3000)) {
            val sample = DlnaImageLimits.sampleSize(w, h)
            assertEquals(0, sample and (sample - 1))
            val width = (w + sample - 1) / sample
            val height = (h + sample - 1) / sample
            assertTrue(width <= 4096 && height <= 4096)
            assertTrue(width.toLong() * height <= 4 * 1024 * 1024)
        }
        for ((w, h) in listOf(0 to 1, 1 to -1, Int.MAX_VALUE to 1, 32768 to 32768, 10001 to 10000)) {
            try { DlnaImageLimits.sampleSize(w, h); fail("unsafe dimensions") } catch (_: IllegalArgumentException) { }
        }
    }

    @Test fun imageKindAndSignatureMustAgree() {
        val png = byteArrayOf(137.toByte(), 80, 78, 71, 13, 10, 26, 10)
        val jpeg = byteArrayOf(255.toByte(), 216.toByte(), 255.toByte())
        assertEquals("image/jpeg", DlnaImageLimits.mime(1))
        assertEquals("image/png", DlnaImageLimits.mime(2))
        assertTrue(DlnaImageLimits.hasSignature(png, 2))
        assertTrue(DlnaImageLimits.hasSignature(jpeg, 1))
        assertFalse(DlnaImageLimits.hasSignature(png, 1))
        assertFalse(DlnaImageLimits.hasSignature(jpeg, 2))
        assertFalse(DlnaImageLimits.hasSignature(png, 3))
        for (n in 0 until png.size) assertFalse(DlnaImageLimits.hasSignature(png.copyOf(n), 2))
        try { DlnaImageLimits.mime(3); fail("unknown image kind") } catch (_: IllegalStateException) { }
    }
}
