package dev.frameark.sender

import android.app.*
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ServiceInfo
import android.content.pm.PackageManager
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.*
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference

/** Owns one OS grant. No sticky restart, reusable grant, exported binder or capture file. */
class ScreenCaptureService : Service() {
    companion object {
        const val ACTION_START = "dev.frameark.sender.CAPTURE_START"
        const val ACTION_STOP = "dev.frameark.sender.CAPTURE_STOP"
        const val EXTRA_HANDOFF = "handoff"
        const val EXTRA_GRANT = "grant"
        val handoff = CaptureHandoff()
        private const val CHANNEL = "frameark.capture"
        private const val NOTIFICATION = 2
    }

    private val native = SenderNative()
    private val stop = AtomicBoolean()
    private val handle = AtomicLong()
    private val projection = AtomicReference<MediaProjection?>()
    private val handler = Handler(Looper.getMainLooper())
    private var worker: Thread? = null
    private var generation = 0L
    private val screenOff = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) { if (intent?.action == Intent.ACTION_SCREEN_OFF) requestStop() }
    }
    private var receiverRegistered = false

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            requestStop()
            if (worker == null) stopSelfResult(startId)
            return START_NOT_STICKY
        }
        val request = intent?.getLongExtra(EXTRA_HANDOFF, 0)?.let(handoff::take)
        @Suppress("DEPRECATION")
        val grant = if (Build.VERSION.SDK_INT >= 33) intent?.getParcelableExtra(EXTRA_GRANT, Intent::class.java) else intent?.getParcelableExtra<Intent>(EXTRA_GRANT)
        if (intent?.action != ACTION_START || request == null || grant == null || Build.VERSION.SDK_INT < 29 || worker != null) {
            if (worker == null) stopSelfResult(startId)
            return START_NOT_STICKY
        }
        stop.set(false)
        generation = CaptureStatus.begin()
        try {
            check(!request.options.audio || checkSelfPermission(android.Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED)
            foreground(request.options.audio)
            check(native.load())
            val value = requireNotNull(getSystemService(MediaProjectionManager::class.java).getMediaProjection(Activity.RESULT_OK, grant))
            projection.set(value)
            value.registerCallback(object : MediaProjection.Callback() {
                override fun onStop() { stop.set(true) }
            }, handler)
            if (Build.VERSION.SDK_INT >= 33) registerReceiver(screenOff, IntentFilter(Intent.ACTION_SCREEN_OFF), RECEIVER_NOT_EXPORTED)
            else registerReceiver(screenOff, IntentFilter(Intent.ACTION_SCREEN_OFF))
            receiverRegistered = true
            worker = Thread({ produce(request, value) }, "frameark-capture").also { it.start() }
        } catch (failure: Exception) {
            CaptureStatus.end(generation, location(failure))
            requestStop()
            stopSelfResult(startId)
        }
        return START_NOT_STICKY
    }

    private fun produce(request: CaptureRequest, value: MediaProjection) {
        var transport: CaptureTransport? = null
        var failureCode = ""
        try {
            if (stop.get()) throw InterruptedException()
            val o = request.options
            val id = native.startCapture(request.ticket, request.pin, o.width, o.height, o.fps, o.audio)
            check(id > 0)
            handle.set(id)
            transport = CaptureTransport(native, id, stop)
            transport.awaitStreaming()
            ScreenCaptureEncoder().run(value, o, resources.configuration.densityDpi, stop, transport::submit) { video, audio, nonSilent, elapsed ->
                CaptureStatus.publish(generation, CaptureStatus.Snapshot(true, "sharing", video, audio, nonSilent, elapsed))
            }
            transport.finish()
        } catch (failure: Exception) {
            if (!stop.get()) failureCode = "${location(failure)}; native=${transport?.stage() ?: -1}"
        } finally {
            runCatching { transport?.close() }
            handle.set(0)
            runCatching { projection.getAndSet(null)?.stop() }
            CaptureStatus.end(generation, failureCode)
            handler.post {
                worker = null
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
            }
        }
    }

    private fun requestStop() { stop.set(true); runCatching { projection.getAndSet(null)?.stop() } }

    private fun foreground(audio: Boolean) {
        getSystemService(NotificationManager::class.java).createNotificationChannel(NotificationChannel(CHANNEL, getString(R.string.capture_channel), NotificationManager.IMPORTANCE_LOW))
        val end = PendingIntent.getService(this, 2, Intent(this, ScreenCaptureService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val open = PendingIntent.getActivity(this, 3, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val notification = Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_frameark).setContentTitle(getString(R.string.capture_notification))
            .setContentText(getString(if (audio) R.string.capture_with_audio else R.string.capture_video_only))
            .setContentIntent(open).setOngoing(true).setCategory(Notification.CATEGORY_SERVICE)
            .addAction(Notification.Action.Builder(null, getString(R.string.capture_stop), end).build()).build()
        if (Build.VERSION.SDK_INT >= 29) startForeground(NOTIFICATION, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION)
        else startForeground(NOTIFICATION, notification)
    }

    override fun onTaskRemoved(rootIntent: Intent?) { requestStop(); super.onTaskRemoved(rootIntent) }
    override fun onDestroy() {
        requestStop()
        handle.get().takeIf { it > 0 }?.let(native::cancel)
        if (receiverRegistered) unregisterReceiver(screenOff)
        stopForeground(STOP_FOREGROUND_REMOVE)
        super.onDestroy()
    }
    override fun onBind(intent: Intent?): IBinder? = null

    private fun location(failure: Exception): String = failure.stackTrace.firstOrNull { it.className.startsWith("dev.frameark.sender.") }?.let { "${it.fileName}:${it.lineNumber}" } ?: "platform"
}
