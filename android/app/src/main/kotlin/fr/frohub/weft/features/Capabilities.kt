package fr.frohub.weft.features

import android.graphics.Bitmap
import fr.frohub.weft.BuildConfig

/**
 * The features of docs/features.md (sections 3 to 9), each with the packet types it receives and
 * sends. [key] is how its switch is saved, [defaultOn] its state until the user changes it. The
 * settings list them in this order.
 */
enum class Feature(val key: String, val incoming: Set<String>, val outgoing: Set<String>, val defaultOn: Boolean = true) {
    // The phone shows the other side's battery too, if it sends one.
    BATTERY("battery", setOf(Packet.BATTERY), setOf(Packet.BATTERY)),
    CONNECTIVITY("connectivity", setOf(Packet.CONNECTIVITY), setOf(Packet.CONNECTIVITY)),
    RING("ring", setOf(Packet.RING, Packet.RING_STOP), setOf(Packet.RING, Packet.RING_STOP)),
    PING("ping", setOf(Packet.PING), setOf(Packet.PING)),
    LINKS("links", setOf(Packet.SHARE_URL), setOf(Packet.SHARE_URL)),
    FILES("files", FILE_TYPES, FILE_TYPES),
    NOTIFICATIONS(
        "notifications",
        setOf(Packet.NOTIFICATION_DISMISS, Packet.NOTIFICATION_ACTION, Packet.NOTIFICATION_REPLY, Packet.NOTIFICATION_REQUEST),
        setOf(Packet.NOTIFICATION, Packet.NOTIFICATION_REMOVED),
    ),
    CALLS("calls", setOf(Packet.CALL_MUTE), setOf(Packet.CALL)),
    // Both ways: the phone's players for the computers, and the computers' players on the device screen.
    MEDIA(
        "media",
        setOf(Packet.MEDIA, Packet.MEDIA_COMMAND, Packet.MEDIA_ART),
        setOf(Packet.MEDIA, Packet.MEDIA_COMMAND, Packet.MEDIA_ART),
    ),
    VOLUME("volume", setOf(Packet.VOLUME, Packet.VOLUME_SET), setOf(Packet.VOLUME, Packet.VOLUME_SET)),
    SMS(
        "sms",
        setOf(Packet.SMS_CONVERSATIONS_REQUEST, Packet.SMS_MESSAGES_REQUEST, Packet.SMS_SEND),
        setOf(Packet.SMS_CONVERSATIONS, Packet.SMS_MESSAGES, Packet.SMS_SENT, Packet.SMS_RECEIVED),
        defaultOn = false,
    ),
    CONTACTS("contacts", setOf(Packet.CONTACTS_REQUEST), setOf(Packet.CONTACTS), defaultOn = false),
    // Computers typing on this phone, only through the Weft keyboard. Before REMOTE: [of] gives
    // `input.key` to this switch, so the keyboard stays off until the user turns it on.
    INPUT("input", setOf(Packet.INPUT_KEY), emptySet(), defaultOn = false),
    // Remote control and presenter towards the computers. On by default: a computer accepts them
    // only once its user allowed this phone to control it.
    REMOTE("remote", emptySet(), setOf(Packet.INPUT_POINTER, Packet.INPUT_CLICK, Packet.INPUT_KEY, Packet.PRESENTER)),
    // Commands live on the computer: the phone only shows its list and asks it to run one.
    COMMANDS("commands", setOf(Packet.COMMAND_LIST), setOf(Packet.COMMAND_RUN)),
    ;

    /** False for a feature this build leaves out (SMS on Google Play): never on, never shown. */
    val available: Boolean get() = this != SMS || BuildConfig.SMS_ENABLED

    companion object {
        /** The feature a packet type belongs to; null for the types every device handles. */
        fun of(type: String): Feature? = entries.firstOrNull { type in it.incoming || type in it.outgoing }
    }
}

private val FILE_TYPES = setOf(
    Packet.FILE_OFFER,
    Packet.FILE_ACCEPT,
    Packet.FILE_REJECT,
    Packet.FILE_ACK,
    Packet.FILE_DONE,
    Packet.FILE_CANCEL,
)

/** Handled whatever the settings say. */
private val BASE_TYPES = setOf(
    Packet.CAPABILITIES, Packet.STATE_REQUEST, Packet.CLIPBOARD_IMAGE_OFFER, Packet.CLIPBOARD_IMAGE_ACCEPT,
    Packet.FILE_ACK, Packet.FILE_DONE, Packet.FILE_CANCEL, Packet.FILE_REJECT,
)

/** This phone's `capabilities` packet: switched-off features are left out of both lists. */
fun capabilitiesOf(enabled: Set<Feature>): Packet.Capabilities = Packet.Capabilities(
    platform = Packet.PLATFORM_ANDROID,
    incoming = BASE_TYPES + enabled.flatMap { it.incoming },
    outgoing = BASE_TYPES + enabled.flatMap { it.outgoing },
)

/**
 * What is known about another device from its packets. Null members: nothing received yet.
 * [mediaAt] is when [media] arrived (ms since epoch), to move the positions of playing players on.
 * [covers]: its last [MAX_COVERS] covers by `art` id, oldest first.
 */
data class PeerInfo(
    val capabilities: Packet.Capabilities? = null,
    val battery: Packet.Battery? = null,
    val connectivity: Packet.Connectivity? = null,
    val media: Packet.Media? = null,
    val mediaAt: Long = 0,
    val covers: Map<String, Bitmap> = emptyMap(),
    val volume: Packet.Volume? = null,
    val commands: List<Command>? = null,
) {
    /** True if the other side says it acts on [type]. */
    fun handles(type: String): Boolean = capabilities?.incoming?.contains(type) == true
}
