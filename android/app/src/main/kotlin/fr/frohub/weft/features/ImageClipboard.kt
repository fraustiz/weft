package fr.frohub.weft.features

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.net.Uri
import android.graphics.BitmapFactory
import androidx.core.content.FileProvider
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private fun safeImageDimensions(bytes: ByteArray): Boolean {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    return bounds.outWidth > 0 && bounds.outHeight > 0 &&
        bounds.outWidth.toLong() * bounds.outHeight <= 16L * 1024 * 1024
}

/** What [readClipboardImage] found, so the user is told why an image is refused. */
sealed interface ImageRead {
    class Image(val mime: String, val bytes: ByteArray) : ImageRead
    /** Over [MAX_CLIPBOARD_IMAGE_BYTES]. */
    data object TooLarge : ImageRead
    /** Not a PNG or JPEG image. */
    data object Unsupported : ImageRead
    /** Not readable, or not an image this phone can decode. */
    data object Unreadable : ImageRead
}

/** Only content URIs are read: no file paths or remote URLs from another app. */
suspend fun readClipboardImage(context: Context, uri: Uri): ImageRead = withContext(Dispatchers.IO) {
    if (uri.scheme != "content") return@withContext ImageRead.Unreadable
    try {
        val mime = context.contentResolver.getType(uri)?.lowercase() ?: return@withContext ImageRead.Unsupported
        if (mime !in setOf("image/png", "image/jpeg")) return@withContext ImageRead.Unsupported
        val bytes = context.contentResolver.openInputStream(uri)?.use { input ->
            val output = ByteArrayOutputStream()
            val buffer = ByteArray(16 * 1024)
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                if (output.size() + count > MAX_CLIPBOARD_IMAGE_BYTES) return@withContext ImageRead.TooLarge
                output.write(buffer, 0, count)
            }
            output.toByteArray()
        } ?: return@withContext ImageRead.Unreadable
        when {
            bytes.isEmpty() -> ImageRead.Unreadable
            !isClipboardImage(mime, bytes) -> ImageRead.Unsupported
            !safeImageDimensions(bytes) -> ImageRead.Unreadable
            else -> ImageRead.Image(mime, bytes)
        }
    } catch (_: java.io.IOException) { ImageRead.Unreadable }
      catch (_: SecurityException) { ImageRead.Unreadable }
      catch (_: IllegalArgumentException) { ImageRead.Unreadable }
}

/** Writes only our private cache directory; FileProvider grants readers access to this URI. */
suspend fun writeClipboardImage(context: Context, mime: String, bytes: ByteArray) {
    val uri = withContext(Dispatchers.IO) {
        if (!isClipboardImage(mime, bytes) || !safeImageDimensions(bytes)) throw java.io.IOException("Invalid image dimensions")
        val directory = File(context.cacheDir, "clipboard-images").apply { mkdirs() }
        // Keep the current clipboard readable; prune only expired copies from this feature.
        directory.listFiles()?.sortedByDescending { it.lastModified() }?.drop(1)?.forEach { it.delete() }
        val file = File(directory, UUID.randomUUID().toString() + if (mime == "image/png") ".png" else ".jpg")
        file.writeBytes(bytes)
        FileProvider.getUriForFile(context, "${context.packageName}.files", file)
    }
    withContext(Dispatchers.Main) {
        context.getSystemService(ClipboardManager::class.java)
            .setPrimaryClip(ClipData.newUri(context.contentResolver, "Weft", uri))
    }
}
