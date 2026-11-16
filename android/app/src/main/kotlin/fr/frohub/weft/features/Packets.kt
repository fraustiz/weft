package fr.frohub.weft.features

import fr.frohub.weft.core.hexToBytesOrNull
import fr.frohub.weft.core.toHex
import fr.frohub.weft.protocol.Weft
import java.math.BigInteger
import kotlin.math.abs
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import org.json.JSONTokener

/**
 * The packets of docs/features.md (kind `0x07`) this app knows. Decoding is tolerant: unknown
 * types and unknown members are ignored, a missing optional member is null ("unknown").
 */
sealed interface Packet {
    val type: String

    data class Capabilities(val platform: String, val incoming: Set<String>, val outgoing: Set<String>) : Packet {
        override val type get() = CAPABILITIES
    }

    data object StateRequest : Packet {
        override val type get() = STATE_REQUEST
    }

    /** [low]: the system's low-battery state; null if the sender does not say. */
    data class Battery(val level: Int, val charging: Boolean, val low: Boolean?) : Packet {
        override val type get() = BATTERY
    }

    /** [network]: one of the `NETWORK_*` values, or null; [signal]: 0 to 4, or null. */
    data class Connectivity(val network: String?, val signal: Int?) : Packet {
        override val type get() = CONNECTIVITY
    }

    data object Ring : Packet {
        override val type get() = RING
    }

    data object RingStop : Packet {
        override val type get() = RING_STOP
    }

    data class Ping(val message: String?) : Packet {
        override val type get() = PING
    }

    data class ShareUrl(val url: String) : Packet {
        override val type get() = SHARE_URL
    }

    /** [id]: the transfer id, 32 lowercase hex characters. */
    data class FileOffer(val id: String, val name: String, val size: Long, val mime: String?) : Packet {
        override val type get() = FILE_OFFER
    }

    data class FileAccept(val id: String, val offset: Long) : Packet {
        override val type get() = FILE_ACCEPT
    }

    /** [reason]: one of the `REJECT_*` values, or whatever the sender put there. */
    data class FileReject(val id: String, val reason: String?) : Packet {
        override val type get() = FILE_REJECT
    }

    data class FileAck(val id: String, val offset: Long) : Packet {
        override val type get() = FILE_ACK
    }

    data class FileDone(val id: String) : Packet {
        override val type get() = FILE_DONE
    }

    data class FileCancel(val id: String) : Packet {
        override val type get() = FILE_CANCEL
    }

    companion object {
        const val CAPABILITIES = "capabilities"
        const val STATE_REQUEST = "state.request"
        const val BATTERY = "battery"
        const val CONNECTIVITY = "connectivity"
        const val RING = "ring"
        const val RING_STOP = "ring.stop"
        const val PING = "ping"
        const val SHARE_URL = "share.url"
        const val FILE_OFFER = "file.offer"
        const val FILE_ACCEPT = "file.accept"
        const val FILE_REJECT = "file.reject"
        const val FILE_ACK = "file.ack"
        const val FILE_DONE = "file.done"
        const val FILE_CANCEL = "file.cancel"

        const val PLATFORM_ANDROID = "android"

        const val NETWORK_5G = "5G"
        const val NETWORK_LTE = "LTE"
        const val NETWORK_3G = "3G"
        const val NETWORK_2G = "2G"
        const val NETWORK_WIFI = "wifi"
        const val NETWORK_NONE = "none"

        const val REJECT_DISABLED = "disabled"
        const val REJECT_TOO_LARGE = "tooLarge"
        const val REJECT_NO_SPACE = "noSpace"
        const val REJECT_BUSY = "busy"

        /** Largest file a transfer can carry: 4 GiB. */
        const val MAX_FILE_SIZE = 4L * 1024 * 1024 * 1024

        /** Largest `share.url` url, in bytes of UTF-8. */
        const val MAX_URL_LENGTH = 2048
    }
}

/**
 * Parses a packet plaintext. Null when it is not a JSON object, has no string `type`, has a type
 * this app does not know, or lacks a member its type requires.
 */
fun decodePacket(plaintext: ByteArray): Packet? {
    val json = try {
        JSONTokener(String(plaintext, Charsets.UTF_8)).nextValue() as? JSONObject
    } catch (_: JSONException) {
        null
    } ?: return null
    val type = json.opt("type") as? String ?: return null
    return when (type) {
        Packet.CAPABILITIES -> Packet.Capabilities(
            platform = json.string("platform") ?: return null,
            incoming = json.strings("incoming"),
            outgoing = json.strings("outgoing"),
        )
        Packet.STATE_REQUEST -> Packet.StateRequest
        Packet.BATTERY -> Packet.Battery(
            level = json.long("level")?.takeIf { it in 0..100 }?.toInt() ?: return null,
            charging = json.bool("charging") ?: return null,
            low = json.bool("low"),
        )
        Packet.CONNECTIVITY -> Packet.Connectivity(
            network = json.string("network"),
            signal = json.long("signal")?.takeIf { it in 0..4 }?.toInt(),
        )
        Packet.RING -> Packet.Ring
        Packet.RING_STOP -> Packet.RingStop
        Packet.PING -> Packet.Ping(json.string("message"))
        Packet.SHARE_URL -> Packet.ShareUrl(json.string("url")?.takeIf(::isShareableUrl) ?: return null)
        Packet.FILE_OFFER -> Packet.FileOffer(
            id = json.transferId() ?: return null,
            name = json.string("name") ?: return null,
            size = json.long("size")?.takeIf { it >= 0 } ?: return null,
            mime = json.string("mime"),
        )
        Packet.FILE_ACCEPT -> Packet.FileAccept(
            json.transferId() ?: return null,
            json.long("offset")?.takeIf { it >= 0 } ?: return null,
        )
        Packet.FILE_REJECT -> Packet.FileReject(json.transferId() ?: return null, json.string("reason"))
        Packet.FILE_ACK -> Packet.FileAck(
            json.transferId() ?: return null,
            json.long("offset")?.takeIf { it >= 0 } ?: return null,
        )
        Packet.FILE_DONE -> Packet.FileDone(json.transferId() ?: return null)
        Packet.FILE_CANCEL -> Packet.FileCancel(json.transferId() ?: return null)
        else -> null
    }
}

/** The JSON plaintext of [packet]; optional members that are null are left out. */
fun encodePacket(packet: Packet): ByteArray {
    val json = JSONObject().put("type", packet.type)
    when (packet) {
        is Packet.Capabilities -> json
            .put("platform", packet.platform)
            .put("incoming", JSONArray(packet.incoming.sorted()))
            .put("outgoing", JSONArray(packet.outgoing.sorted()))
        Packet.StateRequest, Packet.Ring, Packet.RingStop -> Unit
        is Packet.Battery -> json
            .put("level", packet.level)
            .put("charging", packet.charging)
            .apply { packet.low?.let { put("low", it) } }
        is Packet.Connectivity -> json
            .apply { packet.network?.let { put("network", it) } }
            .apply { packet.signal?.let { put("signal", it) } }
        is Packet.Ping -> packet.message?.let { json.put("message", it) }
        is Packet.ShareUrl -> json.put("url", packet.url)
        is Packet.FileOffer -> json
            .put("id", packet.id)
            .put("name", packet.name)
            .put("size", packet.size)
            .apply { packet.mime?.let { put("mime", it) } }
        is Packet.FileAccept -> json.put("id", packet.id).put("offset", packet.offset)
        is Packet.FileReject -> json.put("id", packet.id).apply { packet.reason?.let { put("reason", it) } }
        is Packet.FileAck -> json.put("id", packet.id).put("offset", packet.offset)
        is Packet.FileDone -> json.put("id", packet.id)
        is Packet.FileCancel -> json.put("id", packet.id)
    }
    return json.toString().toByteArray(Charsets.UTF_8)
}

/** An `http` or `https` url of at most 2048 bytes, without whitespace. */
fun isShareableUrl(text: String): Boolean {
    if (text.isEmpty() || text.toByteArray(Charsets.UTF_8).size > Packet.MAX_URL_LENGTH) return false
    if (text.any { it.isWhitespace() || it.isISOControl() }) return false
    val lower = text.lowercase()
    val rest = when {
        lower.startsWith("https://") -> text.substring(8)
        lower.startsWith("http://") -> text.substring(7)
        else -> return false
    }
    // A host is required: "https://" alone is not a link.
    return rest.isNotEmpty() && rest[0] != '/'
}

/** A transfer id as it travels in packets: 32 lowercase hex characters. */
fun transferIdHex(id: ByteArray): String = id.toHex()

private fun JSONObject.string(name: String): String? = opt(name) as? String

private fun JSONObject.bool(name: String): Boolean? = opt(name) as? Boolean

/** An integer member; a fractional or out-of-range number counts as missing. */
private fun JSONObject.long(name: String): Long? {
    val value = opt(name) as? Number ?: return null
    return when (value) {
        is Int, is Long -> value.toLong()
        is BigInteger -> if (value.bitLength() < Long.SIZE_BITS) value.toLong() else null
        // Decimals: Double on Android, BigDecimal in some org.json versions.
        else -> {
            val double = value.toDouble()
            if (double == Math.floor(double) && abs(double) < MAX_EXACT_DOUBLE) double.toLong() else null
        }
    }
}

/** Below this, a double holds every integer exactly. */
private const val MAX_EXACT_DOUBLE = 9.007199254740992E15

/** The string items of an array member; anything else in it is skipped. */
private fun JSONObject.strings(name: String): Set<String> {
    val array = opt(name) as? JSONArray ?: return emptySet()
    return (0 until array.length()).mapNotNull { array.opt(it) as? String }.toSet()
}

private fun JSONObject.transferId(): String? =
    string("id")?.takeIf { it.hexToBytesOrNull(Weft.TRANSFER_ID_LENGTH) != null }?.lowercase()
