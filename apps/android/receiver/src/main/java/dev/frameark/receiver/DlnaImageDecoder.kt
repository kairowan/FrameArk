package dev.frameark.receiver

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ColorSpace
import android.graphics.Matrix
import android.media.ExifInterface
import java.io.IOException

/** No URL or network API: reads only the Rust-authorized revocable byte handle. */
internal object DlnaImageDecoder {
    fun decode(source: RustMediaDataSource, kind: Long, current: () -> Boolean): Bitmap {
        try {
            return decodeBounded(source, kind, current)
        } catch (_: OutOfMemoryError) {
            // Allocation may still fail under process pressure despite fixed caps.
            throw IOException("Image allocation unavailable")
        }
    }

    private fun decodeBounded(source: RustMediaDataSource, kind: Long, current: () -> Boolean): Bitmap {
        if (!current()) throw IOException("Image load cancelled")
        val bytes = ByteArray(DlnaImageLimits.length(source.size))
        var offset = 0
        while (offset < bytes.size) {
            if (!current()) throw IOException("Image load cancelled")
            val count = source.readAt(offset.toLong(), bytes, offset, (bytes.size - offset).coerceAtMost(64 * 1024))
            if (count <= 0) throw IOException("Truncated image source")
            offset += count
        }
        if (!current() || !DlnaImageLimits.hasSignature(bytes, kind)) throw IOException("Invalid image source")
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        if (bounds.outMimeType != DlnaImageLimits.mime(kind)) throw IOException("Image type mismatch")
        val sample = DlnaImageLimits.sampleSize(bounds.outWidth, bounds.outHeight)
        val orientation = if (kind == 1L) {
            bytes.inputStream().use { ExifInterface(it).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL) }
        } else ExifInterface.ORIENTATION_NORMAL
        val options = BitmapFactory.Options().apply {
            inSampleSize = sample
            inPreferredConfig = Bitmap.Config.ARGB_8888
            inPreferredColorSpace = ColorSpace.get(ColorSpace.Named.SRGB)
            inScaled = false
        }
        if (!current()) throw IOException("Image load cancelled")
        val decoded = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options) ?: throw IOException("Image decode failed")
        var result = decoded
        try {
            checkPixels(decoded)
            val matrix = Matrix().apply {
                when (orientation) {
                    ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> setScale(-1f, 1f)
                    ExifInterface.ORIENTATION_ROTATE_180 -> setRotate(180f)
                    ExifInterface.ORIENTATION_FLIP_VERTICAL -> setScale(1f, -1f)
                    ExifInterface.ORIENTATION_TRANSPOSE -> { setRotate(90f); postScale(-1f, 1f) }
                    ExifInterface.ORIENTATION_ROTATE_90 -> setRotate(90f)
                    ExifInterface.ORIENTATION_TRANSVERSE -> { setRotate(-90f); postScale(-1f, 1f) }
                    ExifInterface.ORIENTATION_ROTATE_270 -> setRotate(-90f)
                    ExifInterface.ORIENTATION_UNDEFINED, ExifInterface.ORIENTATION_NORMAL -> Unit
                    else -> throw IOException("Invalid image orientation")
                }
            }
            if (!matrix.isIdentity) result = Bitmap.createBitmap(decoded, 0, 0, decoded.width, decoded.height, matrix, true)
            checkPixels(result)
            if (!current()) throw IOException("Image load cancelled")
            return result
        } catch (error: Throwable) {
            result.recycle()
            throw error
        } finally {
            if (result !== decoded && !decoded.isRecycled) decoded.recycle()
        }
    }

    private fun checkPixels(bitmap: Bitmap) {
        if (bitmap.width !in 1..DlnaImageLimits.MAX_DECODED_EDGE || bitmap.height !in 1..DlnaImageLimits.MAX_DECODED_EDGE ||
            bitmap.width.toLong() * bitmap.height > DlnaImageLimits.MAX_DECODED_PIXELS ||
            bitmap.allocationByteCount > DlnaImageLimits.MAX_DECODED_PIXELS * 4) {
            throw IOException("Decoded image exceeds limits")
        }
    }
}
