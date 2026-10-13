@file:Suppress("DEPRECATION")
package dev.frameark.sender

import android.test.InstrumentationTestCase
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

/** Explicit two-emulator test. Credentials are ephemeral app-private test inputs. */
class ReferenceSenderTest : InstrumentationTestCase() {
    fun testUiStreamsAndStops() {
        val context = instrumentation.targetContext
        val ticketFile = File(context.filesDir, "reference-ticket.txt")
        val pinFile = File(context.filesDir, "reference-pin.txt")
        val activity = instrumentation.startActivitySync(android.content.Intent(context, MainActivity::class.java).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK))
        fun descendants(view: android.view.View): List<android.view.View> = listOf(view) +
            if (view is android.view.ViewGroup) (0 until view.childCount).flatMap { descendants(view.getChildAt(it)) } else emptyList()
        try {
            instrumentation.waitForIdleSync()
            // Screenshot only the blank form, then restore production screenshot protection.
            instrumentation.runOnMainSync { activity.window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE) }
            Thread.sleep(300)
            val bitmap = instrumentation.uiAutomation.takeScreenshot()
            File(context.getExternalFilesDir(null), "reference-sender-empty.png").outputStream().use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
            bitmap.recycle()
            val ticket = ticketFile.readText().trimEnd('\r', '\n')
            val pin = pinFile.readText().trim()
            val views = descendants(activity.window.decorView)
            instrumentation.runOnMainSync {
                activity.window.addFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE)
                val inputs = views.filterIsInstance<android.widget.EditText>()
                inputs[0].setText(ticket)
                views.filterIsInstance<android.widget.Button>().single { it.text == context.getString(R.string.inspect) }.performClick()
                views.filterIsInstance<android.widget.CheckBox>().single { it.text == context.getString(R.string.confirm) }.isChecked = true
                inputs[1].setText(pin)
                views.filterIsInstance<android.widget.Button>().single { it.text == context.getString(R.string.start) }.performClick()
            }
            val deadline = System.nanoTime() + 50_000_000_000L
            var completed = false
            var diagnostic = ""
            while (!completed && System.nanoTime() < deadline) {
                instrumentation.runOnMainSync {
                    diagnostic = views.filterIsInstance<android.widget.TextView>().single { it.tag == "sender-status" }.text.toString()
                    completed = diagnostic == context.getString(R.string.complete)
                }
                if (diagnostic.startsWith("发送失败") || diagnostic == context.getString(R.string.invalid) || diagnostic == context.getString(R.string.unavailable) || diagnostic == context.getString(R.string.cancelled)) break
                Thread.sleep(50)
            }
            assertTrue("foreground UI: $diagnostic", completed)
        } finally {
            ticketFile.delete(); pinFile.delete()
            instrumentation.runOnMainSync { activity.finish() }
        }
    }

    fun testPinnedReferenceStream() {
        val native = SenderNative()
        assertTrue(native.load())
        val ticketFile = File(instrumentation.targetContext.filesDir, "reference-ticket.txt")
        val pinFile = File(instrumentation.targetContext.filesDir, "reference-pin.txt")
        var id = 0L
        try {
            val ticket = ticketFile.readText().trimEnd('\r', '\n')
            val pin = pinFile.readText().trim()
            assertNotNull(native.summary(ticket))
            id = native.start(ticket, pin, 1920, 1080)
            assertTrue("native start", id > 0)
            val deadline = System.nanoTime() + 10_000_000_000L
            while (native.status(id) == 1 && System.nanoTime() < deadline) Thread.sleep(10)
            assertEquals("pair/offer/start", 2, native.status(id))
            var video = 0
            var audio = 0
            ReferenceEncoder().run(1920, 1080, 30, AtomicBoolean(false), { data, pts, isVideo, key ->
                val queueDeadline = System.nanoTime() + 1_000_000_000L
                var result: Int
                do {
                    result = native.submit(id, data, pts, isVideo, key)
                    if (result == 1) Thread.sleep(2)
                } while (result == 1 && System.nanoTime() < queueDeadline)
                assertEquals("queue; native=${native.status(id)}", 0, result)
            }, { v, a -> video = v; audio = a })
            assertEquals(900, video)
            assertTrue("AAC output", audio >= 1300)
            assertEquals(0, native.finish(id))
            val end = System.nanoTime() + 7_000_000_000L
            while (native.status(id) == 2 && System.nanoTime() < end) Thread.sleep(10)
            assertEquals("stop", 3, native.status(id))
            instrumentation.sendStatus(0, android.os.Bundle().apply { putString("stream", "1080p30 video=$video audio=$audio") })
        } finally {
            if (id > 0) native.cancel(id)
            ticketFile.delete(); pinFile.delete()
        }
    }

    fun testEncoderCancellationReleasesForNextRun() {
        val cancel = AtomicBoolean(false)
        var count = 0
        try {
            ReferenceEncoder().run(1280, 720, 3, cancel, { _, _, video, _ -> if (video && ++count == 5) cancel.set(true) }, { _, _ -> })
            fail("expected cancellation")
        } catch (_: InterruptedException) { }
        var after = 0
        ReferenceEncoder().run(1280, 720, 1, AtomicBoolean(false), { _, _, video, _ -> if (video) after++ }, { _, _ -> })
        assertEquals(30, after)
    }
}
