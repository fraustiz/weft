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

    /**
     * A notification shown on the phone. [id] is stable for one notification (its key); [text]
     * is at most [MAX_NOTIFICATION_TEXT] characters; [actions] null when it has none.
     */
    data class Notification(
        val id: String,
        val app: String?,
        val appId: String?,
        val title: String?,
        val text: String?,
        val time: Long?,
        val dismissable: Boolean,
        val actions: List<String>?,
        val reply: Boolean,
        val silent: Boolean,
    ) : Packet {
        override val type get() = NOTIFICATION
    }

    data class NotificationRemoved(val id: String) : Packet {
        override val type get() = NOTIFICATION_REMOVED
    }

    data class NotificationDismiss(val id: String) : Packet {
        override val type get() = NOTIFICATION_DISMISS
    }

    data class NotificationAction(val id: String, val action: String) : Packet {
        override val type get() = NOTIFICATION_ACTION
    }

    data class NotificationReply(val id: String, val text: String) : Packet {
        override val type get() = NOTIFICATION_REPLY
    }

    data object NotificationRequest : Packet {
        override val type get() = NOTIFICATION_REQUEST
    }

    /** [event]: one of the `CALL_*` values. */
    data class Call(val event: String, val number: String?, val name: String?) : Packet {
        override val type get() = CALL
    }

    data object CallMute : Packet {
        override val type get() = CALL_MUTE
    }

    /** One media player; [position] and [length] in ms, [volume] 0 to 100, null when unknown. */
    data class MediaPlayer(
        val id: String,
        val app: String?,
        val title: String?,
        val artist: String?,
        val album: String?,
        val playing: Boolean,
        val position: Long?,
        val length: Long?,
        val canPlay: Boolean,
        val canPause: Boolean,
        val canNext: Boolean,
        val canPrevious: Boolean,
        val canSeek: Boolean,
        val volume: Int? = null,
    )

    /** Every player of the sender; empty when nothing plays. */
    data class Media(val players: List<MediaPlayer>) : Packet {
        override val type get() = MEDIA
    }

    /** [action]: one of the `MEDIA_*` values; [value]: ms for a seek, 0 to 100 for a volume. */
    data class MediaCommand(val player: String, val action: String, val value: Long?) : Packet {
        override val type get() = MEDIA_COMMAND
    }

    /** The phone's media volume, or the computer's default output. [level] 0 to 100. */
    data class Volume(val level: Int, val muted: Boolean) : Packet {
        override val type get() = VOLUME
    }

    /** Either member may be null: only the other one changes. */
    data class VolumeSet(val level: Int?, val muted: Boolean?) : Packet {
        override val type get() = VOLUME_SET
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
        const val NOTIFICATION = "notification"
        const val NOTIFICATION_REMOVED = "notification.removed"
        const val NOTIFICATION_DISMISS = "notification.dismiss"
        const val NOTIFICATION_ACTION = "notification.action"
        const val NOTIFICATION_REPLY = "notification.reply"
        const val NOTIFICATION_REQUEST = "notification.request"
        const val CALL = "call"
        const val CALL_MUTE = "call.mute"
        const val MEDIA = "media"
        const val MEDIA_COMMAND = "media.command"
        const val VOLUME = "volume"
        const val VOLUME_SET = "volume.set"

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

        const val CALL_RINGING = "ringing"
        const val CALL_TALKING = "talking"
        const val CALL_MISSED = "missed"
        const val CALL_ENDED = "ended"
        val CALL_EVENTS = setOf(CALL_RINGING, CALL_TALKING, CALL_MISSED, CALL_ENDED)

        const val MEDIA_PLAY = "play"
        const val MEDIA_PAUSE = "pause"
        const val MEDIA_PLAY_PAUSE = "playPause"
        const val MEDIA_NEXT = "next"
        const val MEDIA_PREVIOUS = "previous"
        const val MEDIA_SEEK = "seek"
        const val MEDIA_SET_VOLUME = "setVolume"
        val MEDIA_ACTIONS = setOf(MEDIA_PLAY, MEDIA_PAUSE, MEDIA_PLAY_PAUSE, MEDIA_NEXT, MEDIA_PREVIOUS, MEDIA_SEEK, MEDIA_SET_VOLUME)

        /** Longest `notification` text, in characters. */
        const val MAX_NOTIFICATION_TEXT = 2000

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
        Packet.NOTIFICATION -> Packet.Notification(
            id = json.string("id") ?: return null,
            app = json.string("app"),
            appId = json.string("appId"),
            title = json.string("title"),
            text = json.string("text")?.take(Packet.MAX_NOTIFICATION_TEXT),
            time = json.long("time"),
            dismissable = json.bool("dismissable") ?: false,
            actions = json.stringList("actions"),
            reply = json.bool("reply") ?: false,
            silent = json.bool("silent") ?: false,
        )
        Packet.NOTIFICATION_REMOVED -> Packet.NotificationRemoved(json.string("id") ?: return null)
        Packet.NOTIFICATION_DISMISS -> Packet.NotificationDismiss(json.string("id") ?: return null)
        Packet.NOTIFICATION_ACTION -> Packet.NotificationAction(json.string("id") ?: return null, json.string("action") ?: return null)
        Packet.NOTIFICATION_REPLY -> Packet.NotificationReply(json.string("id") ?: return null, json.string("text") ?: return null)
        Packet.NOTIFICATION_REQUEST -> Packet.NotificationRequest
        Packet.CALL -> Packet.Call(
            event = json.string("event")?.takeIf { it in Packet.CALL_EVENTS } ?: return null,
            number = json.string("number"),
            name = json.string("name"),
        )
        Packet.CALL_MUTE -> Packet.CallMute
        Packet.MEDIA -> {
            val array = json.opt("players") as? JSONArray ?: return null
            Packet.Media((0 until array.length()).mapNotNull { (array.opt(it) as? JSONObject)?.toPlayer() })
        }
        Packet.MEDIA_COMMAND -> Packet.MediaCommand(
            player = json.string("player") ?: return null,
            action = json.string("action")?.takeIf { it in Packet.MEDIA_ACTIONS } ?: return null,
            value = json.long("value"),
        )
        Packet.VOLUME -> Packet.Volume(
            level = json.percent("level") ?: return null,
            muted = json.bool("muted") ?: return null,
        )
        Packet.VOLUME_SET -> Packet.VolumeSet(json.percent("level"), json.bool("muted"))
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
        is Packet.Notification -> json
            .put("id", packet.id)
            .putOpt("app", packet.app)
            .putOpt("appId", packet.appId)
            .putOpt("title", packet.title)
            .putOpt("text", packet.text)
            .putOpt("time", packet.time)
            .put("dismissable", packet.dismissable)
            .putOpt("actions", packet.actions?.let { JSONArray(it) })
            .put("reply", packet.reply)
            .put("silent", packet.silent)
        is Packet.NotificationRemoved -> json.put("id", packet.id)
        is Packet.NotificationDismiss -> json.put("id", packet.id)
        is Packet.NotificationAction -> json.put("id", packet.id).put("action", packet.action)
        is Packet.NotificationReply -> json.put("id", packet.id).put("text", packet.text)
        Packet.NotificationRequest, Packet.CallMute -> Unit
        is Packet.Call -> json.put("event", packet.event).putOpt("number", packet.number).putOpt("name", packet.name)
        is Packet.Media -> json.put("players", JSONArray(packet.players.map { it.toJson() }))
        is Packet.MediaCommand -> json.put("player", packet.player).put("action", packet.action).putOpt("value", packet.value)
        is Packet.Volume -> json.put("level", packet.level).put("muted", packet.muted)
        is Packet.VolumeSet -> json.putOpt("level", packet.level).putOpt("muted", packet.muted)
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

private fun JSONObject.toPlayer(): Packet.MediaPlayer? = Packet.MediaPlayer(
    id = string("id") ?: return null,
    app = string("app"),
    title = string("title"),
    artist = string("artist"),
    album = string("album"),
    playing = bool("playing") ?: false,
    position = long("position")?.takeIf { it >= 0 },
    length = long("length")?.takeIf { it > 0 },
    canPlay = bool("canPlay") ?: false,
    canPause = bool("canPause") ?: false,
    canNext = bool("canNext") ?: false,
    canPrevious = bool("canPrevious") ?: false,
    canSeek = bool("canSeek") ?: false,
    volume = percent("volume"),
)

private fun Packet.MediaPlayer.toJson(): JSONObject = JSONObject()
    .put("id", id)
    .putOpt("app", app)
    .putOpt("title", title)
    .putOpt("artist", artist)
    .putOpt("album", album)
    .put("playing", playing)
    .putOpt("position", position)
    .putOpt("length", length)
    .put("canPlay", canPlay)
    .put("canPause", canPause)
    .put("canNext", canNext)
    .put("canPrevious", canPrevious)
    .put("canSeek", canSeek)
    .putOpt("volume", volume)

private fun JSONObject.string(name: String): String? = opt(name) as? String

/** An integer member from 0 to 100; anything else counts as missing. */
private fun JSONObject.percent(name: String): Int? = long(name)?.takeIf { it in 0..100 }?.toInt()

/** The string items of an array member, in order; null when there is no array. */
private fun JSONObject.stringList(name: String): List<String>? {
    val array = opt(name) as? JSONArray ?: return null
    return (0 until array.length()).mapNotNull { array.opt(it) as? String }
}

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
