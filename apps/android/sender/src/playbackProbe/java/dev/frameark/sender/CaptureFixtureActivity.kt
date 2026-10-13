package dev.frameark.sender

import android.app.Activity
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.media.*
import android.os.Bundle
import android.view.View
import android.view.WindowManager
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.sin

/** Opt-in debug emulator fixture, never included in an owner APK. Actual OS display/audio source. */
class CaptureFixtureActivity : Activity() {
    private val stopped = AtomicBoolean()
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        setContentView(object : View(this) {
            private val paint = Paint()
            private var frame = 0
            override fun onDraw(canvas: Canvas) {
                paint.color = Color.RED; canvas.drawRect(0f, 0f, width.toFloat(), height / 2f, paint)
                paint.color = Color.BLUE; canvas.drawRect(0f, height / 2f, width.toFloat(), height.toFloat(), paint)
                paint.color = Color.WHITE
                val x = (frame++ * 13 % maxOf(width, 1)).toFloat()
                canvas.drawRect(x, 0f, x + 12, height.toFloat(), paint)
                paint.textSize = 48f; canvas.drawText("FrameArk OS capture fixture", 24f, height / 2f, paint)
                postInvalidateOnAnimation()
            }
        })
        Thread({
            var track: AudioTrack? = null
            try {
                val attributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                if (android.os.Build.VERSION.SDK_INT >= 29) attributes.setAllowedCapturePolicy(AudioAttributes.ALLOW_CAPTURE_BY_ALL)
                val output = AudioTrack.Builder().setAudioAttributes(attributes.build())
                    .setAudioFormat(AudioFormat.Builder().setSampleRate(48000).setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).setEncoding(AudioFormat.ENCODING_PCM_16BIT).build())
                    .setTransferMode(AudioTrack.MODE_STREAM).setBufferSizeInBytes(16384).build().also { track = it }
                output.play()
                val samples = ShortArray(2048)
                var position = 0L
                while (!stopped.get()) {
                    repeat(1024) { i -> val sample = (sin((position + i) * 2 * Math.PI * 440 / 48000) * 3000).toInt().toShort(); samples[i * 2] = sample; samples[i * 2 + 1] = sample }
                    position += 1024
                    if (output.write(samples, 0, samples.size) < 0) break
                }
            } finally { track?.let { runCatching { it.stop() }; it.release() } }
        }, "capture-fixture-tone").start()
    }
    override fun onDestroy() { stopped.set(true); super.onDestroy() }
}
