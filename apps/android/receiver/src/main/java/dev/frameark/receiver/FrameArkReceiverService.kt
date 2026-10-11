package dev.frameark.receiver

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
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
    private lateinit var coordinator: ReceiverSessionCoordinator
    private lateinit var networkCoordinator: ReceiverNetworkCoordinator
    private lateinit var networkObserver: ReceiverNetworkObserver
    private var connectivityNetworkCallback: ConnectivityManager.NetworkCallback? = null
    private val mainHandler = Handler(Looper.getMainLooper())
    private lateinit var multicastLockCoordinator: ReceiverMulticastLockCoordinator
    private var multicastLock: WifiManager.MulticastLock? = null
    private lateinit var identityCoordinator: ReceiverIdentityCoordinator
    private lateinit var trustSnapshotStore: AndroidTrustSnapshotStore
    private var identityReady = false

    override fun onCreate() {
        super.onCreate()
        native = FrameArkNative()
        trustSnapshotStore = AndroidTrustSnapshotStore(
            file = filesDir.resolve(TRUST_SNAPSHOT_FILE),
            native = native,
        )
        playbackLoop = MediaPlaybackLoop(
            MediaPlaybackPump(
                native,
                ::onVideoFrame,
                ::onAudioFrame,
                videoConfigConsumer = ::onVideoConfig,
                audioConfigConsumer = ::onAudioConfig,
            ),
            onError = {
                Log.w(TAG, "media playback loop stopped after consumer failure")
                coordinator.onPlaybackFailure()
            },
        )
        networkCoordinator = ReceiverNetworkCoordinator(native)
        val connectivityManager = getSystemService(ConnectivityManager::class.java)
        networkObserver = ReceiverNetworkObserver(
            register = { listener ->
                val manager = connectivityManager ?: error("connectivity service is unavailable")
                val callback = object : ConnectivityManager.NetworkCallback() {
                    override fun onAvailable(network: Network) = listener.onAvailable()

                    override fun onLost(network: Network) = listener.onLost()

                    override fun onCapabilitiesChanged(
                        network: Network,
                        networkCapabilities: android.net.NetworkCapabilities,
                    ) = listener.onCapabilitiesChanged()

                    override fun onLinkPropertiesChanged(
                        network: Network,
                        linkProperties: android.net.LinkProperties,
                    ) = listener.onLinkPropertiesChanged()
                }
                connectivityNetworkCallback = callback
                manager.registerDefaultNetworkCallback(callback)
            },
            unregister = { _ ->
                connectivityNetworkCallback?.let { callback ->
                    connectivityManager?.unregisterNetworkCallback(callback)
                    connectivityNetworkCallback = null
                }
            },
            onNetworkChanged = {
                mainHandler.post {
                    val recovered = if (::coordinator.isInitialized) coordinator.recover() else false
                    if (!recovered) Log.d(TAG, "network change observed outside running session")
                    if (::networkObserver.isInitialized) networkObserver.markRecoveryComplete()
                }
            },
        )
        val wifiManager = getSystemService(WifiManager::class.java)
        multicastLockCoordinator = ReceiverMulticastLockCoordinator(
            acquireLock = {
                val manager = wifiManager ?: error("Wi-Fi multicast is unavailable")
                multicastLock = manager.createMulticastLock("frameark.discovery").apply {
                    setReferenceCounted(false)
                    acquire()
                }
            },
            releaseLock = {
                multicastLock?.release()
                multicastLock = null
            },
        )
        coordinator = ReceiverSessionCoordinator(native, playbackLoop, network = networkCoordinator)
        identityCoordinator = ReceiverIdentityCoordinator {
            AndroidDeviceIdentityStore().getOrCreate()
        }
        identityReady = when (val identityState = identityCoordinator.initialize()) {
            is ReceiverIdentityCoordinator.State.Ready -> {
                val installed = native.setDeviceIdentity(identityState.identity)
                if (!installed) {
                    Log.e(TAG, "native identity bridge unavailable; receiver will remain stopped")
                    false
                } else {
                    val restored = trustSnapshotStore.restore()
                    if (restored) {
                        Log.i(TAG, "device identity and trust snapshot ready")
                    } else {
                        Log.e(TAG, "trust snapshot could not be restored; receiver will remain stopped")
                    }
                    restored
                }
            }

            is ReceiverIdentityCoordinator.State.Failed -> {
                Log.e(TAG, "device identity unavailable; receiver will remain stopped")
                false
            }

            ReceiverIdentityCoordinator.State.Uninitialized -> false
        }
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val action = ReceiverServicePolicy.normalize(intent?.action)
        if (ReceiverServicePolicy.isStop(action)) {
            networkObserver.stop()
            coordinator.stop()
            multicastLockCoordinator.release()
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelfResult(startId)
            return START_NOT_STICKY
        }
        if (!identityReady) {
            stopSelfResult(startId)
            return START_NOT_STICKY
        }
        multicastLockCoordinator.acquire()
        if (!coordinator.start()) {
            multicastLockCoordinator.release()
            stopSelfResult(startId)
            return START_NOT_STICKY
        }
        if (!networkObserver.start()) {
            Log.w(TAG, "network observer unavailable; automatic recovery is disabled")
        }
        startAsForeground()
        return START_STICKY
    }

    override fun onDestroy() {
        if (!trustSnapshotStore.persist()) {
            Log.w(TAG, "trust snapshot was not persisted")
        }
        networkObserver.close()
        mainHandler.removeCallbacksAndMessages(null)
        coordinator.close()
        multicastLockCoordinator.close()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    /** Persists an explicitly approved Rust trust mutation through Keystore. */
    fun trustPeer(peer: FrameArkNative.TrustedPeer): Boolean =
        native.trustPeer(peer) && trustSnapshotStore.persist()

    /** Persists an explicit Rust trust revocation through Keystore. */
    fun revokePeer(deviceId: String): Boolean =
        native.revokePeer(deviceId) && trustSnapshotStore.persist()

    private fun onVideoFrame(frame: FrameArkNative.MediaFrame) {
        // Surface/MediaCodec ownership is attached by the active UI session.
        // Keep the service loop alive until that platform sink is configured.
        check(frame.kind == FrameArkNative.MediaKind.VIDEO)
    }

    private fun onAudioFrame(frame: FrameArkNative.MediaFrame) {
        // Audio decoder/AudioTrack ownership remains in the platform adapter.
        check(frame.kind == FrameArkNative.MediaKind.AUDIO)
    }

    private fun onVideoConfig(config: FrameArkNative.VideoConfig) {
        Log.i(
            TAG,
            "video config ${config.codec} ${config.width}x${config.height} " +
                "${config.frameRateNumerator}/${config.frameRateDenominator}",
        )
    }

    private fun onAudioConfig(config: FrameArkNative.AudioConfig) {
        Log.i(TAG, "audio config ${config.codec} ${config.sampleRate}Hz/${config.channels}ch")
    }

    private fun startAsForeground() {
        val notification = Notification.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_frameark)
            .setContentTitle(getString(R.string.service_title))
            .setContentText(notificationText())
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

    private fun notificationText(): String = when (val state = networkCoordinator.state()) {
        is ReceiverNetworkCoordinator.State.Ready -> getString(
            R.string.service_text_pairing,
            state.info.port,
            state.info.pairingCode,
        )

        ReceiverNetworkCoordinator.State.Stopped,
        is ReceiverNetworkCoordinator.State.Failed,
        -> getString(R.string.service_text)
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
        private const val TRUST_SNAPSHOT_FILE = "frameark-trust.fats"
        private const val TAG = "FrameArkReceiver"
    }
}
