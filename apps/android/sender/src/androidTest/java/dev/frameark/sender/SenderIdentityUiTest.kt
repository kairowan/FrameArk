@file:Suppress("DEPRECATION")
package dev.frameark.sender

import android.content.Intent
import android.test.InstrumentationTestCase
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.widget.*
import java.io.File

class SenderIdentityUiTest : InstrumentationTestCase() {
    fun testExplicitOptInClearsPinAndConfirmation() {
        val context = instrumentation.targetContext
        val activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        fun descendants(view: View): List<View> = listOf(view) + if (view is ViewGroup)
            (0 until view.childCount).flatMap { descendants(view.getChildAt(it)) } else emptyList()
        fun views() = descendants(activity.window.decorView)
        fun checkbox(resource: Int) = views().filterIsInstance<CheckBox>().single { it.text == context.getString(resource) }
        try {
            instrumentation.waitForIdleSync()
            instrumentation.runOnMainSync {
                assertFalse(checkbox(R.string.identity_use).isChecked)
                assertFalse(checkbox(R.string.identity_use).isEnabled)
                activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
            }
            Thread.sleep(200)
            // Empty form only: no key, identifier, network candidate, PIN or media shown.
            val bitmap = instrumentation.uiAutomation.takeScreenshot()
            File(context.getExternalFilesDir(null), "identity-empty.png").outputStream().use {
                bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)
            }
            bitmap.recycle()
            instrumentation.runOnMainSync {
                activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
                views().filterIsInstance<Button>().single { it.text == context.getString(R.string.identity_load) }.performClick()
            }
            var loaded = false
            val deadline = System.nanoTime() + 5_000_000_000L
            while (!loaded && System.nanoTime() < deadline) {
                instrumentation.runOnMainSync { loaded = checkbox(R.string.identity_use).isEnabled }
                if (!loaded) Thread.sleep(20)
            }
            assertTrue(loaded)
            instrumentation.runOnMainSync {
                assertFalse(checkbox(R.string.identity_use).isChecked)
                assertTrue((views().single { it.tag == "identity-summary" } as TextView).text.contains("SHA-256 (sender):"))
                val pin = views().filterIsInstance<EditText>()[1]
                for (selected in listOf(true, false)) {
                    pin.setText("123456"); checkbox(R.string.confirm).isChecked = true
                    checkbox(R.string.identity_use).isChecked = selected
                    assertTrue(pin.text.isEmpty()); assertFalse(checkbox(R.string.confirm).isChecked)
                }
                checkbox(R.string.identity_use).isChecked = true
                pin.setText("123456"); checkbox(R.string.confirm).isChecked = true
                views().filterIsInstance<Button>().single { it.text == context.getString(R.string.identity_load) }.performClick()
                assertFalse(checkbox(R.string.identity_use).isChecked)
                assertFalse(checkbox(R.string.identity_use).isEnabled)
                assertTrue(pin.text.isEmpty()); assertFalse(checkbox(R.string.confirm).isChecked)
                assertFalse(views().filterIsInstance<SenderIdentityPanel>().single().isReady())
            }
        } finally { instrumentation.runOnMainSync { activity.finish() } }
    }
}
