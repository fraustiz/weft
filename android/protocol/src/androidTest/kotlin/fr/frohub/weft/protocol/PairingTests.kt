// Groups and pairing: the `pairing` section of the Rust reference test
// vectors (crates/weft-protocol/vectors/v1.json), plus the checks of
// crates/weft-protocol/tests/pairing.rs.

package fr.frohub.weft.protocol

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import java.util.Base64

// region Helpers

private fun toHex(bytes: ByteArray): String = bytes.joinToString("") {
    val v = it.toInt() and 0xff
    "0123456789abcdef"[v ushr 4].toString() + "0123456789abcdef"[v and 0x0f]
}

private fun fromHex(text: String): ByteArray {
    require(text.length % 2 == 0) { "odd hex length: $text" }
    fun nibble(c: Char): Int = when (c) {
        in '0'..'9' -> c - '0'
        in 'a'..'f' -> c - 'a' + 10
        else -> throw IllegalArgumentException("bad hex digit in $text")
    }
    return ByteArray(text.length / 2) { i -> ((nibble(text[2 * i]) shl 4) or nibble(text[2 * i + 1])).toByte() }
}

private fun JSONObject.hexBytes(key: String): ByteArray = fromHex(getString(key))

private fun bytes(count: Int, value: Int): ByteArray = ByteArray(count) { value.toByte() }

private inline fun assertWeftError(expected: WeftError, message: String = "", block: () -> Unit) {
    try {
        block()
    } catch (e: WeftException) {
        assertEquals(message, expected, e.error)
        return
    }
    fail("expected $expected $message")
}

/** Devices A, B (top-level vectors) and C (`pairing.device_c`), rebuilt from their seeds. */
private class PairingVectors {
    val root: JSONObject = run {
        val assets = InstrumentationRegistry.getInstrumentation().context.assets
        JSONObject(assets.open("v1.json").use { it.readBytes().toString(Charsets.UTF_8) })
    }
    val pairing: JSONObject = root.getJSONObject("pairing")
    val a = identity(root.getJSONObject("devices").getJSONObject("a"))
    val b = identity(root.getJSONObject("devices").getJSONObject("b"))
    val c = identity(pairing.getJSONObject("device_c"))

    private fun identity(device: JSONObject): Identity =
        Identity.fromSeeds(device.hexBytes("sign_seed"), device.hexBytes("kx_seed"))

    fun identity(name: String): Identity = when (name) {
        "a" -> a
        "b" -> b
        "c" -> c
        else -> throw IllegalArgumentException("unknown device $name")
    }

    fun keys(local: String, peer: String): SessionKeys {
        val p = identity(peer)
        return sessionKeys(identity(local), p.deviceId, p.kxPublicKey)
    }

    fun peerKeys(name: String): PeerKeys = identity(name).let { PeerKeys(it.signPublicKey, it.kxPublicKey) }

    fun envelope(name: String): JSONObject {
        val list = pairing.getJSONArray("envelopes")
        for (i in 0 until list.length()) {
            if (list.getJSONObject(i).getString("name") == name) return list.getJSONObject(i)
        }
        throw AssertionError("no envelope $name")
    }

    fun code(): PairingCode = PairingCode(
        relayUrl = pairing.getString("relay_url"),
        pairingId = pairing.hexBytes("pairing_id"),
        signPublicKey = a.signPublicKey,
        kxPublicKey = a.kxPublicKey,
        secret = pairing.hexBytes("secret"),
    )
}

/** Two devices with their session keys, for the non-vector checks. */
private class TestPair {
    val a = Identity.fromSeeds(bytes(32, 1), bytes(32, 2))
    val b = Identity.fromSeeds(bytes(32, 3), bytes(32, 4))
    val aKeys = sessionKeys(a, b.deviceId, b.kxPublicKey)
    val bKeys = sessionKeys(b, a.deviceId, a.kxPublicKey)

    fun header(from: Identity, to: Identity, kind: Kind): Header = Header(
        kind = kind, sender = from.deviceId, recipient = to.deviceId,
        messageId = bytes(16, 7), counter = 1u,
    )

    /**
     * A frame with a valid header and [plaintextLength] bytes of garbage
     * where the ciphertext should be: only acceptable if it is rejected
     * before decryption.
     */
    fun garbageFrame(kind: Kind, plaintextLength: Int): ByteArray =
        header(a, b, kind).encode() + ByteArray(Weft.NONCE_LENGTH) + ByteArray(plaintextLength + Weft.TAG_LENGTH)
}

private fun peer(n: Int): PeerKeys = PeerKeys(bytes(32, n), bytes(32, n + 100))

private fun peers(count: Int): List<PeerKeys> = List(count) { peer(it) }

private const val URL = "wss://weft.frohub.eu/v1"

private fun pairingCode(
    relayUrl: String,
    pairingId: ByteArray = bytes(16, 0x11),
    signPublicKey: ByteArray = bytes(32, 0x22),
    secret: ByteArray = bytes(32, 0x44),
): PairingCode = PairingCode(relayUrl, pairingId, signPublicKey, bytes(32, 0x33), secret)

private fun payload(code: PairingCode): ByteArray {
    val url = code.relayUrl.toByteArray(Charsets.UTF_8)
    return byteArrayOf(url.size.toByte()) + url + code.pairingId + code.signPublicKey + code.kxPublicKey + code.secret
}

/** A code built from a raw payload with java.util.Base64, independent of the code under test. */
private fun codeFromPayload(payload: ByteArray): String =
    "WEFT1:" + Base64.getUrlEncoder().withoutPadding().encodeToString(payload)

private fun isBase64UrlChar(c: Char): Boolean =
    c in 'A'..'Z' || c in 'a'..'z' || c in '0'..'9' || c == '-' || c == '_'

private fun pairRequest(secret: ByteArray): PairRequest =
    PairRequest.create(secret, bytes(16, 0x11), bytes(32, 0x22), bytes(32, 0x33))

// endregion

// region Vectors

@RunWith(AndroidJUnit4::class)
class PairingVectorTests {
    @Test
    fun pairingCode() {
        val v = PairingVectors()
        assertEquals("a", v.pairing.getString("shown_by"))
        val code = v.code()

        // The payload, built field by field, then its base64url form.
        val url = v.pairing.getString("relay_url").toByteArray(Charsets.UTF_8)
        val expectedPayload = byteArrayOf(url.size.toByte()) + url + v.pairing.hexBytes("pairing_id") +
            v.a.signPublicKey + v.a.kxPublicKey + v.pairing.hexBytes("secret")
        assertEquals(v.pairing.getString("code_payload"), toHex(expectedPayload))
        assertEquals(v.pairing.getString("code"), codeFromPayload(expectedPayload))

        // Encode.
        val encoded = code.encode()
        assertEquals(v.pairing.getString("code"), encoded)
        assertEquals(
            v.pairing.getString("code_payload"),
            toHex(Sodium.base64UrlDecode(encoded.removePrefix("WEFT1:").toByteArray())!!),
        )

        // Decode, every field.
        val decoded = PairingCode.decode(v.pairing.getString("code"))
        assertEquals(code, decoded)
        assertEquals(v.pairing.getString("relay_url"), decoded.relayUrl)
        assertEquals(v.pairing.getString("pairing_id"), toHex(decoded.pairingId))
        assertArrayEquals(v.a.signPublicKey, decoded.signPublicKey)
        assertArrayEquals(v.a.kxPublicKey, decoded.kxPublicKey)
        assertEquals(v.pairing.getString("secret"), toHex(decoded.secret))
    }

    @Test
    fun pairRequestMac() {
        val v = PairingVectors()
        assertEquals("b", v.pairing.getString("joiner"))
        val pairingId = v.pairing.hexBytes("pairing_id")
        val secret = v.pairing.hexBytes("secret")

        val macInput = "weft-pair-v1".toByteArray(Charsets.US_ASCII) + pairingId + v.b.signPublicKey + v.b.kxPublicKey
        assertEquals(v.pairing.getString("mac_input"), toHex(macInput))
        assertEquals(v.pairing.getString("mac"), toHex(Sodium.genericHash32Keyed(macInput, secret)))

        val mac = pairRequestMac(secret, pairingId, v.b.signPublicKey, v.b.kxPublicKey)
        assertEquals(v.pairing.getString("mac"), toHex(mac))

        val request = PairRequest.create(secret, pairingId, v.b.signPublicKey, v.b.kxPublicKey)
        assertEquals(v.pairing.getString("mac"), toHex(request.mac))
        assertTrue(verifyPairRequest(secret, request))
    }

    @Test
    fun controlFrames() {
        val v = PairingVectors()
        val pairingId = v.pairing.hexBytes("pairing_id")
        val request = PairRequest.create(v.pairing.hexBytes("secret"), pairingId, v.b.signPublicKey, v.b.kxPublicKey)
        val expected = mapOf(
            "pair_open" to ControlFrame.PairOpen(pairingId),
            "pair_request" to ControlFrame.PairRequest(request),
            "group_add" to ControlFrame.GroupAdd(v.b.deviceId),
            "group_remove" to ControlFrame.GroupRemove(v.c.deviceId),
            "error_unknown_pairing" to ControlFrame.Error(ControlError.UNKNOWN_PAIRING),
            "error_not_allowed" to ControlFrame.Error(ControlError.NOT_ALLOWED),
            "error_rate_limited" to ControlFrame.Error(ControlError.RATE_LIMITED),
            "error_group_full" to ControlFrame.Error(ControlError.GROUP_FULL),
        )
        val frames = v.pairing.getJSONObject("control_frames")
        assertEquals(expected.keys, frames.keys().asSequence().toSet())
        for ((name, frame) in expected) {
            assertEquals(name, frames.getString(name), toHex(frame.encode()))
            val bytes = frames.hexBytes(name)
            assertEquals(name, frame, ControlFrame.decode(bytes))
            assertEquals(name, FrameType.CONTROL, classify(bytes))
        }
        // The relayed request still verifies.
        val received = ControlFrame.decode(frames.hexBytes("pair_request")) as ControlFrame.PairRequest
        assertTrue(verifyPairRequest(v.pairing.hexBytes("secret"), received.request))
    }

    @Test
    fun deviceC() {
        val v = PairingVectors()
        val expected = v.pairing.getJSONObject("device_c")
        assertEquals(toHex(ByteArray(32) { (0x80 + it).toByte() }), expected.getString("sign_seed"))
        assertEquals(toHex(ByteArray(32) { (0xa0 + it).toByte() }), expected.getString("kx_seed"))
        assertEquals(expected.getString("sign_pk"), toHex(v.c.signPublicKey))
        assertEquals(expected.getString("kx_pk"), toHex(v.c.kxPublicKey))
        assertEquals(expected.getString("device_id"), toHex(v.c.deviceId))
        assertEquals(expected.getString("device_id"), toHex(deviceIdFromSignPublicKey(expected.hexBytes("sign_pk"))))
        assertEquals(expected.getString("device_id"), toHex(v.peerKeys("c").deviceId))
    }

    @Test
    fun sessionKeysAC() {
        val v = PairingVectors()
        val expected = v.pairing.getJSONObject("session_a_c")
        val client = if (compareUnsigned(v.a.deviceId, v.c.deviceId) < 0) "a" else "c"
        assertEquals(expected.getString("client"), client)
        for ((local, peer) in listOf("a" to "c", "c" to "a")) {
            val keys = v.keys(local, peer)
            assertEquals(expected.getJSONObject(local).getString("rx"), toHex(keys.rx))
            assertEquals(expected.getJSONObject(local).getString("tx"), toHex(keys.tx))
        }
        val ac = v.keys("a", "c")
        val ca = v.keys("c", "a")
        assertArrayEquals(ac.tx, ca.rx)
        assertArrayEquals(ac.rx, ca.tx)
    }

    @Test
    fun welcomeEnvelope() {
        val v = PairingVectors()
        val welcome = encodeWelcome(listOf(v.peerKeys("c")))
        assertArrayEquals(byteArrayOf(1) + v.c.signPublicKey + v.c.kxPublicKey, welcome)
        val plaintext = checkEnvelope(v, "welcome_a_to_b", Kind.WELCOME, welcome)
        assertEquals(listOf(v.peerKeys("c")), decodeWelcome(plaintext))
    }

    @Test
    fun deviceAddedEnvelope() {
        val v = PairingVectors()
        val added = encodeDeviceAdded(v.peerKeys("b"))
        assertArrayEquals(v.b.signPublicKey + v.b.kxPublicKey, added)
        val plaintext = checkEnvelope(v, "device_added_a_to_c", Kind.DEVICE_ADDED, added)
        assertEquals(v.peerKeys("b"), decodeDeviceAdded(plaintext))
    }

    @Test
    fun deviceRemovedEnvelope() {
        val v = PairingVectors()
        val removed = encodeDeviceRemoved(v.c.deviceId)
        val plaintext = checkEnvelope(v, "device_removed_a_to_b", Kind.DEVICE_REMOVED, removed)
        assertArrayEquals(v.c.deviceId, decodeDeviceRemoved(plaintext))
    }

    /** Checks every field of one envelope vector; returns the opened plaintext. */
    private fun checkEnvelope(v: PairingVectors, name: String, kind: Kind, typedPlaintext: ByteArray): ByteArray {
        val e = v.envelope(name)
        assertFalse(e.has("plaintext_utf8"))
        val senderName = e.getString("sender")
        val recipientName = e.getString("recipient")
        val senderKeys = v.keys(senderName, recipientName)
        val recipientKeys = v.keys(recipientName, senderName)

        assertEquals(kind, Kind.fromValue(e.getInt("kind").toUByte()))
        val header = Header(
            version = e.getInt("version").toUByte(),
            kind = kind,
            sender = v.identity(senderName).deviceId,
            recipient = v.identity(recipientName).deviceId,
            messageId = e.hexBytes("message_id"),
            counter = e.getLong("counter").toULong(),
        )
        assertEquals(e.getString("plaintext"), toHex(typedPlaintext))
        assertArrayEquals(e.hexBytes("key"), senderKeys.tx)

        val headerBytes = header.encode()
        assertEquals(e.getString("header"), toHex(headerBytes))
        assertEquals(header, Header.decode(headerBytes))

        val frame = seal(header, typedPlaintext, e.hexBytes("nonce"), senderKeys.tx)
        assertEquals(e.getString("frame"), toHex(frame))
        assertEquals(FrameType.ENVELOPE, classify(frame))

        val (opened, plaintext) = open(e.hexBytes("frame"), recipientKeys.rx)
        assertEquals(header, opened)
        assertArrayEquals(typedPlaintext, plaintext)
        return plaintext
    }

    private fun compareUnsigned(x: ByteArray, y: ByteArray): Int {
        for (i in 0 until minOf(x.size, y.size)) {
            val cmp = (x[i].toInt() and 0xff).compareTo(y[i].toInt() and 0xff)
            if (cmp != 0) return cmp
        }
        return x.size.compareTo(y.size)
    }
}

// endregion

// region Group change plaintexts

@RunWith(AndroidJUnit4::class)
class GroupTests {
    @Test
    fun peerKeysDeviceIdIsDerivedFromSignPublicKey() {
        Identity.generate().use { id ->
            assertArrayEquals(id.deviceId, PeerKeys(id.signPublicKey, id.kxPublicKey).deviceId)
        }
    }

    @Test
    fun welcomeRoundTripForEveryCount() {
        for (count in 0..Weft.MAX_WELCOME_DEVICES) {
            val plaintext = encodeWelcome(peers(count))
            assertEquals(1 + count * 64, plaintext.size)
            assertEquals(count, plaintext[0].toInt())
            assertEquals(peers(count), decodeWelcome(plaintext))
        }
        // sign_pk comes first, then kx_pk.
        val plaintext = encodeWelcome(listOf(peer(5)))
        assertArrayEquals(bytes(32, 5), plaintext.copyOfRange(1, 33))
        assertArrayEquals(bytes(32, 105), plaintext.copyOfRange(33, 65))
    }

    @Test
    fun welcomeRejectsMoreThanEightDevices() {
        assertEquals(8, Weft.MAX_WELCOME_DEVICES)
        assertWeftError(WeftError.TooManyDevices(9)) { encodeWelcome(peers(9)) }
        // Well-formed apart from the count.
        assertWeftError(WeftError.TooManyDevices(9)) { decodeWelcome(byteArrayOf(9) + ByteArray(9 * 64)) }
        assertWeftError(WeftError.TooManyDevices(255)) { decodeWelcome(byteArrayOf(0xff.toByte())) }
    }

    @Test
    fun welcomeDecodeRejectsWrongLengths() {
        assertWeftError(WeftError.InvalidPlaintextLength(0)) { decodeWelcome(ByteArray(0)) }
        val plaintext = encodeWelcome(peers(2))
        for (length in 1 until plaintext.size) {
            assertWeftError(WeftError.InvalidPlaintextLength(length), "truncated to $length") {
                decodeWelcome(plaintext.copyOf(length))
            }
        }
        assertWeftError(WeftError.InvalidPlaintextLength(plaintext.size + 1)) { decodeWelcome(plaintext + 0) }
        // The count must match the number of entries, in both directions.
        for (count in listOf(1, 3)) {
            val wrong = plaintext.copyOf().also { it[0] = count.toByte() }
            assertWeftError(WeftError.InvalidPlaintextLength(plaintext.size)) { decodeWelcome(wrong) }
        }
    }

    @Test
    fun deviceAddedRoundTripAndWrongLengths() {
        val plaintext = encodeDeviceAdded(peer(1))
        assertEquals(64, plaintext.size)
        assertEquals(peer(1), decodeDeviceAdded(plaintext))
        for (length in listOf(0, 1, 32, 63)) {
            assertWeftError(WeftError.InvalidPlaintextLength(length)) { decodeDeviceAdded(plaintext.copyOf(length)) }
        }
        assertWeftError(WeftError.InvalidPlaintextLength(65)) { decodeDeviceAdded(ByteArray(65)) }
    }

    @Test
    fun deviceRemovedRoundTripAndWrongLengths() {
        assertArrayEquals(bytes(16, 6), decodeDeviceRemoved(encodeDeviceRemoved(bytes(16, 6))))
        for (length in listOf(0, 1, 15, 17, 32)) {
            assertWeftError(WeftError.InvalidPlaintextLength(length)) { decodeDeviceRemoved(ByteArray(length)) }
        }
        assertWeftError(WeftError.InvalidLength("deviceId", 16, 15)) { encodeDeviceRemoved(ByteArray(15)) }
    }

    @Test
    fun groupEnvelopesRoundTrip() {
        val p = TestPair()
        for (count in listOf(0, 1, Weft.MAX_WELCOME_DEVICES)) {
            val h = p.header(p.a, p.b, Kind.WELCOME)
            val (opened, plaintext) = open(seal(h, encodeWelcome(peers(count)), p.aKeys.tx), p.bKeys.rx)
            assertEquals(h, opened)
            assertEquals(peers(count), decodeWelcome(plaintext))
        }
        var h = p.header(p.a, p.b, Kind.DEVICE_ADDED)
        var result = open(seal(h, encodeDeviceAdded(peer(3)), p.aKeys.tx), p.bKeys.rx)
        assertEquals(h, result.first)
        assertEquals(peer(3), decodeDeviceAdded(result.second))

        h = p.header(p.b, p.a, Kind.DEVICE_REMOVED)
        result = open(seal(h, encodeDeviceRemoved(bytes(16, 4)), p.bKeys.tx), p.aKeys.rx)
        assertEquals(h, result.first)
        assertArrayEquals(bytes(16, 4), decodeDeviceRemoved(result.second))
    }

    @Test
    fun groupKindsDecodeFromHeaderBytes() {
        val p = TestPair()
        for ((byte, kind) in listOf(3 to Kind.WELCOME, 4 to Kind.DEVICE_ADDED, 5 to Kind.DEVICE_REMOVED)) {
            val bytes = p.header(p.a, p.b, kind).encode()
            assertEquals(byte, bytes[1].toInt())
            assertEquals(kind, Header.decode(bytes).kind)
        }
    }

    @Test
    fun sealRejectsMalformedGroupPlaintexts() {
        val p = TestPair()
        val tx = p.aKeys.tx
        val welcome = p.header(p.a, p.b, Kind.WELCOME)
        assertWeftError(WeftError.InvalidPlaintextLength(0)) { seal(welcome, ByteArray(0), tx) }
        assertWeftError(WeftError.InvalidPlaintextLength(1)) { seal(welcome, byteArrayOf(1), tx) }
        assertWeftError(WeftError.InvalidPlaintextLength(64)) { seal(welcome, ByteArray(64), tx) }
        // Right shape for one entry, but the count says two.
        val wrongCount = encodeWelcome(peers(1)).also { it[0] = 2 }
        assertWeftError(WeftError.InvalidPlaintextLength(65)) { seal(welcome, wrongCount, tx) }
        // Nine entries: one more than allowed, too large from the length alone.
        assertWeftError(WeftError.PlaintextTooLarge(1 + 9 * 64)) { seal(welcome, byteArrayOf(9) + ByteArray(9 * 64), tx) }
        // A valid length whose count is out of range.
        assertWeftError(WeftError.TooManyDevices(255)) { seal(welcome, byteArrayOf(0xff.toByte()), tx) }

        val added = p.header(p.a, p.b, Kind.DEVICE_ADDED)
        assertWeftError(WeftError.InvalidPlaintextLength(63)) { seal(added, ByteArray(63), tx) }
        assertWeftError(WeftError.PlaintextTooLarge(65)) { seal(added, ByteArray(65), tx) }
        assertWeftError(WeftError.InvalidPlaintextLength(0)) { seal(added, ByteArray(0), tx) }

        val removed = p.header(p.a, p.b, Kind.DEVICE_REMOVED)
        assertWeftError(WeftError.InvalidPlaintextLength(15)) { seal(removed, ByteArray(15), tx) }
        assertWeftError(WeftError.PlaintextTooLarge(17)) { seal(removed, ByteArray(17), tx) }
    }

    @Test
    fun openRejectsWrongGroupLengthsBeforeDecrypting() {
        val p = TestPair()
        // Every frame below is garbage: a length error (rather than
        // DecryptionFailed) proves the check ran before the AEAD.
        val cases = listOf(
            Triple(Kind.WELCOME, 0, WeftError.InvalidPlaintextLength(0)),
            Triple(Kind.WELCOME, 2, WeftError.InvalidPlaintextLength(2)),
            Triple(Kind.WELCOME, 64, WeftError.InvalidPlaintextLength(64)),
            Triple(Kind.WELCOME, 66, WeftError.InvalidPlaintextLength(66)),
            Triple(Kind.WELCOME, 512, WeftError.InvalidPlaintextLength(512)),
            Triple(Kind.WELCOME, 514, WeftError.PlaintextTooLarge(514)),
            Triple(Kind.WELCOME, 1 + 9 * 64, WeftError.PlaintextTooLarge(577)),
            Triple(Kind.WELCOME, 100_000, WeftError.PlaintextTooLarge(100_000)),
            Triple(Kind.DEVICE_ADDED, 0, WeftError.InvalidPlaintextLength(0)),
            Triple(Kind.DEVICE_ADDED, 63, WeftError.InvalidPlaintextLength(63)),
            Triple(Kind.DEVICE_ADDED, 65, WeftError.PlaintextTooLarge(65)),
            Triple(Kind.DEVICE_REMOVED, 0, WeftError.InvalidPlaintextLength(0)),
            Triple(Kind.DEVICE_REMOVED, 15, WeftError.InvalidPlaintextLength(15)),
            Triple(Kind.DEVICE_REMOVED, 17, WeftError.PlaintextTooLarge(17)),
        )
        for ((kind, length, expected) in cases) {
            assertWeftError(expected, "$kind with $length bytes") { open(p.garbageFrame(kind, length), p.bKeys.rx) }
        }
        // The right lengths get as far as the AEAD.
        for ((kind, length) in listOf(
            Kind.WELCOME to 1, Kind.WELCOME to 65, Kind.WELCOME to 513,
            Kind.DEVICE_ADDED to 64, Kind.DEVICE_REMOVED to 16,
        )) {
            assertWeftError(WeftError.DecryptionFailed, "$kind with $length bytes") {
                open(p.garbageFrame(kind, length), p.bKeys.rx)
            }
        }
        assertEquals(
            Weft.HEADER_LENGTH + Weft.NONCE_LENGTH + 513 + Weft.TAG_LENGTH,
            p.garbageFrame(Kind.WELCOME, 513).size,
        )
    }

    @Test
    fun groupEnvelopeKindCannotBeSwapped() {
        // The kind is part of the associated data: a Device removed envelope
        // cannot be replayed as another kind, even one with a compatible length.
        val p = TestPair()
        val frame = seal(p.header(p.a, p.b, Kind.DEVICE_REMOVED), encodeDeviceRemoved(bytes(16, 4)), p.aKeys.tx)
        frame[1] = Kind.CLIPBOARD_TEXT.value.toByte()
        assertWeftError(WeftError.DecryptionFailed) { open(frame, p.bKeys.rx) }
    }

    @Test
    fun randomHelpersReturnFreshValues() {
        assertFalse(randomMessageId().contentEquals(randomMessageId()))
        assertFalse(randomPairingId().contentEquals(randomPairingId()))
        assertFalse(randomPairingSecret().contentEquals(randomPairingSecret()))
        assertEquals(16, randomMessageId().size)
        assertEquals(16, randomPairingId().size)
        assertEquals(32, randomPairingSecret().size)
        assertFalse(randomMessageId().contentEquals(ByteArray(16)))
        assertFalse(randomPairingSecret().contentEquals(ByteArray(32)))
    }
}

// endregion

// region Pairing code

@RunWith(AndroidJUnit4::class)
class PairingCodeTests {
    @Test
    fun roundTrip() {
        // Url lengths covering the three base64 tail shapes, and both bounds.
        for (url in listOf(URL, "w", "ws", "wss", "wss://rélais.example/v1", "a".repeat(255))) {
            val code = pairingCode(url)
            val encoded = code.encode()
            assertTrue(encoded.startsWith("WEFT1:"))
            assertTrue("not base64url without padding: $encoded", encoded.substring(6).all(::isBase64UrlChar))
            assertEquals(codeFromPayload(payload(code)), encoded)
            assertEquals(code, PairingCode.decode(encoded))
        }
    }

    @Test
    fun usesTheUrlSafeAlphabet() {
        // 0xfb 0xff… produces '-' and '_' where standard base64 has '+' and '/'.
        val code = pairingCode(URL, pairingId = bytes(16, 0xfb), signPublicKey = bytes(32, 0xff))
        val encoded = code.encode()
        assertTrue(encoded.contains('-') && encoded.contains('_'))
        assertEquals(code, PairingCode.decode(encoded))
        // The same code in the standard alphabet is not accepted.
        val standard = encoded.replace('-', '+').replace('_', '/')
        assertWeftError(WeftError.MalformedPairingCode) { PairingCode.decode(standard) }
    }

    @Test
    fun encodeRejectsBadUrlLengths() {
        assertWeftError(WeftError.RelayUrlLength(0)) { pairingCode("").encode() }
        assertWeftError(WeftError.RelayUrlLength(256)) { pairingCode("a".repeat(256)).encode() }
        // Counted in UTF-8 bytes rather than characters.
        assertWeftError(WeftError.RelayUrlLength(256)) { pairingCode("é".repeat(128)).encode() }
        assertWeftError(WeftError.InvalidUtf8) { pairingCode("wss://\uD800").encode() }
    }

    @Test
    fun decodeRejectsBadPrefix() {
        val body = pairingCode(URL).encode().substring(6)
        for (bad in listOf(body, "WEFT2:$body", "weft1:$body", "WEFT1$body", " WEFT1:$body", "", "WEFT1:", "WEFT1")) {
            assertWeftError(WeftError.MalformedPairingCode, bad) { PairingCode.decode(bad) }
        }
    }

    @Test
    fun decodeRejectsBadBase64() {
        // This url makes the payload length a multiple of 3: no base64 tail.
        val encoded = pairingCode("wss://relay.example/v1").encode()
        assertEquals(0, (encoded.length - 6) % 4)
        PairingCode.decode(encoded)

        // Any character outside the alphabet, at any position.
        for (badChar in listOf('+', '/', '=', ' ', '\n', '\t', '.', '~', '%', '*', '\u0000', 'é', '\uD800')) {
            for (position in listOf(6, encoded.length / 2, encoded.length - 1)) {
                val bad = StringBuilder(encoded).replace(position, position + 1, badChar.toString()).toString()
                assertWeftError(WeftError.MalformedPairingCode, "${badChar.code} at $position") {
                    PairingCode.decode(bad)
                }
            }
        }
        // Trailing garbage and whitespace are not ignored.
        for (suffix in listOf("=", "==", "\n", " ", "A", "AAAA")) {
            try {
                PairingCode.decode(encoded + suffix)
                fail("suffix ${suffix.map { it.code }} accepted")
            } catch (_: WeftException) {
            }
        }
    }

    @Test
    fun decodeRejectsPadding() {
        // Urls of 3 and 2 bytes leave a 2- and 1-byte tail: padded base64
        // would end with "=" or "==".
        for ((url, padding) in listOf("wss" to "=", "ws" to "==")) {
            val encoded = pairingCode(url).encode()
            PairingCode.decode(encoded)
            assertEquals(0, (encoded.length - 6 + padding.length) % 4)
            assertWeftError(WeftError.MalformedPairingCode) { PairingCode.decode(encoded + padding) }
            // java.util.Base64 accepts the padded form: the strictness is ours.
            assertNotNull(Base64.getUrlDecoder().decode(encoded.substring(6) + padding))
        }
    }

    @Test
    fun decodeRejectsNonCanonicalBase64() {
        // With a 1-byte tail the last character only carries 2 bits; the 4
        // others must be zero. A lax decoder would map both strings to the
        // same bytes, so two different strings would be the same code.
        val encoded = pairingCode("ws").encode()
        assertEquals('A', encoded.last())
        val nonCanonical = encoded.dropLast(1) + "B"
        assertWeftError(WeftError.MalformedPairingCode) { PairingCode.decode(nonCanonical) }
        // java.util.Base64 is such a lax decoder.
        assertArrayEquals(
            Base64.getUrlDecoder().decode(encoded.substring(6)),
            Base64.getUrlDecoder().decode(nonCanonical.substring(6)),
        )
        // A single leftover character can never come out of an encoder.
        val whole = pairingCode("wss://relay.example/v1").encode()
        assertWeftError(WeftError.MalformedPairingCode) { PairingCode.decode(whole + "A") }
    }

    @Test
    fun sodiumBase64IsStrict() {
        // Matches java.util.Base64 (url, no padding) on every tail shape.
        for (length in 0..200) {
            val bin = ByteArray(length) { (it * 37 + length).toByte() }
            val b64 = Sodium.base64UrlEncode(bin)
            assertEquals(Base64.getUrlEncoder().withoutPadding().encodeToString(bin), b64)
            assertArrayEquals(bin, Sodium.base64UrlDecode(b64.toByteArray()))
        }
        assertArrayEquals(ByteArray(0), Sodium.base64UrlDecode(ByteArray(0)))
        assertArrayEquals(byteArrayOf(0x41), Sodium.base64UrlDecode("QQ".toByteArray()))
        assertArrayEquals(byteArrayOf(0xfb.toByte(), 0xff.toByte()), Sodium.base64UrlDecode("-_8".toByteArray()))
        for (bad in listOf("QR", "QQ=", "QQ==", "Q", "QUJD\n", " QUJD", "QU JD", "+/8", "QUI=", "QUJ", "QUK")) {
            assertNull(bad, Sodium.base64UrlDecode(bad.toByteArray()))
        }
    }

    @Test
    fun decodeRejectsWrongLengths() {
        val full = payload(pairingCode(URL))
        PairingCode.decode(codeFromPayload(full))
        for (length in 1 until full.size) {
            assertWeftError(WeftError.MalformedPairingCode, "truncated to $length") {
                PairingCode.decode(codeFromPayload(full.copyOf(length)))
            }
        }
        for (extra in 1..4) {
            assertWeftError(WeftError.MalformedPairingCode, "$extra extra bytes") {
                PairingCode.decode(codeFromPayload(full + ByteArray(extra)))
            }
        }
        // url_len that does not match the url actually present.
        for (wrongLength in listOf(URL.length - 1, URL.length + 1, 255)) {
            val bad = full.copyOf().also { it[0] = wrongLength.toByte() }
            assertWeftError(WeftError.MalformedPairingCode) { PairingCode.decode(codeFromPayload(bad)) }
        }
        // Far beyond the largest possible code.
        assertWeftError(WeftError.MalformedPairingCode) { PairingCode.decode("WEFT1:" + "A".repeat(1_000_000)) }
        // Just above the bound, in UTF-8 bytes but not in chars.
        val bound = (1 + 255 + 112 + 2) / 3 * 4
        assertWeftError(WeftError.MalformedPairingCode) { PairingCode.decode("WEFT1:" + "é".repeat(bound / 2 + 1)) }
    }

    @Test
    fun decodeRejectsEmptyUrl() {
        // Well-formed apart from url_len = 0.
        val emptyUrl = byteArrayOf(0) + bytes(112, 0x55)
        assertWeftError(WeftError.RelayUrlLength(0)) { PairingCode.decode(codeFromPayload(emptyUrl)) }
    }

    @Test
    fun decodeRejectsInvalidUtf8Url() {
        val bad = payload(pairingCode(URL)).also { it[3] = 0xff.toByte() }
        assertWeftError(WeftError.InvalidUtf8) { PairingCode.decode(codeFromPayload(bad)) }
    }

    @Test
    fun toStringHidesTheSecret() {
        val text = pairingCode(URL, secret = bytes(32, 0xab)).toString()
        assertTrue(text, text.contains(URL))
        assertTrue(text, text.contains("<redacted>"))
        assertFalse(text, text.contains("abab"))
        assertFalse(text, text.contains("171"))
    }

    @Test
    fun wrongFieldLengthsThrow() {
        assertWeftError(WeftError.InvalidLength("pairingId", 16, 15)) { pairingCode(URL, pairingId = ByteArray(15)) }
        assertWeftError(WeftError.InvalidLength("secret", 32, 33)) { pairingCode(URL, secret = ByteArray(33)) }
    }
}

// endregion

// region Pair request MAC

@RunWith(AndroidJUnit4::class)
class PairRequestTests {
    @Test
    fun macVerifies() {
        val secret = randomPairingSecret()
        val request = pairRequest(secret)
        assertArrayEquals(pairRequestMac(secret, bytes(16, 0x11), bytes(32, 0x22), bytes(32, 0x33)), request.mac)
        assertTrue(verifyPairRequest(secret, request))
        // Still valid after a trip through the relay.
        val received = ControlFrame.decode(ControlFrame.PairRequest(request).encode()) as ControlFrame.PairRequest
        assertTrue(verifyPairRequest(secret, received.request))
    }

    @Test
    fun macRejectsAnyFlippedBit() {
        val secret = bytes(32, 0x44)
        val request = pairRequest(secret)
        for (byte in 0 until 32) {
            for (bit in 0 until 8) {
                val mac = request.mac.also { it[byte] = (it[byte].toInt() xor (1 shl bit)).toByte() }
                assertFalse("bit $bit of byte $byte", verifyPairRequest(secret, request.copy(mac = mac)))
            }
        }
        assertFalse(verifyPairRequest(secret, request.copy(mac = ByteArray(32))))
    }

    @Test
    fun macRejectsWrongSecret() {
        val secret = bytes(32, 0x44)
        val request = pairRequest(secret)
        val oneBitOff = secret.copyOf().also { it[31] = (it[31].toInt() xor 0x80).toByte() }
        assertFalse(verifyPairRequest(oneBitOff, request))
        assertFalse(verifyPairRequest(ByteArray(32), request))
        assertFalse(verifyPairRequest(randomPairingSecret(), request))
        assertWeftError(WeftError.InvalidLength("secret", 32, 31)) { verifyPairRequest(ByteArray(31), request) }
    }

    @Test
    fun macCoversEveryField() {
        val secret = bytes(32, 0x44)
        val request = pairRequest(secret)
        // The relay swapping in its own keys, one at a time.
        assertFalse(verifyPairRequest(secret, request.copy(signPublicKey = bytes(32, 0x99))))
        assertFalse(verifyPairRequest(secret, request.copy(kxPublicKey = bytes(32, 0x99))))
        // sign_pk and kx_pk exchanged.
        val swapped = request.copy(signPublicKey = request.kxPublicKey, kxPublicKey = request.signPublicKey)
        assertFalse(verifyPairRequest(secret, swapped))
        assertFalse(
            pairRequestMac(secret, bytes(16, 0x11), bytes(32, 0x22), bytes(32, 0x33))
                .contentEquals(pairRequestMac(secret, bytes(16, 0x11), bytes(32, 0x33), bytes(32, 0x22))),
        )
        assertFalse(verifyPairRequest(secret, request.copy(pairingId = bytes(16, 0x12))))
    }
}

// endregion

// region Control frames, dispatch and host check

@RunWith(AndroidJUnit4::class)
class ControlFrameTests {
    private fun controlFrames(): List<Triple<ControlFrame, Int, Int>> = listOf(
        Triple(ControlFrame.PairOpen(bytes(16, 1)), 0xB0, 17),
        Triple(ControlFrame.PairRequest(pairRequest(bytes(32, 0x44))), 0xB1, 113),
        Triple(ControlFrame.GroupAdd(bytes(16, 2)), 0xB2, 17),
        Triple(ControlFrame.GroupRemove(bytes(16, 3)), 0xB3, 17),
        Triple(ControlFrame.Error(ControlError.UNKNOWN_PAIRING), 0xBE, 2),
        Triple(ControlFrame.Error(ControlError.NOT_ALLOWED), 0xBE, 2),
        Triple(ControlFrame.Error(ControlError.RATE_LIMITED), 0xBE, 2),
        Triple(ControlFrame.Error(ControlError.GROUP_FULL), 0xBE, 2),
    )

    @Test
    fun roundTrip() {
        for ((frame, tag, length) in controlFrames()) {
            val bytes = frame.encode()
            assertEquals("$frame", tag, bytes[0].toInt() and 0xff)
            assertEquals("$frame", length, bytes.size)
            assertEquals(frame, ControlFrame.decode(bytes))
            assertEquals(FrameType.CONTROL, classify(bytes))
        }
    }

    @Test
    fun layouts() {
        assertArrayEquals(byteArrayOf(0xB0.toByte()) + bytes(16, 1), ControlFrame.PairOpen(bytes(16, 1)).encode())
        val request = PairRequest(bytes(16, 1), bytes(32, 2), bytes(32, 3), bytes(32, 4))
        assertArrayEquals(
            byteArrayOf(0xB1.toByte()) + bytes(16, 1) + bytes(32, 2) + bytes(32, 3) + bytes(32, 4),
            ControlFrame.PairRequest(request).encode(),
        )
        for ((code, byte) in listOf(
            ControlError.UNKNOWN_PAIRING to 1, ControlError.NOT_ALLOWED to 2,
            ControlError.RATE_LIMITED to 3, ControlError.GROUP_FULL to 4,
        )) {
            assertArrayEquals(byteArrayOf(0xBE.toByte(), byte.toByte()), ControlFrame.Error(code).encode())
        }
    }

    @Test
    fun decodeRejectsWrongLengths() {
        assertWeftError(WeftError.MalformedControlFrame) { ControlFrame.decode(ByteArray(0)) }
        for ((frame, _, _) in controlFrames()) {
            val bytes = frame.encode()
            for (length in 1 until bytes.size) {
                assertWeftError(WeftError.MalformedControlFrame, "$frame truncated to $length") {
                    ControlFrame.decode(bytes.copyOf(length))
                }
            }
            assertWeftError(WeftError.MalformedControlFrame, "$frame with an extra byte") {
                ControlFrame.decode(bytes + 0)
            }
        }
    }

    @Test
    fun decodeRejectsUnknownControlBytes() {
        for (tag in 0xB4..0xBF) {
            if (tag == 0xBE) continue
            // Whatever the body length.
            for (bodyLength in listOf(0, 1, 16, 112)) {
                assertWeftError(WeftError.UnknownControlFrame(tag.toUByte())) {
                    ControlFrame.decode(byteArrayOf(tag.toByte()) + ByteArray(bodyLength))
                }
            }
        }
    }

    @Test
    fun decodeRejectsOtherFrameTypes() {
        for (tag in listOf(0x00, 0x01, 0xA0, 0xA1, 0xA2, 0xAF, 0xC0, 0xFF)) {
            assertWeftError(WeftError.UnexpectedFrameType(tag.toUByte())) {
                ControlFrame.decode(byteArrayOf(tag.toByte()) + ByteArray(16))
            }
        }
    }

    @Test
    fun decodeRejectsUnknownErrorCodes() {
        for (code in (0x00..0xFF).filter { it !in 0x01..0x04 }) {
            assertWeftError(WeftError.UnknownControlError(code.toUByte())) {
                ControlFrame.decode(byteArrayOf(0xBE.toByte(), code.toByte()))
            }
        }
    }

    @Test
    fun classifyByFirstByte() {
        assertEquals(FrameType.UNKNOWN, classify(ByteArray(0)))
        for (byte in 0x00..0xFF) {
            val expected = when (byte) {
                0x01 -> FrameType.ENVELOPE
                in 0xB0..0xBF -> FrameType.CONTROL
                else -> FrameType.UNKNOWN
            }
            assertEquals("$byte", expected, classify(byteArrayOf(byte.toByte())))
            assertEquals("$byte", expected, classify(byteArrayOf(byte.toByte(), 0, 0)))
        }
    }

    @Test
    fun classifyRealFrames() {
        val p = TestPair()
        assertEquals(FrameType.ENVELOPE, classify(seal(p.header(p.a, p.b, Kind.PING), ByteArray(0), p.aKeys.tx)))
        // Authentication frames are not expected after the handshake.
        assertEquals(FrameType.UNKNOWN, classify(Challenge("weft.frohub.eu", bytes(32, 9)).encode()))
        assertEquals(FrameType.UNKNOWN, classify(byteArrayOf(Weft.AUTH_ACCEPTED.toByte())))
    }

    @Test
    fun checkRelayNameRequiresAnExactMatch() {
        val challenge = Challenge("weft.frohub.eu", bytes(32, 9))
        checkRelayName(challenge, "weft.frohub.eu")
        for (other in listOf(
            "", "evil.example", "weft.frohub.eu:443", "weft.frohub.eu.", "weft.frohub.eu.evil.example",
            "evil.weft.frohub.eu", "WEFT.frohub.eu", " weft.frohub.eu", "weft.frohub.e", "wss://weft.frohub.eu",
        )) {
            assertWeftError(WeftError.RelayNameMismatch, other) { checkRelayName(challenge, other) }
        }
        assertWeftError(WeftError.RelayNameMismatch) { checkRelayName(Challenge("", bytes(32, 9)), "weft.frohub.eu") }
    }

    @Test
    fun controlFrameEqualityIsByValue() {
        assertEquals(ControlFrame.GroupAdd(bytes(16, 2)), ControlFrame.GroupAdd(bytes(16, 2)))
        assertNotEquals(ControlFrame.GroupAdd(bytes(16, 2)), ControlFrame.GroupRemove(bytes(16, 2)))
        assertWeftError(WeftError.InvalidLength("deviceId", 16, 17)) { ControlFrame.GroupAdd(ByteArray(17)) }
    }
}

// endregion
