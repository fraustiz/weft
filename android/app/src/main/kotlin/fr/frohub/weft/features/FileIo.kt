package fr.frohub.weft.features

import android.Manifest
import android.content.ContentResolver
import android.content.ContentValues
import android.content.Context
import android.content.pm.PackageManager
import android.media.MediaScannerConnection
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.ParcelFileDescriptor
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.util.Log
import androidx.annotation.RequiresApi
import androidx.core.content.FileProvider
import java.io.File
import java.io.FileInputStream
import java.io.FileNotFoundException
import java.io.IOException
import java.nio.ByteBuffer

/**
 * A file picked or shared to send, opened at once: the read grant of a shared or picked uri ends
 * with the activity that received it, an open descriptor does not.
 */
class OutgoingFile(val name: String, val size: Long, val mime: String?, private val descriptor: ParcelFileDescriptor) : AutoCloseable {
    private val channel = FileInputStream(descriptor.fileDescriptor).channel
    private var position = 0L

    /**
     * Reads exactly [length] bytes at [offset]. Seeks only when needed, so a provider that hands
     * out a pipe still works as long as nothing has to be read again. Blocking.
     */
    fun read(offset: Long, length: Int): ByteArray {
        if (offset != position) {
            channel.position(offset)
            position = offset
        }
        val buffer = ByteBuffer.allocate(length)
        while (buffer.hasRemaining()) {
            if (channel.read(buffer) < 0) throw IOException("the file got shorter")
        }
        position += length
        return buffer.array()
    }

    override fun close() {
        channel.close()
        descriptor.close()
    }

    companion object {
        /** Opens [uri] and reads its name and size; null if it cannot be read or has no known size. Blocking. */
        fun open(resolver: ContentResolver, uri: Uri): OutgoingFile? {
            var name: String? = null
            var querySize: Long? = null
            try {
                resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { cursor ->
                    if (cursor.moveToFirst()) {
                        val nameColumn = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                        val sizeColumn = cursor.getColumnIndex(OpenableColumns.SIZE)
                        if (nameColumn >= 0 && !cursor.isNull(nameColumn)) name = cursor.getString(nameColumn)
                        if (sizeColumn >= 0 && !cursor.isNull(sizeColumn)) querySize = cursor.getLong(sizeColumn)
                    }
                }
            } catch (e: RuntimeException) {
                // Some providers throw on query (unsupported columns, revoked grant…).
                Log.w(TAG, "could not query a file: ${e.javaClass.simpleName}")
            }
            val descriptor = try {
                resolver.openFileDescriptor(uri, "r")
            } catch (e: FileNotFoundException) {
                null
            } catch (e: SecurityException) {
                null
            } catch (e: IllegalArgumentException) {
                null
            } ?: return null
            val size = descriptor.statSize.takeIf { it >= 0 } ?: querySize?.takeIf { it >= 0 }
            if (size == null || size > Packet.MAX_FILE_SIZE) {
                descriptor.close()
                return null
            }
            // The name only, never a path.
            val fileName = (name ?: uri.lastPathSegment ?: "file").substringAfterLast('/').ifBlank { "file" }
            return OutgoingFile(fileName, size, resolver.getType(uri), descriptor)
        }

        private const val TAG = "WeftFiles"
    }
}

/**
 * Where received files end up: Downloads. From Android 10 through MediaStore (no permission;
 * MediaStore never replaces an existing file, it numbers the new one). Before that, the public
 * Downloads folder with the storage permission, or this app's own Downloads folder without it.
 */
class Downloads(private val context: Context) {
    private val resolver = context.contentResolver

    /** Copies [part] into Downloads as [name] (already sanitised); returns a uri to open it. Blocking. */
    fun save(part: File, name: String): Uri =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) saveToMediaStore(part, name) else saveToFolder(part, name)

    @RequiresApi(Build.VERSION_CODES.Q)
    private fun saveToMediaStore(part: File, name: String): Uri {
        // No MIME type: MediaStore takes it from the extension and never changes the name for it.
        val values = ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME, name)
            put(MediaStore.MediaColumns.IS_PENDING, 1)
        }
        val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values) ?: throw IOException("MediaStore refused the file")
        try {
            val out = resolver.openOutputStream(uri) ?: throw IOException("could not open the new file")
            out.use { stream -> part.inputStream().use { it.copyTo(stream, COPY_BUFFER) } }
            values.clear()
            values.put(MediaStore.MediaColumns.IS_PENDING, 0)
            resolver.update(uri, values, null, null)
            return uri
        } catch (e: IOException) {
            resolver.delete(uri, null, null)
            throw e
        }
    }

    private fun saveToFolder(part: File, name: String): Uri {
        val public = context.checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE) == PackageManager.PERMISSION_GRANTED
        val dir = if (public) {
            @Suppress("DEPRECATION")
            Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS)
        } else {
            context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)
        } ?: throw IOException("no shared storage")
        if (!dir.isDirectory && !dir.mkdirs()) throw IOException("could not create the Downloads folder")
        val target = File(dir, uniqueFileName(name) { File(dir, it).exists() })
        try {
            part.inputStream().use { input -> target.outputStream().use { input.copyTo(it, COPY_BUFFER) } }
        } catch (e: IOException) {
            target.delete()
            throw e
        }
        if (public) MediaScannerConnection.scanFile(context, arrayOf(target.path), null, null)
        return FileProvider.getUriForFile(context, "${context.packageName}.files", target)
    }

    private companion object {
        const val COPY_BUFFER = 128 * 1024
    }
}
