@file:Suppress("DEPRECATION")
package dev.frameark.receiver

import android.content.Intent
import android.test.InstrumentationTestCase
import java.io.File

/** Explicit disposable-emulator pair only; does not enable an exported app endpoint. */
class ReferenceReceiverTest : InstrumentationTestCase() {
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
