package fr.frohub.weft.features

/**
 * The features of this batch (docs/features.md, sections 3, 4 and 9), each with the packet types
 * it receives and sends. [key] is how its switch is saved.
 */
enum class Feature(val key: String, val incoming: Set<String>, val outgoing: Set<String>) {
    // The phone shows the other side's battery too, if it sends one.
    BATTERY("battery", setOf(Packet.BATTERY), setOf(Packet.BATTERY)),
    CONNECTIVITY("connectivity", setOf(Packet.CONNECTIVITY), setOf(Packet.CONNECTIVITY)),
    RING("ring", setOf(Packet.RING, Packet.RING_STOP), setOf(Packet.RING, Packet.RING_STOP)),
    PING("ping", setOf(Packet.PING), setOf(Packet.PING)),
    LINKS("links", setOf(Packet.SHARE_URL), setOf(Packet.SHARE_URL)),
    FILES("files", FILE_TYPES, FILE_TYPES),
    ;

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
private val BASE_TYPES = setOf(Packet.CAPABILITIES, Packet.STATE_REQUEST)

/** This phone's `capabilities` packet: switched-off features are left out of both lists. */
fun capabilitiesOf(enabled: Set<Feature>): Packet.Capabilities = Packet.Capabilities(
    platform = Packet.PLATFORM_ANDROID,
    incoming = BASE_TYPES + enabled.flatMap { it.incoming },
    outgoing = BASE_TYPES + enabled.flatMap { it.outgoing },
)

/** What is known about another device from its packets. Null members: nothing received yet. */
data class PeerInfo(
    val capabilities: Packet.Capabilities? = null,
    val battery: Packet.Battery? = null,
    val connectivity: Packet.Connectivity? = null,
) {
    /** True if the other side says it acts on [type]. */
    fun handles(type: String): Boolean = capabilities?.incoming?.contains(type) == true
}
