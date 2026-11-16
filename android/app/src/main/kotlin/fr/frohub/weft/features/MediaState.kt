package fr.frohub.weft.features

import kotlin.math.abs

/**
 * When the phone's `media` packet leaves (docs/features.md, section 6): on any change but the
 * position, which the receiver moves on by itself while a player plays. A position that is no
 * longer where the receiver would put it (a seek) leaves too. Not thread-safe.
 */
class MediaGate(private val toleranceMillis: Long = SEEK_TOLERANCE_MILLIS) {
    private var sent: Packet.Media? = null
    private var sentAt = 0L

    /** True if [next], seen at [nowMillis], should be sent; it then counts as sent. */
    fun offer(next: Packet.Media, nowMillis: Long): Boolean {
        val last = sent
        val send = last == null ||
            last.withoutPositions() != next.withoutPositions() ||
            next.players.zip(last.players).any { (player, before) -> moved(before, player, nowMillis) }
        if (send) markSent(next, nowMillis)
        return send
    }

    /** Records that [media] left at [nowMillis] (a full resend after a reconnect). */
    fun markSent(media: Packet.Media, nowMillis: Long) {
        sent = media
        sentAt = nowMillis
    }

    private fun moved(before: Packet.MediaPlayer, now: Packet.MediaPlayer, nowMillis: Long): Boolean {
        val expected = extrapolatedPosition(before, sentAt, nowMillis)
        val actual = now.position
        if (expected == null || actual == null) return (expected == null) != (actual == null)
        return abs(actual - expected) > toleranceMillis
    }

    private fun Packet.Media.withoutPositions() = players.map { it.copy(position = null) }

    companion object {
        const val SEEK_TOLERANCE_MILLIS = 2_000L
    }
}

/**
 * Where [player] is at [nowMillis], from its position at [sentAtMillis]: moved on while it plays,
 * never past its length. Null when it has no position.
 */
fun extrapolatedPosition(player: Packet.MediaPlayer, sentAtMillis: Long, nowMillis: Long): Long? {
    val position = player.position ?: return null
    val moved = if (player.playing) position + (nowMillis - sentAtMillis).coerceAtLeast(0) else position
    val length = player.length
    return if (length != null) moved.coerceAtMost(length) else moved
}

/** A stream volume index from [min] to [max] as 0 to 100, rounded. */
fun volumePercent(index: Int, min: Int, max: Int): Int {
    val range = max - min
    if (range <= 0) return 0
    return ((index - min).coerceIn(0, range) * 100 + range / 2) / range
}

/** The stream volume index from [min] to [max] closest to [percent] (0 to 100). */
fun volumeIndex(percent: Int, min: Int, max: Int): Int {
    val range = max - min
    if (range <= 0) return min
    return min + (percent.coerceIn(0, 100) * range + 50) / 100
}

/** Volume changes leave at most four times a second (a slider drag); a mute change at once. */
fun volumeThrottle() = ChangeThrottle<Packet.Volume>(250) { sent, next -> sent.muted != next.muted }
