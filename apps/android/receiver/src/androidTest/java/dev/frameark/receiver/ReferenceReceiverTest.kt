@file:Suppress("DEPRECATION")
package dev.frameark.receiver

import android.content.Intent
import android.test.InstrumentationTestCase
import java.io.File

/** Explicit disposable-emulator pair only; does not enable an exported app endpoint. */
class ReferenceReceiverTest : InstrumentationTestCase() {
    fun testReceivesActualScreenCapture() = receiveCapture(true)
    fun testReceivesVideoOnlyCapture() = receiveCapture(false)
    fun testReceivesRotatingCapture() = receiveCapture(true, true)

    private fun receiveCapture(withAudio: Boolean, rotate: Boolean = false) {
        val context = instrumentation.targetContext
        val activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val native = FrameArkNative()
        val ticket = File(context.filesDir, "reference-ticket.txt")
        val pin = File(context.filesDir, "reference-pin.txt")
        try {
            assertTrue(native.load() is FrameArkNative.LoadResult.Loaded)
            assertTrue(native.setPeerProofRequired(false))
            assertTrue(native.setPairingApprovalRequired(false))
            instrumentation.waitForIdleSync()
            context.startService(Intent(context, FrameArkReceiverService::class.java).setAction(ReceiverServicePolicy.ACTION_START))
            val start = System.nanoTime() + 5_000_000_000L
            while (native.networkInfo() == null && System.nanoTime() < start) Thread.sleep(20)
            val network = requireNotNull(native.networkInfo())
            val beforeVideo = ReceiverPlaybackOutput.renderedVideoFrames()
            val beforeAudio = ReceiverPlaybackOutput.renderedAudioBuffers()
            val beforeConfigs = ReceiverPlaybackOutput.videoConfigurationCount()
            val beforeLevel = ReceiverPlaybackOutput.audioLevel()
            pin.writeText(network.pairingCode.toString().padStart(6, '0'))
            ticket.writeText(requireNotNull(native.pairingTicket("10.0.2.2")))
            fun textures(view: android.view.View): List<android.view.TextureView> =
                (if (view is android.view.TextureView) listOf(view) else emptyList()) +
                    if (view is android.view.ViewGroup) (0 until view.childCount).flatMap { textures(view.getChildAt(it)) } else emptyList()
            var colored = false
            val deadline = System.nanoTime() + 55_000_000_000L
            while (!colored && System.nanoTime() < deadline) {
                instrumentation.runOnMainSync {
                    textures(activity.window.decorView).forEach { texture ->
                        val bitmap = texture.getBitmap(32, 32) ?: return@forEach
                        var red = 0; var blue = 0
                        for (x in 0 until 32) for (y in 0 until 32) {
                            val pixel = bitmap.getPixel(x, y)
                            val r = android.graphics.Color.red(pixel); val b = android.graphics.Color.blue(pixel)
                            val g = android.graphics.Color.green(pixel)
                            if (r > 150 && b < 80 && g < 80) red++
                            if (b > 150 && r < 80 && g < 80) blue++
                        }
                        bitmap.recycle()
                        if (red > 100 && blue > 100) {
                            colored = true
                            texture.getBitmap(360, 640)?.let { evidence ->
                                File(context.getExternalFilesDir(null), "os-capture-decoded.png").outputStream().use { evidence.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
                                evidence.recycle()
                            }
                        }
                    }
                }
                if (!colored) Thread.sleep(100)
            }
            assertTrue("decoded actual red/blue OS screen; ${ReceiverPlaybackOutput.diagnostic()}", colored)
            if (rotate) {
                val texture = textures(activity.window.decorView).single()
                val surface = texture.surfaceTexture
                val priorVideo = ReceiverPlaybackOutput.renderedVideoFrames()
                val priorAudio = ReceiverPlaybackOutput.renderedAudioBuffers()
                instrumentation.runOnMainSync { activity.requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE }
                val rotationDeadline = System.nanoTime() + 20_000_000_000L
                while ((ReceiverPlaybackOutput.videoConfigurationCount() < beforeConfigs + 3 ||
                        ReceiverPlaybackOutput.renderedVideoFrames() < priorVideo + 100 ||
                        ReceiverPlaybackOutput.renderedAudioBuffers() < priorAudio + 100) && System.nanoTime() < rotationDeadline) Thread.sleep(50)
                instrumentation.waitForIdleSync()
                assertFalse("configuration does not destroy Activity", activity.isDestroyed)
                assertSame("configuration keeps decoder SurfaceTexture", surface, texture.surfaceTexture)
                assertTrue("two sender geometry updates: ${ReceiverPlaybackOutput.diagnostic()}", ReceiverPlaybackOutput.videoConfigurationCount() >= beforeConfigs + 3)
                assertTrue("video continues across both rotations", ReceiverPlaybackOutput.renderedVideoFrames() >= priorVideo + 100)
                assertTrue("audio continues across both rotations", ReceiverPlaybackOutput.renderedAudioBuffers() >= priorAudio + 100)
                assertTrue("final portrait output", requireNotNull(ReceiverPlaybackOutput.videoDisplaySize()).let { it.height > it.width })
                assertFalse("session not torn down by rotation", ReceiverPlaybackOutput.outputsReleased())
                assertEquals("local gain/mute unchanged", beforeLevel, ReceiverPlaybackOutput.audioLevel())
                // Configuration delivery and the UI frame callback are asynchronous.
                // Assert off-main so a failure is reported, not an Activity-process crash.
                var fitted = false
                val fitDeadline = System.nanoTime() + 2_000_000_000L
                while (!fitted && System.nanoTime() < fitDeadline) {
                    instrumentation.runOnMainSync {
                        val expected = requireNotNull(ReceiverPlaybackOutput.videoDisplaySize()).fit(texture.width, texture.height)
                        val values = FloatArray(9)
                        texture.getTransform(android.graphics.Matrix()).getValues(values)
                        fitted = kotlin.math.abs(expected.first - values[android.graphics.Matrix.MSCALE_X]) < 0.001f &&
                            kotlin.math.abs(expected.second - values[android.graphics.Matrix.MSCALE_Y]) < 0.001f
                    }
                    if (!fitted) Thread.sleep(20)
                }
                assertTrue("actual TextureView applies aspect-fit transform", fitted)
                instrumentation.runOnMainSync {
                    val bitmap = requireNotNull(texture.getBitmap(360, 640))
                    File(context.getExternalFilesDir(null), "reconfigured-decoded.png").outputStream().use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
                    bitmap.recycle()
                }
                instrumentation.runOnMainSync { activity.requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_PORTRAIT }
            }
            val end = System.nanoTime() + 40_000_000_000L
            while (!ReceiverPlaybackOutput.outputsReleased() && System.nanoTime() < end) Thread.sleep(50)
            assertTrue(ReceiverPlaybackOutput.diagnostic(), ReceiverPlaybackOutput.renderedVideoFrames() - beforeVideo > 100)
            if (withAudio) assertTrue(ReceiverPlaybackOutput.diagnostic(), ReceiverPlaybackOutput.renderedAudioBuffers() - beforeAudio > 100)
            else assertEquals("no unrequested audio", beforeAudio, ReceiverPlaybackOutput.renderedAudioBuffers())
            assertTrue("decoder release", ReceiverPlaybackOutput.outputsReleased())
            instrumentation.sendStatus(0, android.os.Bundle().apply { putString("playback", "red/blue pixels verified; ${ReceiverPlaybackOutput.diagnostic()}") })
        } finally {
            ticket.delete(); pin.delete()
            context.stopService(Intent(context, FrameArkReceiverService::class.java))
            instrumentation.runOnMainSync { activity.finish() }
            val end = System.nanoTime() + 5_000_000_000L
            while (native.isReceiverRunning() && System.nanoTime() < end) Thread.sleep(10)
            assertFalse(native.isReceiverRunning())
        }
    }

    fun testIdleTicketSurvivesSurfaceRecreation() {
        val context = instrumentation.targetContext
        var activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val native = FrameArkNative()
        try {
            assertTrue(native.load() is FrameArkNative.LoadResult.Loaded)
            context.startService(Intent(context, FrameArkReceiverService::class.java).setAction(ReceiverServicePolicy.ACTION_START))
            val deadline = System.nanoTime() + 5_000_000_000L
            while (native.networkInfo() == null && System.nanoTime() < deadline) Thread.sleep(20)
            val ticket = requireNotNull(native.pairingTicket("10.0.2.2"))
            instrumentation.runOnMainSync { activity.finish() }
            instrumentation.waitForIdleSync()
            Thread.sleep(300)
            assertEquals("idle export survives detached surface", ticket, native.pairingTicket("10.0.2.2"))
            activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            instrumentation.waitForIdleSync()
            assertEquals(ticket, native.pairingTicket("10.0.2.2"))
        } finally {
            context.stopService(Intent(context, FrameArkReceiverService::class.java))
            instrumentation.runOnMainSync { activity.finish() }
            val end = System.nanoTime() + 5_000_000_000L
            while (native.isReceiverRunning() && System.nanoTime() < end) Thread.sleep(10)
            assertFalse(native.isReceiverRunning())
        }
    }

    fun testReceivesSeparateSenderProcess() {
        val context = instrumentation.targetContext
        val activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val native = FrameArkNative()
        val ticket = File(context.filesDir, "reference-ticket.txt")
        val pin = File(context.filesDir, "reference-pin.txt")
        try {
            assertTrue(native.load() is FrameArkNative.LoadResult.Loaded)
            assertTrue(native.setPeerProofRequired(false))
            assertTrue(native.setPairingApprovalRequired(false))
            instrumentation.waitForIdleSync()
            Thread.sleep(500)
            context.startService(Intent(context, FrameArkReceiverService::class.java).setAction(ReceiverServicePolicy.ACTION_START))
            val startDeadline = System.nanoTime() + 5_000_000_000L
            while (native.networkInfo() == null && System.nanoTime() < startDeadline) Thread.sleep(20)
            val network = requireNotNull(native.networkInfo())
            val beforeVideo = ReceiverPlaybackOutput.renderedVideoFrames()
            val beforeAudio = ReceiverPlaybackOutput.renderedAudioBuffers()
            // Host forwards this port to this emulator. The sender uses its host gateway.
            pin.writeText(network.pairingCode.toString().padStart(6, '0'))
            ticket.writeText(requireNotNull(native.pairingTicket("10.0.2.2")))
            val end = System.nanoTime() + 90_000_000_000L
            while (ReceiverPlaybackOutput.renderedVideoFrames() - beforeVideo < 880 && System.nanoTime() < end) Thread.sleep(50)
            val drain = System.nanoTime() + 7_000_000_000L
            while (!ReceiverPlaybackOutput.outputsReleased() && System.nanoTime() < drain) Thread.sleep(20)
            instrumentation.waitForIdleSync()
            assertTrue(ReceiverPlaybackOutput.diagnostic(), ReceiverPlaybackOutput.renderedVideoFrames() - beforeVideo >= 880)
            assertTrue(ReceiverPlaybackOutput.diagnostic(), ReceiverPlaybackOutput.renderedAudioBuffers() - beforeAudio >= 1250)
            assertTrue("decoder teardown", ReceiverPlaybackOutput.outputsReleased())
            instrumentation.sendStatus(0, android.os.Bundle().apply { putString("playback", ReceiverPlaybackOutput.diagnostic()) })
        } finally {
            ticket.delete(); pin.delete()
            context.stopService(Intent(context, FrameArkReceiverService::class.java))
            instrumentation.runOnMainSync { activity.finish() }
            val end = System.nanoTime() + 5_000_000_000L
            while (native.isReceiverRunning() && System.nanoTime() < end) Thread.sleep(10)
            assertFalse(native.isReceiverRunning())
        }
    }
}
