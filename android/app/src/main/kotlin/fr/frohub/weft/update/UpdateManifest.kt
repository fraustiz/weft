package fr.frohub.weft.update

import java.net.URI
import java.net.URISyntaxException
import java.util.Locale
import org.json.JSONException
import org.json.JSONObject

/**
 * A version published on GitHub Releases, read from the `android-update.json` asset:
 * `{"versionCode": N, "versionName": "X.Y.Z", "url": "https://…/weft-X.Y.Z.apk", "sha256": "<hex>"}`.
 * [sha256] is lowercase hex.
 */
data class UpdateInfo(
    val versionCode: Long,
    val versionName: String,
    val url: String,
    val sha256: String,
) {
    /** Whether this version replaces the installed one ([installedCode]: its versionCode). */
    fun isNewerThan(installedCode: Long): Boolean = versionCode > installedCode
}

/**
 * Which URLs the updater fetches from: HTTPS on GitHub's release hosts only. A debug build
 * pointed at a test server ([forOverride]) also allows that one server.
 */
data class UpdatePolicy(val allowedHosts: Set<String>, val allowHttp: Boolean = false) {
    fun allows(url: String): Boolean {
        val uri = try {
            URI(url)
        } catch (_: URISyntaxException) {
            return false
        }
        val scheme = uri.scheme?.lowercase(Locale.ROOT) ?: return false
        if (scheme != "https" && !(allowHttp && scheme == "http")) return false
        // No user info: "https://github.com@evil.example/" has the host evil.example anyway, but
        // there is no reason to accept it.
        if (uri.rawUserInfo != null) return false
        val host = uri.host?.lowercase(Locale.ROOT) ?: return false
        return host in allowedHosts
    }

    companion object {
        val GITHUB = UpdatePolicy(
            setOf("github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"),
        )

        /** GitHub, plus the host of [manifestUrl] over http or https. For debug builds only. */
        fun forOverride(manifestUrl: String): UpdatePolicy {
            val host = try {
                URI(manifestUrl).host?.lowercase(Locale.ROOT)
            } catch (_: URISyntaxException) {
                null
            } ?: return GITHUB
            return UpdatePolicy(GITHUB.allowedHosts + host, allowHttp = true)
        }
    }
}

object UpdateManifest {
    private val SHA256_HEX = Regex("^[0-9a-f]{64}$")

    /** Reads `android-update.json`. Throws [UpdateError.Invalid] when anything is missing or wrong. */
    fun parse(json: String, policy: UpdatePolicy = UpdatePolicy.GITHUB): UpdateInfo {
        val root = try {
            JSONObject(json)
        } catch (_: JSONException) {
            throw UpdateError.Invalid("not a JSON object")
        }
        val code = when (val value = root.opt("versionCode")) {
            is Int -> value.toLong()
            is Long -> value
            else -> throw UpdateError.Invalid("versionCode missing or not an integer")
        }
        if (code <= 0) throw UpdateError.Invalid("versionCode must be positive")
        val name = (root.opt("versionName") as? String)?.trim()
        if (name.isNullOrEmpty()) throw UpdateError.Invalid("versionName missing")
        val url = (root.opt("url") as? String)?.trim()
        if (url.isNullOrEmpty()) throw UpdateError.Invalid("url missing")
        if (!policy.allows(url)) throw UpdateError.Invalid("url not allowed")
        val sha = (root.opt("sha256") as? String)?.trim()?.lowercase(Locale.ROOT)
        if (sha == null || !SHA256_HEX.matches(sha)) throw UpdateError.Invalid("sha256 missing or malformed")
        return UpdateInfo(code, name, url, sha)
    }
}

/** Why a check or a download failed. The screens map each kind to a message. */
sealed class UpdateError(message: String) : Exception(message) {
    /** No connection, timeout, TLS failure. */
    class Network(cause: Throwable) : UpdateError("network: ${cause.javaClass.simpleName}")

    /** 404 on the manifest: no release published yet. */
    class NoRelease : UpdateError("no release yet")

    /** Any other HTTP status. */
    class Http(val code: Int) : UpdateError("HTTP $code")

    /** The manifest is malformed, or a redirect leaves the allowed hosts. */
    class Invalid(reason: String) : UpdateError("invalid update: $reason")

    /** The downloaded APK does not match the published hash. */
    class BadHash : UpdateError("sha256 mismatch")

    /** The downloaded APK is not a newer Weft, or Android refused to install it. */
    class Install(reason: String) : UpdateError("install failed: $reason")
}
