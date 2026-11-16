package fr.frohub.weft.features

import fr.frohub.weft.protocol.Weft

/**
 * One row of Android's SMS table (`Telephony.Sms`), as read: [date] in ms, [type] one of the
 * `SMS_TYPE_*` values. Kept free of Android types so the shaping below runs in unit tests.
 */
data class SmsRow(
    val id: Long,
    val thread: Long,
    val address: String?,
    val body: String?,
    val date: Long,
    val type: Int,
    val read: Boolean,
)

// Telephony.Sms.MESSAGE_TYPE_* (unchanged since Android 4.4).
const val SMS_TYPE_INBOX = 1
const val SMS_TYPE_SENT = 2
const val SMS_TYPE_DRAFT = 3
const val SMS_TYPE_OUTBOX = 4
const val SMS_TYPE_FAILED = 5
const val SMS_TYPE_QUEUED = 6

/** The message as it travels; null for drafts and rows of no known type. */
fun SmsRow.toMessage(): SmsMessage? {
    val status = when (type) {
        SMS_TYPE_INBOX -> null
        SMS_TYPE_SENT -> Packet.SMS_STATUS_SENT
        SMS_TYPE_OUTBOX, SMS_TYPE_QUEUED -> Packet.SMS_STATUS_PENDING
        SMS_TYPE_FAILED -> Packet.SMS_STATUS_FAILED
        else -> return null
    }
    return SmsMessage(id, address, body.orEmpty(), date, outgoing = type != SMS_TYPE_INBOX, status = status)
}

/**
 * The threads of [rows] (newest first), at most [max]: the newest message gives the snippet and
 * the time, every address seen in the thread is listed, [unreadThreads] says which have unread
 * messages. [nameOf] looks up a contact name, once per address.
 */
fun conversationsOf(
    rows: Sequence<SmsRow>,
    unreadThreads: Set<Long>,
    nameOf: (String) -> String?,
    max: Int = Packet.MAX_CONVERSATIONS,
): List<SmsConversation> {
    class Thread(val newest: SmsRow) {
        val addresses = LinkedHashSet<String>()
    }
    val threads = LinkedHashMap<Long, Thread>()
    for (row in rows) {
        if (row.toMessage() == null) continue
        val thread = threads[row.thread] ?: run {
            // Older rows of the threads already listed may still add an address; new threads may not.
            if (threads.size >= max) return@run null
            Thread(row).also { threads[row.thread] = it }
        } ?: continue
        row.address?.takeIf { it.isNotBlank() }?.let { thread.addresses += it }
    }
    val names = HashMap<String, String?>()
    return threads.map { (id, thread) ->
        val addresses = thread.addresses.toList()
        SmsConversation(
            thread = id,
            addresses = addresses,
            names = addresses.map { address -> names.getOrPut(address) { nameOf(address) } },
            snippet = thread.newest.body,
            time = thread.newest.date,
            unread = id in unreadThreads,
        )
    }
}

/**
 * The packet [build] makes with as many items as fit in [maxBytes] once encoded, from 0 to
 * [count]: a packet over the limit is split by sending fewer items (docs/features.md, section 7).
 */
fun <P : Packet> largestFitting(count: Int, maxBytes: Int = Weft.MAX_PACKET_LENGTH, build: (Int) -> P): P {
    val all = build(count)
    if (encodePacket(all).size <= maxBytes) return all
    // Bisection: low always fits, high never does.
    var low = 0
    var high = count
    while (high - low > 1) {
        val middle = (low + high) / 2
        if (encodePacket(build(middle)).size <= maxBytes) low = middle else high = middle
    }
    return build(low)
}

/** The newest conversations that fit in one packet. */
fun conversationsPacket(conversations: List<SmsConversation>): Packet.SmsConversations =
    largestFitting(conversations.size) { Packet.SmsConversations(conversations.take(it)) }

/** [messages] oldest first; when they do not all fit, the oldest are left out. */
fun messagesPacket(thread: Long, messages: List<SmsMessage>): Packet.SmsMessages =
    largestFitting(messages.size) { Packet.SmsMessages(thread, messages.takeLast(it)) }

/**
 * [contacts] in packets of at most [maxBytes], each with its `part` (from 1) and `parts`. At
 * least one packet, empty when there is no contact; a contact too large for a packet on its own
 * is left out.
 */
fun contactParts(contacts: List<Contact>, maxBytes: Int = Packet.MAX_CONTACTS_PART): List<Packet.Contacts> {
    // Sizes measured with the widest part numbers there can be, so the real ones fit too.
    val wide = Packet.MAX_CONTACTS
    val empty = encodePacket(Packet.Contacts(emptyList(), wide, wide)).size
    val groups = ArrayList<List<Contact>>()
    var current = ArrayList<Contact>()
    var size = empty
    for (contact in contacts.take(Packet.MAX_CONTACTS)) {
        val itemSize = encodePacket(Packet.Contacts(listOf(contact), wide, wide)).size - empty
        if (empty + itemSize > maxBytes) continue
        // One comma between items.
        val added = if (current.isEmpty()) itemSize else itemSize + 1
        if (size + added > maxBytes) {
            groups += current
            current = ArrayList()
            size = empty + itemSize
        } else {
            size += added
        }
        current += contact
    }
    if (current.isNotEmpty() || groups.isEmpty()) groups += current
    return groups.mapIndexed { index, group -> Packet.Contacts(group, index + 1, groups.size) }
}
