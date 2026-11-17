package fr.frohub.weft.update

import java.io.File
import java.io.IOException
import java.io.InputStream
import java.security.MessageDigest
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response

/** SHA-256 in lowercase hex. */
object Sha256 {
    fun hex(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it) }

    fun of(input: InputStream): String {
        val digest = MessageDigest.getInstance("SHA-256")
        val buffer = ByteArray(BUFFER)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            digest.update(buffer, 0, read)
        }
        return hex(digest.digest())
    }

    fun of(file: File): String = file.inputStream().use(::of)

    /** Whether [file] hashes to [expected] (hex, either case). */
    fun matches(file: File, expected: String): Boolean = of(file).equals(expected.trim(), ignoreCase = true)

    internal const val BUFFER = 64 * 1024
}

/**
 * Fetches the manifest and the APK. No Android here, so it runs in JVM tests. Redirects are
 * followed (GitHub sends release assets to its storage hosts), never from HTTPS to HTTP, and
 * the final URL must still be one [policy] allows.
 */
class UpdateClient(
    private val manifestUrl: String,
    private val policy: UpdatePolicy = UpdatePolicy.GITHUB,
    http: OkHttpClient = OkHttpClient(),
) {
    private val http = http.newBuilder()
        .followRedirects(true)
        .followSslRedirects(false)
        .build()

    /** The latest published version. Throws [UpdateError]. */
    fun fetchLatest(): UpdateInfo {
        if (!policy.allows(manifestUrl)) throw UpdateError.Invalid("manifest url not allowed")
        val body = get(manifestUrl) { response ->
            val length = response.body.contentLength()
            if (length > MAX_MANIFEST_BYTES) throw UpdateError.Invalid("manifest too large")
            val bytes = response.body.byteStream().use { it.readNBytesCompat(MAX_MANIFEST_BYTES + 1) }
            if (bytes.size > MAX_MANIFEST_BYTES) throw UpdateError.Invalid("manifest too large")
            bytes.toString(Charsets.UTF_8)
        }
        return UpdateManifest.parse(body, policy)
    }

    /**
     * Downloads [info]'s APK to [target], reporting `(done, total)` in bytes (total -1 when
     * unknown), and checks its hash. [target] is deleted on any failure. Throws [UpdateError].
     */
    fun download(info: UpdateInfo, target: File, progress: (Long, Long) -> Unit = { _, _ -> }) {
        if (!policy.allows(info.url)) throw UpdateError.Invalid("url not allowed")
        var ok = false
        try {
            val hash = get(info.url) { response ->
                val total = response.body.contentLength()
                if (total > MAX_APK_BYTES) throw UpdateError.Invalid("APK too large")
                val digest = MessageDigest.getInstance("SHA-256")
                var done = 0L
                response.body.byteStream().use { input ->
                    target.outputStream().use { output ->
                        val buffer = ByteArray(Sha256.BUFFER)
                        progress(0, total)
                        while (true) {
                            val read = input.read(buffer)
                            if (read < 0) break
                            done += read
                            if (done > MAX_APK_BYTES) throw UpdateError.Invalid("APK too large")
                            digest.update(buffer, 0, read)
                            output.write(buffer, 0, read)
                            progress(done, total)
                        }
                    }
                }
                Sha256.hex(digest.digest())
            }
            if (hash != info.sha256) throw UpdateError.BadHash()
            ok = true
        } finally {
            if (!ok) target.delete()
        }
    }

    private fun <T> get(url: String, read: (Response) -> T): T {
        val request = Request.Builder().url(url).header("Accept", "application/octet-stream, application/json").build()
        try {
            http.newCall(request).execute().use { response ->
                if (!policy.allows(response.request.url.toString())) throw UpdateError.Invalid("redirected outside the allowed hosts")
                when {
                    response.code == 404 -> throw UpdateError.NoRelease()
                    !response.isSuccessful -> throw UpdateError.Http(response.code)
                }
                return read(response)
            }
        } catch (e: UpdateError) {
            throw e
        } catch (e: IOException) {
            throw UpdateError.Network(e)
        } catch (e: IllegalArgumentException) {
            throw UpdateError.Invalid("bad url")
        }
    }

    private fun InputStream.readNBytesCompat(limit: Int): ByteArray {
        val out = java.io.ByteArrayOutputStream()
        val buffer = ByteArray(8192)
        while (out.size() < limit) {
            val read = read(buffer, 0, minOf(buffer.size, limit - out.size()))
            if (read < 0) break
            out.write(buffer, 0, read)
        }
        return out.toByteArray()
    }

    companion object {
        const val MAX_MANIFEST_BYTES = 16 * 1024
        const val MAX_APK_BYTES = 200L * 1024 * 1024
    }
}

/** When an automatic check is due: switched on, and none in the last [interval] ms. */
object UpdateSchedule {
    const val INTERVAL_MS = 24L * 60 * 60 * 1000

    fun isDue(autoCheck: Boolean, lastCheckMs: Long, nowMs: Long, interval: Long = INTERVAL_MS): Boolean {
        if (!autoCheck) return false
        // A clock set back makes the last check look in the future: check anyway.
        if (lastCheckMs > nowMs) return true
        return nowMs - lastCheckMs >= interval
    }
}
