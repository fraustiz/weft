package fr.frohub.weft.features

import fr.frohub.weft.core.hexToBytesOrNull
import fr.frohub.weft.protocol.encodeFileChunk
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class ClipboardImagesTest {
    private val id = "01".repeat(16)
    private val hash = "ab".repeat(16)
    private fun png(size: Int) = ByteArray(size).also { byteArrayOf(137.toByte(), 80, 78, 71, 13, 10, 26, 10).copyInto(it) }

    @Test fun largeImageUsesAckWindowAndAllowsExplicitResend() = runBlocking {
        data class Wire(val from: String, val packet: Packet? = null, val chunk: ByteArray? = null)
        val wire = ArrayDeque<Wire>()
        var received: ByteArray? = null
        val sender = ClipboardImages({ hash }, { _, p -> wire.add(Wire("sender", p)); true },
            { _, c -> wire.add(Wire("sender", chunk = c)); true }, { _, _ -> error("unexpected clipboard write") })
        val receiver = ClipboardImages({ hash }, { _, p -> wire.add(Wire("receiver", p)); true },
            { _, _ -> error("unexpected chunk") }, { _, bytes -> received = bytes })
        val bytes = png(1_300_000)
        assertEquals(1, sender.offer(listOf("receiver"), "image/png", bytes))
        var steps = 0
        while (wire.isNotEmpty()) {
            check(++steps < 100)
            val frame = wire.removeFirst()
            val target = if (frame.from == "sender") receiver else sender
            if (frame.packet != null) target.packet(frame.from, frame.packet)
            else target.receive(frame.from, frame.chunk!!)
        }
        assertArrayEquals(bytes, received)
        assertEquals(1, receiver.offer(listOf("sender"), "image/png", bytes))
        assertFalse(wire.isEmpty())
    }

    @Test fun offerBudgetExpiryAndInvalidHashAreBounded() = runBlocking {
        var now = 0L
        val replies = mutableListOf<Packet>()
        var writes = 0
        val engine = ClipboardImages({ hash }, { _, p -> replies.add(p); true }, { _, _ -> true }, { _, _ -> writes++ }, { now })
        fun offer(id: String = this@ClipboardImagesTest.id, size: Long = MAX_CLIPBOARD_IMAGE_BYTES.toLong(), digest: String = hash) =
            Packet.ClipboardImageOffer(id, "image/png", size, digest)
        engine.packet("a", offer())
        engine.packet("b", offer())
        engine.packet("c", offer())
        assertTrue(replies.last() is Packet.FileReject)
        now = 30_000
        engine.expire()
        assertEquals(2, replies.filterIsInstance<Packet.FileCancel>().size)
        val bytes = png(30)
        engine.packet("a", offer(size = bytes.size.toLong(), digest = "cd".repeat(16)))
        engine.receive("a", encodeFileChunk(id.hexToBytesOrNull(16)!!, 0uL, bytes))
        assertTrue(replies.last() is Packet.FileCancel)
        assertEquals(0, writes)
    }

    @Test fun imagePacketWireShapeMatchesRust() {
        val packet = Packet.ClipboardImageOffer(id, "image/png", 100, hash)
        assertEquals(packet, decodePacket(encodePacket(packet)))
        val json = String(encodePacket(packet), Charsets.UTF_8)
        assertTrue(json.contains("clipboard.image.offer"))
        assertTrue(json.contains("\"hash\":\"$hash\""))
        assertNull(decodePacket("""{"type":"clipboard.image.offer","id":"$id","mime":"text/plain","size":100,"hash":"$hash"}""".toByteArray()))
    }

    @Test fun imageControlsRemainAvailableWhenFileSharingIsOff() {
        val caps = capabilitiesOf(emptySet())
        assertTrue(caps.incoming.containsAll(setOf(Packet.CLIPBOARD_IMAGE_OFFER, Packet.CLIPBOARD_IMAGE_ACCEPT,
            Packet.FILE_ACK, Packet.FILE_DONE, Packet.FILE_CANCEL, Packet.FILE_REJECT)))
        assertFalse(caps.incoming.contains(Packet.FILE_OFFER))
        assertFalse(caps.incoming.contains(Packet.FILE_ACCEPT))
    }
}
