@file:Suppress("DEPRECATION")

package dev.frameark.receiver

import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.media.MediaMuxer
import android.test.InstrumentationTestCase
import android.view.TextureView
import java.io.File
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.URI
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/** Original synthetic media -> HTTP Range -> Rust -> MediaDataSource -> real OS player. */
class DlnaPlaybackTest : InstrumentationTestCase() {
    fun testExplicitForegroundServiceStopsWhenPageCloses() {
        val context = instrumentation.targetContext
        val address = java.net.NetworkInterface.getNetworkInterfaces().toList()
            .filter { it.isUp && !it.isLoopback }.flatMap { it.inetAddresses.toList() }
            .filterIsInstance<java.net.Inet4Address>().first { it.isSiteLocalAddress }.hostAddress!!
        val activity = instrumentation.startActivitySync(Intent(context, DlnaActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            await("surface available") { DlnaOutputOwner.surface?.isValid == true }
            instrumentation.runOnMainSync {
                context.startForegroundService(Intent(context, DlnaReceiverService::class.java).putExtra(DlnaReceiverService.EXTRA_BIND, address))
            }
            await("normal LAN listener and OS output start") { DlnaNative.descriptionUrl() != null && DlnaOutputOwner.output != null }
            assertTrue(requireNotNull(DlnaNative.descriptionUrl()).startsWith("http://$address:"))
            val output = requireNotNull(DlnaOutputOwner.output)
            instrumentation.runOnMainSync { activity.finish() }
            await("leaving page finishes Rust joins and OS release") {
                DlnaNative.descriptionUrl() == null && DlnaOutputOwner.output == null && DlnaNative.outputToken() == 0L && output.isClosed()
            }
            assertEquals(0L, DlnaNative.outputToken())
        } finally {
            context.stopService(Intent(context, DlnaReceiverService::class.java))
            instrumentation.runOnMainSync { activity.finish() }
        }
    }

    fun testVideoPixelsControlsReplacementAndAudioEos() {
        val context = instrumentation.targetContext
        val native = FrameArkNative()
        assertTrue(native.load() is FrameArkNative.LoadResult.Loaded)
        assertFalse("isolated test requires Native stopped", native.isReceiverRunning())
        assertTrue(native.setDeviceIdentity(AndroidDeviceIdentityStore().getOrCreate()))
        val videoFile = File(context.cacheDir, "dlna-original-test.mp4")
        encodeVideo(videoFile)
        val origin = Origin(videoFile.readBytes(), wave())
        val activity = instrumentation.startActivitySync(Intent(context, DlnaActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            await("surface available") { DlnaOutputOwner.surface?.isValid == true }
            assertEquals(0, DlnaNative.startProbe())
            val token = DlnaNative.outputToken()
            assertTrue(token > 0)
            assertNull(DlnaNative.pollCommand(token + 1))
            DlnaOutputOwner.output = DlnaPlayerOutput(token) { DlnaOutputOwner.surface }
            assertEquals(FrameArkNative.ReceiverResult.Failed(-6), native.startReceiver())
            val endpoint = URI(requireNotNull(DlnaNative.descriptionUrl()))
            fun action(name: String, args: String = ""): String = soap(endpoint, name, args)
            fun load(path: String) {
                val result = action("SetAVTransportURI", "<CurrentURI>http://127.0.0.1:${origin.port}/$path</CurrentURI><CurrentURIMetaData></CurrentURIMetaData>")
                assertTrue(result, result.startsWith("HTTP/1.1 200"))
            }
            fun ok(name: String, args: String = "") { val result = action(name, args); assertTrue(result, result.startsWith("HTTP/1.1 200")) }
            assertTrue(action("GetDeviceCapabilities").contains("<PlayMedia>NETWORK</PlayMedia>"))
            assertTrue(action("GetTransportSettings").contains("<PlayMode>NORMAL</PlayMode>"))
            assertTrue(action("ListPresets").contains("<CurrentPresetNameList>FactoryDefaults</CurrentPresetNameList>"))
            load("video.mp4")
            ok("Play", "<Speed>1</Speed>")
            await("real decoded red pixels", 10_000) {
                var red = false
                instrumentation.runOnMainSync {
                    val view = activity.findViewById<android.view.ViewGroup>(android.R.id.content)
                    val texture = (view.getChildAt(0) as android.view.ViewGroup).let { group -> (0 until group.childCount).map(group::getChildAt).filterIsInstance<TextureView>().single() }
                    val bitmap = texture.bitmap
                    if (bitmap != null) {
                        val pixel = bitmap.getPixel(bitmap.width / 2, bitmap.height / 2)
                        red = Color.red(pixel) > 150 && Color.green(pixel) < 100 && Color.blue(pixel) < 100
                        bitmap.recycle()
                    }
                }
                red
            }
            await("decoder position advances") { action("GetPositionInfo").contains("<RelTime>00:00:01</RelTime>") }
            ok("Pause")
            assertTrue(action("GetTransportInfo").contains("PAUSED_PLAYBACK"))
            ok("Seek", "<Unit>REL_TIME</Unit><Target>00:00:03</Target>")
            assertTrue(action("GetPositionInfo").contains("<RelTime>00:00:03</RelTime>"))
            ok("SetVolume", "<Channel>Master</Channel><DesiredVolume>27</DesiredVolume>")
            assertTrue(action("GetVolume", "<Channel>Master</Channel>").contains("<CurrentVolume>27</CurrentVolume>"))
            ok("SelectPreset", "<PresetName>FactoryDefaults</PresetName>")
            assertTrue(action("GetVolume", "<Channel>Master</Channel>").contains("<CurrentVolume>50</CurrentVolume>"))
            ok("Seek", "<Unit>TRACK_NR</Unit><Target>1</Target>")
            assertTrue(action("GetPositionInfo").contains("<RelTime>00:00:00</RelTime>"))
            assertTrue(action("Next").contains("<errorCode>711</errorCode>"))
            assertTrue(action("Previous").contains("<errorCode>711</errorCode>"))
            ok("Play", "<Speed>1</Speed>")
            val screenshot = instrumentation.uiAutomation.takeScreenshot()
            File(context.getExternalFilesDir(null), "dlna-playback.png").outputStream().use { screenshot.compress(Bitmap.CompressFormat.PNG, 100, it) }
            screenshot.recycle()
            ok("Stop")
            assertTrue(action("GetTransportInfo").contains("STOPPED"))
            ok("Play", "<Speed>1</Speed>")
            load("tone.wav") // Replacement revokes the old source, with a different OS player.
            assertTrue("replacing a playing source resumes the new OS player", action("GetTransportInfo").contains("PLAYING"))
            await("WAV decoder position advances") { action("GetPositionInfo").contains("<RelTime>00:00:01</RelTime>") }
            await("WAV reaches EOS", 12_000) { action("GetTransportInfo").contains("STOPPED") }
            assertTrue("Range reads reached the origin", origin.reads.get() >= 4)
            assertNull("stale consumer never gets another runtime's work", DlnaNative.pollCommand(token + 1))
            instrumentation.sendStatus(0, android.os.Bundle().apply {
                putString("stream", "\nDLNA original MP4: decoded red pixels, Play/Pause/Seek/Stop/replay, FactoryDefaults volume and TRACK_NR; WAV replacement auto-resumes: position + EOS; HTTP Range requests=${origin.reads.get()}; ${DlnaOutputOwner.output?.diagnostic()}\n")
            })
        } finally {
            val output = DlnaOutputOwner.output
            output?.close()
            DlnaOutputOwner.output = null
            DlnaNative.stop()
            await("OS player released") { output?.isClosed() != false }
            assertNull("listener removed", DlnaNative.descriptionUrl())
            assertEquals(0L, DlnaNative.outputToken())
            origin.close()
            instrumentation.runOnMainSync { activity.finish() }
            assertTrue(videoFile.delete())
        }
    }

    fun testByteAdapterBoundsShortReadAndIdempotentClose() {
        var closes = 0
        var requested = 0
        val source = RustMediaDataSource(1, object : DlnaSourceAccess {
            override fun sourceSize(id: Long) = 100L
            override fun sourceRead(id: Long, position: Long, size: Int): ByteArray {
                requested = size
                return if (position >= 100) byteArrayOf() else byteArrayOf(1, 2, 3)
            }
            override fun sourceClose(id: Long) { closes++ }
        })
        val buffer = ByteArray(100_000)
        assertEquals(3, source.readAt(0, buffer, 5, 90_000))
        assertEquals(65_536, requested)
        assertEquals(1.toByte(), buffer[5])
        assertEquals(0, source.readAt(0, buffer, 0, 0))
        assertEquals(-1, source.readAt(100, buffer, 0, 1))
        try { source.readAt(-1, buffer, 0, 1); fail("negative offset") } catch (_: java.io.IOException) { }
        try { source.readAt(0, buffer, Int.MAX_VALUE, 1); fail("overflow bounds") } catch (_: java.io.IOException) { }
        source.close(); source.close()
        assertEquals(1, closes)
        try { source.readAt(0, buffer, 0, 1); fail("closed read") } catch (_: java.io.IOException) { }
    }

    private fun await(label: String, timeoutMs: Long = 5_000, test: () -> Boolean) {
        val deadline = System.nanoTime() + timeoutMs * 1_000_000
        while (System.nanoTime() < deadline) { if (test()) return; Thread.sleep(30) }
        fail("$label; ${DlnaOutputOwner.output?.diagnostic()}")
    }

    private fun soap(endpoint: URI, action: String, args: String): String {
        val rendering = action.endsWith("Volume") || action.endsWith("Preset") || action == "ListPresets"
        val service = if (rendering) "RenderingControl" else "AVTransport"
        val path = if (rendering) "renderingcontrol" else "avtransport"
        val body = "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:$action xmlns:u=\"urn:schemas-upnp-org:service:$service:1\"><InstanceID>0</InstanceID>$args</u:$action></s:Body></s:Envelope>".toByteArray()
        return Socket().use { socket ->
            socket.connect(java.net.InetSocketAddress(endpoint.host, endpoint.port), 2_000)
            socket.soTimeout = 10_000
            val headers = "POST /upnp/control/$path HTTP/1.1\r\nHost: ${endpoint.host}:${endpoint.port}\r\nSOAPACTION: \"urn:schemas-upnp-org:service:$service:1#$action\"\r\nContent-Type: text/xml; charset=utf-8\r\nContent-Length: ${body.size}\r\nConnection: close\r\n\r\n"
            socket.getOutputStream().apply { write(headers.toByteArray()); write(body); flush() }
            val response = socket.getInputStream().readBytes()
            check(response.size <= 128 * 1024)
            response.toString(Charsets.UTF_8)
        }
    }

    private class Origin(private val mp4: ByteArray, private val wav: ByteArray) : AutoCloseable {
        private val socket = ServerSocket(0, 4, InetAddress.getByName("127.0.0.1"))
        private val closed = AtomicBoolean()
        val reads = AtomicInteger()
        val port: Int get() = socket.localPort
        private val worker = Thread {
            while (!closed.get()) {
                try {
                    socket.accept().use { peer ->
                        peer.soTimeout = 2_000
                        val reader = peer.getInputStream().bufferedReader(Charsets.US_ASCII)
                        val first = requireNotNull(reader.readLine())
                        val headers = generateSequence { reader.readLine()?.takeIf(String::isNotEmpty) }.take(32).toList()
                        val range = Regex("(?i)range: bytes=(\\d+)-(\\d+)").matchEntire(headers.first { it.startsWith("range:", true) })!!
                        val bytes = if (first.contains("/tone.wav")) wav else mp4
                        val mime = if (first.contains("/tone.wav")) "audio/wav" else "video/mp4"
                        val start = range.groupValues[1].toInt(); val end = range.groupValues[2].toInt()
                        check(start >= 0 && end >= start && end < bytes.size && end - start < 65_536)
                        val response = "HTTP/1.1 206 Partial Content\r\nContent-Length: ${end-start+1}\r\nContent-Range: bytes $start-$end/${bytes.size}\r\nContent-Type: $mime\r\nETag: \"original-fixture\"\r\nConnection: close\r\n\r\n"
                        peer.getOutputStream().apply { write(response.toByteArray()); write(bytes, start, end-start+1); flush() }
                        reads.incrementAndGet()
                    }
                } catch (error: Exception) { if (!closed.get()) throw error }
            }
        }.apply { name = "DLNA fixture origin"; start() }
        override fun close() { closed.set(true); socket.close(); worker.join(3_000); check(!worker.isAlive) }
    }

    private fun wave(): ByteArray {
        val samples = 48_000 * 3
        return ByteBuffer.allocate(44 + samples * 2).order(ByteOrder.LITTLE_ENDIAN).apply {
            put("RIFF".toByteArray()); putInt(36 + samples * 2); put("WAVEfmt ".toByteArray()); putInt(16)
            putShort(1); putShort(1); putInt(48_000); putInt(96_000); putShort(2); putShort(16)
            put("data".toByteArray()); putInt(samples * 2)
            repeat(samples) { putShort((kotlin.math.sin(it * 2 * Math.PI * 440 / 48_000) * 1_000).toInt().toShort()) }
        }.array()
    }

    private fun encodeVideo(file: File) {
        val width = 160; val height = 120
        val format = MediaFormat.createVideoFormat("video/avc", width, height).apply {
            setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatYUV420Flexible)
            setInteger(MediaFormat.KEY_BIT_RATE, 200_000); setInteger(MediaFormat.KEY_FRAME_RATE, 10); setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1)
        }
        val encoder = MediaCodec.createEncoderByType("video/avc")
        val muxer = MediaMuxer(file.absolutePath, MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4)
        var muxerStarted = false
        try {
            encoder.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE); encoder.start()
            var frames = 0; var inputEos = false; var outputEos = false; var track = -1
            val info = MediaCodec.BufferInfo()
            val deadline = System.nanoTime() + 15_000_000_000L
            while (!outputEos && System.nanoTime() < deadline) {
                if (!inputEos) {
                    val index = encoder.dequeueInputBuffer(10_000)
                    if (index >= 0) {
                        if (frames == 80) { encoder.queueInputBuffer(index, 0, 0, frames * 100_000L, MediaCodec.BUFFER_FLAG_END_OF_STREAM); inputEos = true }
                        else {
                            val image = requireNotNull(encoder.getInputImage(index))
                            image.planes.forEachIndexed { planeIndex, plane ->
                                val w = if (planeIndex == 0) width else width / 2
                                val h = if (planeIndex == 0) height else height / 2
                                val value = when (planeIndex) { 0 -> 81; 1 -> 90; else -> 240 }.toByte()
                                for (y in 0 until h) for (x in 0 until w) plane.buffer.put(y * plane.rowStride + x * plane.pixelStride, value)
                            }
                            image.close()
                            encoder.queueInputBuffer(index, 0, width * height * 3 / 2, frames * 100_000L, 0); frames++
                        }
                    }
                }
                val index = encoder.dequeueOutputBuffer(info, 10_000)
                if (index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) { track = muxer.addTrack(encoder.outputFormat); muxer.start(); muxerStarted = true }
                else if (index >= 0) {
                    if (info.size > 0 && info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG == 0) {
                        check(muxerStarted)
                        val bytes = requireNotNull(encoder.getOutputBuffer(index)); bytes.position(info.offset); bytes.limit(info.offset + info.size)
                        muxer.writeSampleData(track, bytes, info)
                    }
                    outputEos = info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0
                    encoder.releaseOutputBuffer(index, false)
                }
            }
            check(outputEos)
        } finally { runCatching { encoder.stop() }; encoder.release(); if (muxerStarted) muxer.stop(); muxer.release() }
    }
}
