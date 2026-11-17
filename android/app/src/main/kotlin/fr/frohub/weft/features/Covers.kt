package fr.frohub.weft.features

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.MediaMetadata
import android.util.Log
import java.io.ByteArrayOutputStream
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

/** One cover as it leaves in `media.art`. */
class Cover(val id: String, val mime: String, val data: ByteArray)

/**
 * The covers of this phone's players, encoded as JPEG off the main thread. Media sessions hand
 * out a new bitmap on every read (the metadata comes through Binder), so a cover is recognised by
 * its size, a sample of its pixels and the track it belongs to, not by the bitmap object: a
 * play, pause or seek does not encode it again. Keeps the last [MAX_COVERS]. Thread-safe.
 */
class LocalCovers(private val scope: CoroutineScope) {
    private data class Key(val width: Int, val height: Int, val pixels: Long, val track: String)

    private val lock = Any()
    private val done = LinkedHashMap<Key, Cover?>()
    private val pending = HashSet<Key>()

    /**
     * The id of [metadata]'s cover, or null when it has none or it is not encoded yet; then
     * [onReady] runs, on a background thread, once it is.
     */
    fun idFor(metadata: MediaMetadata, onReady: () -> Unit): String? {
        val bitmap = coverOf(metadata) ?: return null
        val track = listOf(MediaMetadata.METADATA_KEY_TITLE, MediaMetadata.METADATA_KEY_ARTIST, MediaMetadata.METADATA_KEY_ALBUM)
            .joinToString("\u0000") { metadata.getString(it).orEmpty() }
        val key = Key(bitmap.width, bitmap.height, sample(bitmap), track)
        synchronized(lock) {
            if (key in done) {
                val cover = done.remove(key)
                done[key] = cover
                return cover?.id
            }
            if (!pending.add(key)) return null
        }
        scope.launch(Dispatchers.Default) {
            val cover = try {
                encodeCover(bitmap)
            } catch (e: RuntimeException) {
                Log.w(TAG, "could not encode a cover: ${e.javaClass.simpleName}")
                null
            }
            synchronized(lock) {
                pending.remove(key)
                done[key] = cover
                while (done.size > MAX_COVERS) done.remove(done.keys.first())
            }
            if (cover != null) onReady()
        }
        return null
    }

    /** The cover [id] names, while it is among the last ones encoded. */
    fun cover(id: String): Cover? = synchronized(lock) { done.values.firstOrNull { it?.id == id } }

    private companion object {
        const val TAG = "WeftCovers"
        const val SAMPLES = 16

        /** The first bitmap of the art, the album art and the display icon. URIs are not loaded. */
        fun coverOf(metadata: MediaMetadata): Bitmap? = listOf(
            MediaMetadata.METADATA_KEY_ART,
            MediaMetadata.METADATA_KEY_ALBUM_ART,
            MediaMetadata.METADATA_KEY_DISPLAY_ICON,
        ).firstNotNullOfOrNull { key -> metadata.getBitmap(key)?.takeIf { !it.isRecycled && it.width > 0 && it.height > 0 } }

        /** A hash of a grid of pixels; 0 for a bitmap whose pixels cannot be read here. */
        fun sample(bitmap: Bitmap): Long {
            if (bitmap.config == Bitmap.Config.HARDWARE) return 0
            var hash = 1125899906842597L
            for (y in 0 until SAMPLES) for (x in 0 until SAMPLES) {
                val pixel = bitmap.getPixel(x * (bitmap.width - 1) / (SAMPLES - 1), y * (bitmap.height - 1) / (SAMPLES - 1))
                hash = 31 * hash + pixel
            }
            return hash
        }

        /** Scaled so its longer side is at most [COVER_SIZE], as JPEG; null if it cannot fit the limit. */
        fun encodeCover(bitmap: Bitmap): Cover? {
            // Hardware bitmaps cannot be scaled on the CPU: copied first.
            val source = if (bitmap.config == Bitmap.Config.HARDWARE) bitmap.copy(Bitmap.Config.ARGB_8888, false) ?: return null else bitmap
            val (width, height) = coverSize(source.width, source.height)
            val scaled = if (width == source.width && height == source.height) source else Bitmap.createScaledBitmap(source, width, height, true)
            try {
                for (quality in listOf(COVER_QUALITY, 60, 40)) {
                    val out = ByteArrayOutputStream()
                    if (!scaled.compress(Bitmap.CompressFormat.JPEG, quality, out)) return null
                    val bytes = out.toByteArray()
                    if (bytes.size in 1..Packet.MAX_ART_BYTES) return Cover(coverId(bytes), Packet.MIME_JPEG, bytes)
                }
                return null
            } finally {
                if (scaled !== bitmap) scaled.recycle()
                if (source !== bitmap && source !== scaled) source.recycle()
            }
        }
    }
}

/** Largest received cover side, in px: a bigger one is decoded at a fraction of its size. */
private const val MAX_RECEIVED_SIDE = 512

/**
 * The image of a received `media.art`: null when it is over the limit, is not the image its
 * `mime` says, or cannot be decoded. Blocking: call it off the main thread.
 */
fun decodeCover(art: Packet.MediaArt): Bitmap? {
    if (art.mime !in Packet.ART_MIMES || art.data.isEmpty() || art.data.size > Packet.MAX_ART_BYTES) return null
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(art.data, 0, art.data.size, bounds)
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0 || bounds.outMimeType != art.mime) return null
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / sample > MAX_RECEIVED_SIDE) sample *= 2
    val options = BitmapFactory.Options().apply { inSampleSize = sample }
    return BitmapFactory.decodeByteArray(art.data, 0, art.data.size, options)
}
