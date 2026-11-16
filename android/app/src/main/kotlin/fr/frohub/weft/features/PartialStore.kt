package fr.frohub.weft.features

import java.io.File
import java.io.IOException
import java.util.Properties

/**
 * Files being received, in a private folder (never in Downloads): `<device>-<id>.part` holds the
 * bytes so far and `<device>-<id>.meta` the offer it belongs to, so a transfer offered again with
 * the same id resumes where it stopped. Both go once the file is complete, cancelled, or 24 hours
 * after its last write. Plain java.io; call it off the main thread.
 */
class PartialStore(private val dir: File, private val clock: () -> Long = System::currentTimeMillis) {

    fun partFile(deviceIdHex: String, id: String) = File(dir, "${key(deviceIdHex, id)}.part")

    private fun metaFile(deviceIdHex: String, id: String) = File(dir, "${key(deviceIdHex, id)}.meta")

    /**
     * Bytes already kept for [offer] from [deviceIdHex], or null if none. Bytes kept for another
     * file under the same id (another name or size) are deleted.
     */
    fun resumable(deviceIdHex: String, offer: Packet.FileOffer): Long? {
        val part = partFile(deviceIdHex, offer.id)
        val meta = readMeta(deviceIdHex, offer.id)
        if (meta == null || !part.isFile) {
            delete(deviceIdHex, offer.id)
            return null
        }
        if (meta.getProperty(NAME) != offer.name || meta.getProperty(SIZE)?.toLongOrNull() != offer.size) {
            delete(deviceIdHex, offer.id)
            return null
        }
        val length = part.length()
        if (length > offer.size) {
            delete(deviceIdHex, offer.id)
            return null
        }
        return length
    }

    /** Starts an empty partial file for [offer]; replaces whatever was kept under that id. */
    fun create(deviceIdHex: String, offer: Packet.FileOffer): File {
        if (!dir.isDirectory && !dir.mkdirs()) throw IOException("could not create the partial folder")
        val meta = Properties().apply {
            setProperty(NAME, offer.name)
            setProperty(SIZE, offer.size.toString())
        }
        metaFile(deviceIdHex, offer.id).outputStream().use { meta.store(it, null) }
        val part = partFile(deviceIdHex, offer.id)
        part.outputStream().close()
        return part
    }

    fun delete(deviceIdHex: String, id: String) {
        partFile(deviceIdHex, id).delete()
        metaFile(deviceIdHex, id).delete()
    }

    /** Deletes every partial file not written for 24 hours, and leftovers without their pair. */
    fun expire() {
        val now = clock()
        val files = dir.listFiles() ?: return
        val parts = files.filter { it.name.endsWith(".part") }.associateBy { it.name.removeSuffix(".part") }
        val metas = files.filter { it.name.endsWith(".meta") }.associateBy { it.name.removeSuffix(".meta") }
        for ((key, part) in parts) {
            if (key !in metas || now - part.lastModified() >= PARTIAL_LIFETIME_MILLIS) {
                part.delete()
                metas[key]?.delete()
            }
        }
        for ((key, meta) in metas) if (key !in parts) meta.delete()
    }

    private fun readMeta(deviceIdHex: String, id: String): Properties? {
        val file = metaFile(deviceIdHex, id)
        if (!file.isFile) return null
        return try {
            Properties().apply { file.inputStream().use { load(it) } }
        } catch (_: IOException) {
            null
        } catch (_: IllegalArgumentException) {
            null
        }
    }

    // Both are hex (checked when the packet was decoded), so safe in a file name.
    private fun key(deviceIdHex: String, id: String) = "$deviceIdHex-$id"

    private companion object {
        const val NAME = "name"
        const val SIZE = "size"
    }
}
