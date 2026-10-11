package dev.frameark.receiver

import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

/**
 * Owns the Android-side scheduling boundary for one encoded-media pump.
 *
 * The loop deliberately schedules only bounded `drainOnce` calls. Rust owns
 * queue bounds and ordering while platform consumers own codec backpressure;
 * a consumer exception stops this loop and is reported to the lifecycle owner
 * instead of allowing an executor thread to keep running in a broken state.
 */
class MediaPlaybackLoop(
    private val pump: MediaPlaybackPump,
    tickIntervalMs: Long = DEFAULT_TICK_INTERVAL_MS,
    private val executorFactory: () -> ScheduledExecutorService = {
        Executors.newSingleThreadScheduledExecutor { runnable ->
            Thread(runnable, "frameark-media-playback").apply { isDaemon = true }
        }
    },
    private val onError: (Throwable) -> Unit = {},
) : AutoCloseable {
    companion object {
        const val DEFAULT_TICK_INTERVAL_MS = 16L
        const val MIN_TICK_INTERVAL_MS = 1L
        const val MAX_TICK_INTERVAL_MS = 1_000L
    }

    private val intervalMs = tickIntervalMs.also {
        require(it in MIN_TICK_INTERVAL_MS..MAX_TICK_INTERVAL_MS) {
            "playback tick interval is outside the bounded range"
        }
    }
    private val lock = Any()
    private var executor: ScheduledExecutorService? = null
    private var scheduled: ScheduledFuture<*>? = null

    @Volatile
    private var running = false

    /** Starts one fixed-rate drain loop, or returns false if the native ABI is unavailable. */
    fun start(): Boolean = synchronized(lock) {
        if (running) return true
        if (!pump.start()) return false
        val executor = executor ?: executorFactory().also { this.executor = it }
        running = true
        try {
            scheduled = executor.scheduleAtFixedRate(
                ::tick,
                0,
                intervalMs,
                TimeUnit.MILLISECONDS,
            )
            true
        } catch (error: RuntimeException) {
            running = false
            pump.stop()
            throw error
        }
    }

    /** Stops future ticks and leaves native session ownership to the caller. */
    fun stop() {
        synchronized(lock) {
            running = false
            scheduled?.cancel(false)
            scheduled = null
            pump.stop()
        }
    }

    /** Whether a scheduled drain loop is currently active. */
    fun isRunning(): Boolean = running

    /** Stops the loop and releases its executor. Safe to call repeatedly. */
    override fun close() {
        stop()
        synchronized(lock) {
            executor?.shutdownNow()
            executor = null
        }
    }

    private fun tick() {
        if (!running) return
        try {
            pump.drainOnce()
        } catch (error: Throwable) {
            synchronized(lock) {
                running = false
                scheduled?.cancel(false)
                scheduled = null
                pump.stop()
            }
            runCatching { onError(error) }
        }
    }
}
