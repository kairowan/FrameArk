package dev.frameark.receiver

/** Present only in the instrumentation APK; native counterpart is opt-in. */
class PlaybackNetworkProbe {
    external fun send(video: ByteArray, audio: ByteArray): Int
    external fun diagnostic(): String?
}
