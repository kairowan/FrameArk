@file:Suppress("DEPRECATION")
package dev.frameark.sender

import android.app.Activity
import android.content.Intent
import android.os.Build
import android.test.InstrumentationTestCase
import android.view.View
import android.view.ViewGroup
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.*
import java.io.File

/** Consent is clicked in System UI, never bypassed with PROJECT_MEDIA app-ops. */
class ScreenCaptureTest : InstrumentationTestCase() {
    fun testEmptyCaptureUi() {
        val context = instrumentation.targetContext
        val activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        fun scroll(view: View): ScrollView? = if (view is ScrollView) view else if (view is ViewGroup) (0 until view.childCount).firstNotNullOfOrNull { scroll(view.getChildAt(it)) } else null
        try {
            instrumentation.waitForIdleSync()
            instrumentation.runOnMainSync {
                scroll(activity.window.decorView)?.fullScroll(View.FOCUS_DOWN)
                activity.window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE)
            }
            Thread.sleep(300)
            val bitmap = instrumentation.uiAutomation.takeScreenshot()
            File(context.getExternalFilesDir(null), "screen-capture-empty.png").outputStream().use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
            bitmap.recycle()
        } finally {
            instrumentation.runOnMainSync { activity.window.addFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE); activity.finish() }
        }
    }

    fun testCapturesOsScreenAndPlaybackAudio() = capture(true)
    fun testVideoOnlyStopsOnLock() = capture(false)
    fun testRotationKeepsVideoAndPlaybackAudio() = capture(true, true)

    private fun capture(withAudio: Boolean, rotate: Boolean = false) {
        assertTrue("disposable emulator only", Build.MODEL.startsWith("sdk_gphone") && Build.SUPPORTED_ABIS.contains("x86_64"))
        val context = instrumentation.targetContext
        val ticket = File(context.filesDir, "reference-ticket.txt")
        val pin = File(context.filesDir, "reference-pin.txt")
        instrumentation.uiAutomation.grantRuntimePermission(context.packageName, android.Manifest.permission.RECORD_AUDIO)
        if (Build.VERSION.SDK_INT >= 33) instrumentation.uiAutomation.grantRuntimePermission(context.packageName, android.Manifest.permission.POST_NOTIFICATIONS)
        val activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        var fixture: Activity? = null
        fun descendants(view: View): List<View> = listOf(view) + if (view is ViewGroup) (0 until view.childCount).flatMap { descendants(view.getChildAt(it)) } else emptyList()
        try {
            instrumentation.waitForIdleSync()
            val views = descendants(activity.window.decorView)
            instrumentation.runOnMainSync {
                val inputs = views.filterIsInstance<EditText>()
                inputs[0].setText(ticket.readText().trimEnd('\r', '\n'))
                views.filterIsInstance<Button>().single { it.text == context.getString(R.string.inspect) }.performClick()
                views.filterIsInstance<CheckBox>().single { it.text == context.getString(R.string.confirm) }.isChecked = true
                views.filterIsInstance<CheckBox>().single { it.text == context.getString(R.string.capture_audio) }.isChecked = withAudio
                views.filterIsInstance<CheckBox>().single { it.text == context.getString(R.string.capture_sixty) }.isChecked = true
                inputs[1].setText(pin.readText().trim())
                views.filterIsInstance<Button>().single { it.text == context.getString(R.string.capture_start) }.performClick()
            }
            if (withAudio) {
                assertTrue("OS denial button", clickConsent("Cancel"))
                instrumentation.waitForIdleSync()
                assertFalse("denial must not start capture", CaptureStatus.snapshot.active)
                Thread.sleep(300)
                instrumentation.runOnMainSync {
                    assertEquals(context.getString(R.string.capture_denied), views.filterIsInstance<TextView>().single { it.tag == "sender-status" }.text.toString())
                    views.filterIsInstance<CheckBox>().single { it.text == context.getString(R.string.confirm) }.isChecked = true
                    views.filterIsInstance<EditText>()[1].setText(pin.readText().trim())
                    views.filterIsInstance<Button>().single { it.text == context.getString(R.string.capture_start) }.performClick()
                }
            }
            assertTrue("OS consent button", clickConsent("Start now"))
            val connected = System.nanoTime() + 20_000_000_000L
            while (CaptureStatus.snapshot.phase != "sharing" && System.nanoTime() < connected && CaptureStatus.snapshot.phase != "failed") Thread.sleep(50)
            assertTrue("capture startup: ${CaptureStatus.snapshot}", CaptureStatus.snapshot.active)
            fixture = instrumentation.startActivitySync(Intent().setClassName(context.packageName, "dev.frameark.sender.CaptureFixtureActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            if (rotate) {
                val initial = System.nanoTime() + 10_000_000_000L
                while (CaptureStatus.snapshot.video < 100 && CaptureStatus.snapshot.active && System.nanoTime() < initial) Thread.sleep(50)
                for (orientation in listOf(android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE, android.content.pm.ActivityInfo.SCREEN_ORIENTATION_PORTRAIT)) {
                    val before = CaptureStatus.snapshot
                    instrumentation.runOnMainSync { fixture.requestedOrientation = orientation }
                    val resized = System.nanoTime() + 12_000_000_000L
                    fun advanced(): Boolean {
                        val current = CaptureStatus.snapshot
                        return current.videoUpdates > before.videoUpdates && current.video > before.video + 50 &&
                            current.audio > before.audio + 50 && current.nonSilentAudio > before.nonSilentAudio + 10 &&
                            (if (orientation == android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE) current.width > current.height else current.height > current.width)
                    }
                    while (!advanced() && CaptureStatus.snapshot.active && System.nanoTime() < resized) Thread.sleep(50)
                    assertTrue("rotation continued video and non-silent playback PCM: ${CaptureStatus.snapshot}", advanced())
                    instrumentation.sendStatus(0, android.os.Bundle().apply { putString("rotation", CaptureStatus.snapshot.toString()) })
                }
            }
            val deadline = System.nanoTime() + 30_000_000_000L
            while (CaptureStatus.snapshot.seconds < 25 && CaptureStatus.snapshot.active && System.nanoTime() < deadline) Thread.sleep(50)
            val captured = CaptureStatus.snapshot
            assertTrue("actual video: $captured", captured.video > 100)
            if (withAudio) assertTrue("playback PCM, not silence: $captured", captured.audio > 100 && captured.nonSilentAudio > 10)
            else assertEquals("no audio without opt-in", 0, captured.audio + captured.nonSilentAudio)
            if (withAudio) context.startService(Intent(context, ScreenCaptureService::class.java).setAction(ScreenCaptureService.ACTION_STOP))
            else assertTrue(instrumentation.uiAutomation.performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_LOCK_SCREEN))
            val stopped = System.nanoTime() + 10_000_000_000L
            while (CaptureStatus.snapshot.active && System.nanoTime() < stopped) Thread.sleep(20)
            assertEquals("clean foreground stop: ${CaptureStatus.snapshot}", "stopped", CaptureStatus.snapshot.phase)
            assertFalse(CaptureStatus.snapshot.active)
            val manager = context.getSystemService(android.app.ActivityManager::class.java)
            val gone = System.nanoTime() + 3_000_000_000L
            fun serviceExists() = manager.getRunningServices(100).any { it.service.className == ScreenCaptureService::class.java.name }
            while (serviceExists() && System.nanoTime() < gone) Thread.sleep(20)
            assertFalse("foreground service removed", serviceExists())
            instrumentation.sendStatus(0, android.os.Bundle().apply { putString("capture", captured.toString()) })
        } finally {
            context.stopService(Intent(context, ScreenCaptureService::class.java))
            instrumentation.runOnMainSync { fixture?.finish(); activity.finish() }
            ticket.delete(); pin.delete()
            if (!withAudio) instrumentation.uiAutomation.executeShellCommand("input keyevent KEYCODE_WAKEUP").close()
        }
    }

    private fun clickConsent(label: String): Boolean {
        val consentDeadline = System.nanoTime() + 8_000_000_000L
        var accepted = false
        while (!accepted && System.nanoTime() < consentDeadline) {
            val root = instrumentation.uiAutomation.rootInActiveWindow
            if (root != null && root.packageName == "com.android.systemui") {
                fun find(node: AccessibilityNodeInfo): AccessibilityNodeInfo? {
                    if (node.text?.toString()?.equals(label, true) == true && node.isClickable) return node
                    for (i in 0 until node.childCount) { val child = node.getChild(i) ?: continue; find(child)?.let { return it } }
                    return null
                }
                accepted = find(root)?.performAction(AccessibilityNodeInfo.ACTION_CLICK) == true
            }
            if (!accepted) Thread.sleep(100)
        }
        return accepted
    }
}
