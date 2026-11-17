package fr.frohub.weft.update

import com.sun.net.httpserver.HttpServer
import java.net.InetSocketAddress
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** The manifest, the allowed hosts, versions, the daily schedule, hashing and the download. */
class UpdateTest {
    private val sha = "a".repeat(64)
    private val url = "https://github.com/fraustiz/weft/releases/download/app-v0.2.0/weft-0.2.0.apk"

    private fun json(
        code: String = "120",
        name: String = "\"0.2.0\"",
        url: String = "\"${this.url}\"",
        sha: String = "\"${this.sha}\"",
        extra: String = "",
    ) = """{"versionCode": $code, "versionName": $name, "url": $url, "sha256": $sha$extra}"""

    // region Manifest

    @Test
    fun parsesTheManifest() {
        assertEquals(UpdateInfo(120, "0.2.0", url, sha), UpdateManifest.parse(json()))
    }

    @Test
    fun ignoresExtraFieldsAndNormalisesTheHash() {
        val info = UpdateManifest.parse(json(sha = "\"${"AB".repeat(32)}\"", extra = """, "notes": "x", "minSdk": 26"""))
        assertEquals("ab".repeat(32), info.sha256)
    }

    @Test
    fun rejectsMissingOrWrongFields() {
        val bad = listOf(
            """{"versionName": "0.2.0", "url": "$url", "sha256": "$sha"}""",
            """{"versionCode": 120, "url": "$url", "sha256": "$sha"}""",
            """{"versionCode": 120, "versionName": "0.2.0", "sha256": "$sha"}""",
            """{"versionCode": 120, "versionName": "0.2.0", "url": "$url"}""",
            json(code = "\"120\""),
            json(code = "1.5"),
            json(code = "0"),
            json(code = "-3"),
            json(name = "\"  \""),
            json(sha = "\"abc\""),
            json(sha = "\"${"g".repeat(64)}\""),
            json(url = "42"),
            "[]",
            "not json",
            "",
        )
        for (text in bad) {
            assertThrows(text, UpdateError.Invalid::class.java) { UpdateManifest.parse(text) }
        }
    }

    @Test
    fun allowsOnlyHttpsGitHubHosts() {
        val policy = UpdatePolicy.GITHUB
        assertTrue(policy.allows(url))
        assertTrue(policy.allows("https://objects.githubusercontent.com/a/b.apk"))
        assertTrue(policy.allows("https://release-assets.githubusercontent.com/a/b.apk?x=1"))
        assertTrue(policy.allows("https://GitHub.com/x.apk"))
        assertFalse(policy.allows("http://github.com/x.apk"))
        assertFalse(policy.allows("https://evil.example/x.apk"))
        assertFalse(policy.allows("https://github.com.evil.example/x.apk"))
        assertFalse(policy.allows("https://evilgithub.com/x.apk"))
        assertFalse(policy.allows("https://raw.githubusercontent.com/x.apk"))
        assertFalse(policy.allows("https://github.com@evil.example/x.apk"))
        assertFalse(policy.allows("https://user@github.com/x.apk"))
        assertFalse(policy.allows("file:///sdcard/x.apk"))
        assertFalse(policy.allows("ftp://github.com/x.apk"))
        assertFalse(policy.allows("//github.com/x.apk"))
        assertFalse(policy.allows("not a url"))
        assertFalse(policy.allows(""))

        assertThrows(UpdateError.Invalid::class.java) { UpdateManifest.parse(json(url = "\"https://evil.example/weft.apk\"")) }
        assertThrows(UpdateError.Invalid::class.java) { UpdateManifest.parse(json(url = "\"http://github.com/weft.apk\"")) }
    }

    @Test
    fun aDebugOverrideAllowsItsOwnHostOnly() {
        val policy = UpdatePolicy.forOverride("http://10.0.2.2:8000/android-update.json")
        assertTrue(policy.allows("http://10.0.2.2:8000/weft.apk"))
        assertTrue(policy.allows(url))
        assertFalse(policy.allows("http://10.0.2.3:8000/weft.apk"))
    }

    // endregion

    // region Versions and schedule

    @Test
    fun comparesVersionCodes() {
        val info = UpdateInfo(120, "0.2.0", url, sha)
        assertTrue(info.isNewerThan(1))
        assertTrue(info.isNewerThan(119))
        assertFalse(info.isNewerThan(120))
        assertFalse(info.isNewerThan(121))
    }

    @Test
    fun checksAtMostOnceADay() {
        val day = UpdateSchedule.INTERVAL_MS
        val now = 10 * day
        assertTrue(UpdateSchedule.isDue(true, 0, now))
        assertTrue(UpdateSchedule.isDue(true, now - day, now))
        assertFalse(UpdateSchedule.isDue(true, now - day + 1, now))
        assertFalse(UpdateSchedule.isDue(false, 0, now))
        // Clock set back.
        assertTrue(UpdateSchedule.isDue(true, now + 1000, now))
    }

    // endregion

    // region Hash and download

    @Test
    fun hashesFiles() {
        val file = Files.createTempFile("weft", ".bin").toFile()
        try {
            file.writeText("abc")
            val expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            assertEquals(expected, Sha256.of(file))
            assertTrue(Sha256.matches(file, expected.uppercase()))
            assertFalse(Sha256.matches(file, "0".repeat(64)))
        } finally {
            file.delete()
        }
    }

    @Test
    fun fetchesAndDownloadsThroughRedirects() = withServer { base, apk ->
        val apkHash = Sha256.hex(java.security.MessageDigest.getInstance("SHA-256").digest(apk))
        manifest = """{"versionCode": 7, "versionName": "1.0.0", "url": "$base/redirect", "sha256": "$apkHash"}"""
        val client = UpdateClient("$base/android-update.json", UpdatePolicy.forOverride(base))
        val info = client.fetchLatest()
        assertEquals(7L, info.versionCode)

        val target = Files.createTempFile("weft", ".apk").toFile()
        var last = 0L to 0L
        client.download(info, target) { done, total -> last = done to total }
        assertTrue(target.readBytes().contentEquals(apk))
        assertEquals(apk.size.toLong() to apk.size.toLong(), last)
        target.delete()
    }

    @Test
    fun aWrongHashDeletesTheDownload() = withServer { base, _ ->
        val client = UpdateClient("$base/android-update.json", UpdatePolicy.forOverride(base))
        val target = Files.createTempFile("weft", ".apk").toFile()
        val info = UpdateInfo(7, "1.0.0", "$base/weft.apk", "0".repeat(64))
        assertThrows(UpdateError.BadHash::class.java) { client.download(info, target) }
        assertFalse(target.exists())
    }

    @Test
    fun aMissingReleaseIsNotAnError() = withServer { base, _ ->
        manifest = null
        val client = UpdateClient("$base/android-update.json", UpdatePolicy.forOverride(base))
        assertThrows(UpdateError.NoRelease::class.java) { client.fetchLatest() }
    }

    @Test
    fun aRedirectOutsideTheAllowedHostsIsRefused() = withServer { base, _ ->
        // localhost is a different host from 127.0.0.1, so not allowed by the policy.
        redirectTo = base.replace("127.0.0.1", "localhost") + "/weft.apk"
        val client = UpdateClient("$base/android-update.json", UpdatePolicy.forOverride(base))
        val target = Files.createTempFile("weft", ".apk").toFile()
        val info = UpdateInfo(7, "1.0.0", "$base/redirect", "0".repeat(64))
        assertThrows(UpdateError.Invalid::class.java) { client.download(info, target) }
        assertFalse(target.exists())
    }

    @Test
    fun noServerIsANetworkError() {
        val client = UpdateClient("http://127.0.0.1:1/android-update.json", UpdatePolicy.forOverride("http://127.0.0.1:1/"))
        assertThrows(UpdateError.Network::class.java) { client.fetchLatest() }
    }

    @Test
    fun aManifestUrlOutsideThePolicyIsRefused() {
        val client = UpdateClient("https://evil.example/android-update.json")
        assertThrows(UpdateError.Invalid::class.java) { client.fetchLatest() }
    }

    // endregion

    private var manifest: String? = null
    private var redirectTo: String? = null

    /** A local server: the manifest, `/weft.apk`, and `/redirect` to [redirectTo] (or the APK). */
    private fun withServer(block: (String, ByteArray) -> Unit) {
        val apk = ByteArray(200_000) { (it * 31).toByte() }
        val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        val base = "http://127.0.0.1:${server.address.port}"
        server.createContext("/") { exchange ->
            val path = exchange.requestURI.path
            val body: ByteArray? = when (path) {
                "/android-update.json" -> manifest?.toByteArray()
                "/weft.apk" -> apk
                else -> null
            }
            when {
                path == "/redirect" -> {
                    exchange.responseHeaders.add("Location", redirectTo ?: "$base/weft.apk")
                    exchange.sendResponseHeaders(302, -1)
                }
                body == null -> exchange.sendResponseHeaders(404, -1)
                else -> {
                    exchange.sendResponseHeaders(200, body.size.toLong())
                    exchange.responseBody.use { it.write(body) }
                }
            }
            exchange.close()
        }
        server.start()
        manifest = """{"versionCode": 7, "versionName": "1.0.0", "url": "$base/weft.apk", "sha256": "${"0".repeat(64)}"}"""
        try {
            block(base, apk)
        } finally {
            server.stop(0)
        }
    }
}
