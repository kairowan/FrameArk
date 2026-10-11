package dev.frameark.receiver

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log

/**
 * M2 lifecycle owner for a receiver session.
 *
 * Rust owns discovery, pairing, session state, and media policy. This service
 * only keeps the Android process eligible for playback, owns the bounded media
 * polling loop, and exposes explicit start/stop actions for the Rust session.
 */
class FrameArkReceiverService : Service() {
    private lateinit var native: FrameArkNative
    private lateinit var playbackLoop: MediaPlaybackLoop

    override fun onCreate() {
        super.onCreate()
        native = FrameArkNative()
        playbackLoop = MediaPlaybackLoop(
            MediaPlaybackPump(native, ::onVideoFrame, ::onAudioFrame),
            onError = { Log.w(TAG, "media playback loop stopped after consumer failure") },
        )
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val action = ReceiverServicePolicy.normalize(intent?.action)
        if (ReceiverServicePolicy.isStop(action)) {
            playbackLoop.stop()
            native.stopReceiver()
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelfResult(startId)
            return START_NOT_STICKY
        }
        when (native.startReceiver()) {
            is FrameArkNative.ReceiverResult.Started,
            is FrameArkNative.ReceiverResult.AlreadyStarted,
            -> Unit

            else -> {
                stopSelfResult(startId)
                return START_NOT_STICKY
            }
        }
        if (!playbackLoop.start()) {
            native.stopReceiver()
            stopSelfResult(startId)
            return START_NOT_STICKY
        }
        startAsForeground()
        return START_STICKY
    }

    override fun onDestroy() {
        playbackLoop.close()
        native.stopReceiver()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun onVideoFrame(frame: FrameArkNative.MediaFrame) {
        // Surface/MediaCodec ownership is attached by the active UI session.
        // Keep the service loop alive until that platform sink is configured.
        check(frame.kind == FrameArkNative.MediaKind.VIDEO)
    }

    private fun onAudioFrame(frame: FrameArkNative.MediaFrame) {
        // Audio decoder/AudioTrack ownership remains in the platform adapter.
        check(frame.kind == FrameArkNative.MediaKind.AUDIO)
    }

    private fun startAsForeground() {
        val notification = Notification.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_frameark)
            .setContentTitle(getString(R.string.service_title))
            .setContentText(getString(R.string.service_text))
            .setOngoing(true)
            .build()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                getString(R.string.service_channel),
                NotificationManager.IMPORTANCE_LOW,
            )
            getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }
    }

    companion object {
        private const val CHANNEL_ID = "frameark.receiver.playback"
        private const val NOTIFICATION_ID = 1001
        private const val TAG = "FrameArkReceiver"
    }
}
