package fr.frohub.weft.features

/** One action of a posted notification: its label, and whether it takes typed text (an inline reply). */
data class PostedAction(val label: String, val takesText: Boolean)

/** What Weft reads from a notification posted on this phone, before any filtering. */
data class PostedNotification(
    val key: String,
    val packageName: String,
    val appLabel: String?,
    val title: String?,
    val text: String?,
    val time: Long,
    val ongoing: Boolean,
    val groupSummary: Boolean,
    val clearable: Boolean,
    val actions: List<PostedAction>,
)

/**
 * Which notifications go to the computers, and as what (docs/features.md, section 5). Remembers
 * what was sent per key, so an update leaves as `silent` and an unchanged repost not at all, and
 * so a removal is told only for a notification the computers know. Not thread-safe.
 */
class NotificationMirror(private val ownPackage: String) {
    private val sent = HashMap<String, Packet.Notification>()

    /** Weft's own, ongoing, group summaries, excluded apps and empty ones stay on the phone. */
    fun mirrors(posted: PostedNotification, excluded: Set<String>): Boolean =
        posted.packageName != ownPackage &&
            !posted.ongoing &&
            !posted.groupSummary &&
            posted.packageName !in excluded &&
            !(posted.title.isNullOrBlank() && posted.text.isNullOrBlank())

    /** The packet for a posted or updated notification; null when there is nothing to tell. */
    fun posted(posted: PostedNotification, excluded: Set<String>): Packet? {
        if (!mirrors(posted, excluded)) {
            // An update that turned it ongoing (a call answered), or an app excluded meanwhile.
            return removed(posted.key)
        }
        val packet = notificationPacket(posted, silent = false)
        return when (sent.put(posted.key, packet)) {
            null -> packet
            packet -> null
            else -> packet.copy(silent = true)
        }
    }

    fun removed(key: String): Packet.NotificationRemoved? = if (sent.remove(key) != null) Packet.NotificationRemoved(key) else null

    /** For `notification.request`: every active one again, silent since it was already shown. */
    fun all(active: List<PostedNotification>, excluded: Set<String>): List<Packet.Notification> {
        sent.clear()
        return active.filter { mirrors(it, excluded) }.map { posted ->
            notificationPacket(posted, silent = false).also { sent[posted.key] = it }.copy(silent = true)
        }
    }

    fun clear() = sent.clear()
}

/**
 * The `notification` packet of [posted]: title and text cut to 2000 characters, the labels of
 * its plain actions, and `reply` when one action takes typed text.
 */
fun notificationPacket(posted: PostedNotification, silent: Boolean): Packet.Notification {
    val labels = posted.actions.filter { !it.takesText && it.label.isNotBlank() }.map { it.label }.distinct()
    return Packet.Notification(
        id = posted.key,
        app = posted.appLabel,
        appId = posted.packageName,
        title = posted.title?.let { truncate(it, Packet.MAX_NOTIFICATION_TEXT) },
        text = posted.text?.let { truncate(it, Packet.MAX_NOTIFICATION_TEXT) },
        time = posted.time,
        dismissable = posted.clearable,
        actions = labels.ifEmpty { null },
        reply = posted.actions.any { it.takesText },
        silent = silent,
    )
}

/** At most [max] characters, never cutting a surrogate pair in half. */
fun truncate(text: String, max: Int): String {
    if (text.length <= max) return text
    val end = if (Character.isHighSurrogate(text[max - 1])) max - 1 else max
    return text.substring(0, end)
}
