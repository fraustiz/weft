// Interoperability checks against the Rust reference test vectors
// (crates/weft-protocol/vectors/v1.json, packaged as an androidTest asset),
// plus a few negative checks.

package fr.frohub.weft.protocol

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

// region Vector loading

private fun hex(bytes: ByteArray): String = bytes.joinToString("") {
    val v = it.toInt() and 0xff
    "0123456789abcdef"[v ushr 4].toString() + "0123456789abcdef"[v and 0x0f]
}

private fun unhex(text: String): ByteArray {
    require(text.length % 2 == 0) { "odd hex length: $text" }
    fun nibble(c: Char): Int = when (c) {
        in '0'..'9' -> c - '0'
        in 'a'..'f' -> c - 'a' + 10
        else -> throw IllegalArgumentException("bad hex digit in $text")
    }
    return ByteArray(text.length / 2) { i -> ((nibble(text[2 * i]) shl 4) or nibble(text[2 * i + 1])).toByte() }
}

private fun loadVectors(): JSONObject {
    val assets = InstrumentationRegistry.getInstrumentation().context.assets
    val text = assets.open("v1.json").use { it.readBytes().toString(Charsets.UTF_8) }
    return JSONObject(text)
}

private fun JSONObject.bytes(key: String): ByteArray = unhex(getString(key))

/** Both devices, rebuilt from the seeds in the vectors. */
private class Devices {
    val vectors = loadVectors()
    val a: Identity
    val b: Identity

    init {
        val devices = vectors.getJSONObject("devices")
        val da = devices.getJSONObject("a")
        val db = devices.getJSONObject("b")
        a = Identity.fromSeeds(da.bytes("sign_seed"), da.bytes("kx_seed"))
        b = Identity.fromSeeds(db.bytes("sign_seed"), db.bytes("kx_seed"))
    }

    fun identity(name: String): Identity = when (name) {
        "a" -> a
        "b" -> b
        else -> throw IllegalArgumentException("unknown device $name")
    }

    fun keys(name: String): SessionKeys {
        val local = identity(name)
        val peer = if (name == "a") b else a
        return sessionKeys(local, peer.deviceId, peer.kxPublicKey)
    }

    fun envelope(index: Int): JSONObject = vectors.getJSONArray("envelopes").getJSONObject(index)
}

private inline fun expectError(expected: WeftError, block: () -> Unit) {
    try {
        block()
    } catch (e: WeftException) {
        assertEquals(expected, e.error)
        return
    }
    fail("expected $expected")
}

private inline fun expectAnyError(block: () -> Unit) {
    try {
        block()
    } catch (_: WeftException) {
        return
    }
    fail("expected a WeftException")
}

// endregion

// region Positive checks (every vector field)

@RunWith(AndroidJUnit4::class)
class VectorTests {
    @Test
    fun protocolVersion() {
        assertEquals(Weft.VERSION.toInt(), loadVectors().getInt("protocol_version"))
    }

    @Test
    fun deviceKeys() {
        val d = Devices()
        for (name in listOf("a", "b")) {
            val expected = d.vectors.getJSONObject("devices").getJSONObject(name)
            val identity = d.identity(name)
            assertEquals(expected.getString("sign_pk"), hex(identity.signPublicKey))
            assertEquals(expected.getString("kx_pk"), hex(identity.kxPublicKey))
            assertEquals(expected.getString("device_id"), hex(identity.deviceId))
            assertEquals(expected.getString("device_id"), hex(deviceIdFromSignPublicKey(expected.bytes("sign_pk"))))
        }
    }

    @Test
    fun clientRole() {
        val d = Devices()
        val client = if (lexicographicallyPrecedes(d.a.deviceId, d.b.deviceId)) "a" else "b"
        assertEquals(d.vectors.getJSONObject("session").getString("client"), client)
    }

    @Test
    fun sessionKeysMatch() {
        val d = Devices()
        for (name in listOf("a", "b")) {
            val expected = d.vectors.getJSONObject("session").getJSONObject(name)
            val keys = d.keys(name)
            assertEquals(expected.getString("rx"), hex(keys.rx))
            assertEquals(expected.getString("tx"), hex(keys.tx))
        }
    }

    @Test
    fun sessionKeysAreCrossed() {
        val d = Devices()
        val a = d.keys("a")
        val b = d.keys("b")
        assertArrayEquals(a.tx, b.rx)
        assertArrayEquals(a.rx, b.tx)
    }

    @Test
    fun envelopeClipboardText() = checkEnvelope(0)

    @Test
    fun envelopePing() = checkEnvelope(1)

    @Test
    fun packetVector() {
        val d = Devices()
        val section = d.vectors.getJSONObject("features").getJSONObject("packet")
        assertEquals(Weft.MAX_PACKET_LENGTH, section.getInt("max_len"))
        val v = section.getJSONObject("envelope")
        assertEquals("packet_a_to_b", v.getString("name"))
        assertEquals(Kind.PACKET.value.toInt(), v.getInt("kind"))
        val json = section.getString("json")
        assertEquals(json, v.getString("plaintext_utf8"))
        assertArrayEquals(json.toByteArray(Charsets.UTF_8), checkEnvelope(d, v))
    }

    @Test
    fun fileChunkVector() {
        val d = Devices()
        val section = d.vectors.getJSONObject("features").getJSONObject("file_chunk")
        assertEquals(Weft.MAX_FILE_CHUNK_DATA_LENGTH, section.getInt("max_data_len"))
        val v = section.getJSONObject("envelope")
        assertEquals("file_chunk_b_to_a", v.getString("name"))
        assertEquals(Kind.FILE_CHUNK.value.toInt(), v.getInt("kind"))
        val plaintext = checkEnvelope(d, v)
        val expected = FileChunk(section.bytes("transfer_id"), section.getLong("offset").toULong(), section.bytes("data"))
        assertEquals(expected, decodeFileChunk(plaintext))
        assertArrayEquals(plaintext, encodeFileChunk(expected.transferId, expected.offset, expected.data))
    }

    private fun checkEnvelope(index: Int) {
        val d = Devices()
        checkEnvelope(d, d.envelope(index))
    }

    /** Checks every field of one envelope vector [v]; returns its plaintext. */
    private fun checkEnvelope(d: Devices, v: JSONObject): ByteArray {
        val senderName = v.getString("sender")
        val recipientName = v.getString("recipient")
        val sender = d.identity(senderName)
        val recipient = d.identity(recipientName)
        val senderKeys = d.keys(senderName)
        val recipientKeys = d.keys(recipientName)

        val kind = Kind.fromValue(v.getInt("kind").toUByte()) ?: throw AssertionError("unknown kind")
        val header = Header(
            version = v.getInt("version").toUByte(),
            kind = kind,
            sender = sender.deviceId,
            recipient = recipient.deviceId,
            messageId = v.bytes("message_id"),
            counter = v.getLong("counter").toULong(),
        )
        val plaintext = v.bytes("plaintext")

        // Inputs are consistent with the rest of the document.
        if (v.has("plaintext_utf8")) {
            assertArrayEquals(plaintext, v.getString("plaintext_utf8").toByteArray(Charsets.UTF_8))
        }
        assertArrayEquals(v.bytes("key"), senderKeys.tx)

        // Header bytes and round trip.
        val headerBytes = header.encode()
        assertEquals(Weft.HEADER_LENGTH, headerBytes.size)
        assertEquals(v.getString("header"), hex(headerBytes))
        assertEquals(header, Header.decode(headerBytes))

        // Full frame from seal with the fixed nonce.
        val frame = seal(header, plaintext, v.bytes("nonce"), senderKeys.tx)
        assertEquals(v.getString("frame"), hex(frame))

        // Open the committed frame.
        val (opened, openedPlaintext) = open(v.bytes("frame"), recipientKeys.rx)
        assertEquals(header, opened)
        assertArrayEquals(plaintext, openedPlaintext)
        return openedPlaintext
    }

    @Test
    fun randomNonceSealRoundTrip() {
        val d = Devices()
        val header = Header(
            kind = Kind.CLIPBOARD_TEXT, sender = d.a.deviceId, recipient = d.b.deviceId,
            messageId = ByteArray(16) { 7 }, counter = 42u,
        )
        val text = "round trip".toByteArray(Charsets.UTF_8)
        val first = seal(header, text, d.keys("a").tx)
        val second = seal(header, text, d.keys("a").tx)
        assertFalse(first.contentEquals(second))
        val (_, plaintext) = open(first, d.keys("b").rx)
        assertArrayEquals(text, plaintext)
    }

    @Test
    fun auth() {
        val d = Devices()
        val v = d.vectors.getJSONObject("auth")
        assertEquals("a", v.getString("device"))

        val challenge = Challenge(v.getString("relay_name"), v.bytes("nonce"))
        assertEquals(v.getString("challenge_frame"), hex(challenge.encode()))
        assertEquals(challenge, Challenge.decode(v.bytes("challenge_frame")))
        assertEquals(v.getString("signed_message"), hex(challenge.signedMessage))

        val response = signAuth(d.a, challenge)
        assertEquals(v.getString("signature"), hex(response.signature))
        assertEquals(v.getString("response_frame"), hex(response.encode()))
        assertEquals(response, AuthResponse.decode(v.bytes("response_frame")))

        assertArrayEquals(d.a.deviceId, verifyAuth(challenge, response))
        assertEquals(v.getString("accepted_frame"), hex(byteArrayOf(Weft.AUTH_ACCEPTED.toByte())))
    }

    private fun lexicographicallyPrecedes(x: ByteArray, y: ByteArray): Boolean {
        for (i in 0 until minOf(x.size, y.size)) {
            val cmp = (x[i].toInt() and 0xff).compareTo(y[i].toInt() and 0xff)
            if (cmp != 0) return cmp < 0
        }
        return x.size < y.size
    }

    /** weft-lan-v3, as in crates/weft-protocol/tests/vectors.rs (committed_lan_vectors_verify_at_both_ends). */
    @Test
    fun lanHandshakeAndReceipts() {
        val d = Devices()
        val lan = d.vectors.getJSONObject("lan")
        val aNonce = lan.bytes("a_nonce")
        val bNonce = lan.bytes("b_nonce")
        assertArrayEquals(aNonce, Lan.nonce(lan.bytes("a_challenge_frame")))
        assertArrayEquals(bNonce, Lan.nonce(lan.bytes("b_challenge_frame")))
        // Ed25519 is deterministic: both answers match byte for byte and verify at the other end.
        assertEquals(lan.getString("a_auth_frame"), hex(Lan.authenticate(d.a, bNonce, aNonce)))
        assertEquals(lan.getString("b_auth_frame"), hex(Lan.authenticate(d.b, aNonce, bNonce)))
        assertTrue(Lan.verify(lan.bytes("a_auth_frame"), d.a.signPublicKey, bNonce, aNonce))
        assertTrue(Lan.verify(lan.bytes("b_auth_frame"), d.b.signPublicKey, aNonce, bNonce))
        assertFalse(Lan.verify(lan.bytes("a_auth_frame"), d.a.signPublicKey, aNonce, bNonce))

        val atA = Lan.receiptKeys(d.a, d.b.deviceId, d.b.kxPublicKey, aNonce, bNonce)
        val atB = Lan.receiptKeys(d.b, d.a.deviceId, d.a.kxPublicKey, bNonce, aNonce)
        val receipts = lan.getJSONObject("receipts")
        for ((name, verifier, signer) in listOf(
            Triple("a_to_b", atA, atB),
            Triple("b_to_a", atB, atA),
        )) {
            val receipt = receipts.getJSONObject(name)
            assertEquals(name, receipt.getString("key"), hex(verifier.outgoing.bytes))
            assertEquals(name, receipt.getString("key"), hex(signer.incoming.bytes))
            val id = receipt.bytes("message_id")
            val mac = receipt.bytes("mac")
            assertEquals(name, receipt.getString("mac"), hex(signer.mac(id)))
            assertTrue(name, verifier.verify(id, mac))
            assertFalse(name, signer.verify(id, mac))
            assertEquals(name, receipt.getString("record"), hex(ByteArray(4) + id + mac))
        }
    }
}

// endregion

// region Negative checks

@RunWith(AndroidJUnit4::class)
class NegativeTests {
    @Test
    fun tamperedHeaderByteFailsToOpen() {
        val d = Devices()
        for (offset in listOf(0, 1, 2, 20, 40, 57)) {
            val frame = d.envelope(0).bytes("frame")
            frame[offset] = (frame[offset].toInt() xor 0x01).toByte()
            expectAnyError { open(frame, d.keys("b").rx) }
        }
    }

    @Test
    fun tamperedSenderIsDecryptionFailure() {
        val d = Devices()
        val frame = d.envelope(0).bytes("frame")
        frame[5] = (frame[5].toInt() xor 0x80).toByte()
        expectError(WeftError.DecryptionFailed) { open(frame, d.keys("b").rx) }
    }

    @Test
    fun tamperedVersionAndKindAreRejected() {
        val d = Devices()
        val original = d.envelope(0).bytes("frame")
        var frame = original.copyOf()
        frame[0] = 0x02
        expectError(WeftError.UnsupportedVersion(0x02u)) { open(frame, d.keys("b").rx) }
        frame = original.copyOf()
        frame[1] = 0x7f
        expectError(WeftError.UnknownKind(0x7fu)) { open(frame, d.keys("b").rx) }
        // A clipboard frame relabelled as a ping is rejected before decrypting.
        frame = original.copyOf()
        frame[1] = Kind.PING.value.toByte()
        expectError(WeftError.PingNotEmpty) { open(frame, d.keys("b").rx) }
    }

    @Test
    fun highBitHeaderBytesAreReportedUnsigned() {
        val d = Devices()
        val frame = d.envelope(0).bytes("frame")
        frame[0] = 0xFF.toByte()
        expectError(WeftError.UnsupportedVersion(0xFFu)) { open(frame, d.keys("b").rx) }
        frame[0] = 0x01
        frame[1] = 0x81.toByte()
        expectError(WeftError.UnknownKind(0x81u)) { open(frame, d.keys("b").rx) }
    }

    @Test
    fun wrongKeyFailsToOpen() {
        val d = Devices()
        val frame = d.envelope(0).bytes("frame")
        expectError(WeftError.DecryptionFailed) { open(frame, d.keys("a").rx) }
    }

    @Test
    fun shortAndEmptyFramesAreRejected() {
        val d = Devices()
        val frame = d.envelope(1).bytes("frame")
        expectError(WeftError.FrameTooShort(97)) { open(frame.copyOf(97), d.keys("a").rx) }
        expectError(WeftError.FrameTooShort(0)) { open(ByteArray(0), d.keys("a").rx) }
        expectError(WeftError.HeaderLength(57)) { Header.decode(ByteArray(57)) }
    }

    @Test
    fun wrongRelayNameFailsVerifyAuth() {
        val d = Devices()
        val v = d.vectors.getJSONObject("auth")
        val challenge = Challenge(v.getString("relay_name"), v.bytes("nonce"))
        val response = signAuth(d.a, challenge)
        val other = Challenge("evil.example.org", v.bytes("nonce"))
        expectError(WeftError.InvalidSignature) { verifyAuth(other, response) }
    }

    @Test
    fun wrongNonceFailsVerifyAuth() {
        val d = Devices()
        val v = d.vectors.getJSONObject("auth")
        val challenge = Challenge(v.getString("relay_name"), v.bytes("nonce"))
        val response = signAuth(d.a, challenge)
        val other = Challenge(v.getString("relay_name"), ByteArray(32))
        expectError(WeftError.InvalidSignature) { verifyAuth(other, response) }
    }

    @Test
    fun authFramesNeedExactLength() {
        val v = loadVectors().getJSONObject("auth")
        val challenge = v.bytes("challenge_frame")
        val response = v.bytes("response_frame")
        expectError(WeftError.MalformedAuthFrame) { Challenge.decode(challenge + 0) }
        expectError(WeftError.MalformedAuthFrame) { Challenge.decode(challenge.copyOf(challenge.size - 1)) }
        expectError(WeftError.MalformedAuthFrame) { Challenge.decode(ByteArray(0)) }
        expectError(WeftError.MalformedAuthFrame) { Challenge.decode(byteArrayOf(0xA0.toByte())) }
        expectError(WeftError.UnexpectedFrameType(0xA1u)) { Challenge.decode(response) }
        expectError(WeftError.MalformedAuthFrame) { AuthResponse.decode(response + 0) }
        expectError(WeftError.MalformedAuthFrame) { AuthResponse.decode(response.copyOf(response.size - 1)) }
        expectError(WeftError.UnexpectedFrameType(0xA0u)) { AuthResponse.decode(challenge) }
    }

    @Test
    fun invalidUtf8RelayNameIsRejected() {
        val frame = byteArrayOf(0xA0.toByte(), 2, 0xC3.toByte(), 0x28) + ByteArray(32)
        expectError(WeftError.InvalidUtf8) { Challenge.decode(frame) }
    }

    @Test
    fun relayNameTooLong() {
        expectError(WeftError.RelayNameTooLong) { Challenge("x".repeat(256), ByteArray(32)) }
        // 255 bytes is the limit, counted in UTF-8 bytes rather than characters.
        Challenge("x".repeat(255), ByteArray(32))
        expectError(WeftError.RelayNameTooLong) { Challenge("é".repeat(128), ByteArray(32)) }
    }

    @Test
    fun sealValidatesPlaintext() {
        val d = Devices()
        val tx = d.keys("a").tx
        val nonce = ByteArray(24)
        val text = Header(
            kind = Kind.CLIPBOARD_TEXT, sender = d.a.deviceId, recipient = d.b.deviceId,
            messageId = ByteArray(16), counter = 1u,
        )
        val ping = Header(
            kind = Kind.PING, sender = d.a.deviceId, recipient = d.b.deviceId,
            messageId = ByteArray(16), counter = 1u,
        )
        expectError(WeftError.InvalidUtf8) { seal(text, byteArrayOf(0xFF.toByte()), nonce, tx) }
        expectError(WeftError.PlaintextTooLarge(256 * 1024 + 1)) {
            seal(text, ByteArray(256 * 1024 + 1) { 0x61 }, nonce, tx)
        }
        seal(text, ByteArray(256 * 1024) { 0x61 }, nonce, tx)
        expectError(WeftError.PingNotEmpty) { seal(ping, byteArrayOf(0), nonce, tx) }
        val badVersion = text.copy(version = 2u)
        expectError(WeftError.UnsupportedVersion(2u)) { seal(badVersion, ByteArray(0), nonce, tx) }
    }

    @Test
    fun strictUtf8RejectsSurrogatesAndOverlongs() {
        val d = Devices()
        val tx = d.keys("a").tx
        val text = Header(
            kind = Kind.CLIPBOARD_TEXT, sender = d.a.deviceId, recipient = d.b.deviceId,
            messageId = ByteArray(16), counter = 1u,
        )
        for (bad in listOf("eda080", "c0af", "f4908080", "e282")) {
            expectError(WeftError.InvalidUtf8) { seal(text, unhex(bad), ByteArray(24), tx) }
        }
    }

    @Test
    fun sameDeviceIdIsRejected() {
        val d = Devices()
        expectError(WeftError.SameDeviceId) { sessionKeys(d.a, d.a.deviceId, d.a.kxPublicKey) }
    }

    @Test
    fun wrongInputLengthsThrow() {
        expectAnyError { Identity.fromSeeds(ByteArray(31), ByteArray(32)) }
        expectAnyError { deviceIdFromSignPublicKey(ByteArray(33)) }
        expectAnyError { open(ByteArray(200), ByteArray(31)) }
    }

    @Test
    fun generatedIdentitiesDiffer() {
        Identity.generate().use { x ->
            Identity.generate().use { y ->
                assertFalse(x.deviceId.contentEquals(y.deviceId))
                assertArrayEquals(x.deviceId, deviceIdFromSignPublicKey(x.signPublicKey))
                val xy = sessionKeys(x, y.deviceId, y.kxPublicKey)
                val yx = sessionKeys(y, x.deviceId, x.kxPublicKey)
                assertTrue(xy.tx.contentEquals(yx.rx) && xy.rx.contentEquals(yx.tx))
            }
        }
    }

    @Test
    fun closedIdentityCannotSign() {
        val d = Devices()
        val v = d.vectors.getJSONObject("auth")
        val challenge = Challenge(v.getString("relay_name"), v.bytes("nonce"))
        d.a.close()
        try {
            signAuth(d.a, challenge)
            fail("expected IllegalStateException")
        } catch (_: IllegalStateException) {
        }
        assertNotEquals(0, d.a.deviceId.size)
    }
}

// endregion
