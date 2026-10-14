@file:Suppress("DEPRECATION")
package dev.frameark.receiver

import android.content.Intent
import android.test.InstrumentationTestCase

/** Operator-run disposable emulator fixture; no exported endpoint or credential files. */
class DiscoveryReceiverTest : InstrumentationTestCase() {
    fun testAdvertisesForSender() {
        val context = instrumentation.targetContext
        val activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val native = FrameArkNative()
        try {
            assertTrue(native.load() is FrameArkNative.LoadResult.Loaded)
            instrumentation.waitForIdleSync()
            context.startService(Intent(context, FrameArkReceiverService::class.java).setAction(ReceiverServicePolicy.ACTION_START))
            val deadline = System.nanoTime() + 5_000_000_000L
            while (native.networkInfo() == null && System.nanoTime() < deadline) Thread.sleep(20)
            assertNotNull(native.networkInfo())
            instrumentation.sendStatus(0, android.os.Bundle().apply { putString("discovery", "ready; no credentials exported") })
            Thread.sleep(40_000)
        } finally {
            context.stopService(Intent(context, FrameArkReceiverService::class.java))
            instrumentation.runOnMainSync { activity.finish() }
            val deadline = System.nanoTime() + 5_000_000_000L
            while (native.isReceiverRunning() && System.nanoTime() < deadline) Thread.sleep(20)
            assertFalse(native.isReceiverRunning())
        }
    }
}
