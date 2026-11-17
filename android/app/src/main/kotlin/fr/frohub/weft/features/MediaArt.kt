package fr.frohub.weft.features

import fr.frohub.weft.core.toHex
import java.security.MessageDigest

/**
 * Covers (docs/features.md, section 6, "Artwork"): the parts that need no Android classes. The
 * bitmaps themselves are in Covers.kt.
 */

/** How many covers are kept per device, by id. */
const val MAX_COVERS = 8

/** The longer side of a cover this phone sends, in px. */
const val COVER_SIZE = 300

/** The JPEG quality of a cover this phone sends. */
const val COVER_QUALITY = 80

/** The id of a cover: the first 32 hex characters of the SHA-256 of its bytes. */
fun coverId(image: ByteArray): String = MessageDigest.getInstance("SHA-256").digest(image).toHex().take(32)

/** [width] by [height] scaled down (never up) so that its longer side is at most [max], at least 1 px each. */
fun coverSize(width: Int, height: Int, max: Int = COVER_SIZE): Pair<Int, Int> {
    val longer = maxOf(width, height)
    if (longer <= max) return width to height
    val scale = max.toDouble() / longer
    return (width * scale).toInt().coerceIn(1, max) to (height * scale).toInt().coerceIn(1, max)
}

/** These covers with [cover] as [id], newest last; the oldest go beyond [limit]. */
fun <T> Map<String, T>.plusCover(id: String, cover: T, limit: Int = MAX_COVERS): Map<String, T> {
    val next = LinkedHashMap(this)
    next.remove(id)
    next[id] = cover
    val extra = next.size - limit
    if (extra > 0) next.keys.take(extra).forEach(next::remove)
    return next
}

/**
 * Which covers each device got on the current connection, so that one is sent at most once per
 * connection and device. Forgotten when this phone reconnects ([forgetAll]) or a device comes
 * back ([forget]). Not thread-safe.
 */
class ArtDelivery(private val perDevice: Int = 64) {
    private val sent = HashMap<String, LinkedHashSet<String>>()

    /** The devices among [deviceIds] that have not got [id] yet; from now on they have it. */
    fun due(id: String, deviceIds: Collection<String>): List<String> = deviceIds.distinct().filter { device ->
        val ids = sent.getOrPut(device) { LinkedHashSet() }
        if (!ids.add(id)) return@filter false
        // Bounded: a device that would get thousands of covers on one connection gets old ones again.
        if (ids.size > perDevice) ids.remove(ids.first())
        true
    }

    fun forget(deviceId: String) {
        sent.remove(deviceId)
    }

    fun forgetAll() = sent.clear()
}
