// Device name envelopes (kind 0x06): the device_name section of the Rust reference vectors,
// and the length, UTF-8 and control character rules of crates/weft-protocol/tests/device_name.rs,
// in encodeDeviceName / decodeDeviceName and in seal / open.

package fr.frohub.weft.protocol

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

private fun hexOf(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it.toInt() and 0xff) }

private fun bytesOf(hex: String): ByteArray = ByteArray(hex.length / 2) { hex.substring(2 * it, 2 * it + 2).toInt(16).toByte() }

private fun vectors(): JSONObject {
    val assets = InstrumentationRegistry.getInstrumentation().context.assets
    return JSONObject(assets.open("v1.json").use { it.readBytes().toString(Charsets.UTF_8) })
}

private fun errorOf(block: () -> Unit): WeftError? = try {
    block()
    null
} catch (e: WeftException) {
    e.error
}

@RunWith(AndroidJUnit4::class)
class DeviceNameTests {
    private val a = Identity.fromSeeds(ByteArray(32) { 1 }, ByteArray(32) { 2 })
    private val b = Identity.fromSeeds(ByteArray(32) { 3 }, ByteArray(32) { 4 })
    private val aKeys = sessionKeys(a, b.deviceId, b.kxPublicKey)
    private val bKeys = sessionKeys(b, a.deviceId, a.kxPublicKey)

    private fun header(kind: Kind = Kind.DEVICE_NAME) = Header(
        kind = kind,
        sender = a.deviceId,
        recipient = b.deviceId,
        messageId = ByteArray(16) { 7 },
        counter = 1u,
    )

    /** Checks a name through encode, decode, seal and (when sealing works) open. */
    private fun check(name: ByteArray, expected: WeftError?) {
        val sealError = errorOf { seal(header(), name, aKeys.tx) }
        val decodeError = errorOf { decodeDeviceName(name) }
        val text = decodeUtf8(name)
        val label = hexOf(name)
        if (expected == null) {
            assertArrayEquals(label, name, encodeDeviceName(text!!))
            assertEquals(label, text, decodeDeviceName(name))
            val (opened, plaintext) = open(seal(header(), name, aKeys.tx), bKeys.rx)
            assertEquals(Kind.DEVICE_NAME, opened.kind)
            assertArrayEquals(name, plaintext)
        } else {
            if (text != null) assertEquals(label, expected, errorOf { encodeDeviceName(text) })
            assertEquals(label, expected, decodeError)
            assertEquals(label, expected, sealError)
        }
    }

    private fun check(name: String, expected: WeftError?) = check(name.toByteArray(Charsets.UTF_8), expected)

    @Test
    fun vector() {
        val doc = vectors()
        val section = doc.getJSONObject("device_name")
        val v = section.getJSONObject("envelope")
        val name = section.getString("name")
        assertEquals("Téléphone de test", name)
        assertEquals(Weft.MAX_DEVICE_NAME_LENGTH, section.getInt("max_len"))
        assertEquals("device_name_b_to_a", v.getString("name"))
        assertEquals(Kind.DEVICE_NAME.value.toInt(), v.getInt("kind"))
        assertEquals(name, v.getString("plaintext_utf8"))
        assertEquals(v.getString("plaintext"), hexOf(encodeDeviceName(name)))

        val devices = doc.getJSONObject("devices")
        fun identity(key: String) = devices.getJSONObject(key).let {
            Identity.fromSeeds(bytesOf(it.getString("sign_seed")), bytesOf(it.getString("kx_seed")))
        }
        val sender = identity(v.getString("sender"))
        val recipient = identity(v.getString("recipient"))
        val senderKeys = sessionKeys(sender, recipient.deviceId, recipient.kxPublicKey)
        val recipientKeys = sessionKeys(recipient, sender.deviceId, sender.kxPublicKey)
        assertEquals(v.getString("key"), hexOf(senderKeys.tx))

        val header = Header(
            version = v.getInt("version").toUByte(),
            kind = Kind.fromValue(v.getInt("kind").toUByte())!!,
            sender = sender.deviceId,
            recipient = recipient.deviceId,
            messageId = bytesOf(v.getString("message_id")),
            counter = v.getLong("counter").toULong(),
        )
        assertEquals(v.getString("header"), hexOf(header.encode()))

        // Seal with the fixed nonce gives the committed frame.
        val frame = seal(header, encodeDeviceName(name), bytesOf(v.getString("nonce")), senderKeys.tx)
        assertEquals(v.getString("frame"), hexOf(frame))

        // The committed frame opens and decodes.
        val (opened, plaintext) = open(bytesOf(v.getString("frame")), recipientKeys.rx)
        assertEquals(header, opened)
        assertEquals("Téléphone de test", decodeDeviceName(plaintext))
    }

    @Test
    fun kindByteIs0x06() {
        val bytes = header().encode()
        assertEquals(0x06, bytes[1].toInt())
        assertEquals(Kind.DEVICE_NAME, Header.decode(bytes).kind)
    }

    @Test
    fun validNames() {
        assertEquals(64, Weft.MAX_DEVICE_NAME_LENGTH)
        check("x", null)
        check("Téléphone de test", null)
        check("MacBook Air ✂", null)
        check("a".repeat(64), null)
        // 32 two-byte characters: exactly 64 bytes.
        check("é".repeat(32), null)
        check("  spaces are fine  ", null)
    }

    @Test
    fun lengthIs1To64Bytes() {
        check("", WeftError.DeviceNameLength(0))
        check("a".repeat(65), WeftError.DeviceNameLength(65))
        // 64 bytes of 'a' and one 2-byte character: 66 bytes, 65 characters.
        check("a".repeat(64) + "é", WeftError.DeviceNameLength(66))
        // 33 two-byte characters: 66 bytes.
        check("é".repeat(33), WeftError.DeviceNameLength(66))
        check("a".repeat(1000), WeftError.DeviceNameLength(1000))
    }

    @Test
    fun controlCharactersAreRejectedAtEachBoundary() {
        for (c in listOf('\u0000', '\u0001', '\n', '\u001f', '\u007f', '\u0080', '\u0085', '\u009f')) {
            for (name in listOf("ab${c}cd", "$c", "${c}x", "x$c")) {
                check(name, WeftError.DeviceNameControlCharacter)
            }
        }
        // Just outside the two ranges.
        for (c in listOf(' ', '~', ' ', 'ÿ')) {
            check("ab${c}cd", null)
        }
    }

    @Test
    fun invalidUtf8IsRejected() {
        for (hex in listOf(
            "ff",
            "616263c3",
            "c328",
            "eda080", // UTF-16 surrogate
            "c0af", // overlong encoding
        )) {
            check(bytesOf(hex), WeftError.InvalidUtf8)
        }
        // A string with an unpaired surrogate cannot be encoded either.
        assertEquals(WeftError.InvalidUtf8, errorOf { encodeDeviceName("ab\ud800") })
    }

    /** A frame with a valid Device name header and [length] bytes of garbage as the plaintext. */
    private fun garbageFrame(length: Int): ByteArray =
        header().encode() + ByteArray(Weft.NONCE_LENGTH) + ByteArray(length + Weft.TAG_LENGTH)

    @Test
    fun badLengthsAreRejectedBeforeDecrypting() {
        for (length in listOf(0, 65, 100, 256 * 1024)) {
            assertEquals("$length bytes", WeftError.DeviceNameLength(length), errorOf { open(garbageFrame(length), bKeys.rx) })
        }
        // Lengths 1 and 64 get as far as the AEAD.
        for (length in listOf(1, 64)) {
            assertEquals(WeftError.DecryptionFailed, errorOf { open(garbageFrame(length), bKeys.rx) })
        }
        assertEquals(Weft.HEADER_LENGTH + Weft.NONCE_LENGTH + 64 + Weft.TAG_LENGTH, garbageFrame(64).size)
    }

    /** A Device name envelope whose plaintext is encrypted as is, bypassing the checks in seal. */
    private fun rawFrame(plaintext: ByteArray): ByteArray {
        val headerBytes = header().encode()
        val nonce = ByteArray(Weft.NONCE_LENGTH) { 9 }
        return headerBytes + nonce + Sodium.aeadEncrypt(plaintext, headerBytes, nonce, aKeys.tx)
    }

    @Test
    fun openChecksTheDecryptedName() {
        assertEquals("Téléphone", decodeDeviceName(open(rawFrame("Téléphone".toByteArray()), bKeys.rx).second))
        val cases = listOf(
            "tab\there".toByteArray() to WeftError.DeviceNameControlCharacter,
            "c1 \u0085".toByteArray() to WeftError.DeviceNameControlCharacter,
            bytesOf("fffe") to WeftError.InvalidUtf8,
            bytesOf("63757420c3") to WeftError.InvalidUtf8,
        )
        for ((plaintext, expected) in cases) {
            assertEquals(hexOf(plaintext), expected, errorOf { open(rawFrame(plaintext), bKeys.rx) })
        }
    }

    @Test
    fun kindCannotBeSwappedWithClipboardText() {
        val frame = seal(header(), "Laptop".toByteArray(), aKeys.tx)
        frame[1] = Kind.CLIPBOARD_TEXT.value.toByte()
        try {
            open(frame, bKeys.rx)
            fail("expected DecryptionFailed")
        } catch (e: WeftException) {
            assertEquals(WeftError.DecryptionFailed, e.error)
        }
    }
}
