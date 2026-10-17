package dev.frameark.receiver

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.view.Surface

/** Process-local surface/output ownership, never a second protocol state machine. */
internal object DlnaOutputOwner {
    @Volatile var surface: Surface? = null
    @Volatile var images: DlnaImageTarget? = null
    @Volatile var subtitles: DlnaSubtitleTarget? = null
    @Volatile var output: DlnaPlayerOutput? = null
    @Volatile var failure = 0
}

/** Starts only from an explicit foreground UI opt-in; never restarts itself. */
class DlnaReceiverService : Service() {
    private var lock: WifiManager.MulticastLock? = null
    private var started = false

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) { stopSelf(); return START_NOT_STICKY }
        if (started) return START_NOT_STICKY
        val bind = intent?.getStringExtra(EXTRA_BIND) ?: run { stopSelf(); return START_NOT_STICKY }
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(CHANNEL, getString(R.string.dlna_title), NotificationManager.IMPORTANCE_LOW))
        val notification = Notification.Builder(this, CHANNEL).setSmallIcon(R.drawable.ic_frameark)
            .setContentTitle(getString(R.string.dlna_title)).setContentText(getString(R.string.dlna_notification)).setOngoing(true).build()
        if (Build.VERSION.SDK_INT >= 29) startForeground(1002, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
        else startForeground(1002, notification)
        val native = FrameArkNative()
        if (native.load() !is FrameArkNative.LoadResult.Loaded || !runCatching { native.setDeviceIdentity(AndroidDeviceIdentityStore().getOrCreate()) }.getOrDefault(false)) {
            DlnaOutputOwner.failure = -3
            stopSelf()
            return START_NOT_STICKY
        }
        try {
            lock = getSystemService(WifiManager::class.java)?.createMulticastLock("frameark.dlna")?.apply { setReferenceCounted(false); acquire() }
            DlnaOutputOwner.failure = DlnaNative.start(bind)
            started = DlnaOutputOwner.failure == 0
            if (started) DlnaOutputOwner.output = DlnaPlayerOutput(DlnaNative.outputToken(), DlnaOutputOwner.images, DlnaOutputOwner.subtitles) { DlnaOutputOwner.surface }
            else stopSelf()
        } catch (_: Exception) { DlnaOutputOwner.failure = -4; stopSelf() }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        DlnaOutputOwner.output?.close()
        DlnaOutputOwner.output = null
        if (started) DlnaNative.stop()
        started = false
        runCatching { lock?.release() }
        lock = null
        stopForeground(STOP_FOREGROUND_REMOVE)
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        const val ACTION_STOP = "dev.frameark.receiver.DLNA_STOP"
        const val EXTRA_BIND = "bind_address"
        private const val CHANNEL = "frameark.receiver.dlna"
    }
}
