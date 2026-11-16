package fr.frohub.weft.core

import android.security.NetworkSecurityPolicy
import fr.frohub.weft.protocol.Weft
import java.net.URI
import java.net.URISyntaxException
import java.util.Locale

/** Where a relay url points to. */
data class RelayEndpoint(
    val url: String,
    /** `wss://` rather than `ws://`. */
    val tls: Boolean,
    /**
     * Lowercased host, without port or IPv6 brackets: the name the relay must announce in its
     * challenge (docs/protocol.md, section 4).
     */
    val host: String,
) {
    /**
     * Whether this build may connect to it: `wss://` always, `ws://` only where the network
     * security config allows cleartext (debug builds, loopback and the emulator host).
     */
    val allowed: Boolean
        get() = tls || NetworkSecurityPolicy.getInstance().isCleartextTrafficPermitted(host)

    companion object {
        /** Parses a relay url: `ws://` or `wss://`, with a host, 1 to 255 bytes. Null if invalid. */
        fun parse(url: String): RelayEndpoint? {
            val length = url.toByteArray(Charsets.UTF_8).size
            if (length == 0 || length > Weft.MAX_RELAY_URL_LENGTH) return null
            val uri = try {
                URI(url)
            } catch (_: URISyntaxException) {
                return null
            }
            val tls = when (uri.scheme?.lowercase(Locale.ROOT)) {
                "wss" -> true
                "ws" -> false
                else -> return null
            }
            val host = uri.host?.removePrefix("[")?.removeSuffix("]")?.lowercase(Locale.ROOT)
            if (host.isNullOrEmpty()) return null
            return RelayEndpoint(url, tls, host)
        }
    }
}
