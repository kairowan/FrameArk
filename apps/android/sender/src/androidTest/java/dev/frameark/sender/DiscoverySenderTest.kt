@file:Suppress("DEPRECATION")
package dev.frameark.sender

import android.content.Intent
import android.test.InstrumentationTestCase
import android.view.View
import android.view.ViewGroup
import android.widget.*
import java.io.File

/** Start DiscoveryReceiverTest concurrently in the same disposable emulator. */
class DiscoverySenderTest : InstrumentationTestCase() {
    fun testSelectionRequiresFreshConfirmationAndPin() {
        val context = instrumentation.targetContext
        val activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        fun descendants(view: View): List<View> = listOf(view) +
            if (view is ViewGroup) (0 until view.childCount).flatMap { descendants(view.getChildAt(it)) } else emptyList()
        fun views() = descendants(activity.window.decorView)
        fun button(resource: Int) = views().filterIsInstance<Button>().single { it.text == context.getString(resource) }
        try {
            instrumentation.waitForIdleSync()
            // Capture the empty form only, before any address, certificate, or PIN exists.
            instrumentation.runOnMainSync { activity.window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE) }
            Thread.sleep(200)
            val bitmap = instrumentation.uiAutomation.takeScreenshot()
            File(context.getExternalFilesDir(null), "discovery-empty.png").outputStream().use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
            bitmap.recycle()
            instrumentation.runOnMainSync {
                activity.window.addFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE)
                button(R.string.discovery_start).performClick()
            }
            var selected: Button? = null
            val deadline = System.nanoTime() + 14_000_000_000L
            while (selected == null && System.nanoTime() < deadline) {
                instrumentation.runOnMainSync {
                    val choices = views().single { it.tag == "discovery-choices" } as ViewGroup
                    selected = (0 until choices.childCount).map { choices.getChildAt(it) }.filterIsInstance<Button>().firstOrNull()
                }
                if (selected == null) Thread.sleep(100)
            }
            assertNotNull("real mDNS candidate from Receiver", selected)
            instrumentation.runOnMainSync {
                val inputs = views().filterIsInstance<EditText>()
                val confirm = views().filterIsInstance<CheckBox>().single { it.text == context.getString(R.string.confirm) }
                inputs[0].setText("frameark-pair-v1\n192.168.1.1:1\n01")
                button(R.string.inspect).performClick(); confirm.isChecked = true; inputs[1].setText("123456")
                selected!!.performClick()
                assertTrue(inputs[0].text.startsWith("frameark-pair-v1\n"))
                assertFalse(confirm.isChecked); assertTrue(inputs[1].text.isEmpty())
                assertTrue(views().filterIsInstance<TextView>().any { it.text.contains("SHA-256 (TLS):") })
                button(R.string.start).performClick()
                assertEquals(context.getString(R.string.invalid), (views().single { it.tag == "sender-status" } as TextView).text)
                button(R.string.discovery_stop).performClick()
                assertEquals(0, (views().single { it.tag == "discovery-choices" } as ViewGroup).childCount)
                inputs[0].text.clear()
            }
            instrumentation.sendStatus(0, android.os.Bundle().apply { putString("discovery", "actual candidate selected; prior approval/PIN cleared; no automatic send; cancel cleared list") })
        } finally { instrumentation.runOnMainSync { activity.finish() } }
    }
}
