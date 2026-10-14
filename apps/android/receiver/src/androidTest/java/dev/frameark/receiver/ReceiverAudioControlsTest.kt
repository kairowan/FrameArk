@file:Suppress("DEPRECATION")

package dev.frameark.receiver

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.media.AudioManager
import android.test.InstrumentationTestCase
import android.view.KeyEvent
import android.view.View
import android.widget.Button
import android.widget.CheckBox
import android.widget.SeekBar
import android.widget.TextView
import java.io.File

class ReceiverAudioControlsTest : InstrumentationTestCase() {
    fun testRealAudioTrackAcceptsLiveGainAndMutedReconfiguration() {
        val manager = instrumentation.targetContext.getSystemService(AudioManager::class.java)
        val systemVolume = manager.getStreamVolume(AudioManager.STREAM_MUSIC)
        var written = 0
        val output = AudioOutputRenderer(onWritten = { written++ })
        try {
            output.setGain(0f)
            output.configure(PcmTrackConfig(48_000, 2))
            for (gain in listOf(0f, 0.25f, 1f, 0f)) {
                output.setGain(gain) // Production adapter checks AudioTrack.SUCCESS.
                output.render(ByteArray(1920))
                assertTrue(output.isConfigured())
            }
            val deadline = System.nanoTime() + 2_000_000_000L
            while (written < 4 && System.nanoTime() < deadline) { output.drain(); Thread.sleep(5) }
            assertEquals(4, written)
            output.reset()
            output.configure(PcmTrackConfig(16_000, 1))
            output.render(ByteArray(640))
            assertTrue(output.isConfigured())
            assertEquals(systemVolume, manager.getStreamVolume(AudioManager.STREAM_MUSIC))
        } finally { output.reset() }
        assertFalse(output.isConfigured())
    }

    fun testDpadControlsPersistWithoutImplicitUnmute() {
        val context = instrumentation.targetContext
        val settings = PlaybackAudioSettings(context)
        val original = settings.read()
        assertTrue(settings.save(PlaybackAudioLevel(40, true)))
        val activity = instrumentation.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        fun <T : View> view(tag: String): T = activity.window.decorView.findViewWithTag(tag)
        try {
            instrumentation.waitForIdleSync()
            assertEquals(PlaybackAudioLevel(40, true), ReceiverPlaybackOutput.audioLevel())
            instrumentation.setInTouchMode(false)
            instrumentation.runOnMainSync { assertTrue(view<Button>("audio-level-up").requestFocus()) }
            instrumentation.sendKeyDownUpSync(KeyEvent.KEYCODE_DPAD_CENTER)
            instrumentation.waitForIdleSync()
            assertEquals(PlaybackAudioLevel(50, true), settings.read())
            instrumentation.runOnMainSync { assertTrue(view<SeekBar>("audio-level-slider").requestFocus()) }
            instrumentation.sendKeyDownUpSync(KeyEvent.KEYCODE_DPAD_RIGHT)
            instrumentation.waitForIdleSync()
            assertEquals(PlaybackAudioLevel(55, true), settings.read())
            instrumentation.runOnMainSync { assertTrue(view<CheckBox>("audio-level-mute").requestFocus()) }
            instrumentation.sendKeyDownUpSync(KeyEvent.KEYCODE_DPAD_CENTER)
            instrumentation.waitForIdleSync()
            assertEquals(PlaybackAudioLevel(55, false), settings.read())
            assertEquals(settings.read(), ReceiverPlaybackOutput.audioLevel())
            ReceiverPlaybackOutput.reset()
            assertEquals(PlaybackAudioLevel(55, false), ReceiverPlaybackOutput.audioLevel())
            // A later Activity/Service initialization must not overwrite a live choice.
            ReceiverPlaybackOutput.initializeAudioLevel(PlaybackAudioLevel(100))
            assertEquals(PlaybackAudioLevel(55, false), ReceiverPlaybackOutput.audioLevel())
            instrumentation.runOnMainSync { view<CheckBox>("audio-level-mute").performClick() }
            instrumentation.waitForIdleSync()
            assertEquals(PlaybackAudioLevel(55, true), PlaybackAudioSettings(context).read())
            Thread.sleep(200)
            // Idle receiver only: no PIN, fingerprint, network address or private media.
            val image = instrumentation.uiAutomation.takeScreenshot()
            File(context.getExternalFilesDir(null), "audio-controls.png").outputStream().use {
                image.compress(Bitmap.CompressFormat.PNG, 100, it)
            }
            image.recycle()
        } finally {
            instrumentation.runOnMainSync { activity.finish() }
            ReceiverPlaybackOutput.setAudioLevel(original)
            settings.save(original)
        }
    }

    fun testCorruptPreferenceFallsBackToSilence() {
        val context = instrumentation.targetContext
        val settings = PlaybackAudioSettings(context)
        val original = settings.read()
        try {
            val preferences = context.getSharedPreferences("playback-audio", Context.MODE_PRIVATE)
            assertTrue(preferences.edit().putString("percent", "invalid").commit())
            assertEquals(PlaybackAudioLevel(0, true), settings.read())
            assertTrue(preferences.edit().putInt("percent", 101).commit())
            assertEquals(PlaybackAudioLevel(0, true), settings.read())
        } finally { assertTrue(settings.save(original)) }
    }

    fun testPanelReportsUnsavedAndOutputFailureWithoutUndoingSafeState() {
        instrumentation.runOnMainSync {
            val context = instrumentation.targetContext
            var level = PlaybackAudioLevel(40)
            var failOutput = false
            val panel = PlaybackAudioPanel(context, { level }) { value, persist ->
                assertTrue("button changes request persistence", persist)
                level = if (failOutput) value.copy(muted = true) else value
                if (failOutput) PlaybackAudioPanel.ChangeResult.OUTPUT_FAILED
                else PlaybackAudioPanel.ChangeResult.NOT_SAVED
            }
            panel.findViewWithTag<Button>("audio-level-up").performClick()
            assertEquals(PlaybackAudioLevel(50), level)
            assertEquals(context.getString(R.string.audio_level_not_saved), panel.findViewWithTag<TextView>("audio-level-result").text)
            failOutput = true
            panel.findViewWithTag<CheckBox>("audio-level-mute").performClick()
            assertEquals(PlaybackAudioLevel(50, true), level)
            assertTrue(panel.findViewWithTag<CheckBox>("audio-level-mute").isChecked)
            assertEquals(context.getString(R.string.audio_level_failed), panel.findViewWithTag<TextView>("audio-level-result").text)
        }
    }
}
