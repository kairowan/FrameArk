package dev.frameark.receiver

import android.content.Context

/** App-private, non-sensitive output preference; no peer or session material. */
class PlaybackAudioSettings(context: Context) {
    private val preferences = context.applicationContext.getSharedPreferences("playback-audio", Context.MODE_PRIVATE)

    fun read(): PlaybackAudioLevel = try {
        PlaybackAudioLevel(preferences.getInt("percent", 50), preferences.getBoolean("muted", false))
    } catch (_: Exception) {
        // Corrupt/type-mismatched preferences must not unexpectedly restore sound.
        PlaybackAudioLevel(0, true)
    }

    fun save(value: PlaybackAudioLevel): Boolean = try {
        preferences.edit().putInt("percent", value.percent).putBoolean("muted", value.muted).commit()
    } catch (_: Exception) { false }
}
