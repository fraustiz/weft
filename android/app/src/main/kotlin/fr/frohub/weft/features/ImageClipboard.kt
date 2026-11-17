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

/** Only content URIs are read: no file paths or remote URLs from another app. */
suspend fun readClipboardImage(context: Context, uri: Uri): Pair<String, ByteArray>? = withContext(Dispatchers.IO) {
    if (uri.scheme != "content") return@withContext null
    try {
        val mime = context.contentResolver.getType(uri)?.lowercase() ?: return@withContext null
        if (mime !in setOf("image/png", "image/jpeg")) return@withContext null
        val bytes = context.contentResolver.openInputStream(uri)?.use { input ->
            val output = ByteArrayOutputStream()
            val buffer = ByteArray(16 * 1024)
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                if (output.size() + count > MAX_CLIPBOARD_IMAGE_BYTES) return@withContext null
                output.write(buffer, 0, count)
            }
            output.toByteArray()
        } ?: return@withContext null
        (mime to bytes).takeIf { isClipboardImage(mime, bytes) && safeImageDimensions(bytes) }
    } catch (_: java.io.IOException) { null }
      catch (_: SecurityException) { null }
      catch (_: IllegalArgumentException) { null }
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
