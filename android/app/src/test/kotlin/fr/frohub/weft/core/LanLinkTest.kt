package fr.frohub.weft.core

import fr.frohub.weft.protocol.Kind
import fr.frohub.weft.protocol.Lan
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.IOException
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test

class LanLinkTest {
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
            val link = LanLink(loop.local, this, timeoutMillis = 500)
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
            val left = LanLink(loop.local, this, timeoutMillis = 2_000)
            val right = LanLink(loop.remote, this, timeoutMillis = 2_000)
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

    @Test fun aReceiptMustNameThePendingFrame() = runBlocking {
        Loopback().use { loop ->
            val link = LanLink(loop.local, this, timeoutMillis = 2_000)
            val reader = launch(Dispatchers.IO) {
                try { link.read { null } } catch (_: IOException) { link.close() }
            }
            loop.remote.soTimeout = 2_000
            val sending = async(Dispatchers.IO) { link.send(ByteArray(300), ByteArray(16) { 1 }) }
            try {
                val input = DataInputStream(loop.remote.getInputStream())
                withTimeout(2_000) {
                    async(Dispatchers.IO) {
                        input.readFully(ByteArray(4 + 300))
                        DataOutputStream(loop.remote.getOutputStream()).apply {
                            writeInt(0)
                            write(ByteArray(16) { 2 })
                        }
                    }.await()
                    assertFalse(sending.await())
                }
            } finally {
                link.close()
            }
            reader.join()
        }
    }
}
