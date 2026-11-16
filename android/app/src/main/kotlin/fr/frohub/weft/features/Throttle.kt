package fr.frohub.weft.features

/**
 * Rate limit for a status that is sent on change (battery, connectivity): a new value leaves at
 * once if it differs from the last one sent in a way [urgent] says matters, otherwise at most once
 * per [intervalMillis]. A value held back leaves when the interval is over (see [Decision.Later]),
 * unless a newer one replaces it. Not thread-safe: call it from one place.
 */
class ChangeThrottle<T : Any>(
    private val intervalMillis: Long,
    private val urgent: (sent: T, next: T) -> Boolean,
) {
    private var sent: T? = null
    private var sentAt = 0L

    sealed interface Decision {
        data object Now : Decision
        /** Send at [atMillis], if nothing newer came in by then. */
        data class Later(val atMillis: Long) : Decision
        /** The same value as the last one sent. */
        data object Skip : Decision
    }

    /** What to do with [value], seen at [nowMillis]. A [Decision.Now] counts as sent. */
    fun offer(value: T, nowMillis: Long): Decision {
        val last = sent
        return when {
            last == value -> Decision.Skip
            last == null || urgent(last, value) || nowMillis - sentAt >= intervalMillis -> {
                markSent(value, nowMillis)
                Decision.Now
            }
            else -> Decision.Later(sentAt + intervalMillis)
        }
    }

    /** Records that [value] left at [nowMillis] (a [Decision.Later] carried out, or a full resend). */
    fun markSent(value: T, nowMillis: Long) {
        sent = value
        sentAt = nowMillis
    }
}

/** docs/features.md, section 3: at most once per minute unless `charging` or `low` changes. */
fun batteryThrottle() = ChangeThrottle<Packet.Battery>(60_000) { sent, next ->
    sent.charging != next.charging || sent.low != next.low
}

/** A change of network type leaves at once; signal level changes at most once per minute. */
fun connectivityThrottle() = ChangeThrottle<Packet.Connectivity>(60_000) { sent, next -> sent.network != next.network }
