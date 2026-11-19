package fr.frohub.weft.core

import fr.frohub.weft.protocol.Kind
import fr.frohub.weft.protocol.Lan
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.IOException
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.security.MessageDigest
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test

class LanLinkTest {
    /**
     * Stands in for [Lan.ReceiptKeys] on the JVM, where libsodium is not available: HMAC-SHA256
     * truncated to 16 bytes, one key per direction. The real construction is checked against the
     * shared vectors in :protocol (LanVectorTests) and in androidTest (VectorTests).
     */
    private class FakeReceipts(private val outgoing: ByteArray, private val incoming: ByteArray) : Lan.Receipts {
        override fun mac(messageId: ByteArray): ByteArray = mac(incoming, messageId)
        override fun verify(messageId: ByteArray, mac: ByteArray): Boolean =
            MessageDigest.isEqual(mac(outgoing, messageId), mac)

        /** The other end of the same connection. */
        fun peer() = FakeReceipts(incoming, outgoing)

        companion object {
            fun mac(key: ByteArray, id: ByteArray): ByteArray =
                Mac.getInstance("HmacSHA256").apply { init(SecretKeySpec(key, "HmacSHA256")) }.doFinal(id).copyOf(16)
        }
    }

    private val keys = FakeReceipts(ByteArray(32) { 1 }, ByteArray(32) { 2 })

    private class Loopback : AutoCloseable {
        private val server = ServerSocket(0, 1, InetAddress.getLoopbackAddress())
        val local = Socket(InetAddress.getLoopbackAddress(), server.localPort)
        val remote = server.accept()

        override fun close() {
            local.close()
            remote.close()
            server.close()
        }
    }

    @Test fun openSilentPeerFallsBackWithTheOriginalBytesInOrder() = runBlocking {
        Loopback().use { loop ->
            val link = LanLink(loop.local, this, timeoutMillis = 500, keys)
            val first = ByteArray(300) { 1 }
            val second = ByteArray(500) { 2 }
            val id = ByteArray(16) { 3 }
            loop.remote.soTimeout = 2_000
            val received = async(Dispatchers.IO) {
                val input = DataInputStream(loop.remote.getInputStream())
                ByteArray(input.readInt()).also(input::readFully)
            }
            val reader = launch(Dispatchers.IO) { runCatching { link.read { null } } }
            val relayed = mutableListOf<ByteArray>()
            val sending = async(Dispatchers.IO) {
                for (frame in listOf(first, second)) {
                    assertTrue(routeSealedEnvelope(frame, Kind.PACKET, true,
                        direct = { link.send(it, id) },
                        relay = { relayed.add(it) }))
                }
            }
            // The peer reads the entire first frame and leaves its socket open without replying.
            assertArrayEquals(first, withTimeout(2_000) { received.await() })
            withTimeout(2_000) { sending.await() }
            assertEquals(2, relayed.size)
            assertSame(first, relayed[0])
            assertSame(second, relayed[1])
            reader.join()
        }
    }

    @Test fun simultaneousSendsReceiveReceiptsEvenWhileTheApplicationIsIdle() = runBlocking {
        Loopback().use { loop ->
            val left = LanLink(loop.local, this, timeoutMillis = 2_000, keys)
            val right = LanLink(loop.remote, this, timeoutMillis = 2_000, keys.peer())
            val first = ByteArray(Lan.MAX_FRAME_LENGTH) { 1 }
            val second = ByteArray(Lan.MAX_FRAME_LENGTH) { 2 }
            val firstId = ByteArray(16) { 3 }
            val secondId = ByteArray(16) { 4 }
            val leftReader = launch(Dispatchers.IO) {
                runCatching { left.read { assertArrayEquals(second, it); secondId } }
            }
            val rightReader = launch(Dispatchers.IO) {
                runCatching { right.read { assertArrayEquals(first, it); firstId } }
            }
            try {
                val fromLeft = async(Dispatchers.IO) { left.send(first, firstId) }
                val fromRight = async(Dispatchers.IO) { right.send(second, secondId) }
                assertTrue(fromLeft.await())
                assertTrue(fromRight.await())
            } finally {
                left.close()
                right.close()
            }
            leftReader.join()
            rightReader.join()
        }
    }

    /**
     * The peer (or an on-path attacker) reads the frame and answers with [receipt]. Returns
     * whether the send succeeded, and what went to the relay.
     */
    private fun sendAndAnswer(receipt: (ByteArray) -> ByteArray): Pair<Boolean, List<ByteArray>> = runBlocking {
        Loopback().use { loop ->
            val link = LanLink(loop.local, this, timeoutMillis = 2_000, keys)
            val reader = launch(Dispatchers.IO) {
                try { link.read { null } } catch (_: IOException) { link.close() }
            }
            loop.remote.soTimeout = 2_000
            val frame = ByteArray(300) { 5 }
            val id = ByteArray(16) { 1 }
            val relayed = mutableListOf<ByteArray>()
            val sending = async(Dispatchers.IO) {
                routeSealedEnvelope(frame, Kind.PACKET, true,
                    direct = { link.send(it, id) },
                    relay = { relayed.add(it) })
                relayed.isEmpty()
            }
            try {
                val input = DataInputStream(loop.remote.getInputStream())
                withTimeout(3_000) {
                    async(Dispatchers.IO) {
                        input.readFully(ByteArray(4 + 300))
                        loop.remote.getOutputStream().write(receipt(id))
                        // Like an attacker, close: a short record must not leave the sender waiting.
                        loop.remote.shutdownOutput()
                    }.await()
                    val direct = sending.await()
                    if (!direct) assertSame(frame, relayed.single())
                    direct to relayed.toList()
                }
            } finally {
                link.close()
                reader.join()
            }
        }
    }

    private fun record(id: ByteArray, mac: ByteArray) = ByteArray(4) + id + mac

    @Test fun aGenuineReceiptConfirmsTheFrame() {
        assertTrue(sendAndAnswer { id -> record(id, keys.peer().mac(id)) }.first)
    }

    @Test fun aReceiptMustNameThePendingFrame() {
        val other = ByteArray(16) { 2 }
        assertFalse(sendAndAnswer { record(other, keys.peer().mac(other)) }.first)
    }

    @Test fun aForgedReceiptFallsBackToTheRelay() {
        // Right message ID, wrong MAC: the ID is visible in the clear envelope header.
        assertFalse(sendAndAnswer { id -> record(id, keys.peer().mac(id).also { it[15] = (it[15] + 1).toByte() }) }.first)
        assertFalse(sendAndAnswer { id -> record(id, ByteArray(16)) }.first)
        // A weft-lan-v2 receipt has no MAC at all.
        assertFalse(sendAndAnswer { id -> ByteArray(4) + id }.first)
    }

    @Test fun aReceiptFromTheOtherDirectionOrAnotherConnectionIsRefused() {
        // Our own incoming key is the key of the other direction.
        assertFalse(sendAndAnswer { id -> record(id, keys.mac(id)) }.first)
        val otherConnection = FakeReceipts(ByteArray(32) { 3 }, ByteArray(32) { 4 })
        assertFalse(sendAndAnswer { id -> record(id, otherConnection.peer().mac(id)) }.first)
    }
}
