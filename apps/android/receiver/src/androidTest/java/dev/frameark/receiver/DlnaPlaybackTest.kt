@file:Suppress("DEPRECATION")

package dev.frameark.receiver

import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.media.ExifInterface
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.media.MediaMuxer
import android.test.InstrumentationTestCase
import android.view.TextureView
import java.io.File
import java.io.ByteArrayOutputStream
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
    fun testFixtureAcceptsCancellationBeforeRequestAndMidHeaders() {
        val origin = Origin(byteArrayOf(42), byteArrayOf())
        try {
            Socket("127.0.0.1", origin.port).use { it.shutdownOutput() }
            Socket("127.0.0.1", origin.port).use {
                it.getOutputStream().write("GET /video.mp4 HTTP/1.1\r\nRange: bytes=0-0\r\n".toByteArray())
                it.shutdownOutput()
            }
            Socket("127.0.0.1", origin.port).use {
                it.soTimeout = 2_000
                it.getOutputStream().write("GET /video.mp4 HTTP/1.1\r\nRange: bytes=0-0\r\n\r\n".toByteArray())
                val response = it.getInputStream().readBytes()
                assertTrue(response.toString(Charsets.US_ASCII).startsWith("HTTP/1.1 206"))
                assertEquals(42, response.last().toInt())
            }
            await("fixture serves after two cancelled connections") { origin.reads.get() == 1 }
        } finally { origin.close() }
    }

    fun testSubtitlesFollowRealPlaybackSeekSelectionAndReplacement() {
        val context = instrumentation.targetContext
        val native = FrameArkNative()
        assertTrue(native.load() is FrameArkNative.LoadResult.Loaded)
        assertTrue(native.setDeviceIdentity(AndroidDeviceIdentityStore().getOrCreate()))
        val videoFile = File(context.cacheDir, "original-subtitle-video.mp4")
        encodeVideo(videoFile)
        val srt = "1\n00:00:00,000 --> 00:00:02,000\nFirst caption\n\n2\n00:00:02,000 --> 00:00:05,000\n帧舟字幕 · 第二幕\n\n3\n00:00:05,000 --> 00:00:07,000\nLast caption\n"
        val vtt = "WEBVTT\n\n00:00.000 --> 00:07.000\n<v Fixture>WebVTT alternate</v>\n"
        val origin = Origin(videoFile.readBytes(), wave(), extra = mapOf(
            "/captions.srt" to (srt.toByteArray() to "text/srt"),
            "/captions.vtt" to (vtt.toByteArray() to "text/vtt"),
            "/bad.srt" to ("not a subtitle".toByteArray() to "text/srt"),
            "/wrong.srt" to (vtt.toByteArray() to "text/vtt"),
            "/huge.srt" to (ByteArray(1024 * 1024 + 1) { 32 } to "text/srt"),
        ))
        val activity = instrumentation.startActivitySync(Intent(context, DlnaActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        fun text(): String {
            var result = ""
            instrumentation.runOnMainSync { result = activity.findViewById<android.view.ViewGroup>(android.R.id.content).findViewWithTag<android.widget.TextView>("dlna-subtitle").text.toString() }
            return result
        }
        try {
            await("surface available") { DlnaOutputOwner.surface?.isValid == true }
            assertEquals(0, DlnaNative.startProbe())
            val output = DlnaPlayerOutput(DlnaNative.outputToken(), DlnaOutputOwner.images, DlnaOutputOwner.subtitles) { DlnaOutputOwner.surface }
            DlnaOutputOwner.output = output
            val endpoint = URI(requireNotNull(DlnaNative.descriptionUrl()))
            fun action(name: String, args: String = "") = soap(endpoint, name, args)
            fun ok(name: String, args: String = "") { val result = action(name, args); assertTrue(result, result.startsWith("HTTP/1.1 200")) }
            val media = "http://127.0.0.1:${origin.port}/video.mp4"
            fun load(captionPath: String = "captions.srt", captionHost: String = "127.0.0.1"): String {
                val metadata = "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\"><item><res protocolInfo=\"http-get:*:video/mp4:*\">$media</res><res protocolInfo=\"http-get:*:text/srt:*\" xml:lang=\"zh\">http://$captionHost:${origin.port}/$captionPath</res><res protocolInfo=\"http-get:*:text/vtt:*\" xml:lang=\"en\">http://127.0.0.1:${origin.port}/captions.vtt</res></item></DIDL-Lite>"
                val escaped = metadata.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
                return action("SetAVTransportURI", "<CurrentURI>$media</CurrentURI><CurrentURIMetaData>$escaped</CurrentURIMetaData>")
            }
            assertTrue(load().startsWith("HTTP/1.1 200"))
            assertEquals("", text())
            val choices = requireNotNull(output.subtitleChoices())
            assertEquals(listOf("zh · SRT", "en · WebVTT"), choices.labels.toList())
            ok("Play", "<Speed>1</Speed>")
            await("first subtitle reaches visible Android text") { text() == "First caption" }
            ok("Pause")
            val paused = text(); Thread.sleep(300); assertEquals(paused, text())
            ok("Seek", "<Unit>REL_TIME</Unit><Target>00:00:03</Target>")
            await("paused forward seek updates subtitle") { text() == "帧舟字幕 · 第二幕" }
            val screenshot = committedWindow(activity)
            try {
                File(context.getExternalFilesDir(null), "dlna-subtitles.png").outputStream().use { screenshot.compress(Bitmap.CompressFormat.PNG, 100, it) }
                var bounds = intArrayOf(0, 0, 0, 0)
                instrumentation.runOnMainSync {
                    val view = activity.findViewById<android.view.ViewGroup>(android.R.id.content).findViewWithTag<android.widget.TextView>("dlna-subtitle")
                    val xy = IntArray(2); view.getLocationInWindow(xy)
                    bounds = intArrayOf(xy[0], xy[1], view.width, view.height)
                }
                var white = 0
                for (y in bounds[1] until bounds[1] + bounds[3]) for (x in bounds[0] until bounds[0] + bounds[2]) {
                    val p = screenshot.getPixel(x, y)
                    if (Color.red(p) > 220 && Color.green(p) > 220 && Color.blue(p) > 220) white++
                }
                assertTrue("actual caption pixels rendered: $white", white > 30)
            } finally { screenshot.recycle() }
            // Use the actual subtitle menu, not only the native selection seam.
            instrumentation.runOnMainSync {
                val root = activity.findViewById<android.view.ViewGroup>(android.R.id.content)
                fun click(view: android.view.View): Boolean {
                    if (view is android.widget.Button && view.text == activity.getString(R.string.dlna_subtitles)) return view.performClick()
                    if (view is android.view.ViewGroup) for (i in 0 until view.childCount) if (click(view.getChildAt(i))) return true
                    return false
                }
                assertTrue(click(root))
            }
            instrumentation.waitForIdleSync()
            await("subtitle menu becomes accessible and accepts selection") {
                instrumentation.uiAutomation.rootInActiveWindow
                    ?.findAccessibilityNodeInfosByText("en · WebVTT")?.firstOrNull()
                    ?.performAction(android.view.accessibility.AccessibilityNodeInfo.ACTION_CLICK) == true
            }
            await("menu selects alternate WebVTT track") { text() == "WebVTT alternate" }
            output.selectSubtitle(choices.sourceId, -1)
            await("subtitle off clears output") { text().isEmpty() }
            output.selectSubtitle(choices.sourceId, 0)
            await("subtitle re-enabled while paused") { text() == "帧舟字幕 · 第二幕" }
            assertTrue("other peer literal must fail before subtitle I/O", load(captionHost = "127.0.0.2").startsWith("HTTP/1.1 500"))
            assertTrue("malformed subtitle must leave old output intact", load("bad.srt").startsWith("HTTP/1.1 500"))
            assertTrue("MIME mismatch must fail", load("wrong.srt").startsWith("HTTP/1.1 500"))
            assertTrue("oversized subtitle must fail before full download", load("huge.srt").startsWith("HTTP/1.1 500"))
            assertEquals("帧舟字幕 · 第二幕", text())
            ok("Stop")
            await("stop clears captions") { text().isEmpty() }
            ok("Play", "<Speed>1</Speed>")
            await("replay seeks subtitle timeline backwards") { text() == "First caption" }
            ok("Pause")
            ok("Seek", "<Unit>REL_TIME</Unit><Target>00:00:06</Target>")
            await("later cue") { text() == "Last caption" }
            ok("Play", "<Speed>1</Speed>")
            await("EOS clears caption", 5_000) { action("GetTransportInfo").contains("STOPPED") && text().isEmpty() }
            ok("SetAVTransportURI", "<CurrentURI>http://127.0.0.1:${origin.port}/tone.wav</CurrentURI><CurrentURIMetaData></CurrentURIMetaData>")
            assertTrue(requireNotNull(output.subtitleChoices()).labels.isEmpty())
            assertNull(DlnaNative.subtitleText(choices.sourceId, 3_000))
            assertFalse(DlnaNative.selectSubtitle(choices.sourceId, 0))
            output.selectSubtitle(choices.sourceId, 0)
            assertEquals("", text())
            instrumentation.sendStatus(0, android.os.Bundle().apply { putString("stream", "\nDLNA subtitles: real H.264 playback, Unicode screen pixels, pause/seek/replay/EOS, menu track selection, off, source-policy/malformed rejection and replacement revocation; ranges=${origin.reads.get()}\n") })
        } finally {
            val output = DlnaOutputOwner.output
            output?.close(); DlnaOutputOwner.output = null; DlnaNative.stop()
            await("subtitle output closed") { output?.isClosed() != false }
            origin.close()
            instrumentation.runOnMainSync { activity.finish() }
            assertTrue(videoFile.delete())
        }
    }

    fun testSubtitleTargetRejectsStaleAndClosedWork() {
        val context = instrumentation.targetContext
        val activity = instrumentation.startActivitySync(Intent(context, DlnaActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            await("surface available") { DlnaOutputOwner.surface?.isValid == true }
            val target = requireNotNull(DlnaOutputOwner.subtitles)
            target.present(1, "stale", { false })
            target.present(2, "current", { true })
            target.retire(1)
            instrumentation.waitForIdleSync()
            instrumentation.runOnMainSync {
                val view = activity.findViewById<android.view.ViewGroup>(android.R.id.content).findViewWithTag<android.widget.TextView>("dlna-subtitle")
                assertEquals("current", view.text.toString())
                target.close()
            }
            target.present(2, "closed", { true })
            instrumentation.waitForIdleSync()
            instrumentation.runOnMainSync { assertEquals("", activity.findViewById<android.view.ViewGroup>(android.R.id.content).findViewWithTag<android.widget.TextView>("dlna-subtitle").text.toString()) }
        } finally { instrumentation.runOnMainSync { activity.finish() } }
    }

    fun testImageTargetRejectsStaleAndClosedWork() {
        val context = instrumentation.targetContext
        val activity = instrumentation.startActivitySync(Intent(context, DlnaActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val red = Bitmap.createBitmap(80, 40, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.RED) }
        val green = Bitmap.createBitmap(80, 40, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.GREEN) }
        try {
            await("surface available") { DlnaOutputOwner.surface?.isValid == true }
            val target = requireNotNull(DlnaOutputOwner.images)
            val result = AtomicInteger(-1)
            target.present(1, red, { false }) { result.set(if (it) 1 else 0) }
            await("stale display rejected") { result.get() == 0 }
            result.set(-1)
            target.present(2, green, { true }) { result.set(if (it) 1 else 0) }
            await("current display accepted") { result.get() == 1 }
            target.retire(1, null)
            instrumentation.waitForIdleSync()
            awaitPicture(activity, Color.GREEN)
            instrumentation.runOnMainSync { target.close() }
            result.set(-1)
            target.present(2, red, { true }) { result.set(if (it) 1 else 0) }
            await("closed display rejected") { result.get() == 0 }
            awaitPicture(activity, Color.BLACK)
        } finally {
            instrumentation.runOnMainSync { activity.finish() }
            red.recycle() // Rejected work never published this bitmap to RenderThread.
        }
    }

    fun testStillDecoderBoundsShortReadsAndAllJpegOrientations() {
        val context = instrumentation.targetContext
        val expected = listOf(
            listOf(Color.RED, Color.GREEN, Color.BLUE, Color.YELLOW),
            listOf(Color.GREEN, Color.RED, Color.YELLOW, Color.BLUE),
            listOf(Color.YELLOW, Color.BLUE, Color.GREEN, Color.RED),
            listOf(Color.BLUE, Color.YELLOW, Color.RED, Color.GREEN),
            listOf(Color.RED, Color.BLUE, Color.GREEN, Color.YELLOW),
            listOf(Color.BLUE, Color.RED, Color.YELLOW, Color.GREEN),
            listOf(Color.YELLOW, Color.GREEN, Color.BLUE, Color.RED),
            listOf(Color.GREEN, Color.YELLOW, Color.RED, Color.BLUE),
        )
        val original = Bitmap.createBitmap(80, 40, Bitmap.Config.ARGB_8888)
        for (y in 0 until 40) for (x in 0 until 80) original.setPixel(x, y, expected[0][(if (y >= 20) 2 else 0) + if (x >= 40) 1 else 0])
        val file = File(context.cacheDir, "original-dlna-orientation.jpg")
        try {
            for (orientation in 1..8) {
                file.outputStream().use { assertTrue(original.compress(Bitmap.CompressFormat.JPEG, 100, it)) }
                ExifInterface(file.absolutePath).apply { setAttribute(ExifInterface.TAG_ORIENTATION, orientation.toString()); saveAttributes() }
                val decoded = memorySource(file.readBytes()).use { DlnaImageDecoder.decode(it, 1) { true } }
                try {
                    assertEquals(if (orientation <= 4) 80 else 40, decoded.width)
                    assertEquals(if (orientation <= 4) 40 else 80, decoded.height)
                    val positions = listOf(1 to 1, 3 to 1, 1 to 3, 3 to 3)
                    for ((index, point) in positions.withIndex()) assertColor(expected[orientation - 1][index], decoded.getPixel(decoded.width * point.first / 4, decoded.height * point.second / 4))
                } finally { decoded.recycle() }
            }
        } finally { original.recycle(); assertTrue(file.delete()) }
        val large = photo(Color.BLUE, 4000, 3000, Bitmap.CompressFormat.JPEG)
        val sampled = memorySource(large).use { DlnaImageDecoder.decode(it, 1) { true } }
        try { assertEquals(2000, sampled.width); assertEquals(1500, sampled.height); assertColor(Color.BLUE, sampled.getPixel(1000, 750)) } finally { sampled.recycle() }
        val png = photo(Color.RED)
        for (action in listOf<() -> Unit>(
            { memorySource(png).use { DlnaImageDecoder.decode(it, 1) { true }.recycle() } },
            { memorySource(byteArrayOf(137.toByte(), 80, 78, 71, 13, 10, 26, 10)).use { DlnaImageDecoder.decode(it, 2) { true }.recycle() } },
            { memorySource(png, 16L * 1024 * 1024 + 1).use { DlnaImageDecoder.decode(it, 2) { true }.recycle() } },
            { memorySource(png).use { DlnaImageDecoder.decode(it, 2) { false }.recycle() } },
            { memorySource(png, png.size.toLong() + 10).use { DlnaImageDecoder.decode(it, 2) { true }.recycle() } },
        )) { try { action(); fail("invalid image accepted") } catch (_: Exception) { } }
    }

    fun testStillImagePixelsTransportReplacementAndFailureCleanup() {
        val context = instrumentation.targetContext
        val native = FrameArkNative()
        assertTrue(native.load() is FrameArkNative.LoadResult.Loaded)
        assertTrue(native.setDeviceIdentity(AndroidDeviceIdentityStore().getOrCreate()))
        val origin = Origin(byteArrayOf(), wave(), photo(Color.RED), photo(Color.GREEN, 80, 160, Bitmap.CompressFormat.JPEG))
        val activity = instrumentation.startActivitySync(Intent(context, DlnaActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            await("surface available") { DlnaOutputOwner.surface?.isValid == true }
            assertEquals(0, DlnaNative.startProbe())
            DlnaOutputOwner.output = DlnaPlayerOutput(DlnaNative.outputToken(), DlnaOutputOwner.images) { DlnaOutputOwner.surface }
            val endpoint = URI(requireNotNull(DlnaNative.descriptionUrl()))
            fun action(name: String, args: String = "") = soap(endpoint, name, args)
            fun ok(name: String, args: String = "") { val result = action(name, args); assertTrue(result, result.startsWith("HTTP/1.1 200")) }
            fun load(path: String) = ok("SetAVTransportURI", "<CurrentURI>http://127.0.0.1:${origin.port}/$path</CurrentURI><CurrentURIMetaData></CurrentURIMetaData>")
            load("photo.png")
            assertTrue(action("GetTransportInfo").contains("STOPPED"))
            awaitPicture(activity, Color.BLACK)
            ok("Play", "<Speed>1</Speed>")
            awaitPicture(activity, Color.RED)
            assertTrue(action("GetPositionInfo").contains("<TrackDuration>00:00:00</TrackDuration>"))
            assertTrue(action("GetPositionInfo").contains("<RelTime>00:00:00</RelTime>"))
            ok("Pause")
            awaitPicture(activity, Color.RED)
            assertTrue(action("GetTransportInfo").contains("PAUSED_PLAYBACK"))
            ok("Seek", "<Unit>TRACK_NR</Unit><Target>1</Target>")
            assertTrue(action("Seek", "<Unit>REL_TIME</Unit><Target>00:00:01</Target>").contains("<errorCode>711</errorCode>"))
            ok("SetVolume", "<Channel>Master</Channel><DesiredVolume>35</DesiredVolume>")
            ok("Stop")
            awaitPicture(activity, Color.BLACK)
            ok("Play", "<Speed>1</Speed>")
            awaitPicture(activity, Color.RED)
            repeat(3) {
                load("photo.jpg")
                awaitPicture(activity, Color.GREEN)
                assertTrue(action("GetTransportInfo").contains("PLAYING"))
                load("photo.png")
                awaitPicture(activity, Color.RED)
            }
            val screenshot = instrumentation.uiAutomation.takeScreenshot()
            File(context.getExternalFilesDir(null), "dlna-image.png").outputStream().use { screenshot.compress(Bitmap.CompressFormat.PNG, 100, it) }
            screenshot.recycle()
            val bad = action("SetAVTransportURI", "<CurrentURI>http://127.0.0.1:${origin.port}/broken.png</CurrentURI><CurrentURIMetaData></CurrentURIMetaData>")
            assertTrue(bad, bad.startsWith("HTTP/1.1 500"))
            await("invalid image fails closed") { action("GetTransportInfo").contains("ERROR_OCCURRED") }
            awaitPicture(activity, Color.BLACK)
            assertTrue(origin.reads.get() >= 16)
            instrumentation.sendStatus(0, android.os.Bundle().apply { putString("stream", "\nDLNA images: actual PNG/JPEG screen pixels; 3 replacements; timeless seek, pause/stop/replay and malformed-image cleanup; Range requests=${origin.reads.get()}\n") })
        } finally {
            val output = DlnaOutputOwner.output
            output?.close(); DlnaOutputOwner.output = null; DlnaNative.stop()
            await("image output closed") { output?.isClosed() != false }
            origin.close()
            instrumentation.runOnMainSync { activity.finish() }
        }
    }

    private fun awaitPicture(activity: android.app.Activity, color: Int) {
        await("actual picture screen pixels") {
            val screenshot = instrumentation.uiAutomation.takeScreenshot()
            try {
                var point = 0 to 0
                instrumentation.runOnMainSync {
                    val view = activity.findViewById<android.view.ViewGroup>(android.R.id.content).findViewWithTag<android.widget.ImageView>("dlna-picture")
                    val xy = IntArray(2); view.getLocationOnScreen(xy)
                    point = xy[0] + view.width / 2 to xy[1] + view.height / 2
                }
                similarColor(color, screenshot.getPixel(point.first, point.second))
            } finally { screenshot.recycle() }
        }
    }

    private fun similarColor(expected: Int, actual: Int) = kotlin.math.abs(Color.red(expected) - Color.red(actual)) < 35 && kotlin.math.abs(Color.green(expected) - Color.green(actual)) < 35 && kotlin.math.abs(Color.blue(expected) - Color.blue(actual)) < 35
    private fun assertColor(expected: Int, actual: Int) { assertTrue("expected $expected, actual $actual", similarColor(expected, actual)) }

    private fun memorySource(bytes: ByteArray, length: Long = bytes.size.toLong()) = RustMediaDataSource(1, object : DlnaSourceAccess {
        override fun sourceSize(id: Long) = length
        override fun sourceRead(id: Long, position: Long, size: Int): ByteArray = if (position >= bytes.size) byteArrayOf() else bytes.copyOfRange(position.toInt(), minOf(bytes.size, position.toInt() + size, position.toInt() + 31))
        override fun sourceClose(id: Long) = Unit
    })

    private fun photo(color: Int, width: Int = 160, height: Int = 80, format: Bitmap.CompressFormat = Bitmap.CompressFormat.PNG): ByteArray {
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        try { bitmap.eraseColor(color); return ByteArrayOutputStream().use { assertTrue(bitmap.compress(format, 100, it)); it.toByteArray() } } finally { bitmap.recycle() }
    }

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
        val origin = Origin(videoFile.readBytes(), wave(), photo(Color.GREEN))
        val activity = instrumentation.startActivitySync(Intent(context, DlnaActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            await("surface available") { DlnaOutputOwner.surface?.isValid == true }
            assertEquals(0, DlnaNative.startProbe())
            val token = DlnaNative.outputToken()
            assertTrue(token > 0)
            assertNull(DlnaNative.pollCommand(token + 1))
            DlnaOutputOwner.output = DlnaPlayerOutput(token, DlnaOutputOwner.images) { DlnaOutputOwner.surface }
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
                    val texture = view.findViewWithTag<TextureView>("dlna-video")
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
            load("photo.png")
            awaitPicture(activity, Color.GREEN)
            load("video.mp4")
            await("video output restarts after picture") { DlnaOutputOwner.output?.diagnostic()?.contains("video-start notifications: 2") == true }
            load("tone.wav") // Replacement revokes the old source, with a different OS player.
            assertTrue("replacing a playing source resumes the new OS player", action("GetTransportInfo").contains("PLAYING"))
            await("WAV decoder position advances") { action("GetPositionInfo").contains("<RelTime>00:00:01</RelTime>") }
            await("WAV reaches EOS", 12_000) { action("GetTransportInfo").contains("STOPPED") }
            assertTrue("Range reads reached the origin", origin.reads.get() >= 4)
            assertNull("stale consumer never gets another runtime's work", DlnaNative.pollCommand(token + 1))
            instrumentation.sendStatus(0, android.os.Bundle().apply {
                putString("stream", "\nDLNA original MP4: decoded red pixels, Play/Pause/Seek/Stop/replay, FactoryDefaults volume and TRACK_NR; image/video round trip; WAV replacement auto-resumes: position + EOS; HTTP Range requests=${origin.reads.get()}; ${DlnaOutputOwner.output?.diagnostic()}\n")
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

    /** Capture the actual submitted UI buffer, not a stale compositor frame. */
    private fun committedWindow(activity: android.app.Activity): Bitmap {
        val committed = java.util.concurrent.CountDownLatch(1)
        var width = 0; var height = 0
        instrumentation.runOnMainSync {
            val view = activity.window.decorView
            check(view.isHardwareAccelerated)
            width = view.width; height = view.height
            if (android.os.Build.VERSION.SDK_INT >= 29) {
                view.viewTreeObserver.registerFrameCommitCallback { committed.countDown() }
                view.invalidate()
            } else error("This render evidence fixture requires API 29+")
        }
        assertTrue("caption frame committed", committed.await(3, java.util.concurrent.TimeUnit.SECONDS))
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        val copied = java.util.concurrent.CountDownLatch(1)
        val result = AtomicInteger(-1)
        instrumentation.runOnMainSync {
            android.view.PixelCopy.request(activity.window, bitmap, {
                result.set(it); copied.countDown()
            }, android.os.Handler(android.os.Looper.getMainLooper()))
        }
        assertTrue("caption window copied", copied.await(3, java.util.concurrent.TimeUnit.SECONDS))
        assertEquals(android.view.PixelCopy.SUCCESS, result.get())
        return bitmap
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

    private class Origin(private val mp4: ByteArray, private val wav: ByteArray, private val png: ByteArray = byteArrayOf(), private val jpeg: ByteArray = byteArrayOf(), private val extra: Map<String, Pair<ByteArray, String>> = emptyMap()) : AutoCloseable {
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
                        // A revoked source can close before sending any request or
                        // halfway through headers. EOF is cancellation, not a bad Range.
                        val first = reader.readLine() ?: return@use
                        val headers = ArrayList<String>()
                        while (true) {
                            val line = reader.readLine() ?: return@use
                            if (line.isEmpty()) break
                            check(headers.size < 32)
                            headers.add(line)
                        }
                        val range = Regex("(?i)range: bytes=(\\d+)-(\\d+)").matchEntire(headers.first { it.startsWith("range:", true) })!!
                        val (bytes, mime) = extra[first.split(' ')[1]] ?: when {
                            first.contains("/tone.wav") -> wav to "audio/wav"
                            first.contains("/photo.png") -> png to "image/png"
                            first.contains("/photo.jpg") -> jpeg to "image/jpeg"
                            first.contains("/broken.png") -> ByteArray(128) to "image/png"
                            else -> mp4 to "video/mp4"
                        }
                        val start = range.groupValues[1].toInt(); val end = range.groupValues[2].toInt()
                        check(start >= 0 && end >= start && end < bytes.size && end - start < 65_536)
                        val response = "HTTP/1.1 206 Partial Content\r\nContent-Length: ${end-start+1}\r\nContent-Range: bytes $start-$end/${bytes.size}\r\nContent-Type: $mime\r\nETag: \"original-fixture\"\r\nConnection: close\r\n\r\n"
                        try {
                            peer.getOutputStream().apply { write(response.toByteArray()); write(bytes, start, end-start+1); flush() }
                        } catch (_: java.net.SocketException) { return@use } // Reader revoked while response was in flight.
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
