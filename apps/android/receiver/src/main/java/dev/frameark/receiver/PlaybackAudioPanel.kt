package dev.frameark.receiver

import android.content.Context
import android.view.View
import android.widget.Button
import android.widget.CheckBox
import android.widget.LinearLayout
import android.widget.SeekBar
import android.widget.TextView

/** Touch/D-pad controls for this app's audio, never the device-wide stream volume. */
class PlaybackAudioPanel(
    context: Context,
    private val read: () -> PlaybackAudioLevel,
    private val change: (PlaybackAudioLevel, Boolean) -> ChangeResult,
) : LinearLayout(context) {
    enum class ChangeResult { APPLIED, NOT_SAVED, OUTPUT_FAILED }

    private val summary = TextView(context).apply {
        textSize = 18f
        tag = "audio-level-summary"
        accessibilityLiveRegion = View.ACCESSIBILITY_LIVE_REGION_POLITE
    }
    private val slider = SeekBar(context).apply {
        max = 100
        keyProgressIncrement = 5
        tag = "audio-level-slider"
        isSaveEnabled = false
        minimumHeight = (48 * resources.displayMetrics.density).toInt()
        filterTouchesWhenObscured = true
        contentDescription = context.getString(R.string.audio_level_slider)
    }
    private val mute = CheckBox(context).apply {
        text = context.getString(R.string.audio_level_mute)
        tag = "audio-level-mute"
        isSaveEnabled = false
        filterTouchesWhenObscured = true
        minimumHeight = (48 * resources.displayMetrics.density).toInt()
    }
    private val result = TextView(context).apply {
        tag = "audio-level-result"
        accessibilityLiveRegion = View.ACCESSIBILITY_LIVE_REGION_POLITE
    }
    private var refreshing = false
    private var dragging = false

    init {
        orientation = VERTICAL
        tag = "audio-level-panel"
        val padding = (16 * resources.displayMetrics.density).toInt()
        setPadding(padding, padding, padding, padding)
        addView(summary)
        addView(TextView(context).apply { text = context.getString(R.string.audio_level_help) })
        addView(slider)
        addView(LinearLayout(context).apply {
            orientation = HORIZONTAL
            for ((delta, label) in listOf(-10 to R.string.audio_level_down, 10 to R.string.audio_level_up)) {
                addView(Button(context).apply {
                    text = context.getString(label)
                    tag = if (delta < 0) "audio-level-down" else "audio-level-up"
                    filterTouchesWhenObscured = true
                    setOnClickListener { apply(read().stepped(delta), true) }
                }, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
            }
        })
        addView(mute)
        addView(result)
        slider.setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
            override fun onStartTrackingTouch(seekBar: SeekBar) { dragging = true }
            override fun onProgressChanged(seekBar: SeekBar, progress: Int, fromUser: Boolean) {
                if (fromUser && !refreshing) apply(read().copy(percent = progress), !dragging)
            }
            override fun onStopTrackingTouch(seekBar: SeekBar) {
                dragging = false
                apply(read(), true)
            }
        })
        mute.setOnCheckedChangeListener { _, checked ->
            if (!refreshing) apply(read().copy(muted = checked), true)
        }
        refresh()
    }

    private fun apply(value: PlaybackAudioLevel, persist: Boolean) {
        val outcome = change(value, persist)
        result.text = when (outcome) {
            ChangeResult.APPLIED -> ""
            ChangeResult.NOT_SAVED -> context.getString(R.string.audio_level_not_saved)
            ChangeResult.OUTPUT_FAILED -> context.getString(R.string.audio_level_failed)
        }
        refresh()
    }

    fun refresh() {
        refreshing = true
        try {
            val level = read()
            slider.progress = level.percent
            mute.isChecked = level.muted
            summary.text = context.getString(
                if (level.muted) R.string.audio_level_muted else R.string.audio_level_value,
                level.percent,
            )
        } finally { refreshing = false }
    }
}
