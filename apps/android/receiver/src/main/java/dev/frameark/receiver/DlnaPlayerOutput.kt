package dev.frameark.receiver

import android.media.AudioAttributes
import android.media.MediaPlayer
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.view.Surface
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

/** OS player ownership only: every MediaPlayer call runs on this one Looper. */
internal class DlnaPlayerOutput(private val token: Long, private val surface: () -> Surface?) : AutoCloseable {
    init { require(token > 0) }
    private val thread = HandlerThread("FrameArk DLNA output").apply { start() }
    private val handler = Handler(thread.looper)
    private val running = AtomicBoolean(true)
    private val closed = AtomicBoolean()
    private var player: MediaPlayer? = null
    private var source: RustMediaDataSource? = null
    private var pending = 0L
    private var accepted = 0L
    private var volume = 0.5f
    private var prepared = false
    private var lastFeedback = 0L
    private val rendered = AtomicLong()
    @Volatile private var status = "Waiting for media"
    @Volatile private var dimensions: VideoDisplaySize? = null

    fun diagnostic(): String = "DLNA: $status; video-start notifications: ${rendered.get()}"
    fun videoDisplaySize(): VideoDisplaySize? = dimensions
    fun isClosed(): Boolean = closed.get()

    private val poll = object : Runnable {
        override fun run() {
            if (!running.get()) return
            try {
                DlnaNative.pollCommand(token)?.let { command ->
                    if (command.size != 4) error("invalid command shape")
                    if (command[1] == 7L) { releasePlayer(); status = "Stopped" }
                    else execute(command)
                }
                val now = SystemClock.elapsedRealtime()
                val active = player
                if (prepared && pending == 0L && accepted > 0 && active != null && now - lastFeedback >= 200L) {
                    observe(active)
                    lastFeedback = now
                }
            } catch (_: Exception) { fail() }
            if (running.get()) handler.postDelayed(this, 20L)
        }
    }

    init { handler.post(poll) }

    private fun execute(command: LongArray) {
        val id = command[0]
        val kind = command[1]
        val argument = command[3]
        check(id > 0 && pending == 0L)
        pending = id
        if (kind == 1L) {
            releasePlayer(keepPending = true)
            val target = surface()?.takeIf { it.isValid } ?: error("surface unavailable")
            val data = RustMediaDataSource(command[2])
            source = data
            val next = MediaPlayer()
            player = next
            next.setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MOVIE).build())
            next.setSurface(target)
            next.setOnPreparedListener { instance ->
                if (!isCurrent(instance)) return@setOnPreparedListener
                try {
                    prepared = true
                    instance.setVolume(volume, volume)
                    dimensions = size(instance.videoWidth, instance.videoHeight)
                    status = "Prepared"
                    finish(id)
                } catch (_: Exception) { fail() }
            }
            next.setOnSeekCompleteListener { instance ->
                if (!isCurrent(instance) || pending == 0L) return@setOnSeekCompleteListener
                try { finish(pending) } catch (_: Exception) { fail() }
            }
            next.setOnCompletionListener { instance ->
                if (!isCurrent(instance)) return@setOnCompletionListener
                status = "Ended"
                if (pending == 0L && accepted > 0) DlnaNative.feedback(accepted, 1, 0, 0)
            }
            next.setOnErrorListener { instance, _, _ ->
                if (isCurrent(instance)) fail()
                true
            }
            next.setOnVideoSizeChangedListener { instance, width, height ->
                if (isCurrent(instance)) dimensions = size(width, height)
            }
            next.setOnInfoListener { instance, what, _ ->
                if (isCurrent(instance) && what == MediaPlayer.MEDIA_INFO_VIDEO_RENDERING_START) rendered.incrementAndGet()
                false
            }
            next.setDataSource(data)
            status = "Preparing"
            next.prepareAsync()
            return
        }
        if (kind == 6L) {
            require(argument in 0L..100L)
            volume = argument / 100f
            if (prepared) player?.setVolume(volume, volume)
            finish(id)
            return
        }
        val active = player?.takeIf { prepared } ?: error("output unavailable")
        when (kind) {
            2L -> { active.start(); status = "Playing"; finish(id) }
            3L -> { if (active.isPlaying) active.pause(); status = "Paused"; finish(id) }
            4L -> {
                if (active.isPlaying) active.pause()
                status = "Stopped"
                active.seekTo(0L, MediaPlayer.SEEK_CLOSEST)
            }
            5L -> {
                require(argument >= 0)
                active.seekTo(argument, MediaPlayer.SEEK_CLOSEST)
            }
            else -> error("unsupported output operation")
        }
    }

    private fun isCurrent(instance: MediaPlayer): Boolean = running.get() && player === instance

    private fun finish(id: Long) {
        if (pending != id) return
        pending = 0
        if (DlnaNative.acknowledge(id, true)) accepted = id
        else { releasePlayer(); status = "Command expired" }
    }

    private fun observe(instance: MediaPlayer) {
        val duration = instance.duration.toLong()
        val position = instance.currentPosition.toLong().coerceAtLeast(0)
        DlnaNative.feedback(accepted, 0, if (duration >= 0) position.coerceAtMost(duration) else position, duration)
    }

    private fun fail() {
        val command = pending
        val previous = accepted
        releasePlayer()
        status = "Playback failed"
        if (command > 0) DlnaNative.acknowledge(command, false)
        else if (previous > 0) DlnaNative.feedback(previous, 2, 0, 0)
    }

    private fun releasePlayer(keepPending: Boolean = false) {
        prepared = false
        accepted = 0
        dimensions = null
        if (!keepPending) pending = 0
        val old = player
        player = null
        // Revoke network reads first so OS teardown cannot wait on a live origin.
        runCatching { source?.close() }
        source = null
        runCatching { old?.release() }
    }

    override fun close() {
        if (!running.compareAndSet(true, false)) return
        handler.removeCallbacks(poll)
        handler.post { try { releasePlayer() } finally { closed.set(true); thread.quitSafely() } }
    }

    private fun size(width: Int, height: Int): VideoDisplaySize? =
        if (width in 1..8192 && height in 1..8192) VideoDisplaySize(width, height) else null
}
