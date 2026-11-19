// The weft-lan-v3 section of the Rust vectors (crates/weft-protocol/vectors/v1.json, read in
// place as a test resource), checked on the JVM. libsodium is not available here, so BLAKE2b and
// Ed25519 come from Bouncy Castle: this checks the Kotlin byte layout and the documented
// construction. androidTest VectorTests.lanHandshakeAndReceipts checks the same values through
// libsodium, and crates/weft-protocol/tests/vectors.rs through Rust.

package fr.frohub.weft.protocol

import org.bouncycastle.crypto.digests.Blake2bDigest
import org.bouncycastle.crypto.params.Ed25519PublicKeyParameters
import org.bouncycastle.crypto.signers.Ed25519Signer
import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LanVectorTests {
    private val vectors = JSONObject(
        javaClass.classLoader!!.getResourceAsStream("v1.json")!!.use { it.readBytes().toString(Charsets.UTF_8) },
    )
    private val lan = vectors.getJSONObject("lan")

    private fun JSONObject.bytes(key: String): ByteArray {
        val text = getString(key)
        return ByteArray(text.length / 2) { text.substring(2 * it, 2 * it + 2).toInt(16).toByte() }
    }

    private fun device(name: String) = vectors.getJSONObject("devices").getJSONObject(name)

    /** libsodium's `crypto_generichash(outlen, input, key)`. */
    private fun blake2b(outLength: Int, input: ByteArray, key: ByteArray): ByteArray {
        val digest = Blake2bDigest(key, outLength, null, null)
        digest.update(input, 0, input.size)
        return ByteArray(outLength).also { digest.doFinal(it, 0) }
    }

    private fun ed25519Verify(publicKey: ByteArray, message: ByteArray, signature: ByteArray): Boolean =
        Ed25519Signer().run {
            init(false, Ed25519PublicKeyParameters(publicKey, 0))
            update(message, 0, message.size)
            verifySignature(signature)
        }

    @Test
    fun challengeFramesUseTheV3TypeByte() {
        for (name in listOf("a", "b")) {
            val frame = lan.bytes("${name}_challenge_frame")
            assertEquals(Lan.CHALLENGE_TYPE, frame[0])
            assertArrayEquals(lan.bytes("${name}_nonce"), Lan.nonce(frame))
            // A weft-lan-v2 peer's challenge is refused before any answer is signed.
            assertNull(Lan.nonce(byteArrayOf(0xc0.toByte()) + lan.bytes("${name}_nonce")))
        }
    }

    @Test
    fun authFramesSignTheV3ContextAndNonceOrder() {
        val aNonce = lan.bytes("a_nonce")
        val bNonce = lan.bytes("b_nonce")
        val context = "weft-lan-v3".toByteArray(Charsets.US_ASCII)
        for ((name, message) in listOf("a" to context + bNonce + aNonce, "b" to context + aNonce + bNonce)) {
            assertArrayEquals(message, lan.bytes("${name}_signed_message"))
            val frame = lan.bytes("${name}_auth_frame")
            assertEquals(Lan.AUTH_LENGTH, frame.size)
            assertEquals(Lan.AUTH_TYPE, frame[0])
            assertArrayEquals(device(name).bytes("device_id"), frame.copyOfRange(1, 17))
            val signature = frame.copyOfRange(17, 81)
            assertTrue(ed25519Verify(device(name).bytes("sign_pk"), message, signature))
            // Reflected (swapped nonces) or v2-context messages do not verify.
            val other = if (name == "a") context + aNonce + bNonce else context + bNonce + aNonce
            assertFalse(ed25519Verify(device(name).bytes("sign_pk"), other, signature))
        }
    }

    @Test
    fun receiptKeysAndMacs() {
        val session = vectors.getJSONObject("session")
        val ids = mapOf("a" to device("a").bytes("device_id"), "b" to device("b").bytes("device_id"))
        val nonces = mapOf("a" to lan.bytes("a_nonce"), "b" to lan.bytes("b_nonce"))
        val keys = mutableListOf<ByteArray>()
        for ((sender, recipient) in listOf("a" to "b", "b" to "a")) {
            val receipt = lan.getJSONObject("receipts").getJSONObject("${sender}_to_$recipient")
            val input = Lan.receiptKeyInput(ids[sender]!!, ids[recipient]!!, nonces[sender]!!, nonces[recipient]!!)
            assertArrayEquals(receipt.bytes("key_input"), input)
            // The sender's tx is the recipient's rx: both ends derive the same key.
            val directionKey = session.getJSONObject(sender).bytes("tx")
            assertArrayEquals(directionKey, session.getJSONObject(recipient).bytes("rx"))
            val key = blake2b(32, input, directionKey)
            assertArrayEquals(receipt.bytes("key"), key)
            val id = receipt.bytes("message_id")
            val mac = blake2b(Lan.RECEIPT_MAC_LENGTH, id, key)
            assertArrayEquals(receipt.bytes("mac"), mac)
            assertArrayEquals(receipt.bytes("record"), ByteArray(4) + id + mac)
            assertEquals(Lan.RECEIPT_RECORD_LENGTH, receipt.bytes("record").size)
            keys += key
        }
        assertFalse(keys[0].contentEquals(keys[1]))
    }
}
