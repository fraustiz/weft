package fr.frohub.weft.core

import fr.frohub.weft.protocol.Header
import fr.frohub.weft.protocol.Kind
import fr.frohub.weft.protocol.Lan
import fr.frohub.weft.protocol.Weft
import fr.frohub.weft.protocol.WeftException
import org.json.JSONObject

/** Numeric IPv4 only: no DNS, public destinations, privileged ports or unbounded lists. */
internal object LanRules {
    fun privateIpv4(text: String): Boolean {
        val parts = text.split('.')
        if (parts.size != 4) return false
        val bytes = parts.map { part ->
            if (part.isEmpty() || part.length > 3 || part.any { it !in '0'..'9' } ||
                part.length > 1 && part[0] == '0') return false
            part.toIntOrNull()?.takeIf { it in 0..255 } ?: return false
        }
        return bytes[0] == 10 || bytes[0] == 127 ||
            bytes[0] == 172 && bytes[1] in 16..31 || bytes[0] == 192 && bytes[1] == 168
    }

    fun endpoints(json: JSONObject): Pair<Int, List<String>>? {
        if (json.optString("type") != "lan.endpoints") return null
        val port = json.opt("port") as? Number ?: return null
        val value = port.toInt()
        if (value !in 1024..65535 || port.toDouble() != value.toDouble()) return null
        val addrs = json.optJSONArray("addrs") ?: return null
        val valid = (0 until minOf(addrs.length(), 4)).mapNotNull { index ->
            (addrs.opt(index) as? String)?.takeIf(::privateIpv4)
        }.distinct()
        return value to valid
    }

    fun envelope(frame: ByteArray, sender: String, recipient: String): Header? {
        if (frame.size !in (Weft.HEADER_LENGTH + Weft.NONCE_LENGTH + Weft.TAG_LENGTH)..Lan.MAX_FRAME_LENGTH) return null
        return try {
            val header = Header.decode(frame.copyOfRange(0, Weft.HEADER_LENGTH))
            header.takeIf {
                it.sender.toHex() == sender && it.recipient.toHex() == recipient &&
                    (it.kind == Kind.CLIPBOARD_TEXT || it.kind == Kind.PACKET || it.kind == Kind.FILE_CHUNK)
            }
        } catch (_: WeftException) { null }
    }
}
