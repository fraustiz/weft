package fr.frohub.weft.features

import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TransfersTest {
    private val chunk = 131072L
    private val id = "00112233445566778899aabbccddeeff"

    private fun OutgoingTransfer.drain(): List<OutgoingTransfer.Chunk> = generateSequence { nextChunk() }.toList()

    // region Sender

    @Test
    fun sendsNothingBeforeTheAccept() {
        val transfer = OutgoingTransfer(id, 10 * chunk, 0)
        assertEquals(OutgoingTransfer.State.OFFERED, transfer.state)
        assertNull(transfer.nextChunk())
    }

    @Test
    fun keepsAtMostEightChunksInFlight() {
        val transfer = OutgoingTransfer(id, 20 * chunk + 5, 0)
        assertTrue(transfer.onAccept(0, 1))
        val first = transfer.drain()
        assertEquals(MAX_UNACKED_CHUNKS, first.size)
        assertEquals((0 until 8).map { OutgoingTransfer.Chunk(it * chunk, chunk.toInt()) }, first)
        // One ack frees one slot.
        transfer.onAck(chunk, 2)
        assertEquals(listOf(OutgoingTransfer.Chunk(8 * chunk, chunk.toInt())), transfer.drain())
        // An ack for several chunks frees as many.
        transfer.onAck(5 * chunk, 3)
        assertEquals(4, transfer.drain().size)
        assertEquals(8, transfer.inFlight)
    }

    @Test
    fun lastChunkIsShorter() {
        val transfer = OutgoingTransfer(id, chunk + 10, 0)
        transfer.onAccept(0, 0)
        assertEquals(listOf(OutgoingTransfer.Chunk(0, chunk.toInt()), OutgoingTransfer.Chunk(chunk, 10)), transfer.drain())
        transfer.onAck(chunk + 10, 1)
        assertEquals(0, transfer.inFlight)
        assertTrue(transfer.isActive)
        transfer.onDone()
        assertEquals(OutgoingTransfer.State.DONE, transfer.state)
        assertFalse(transfer.isActive)
    }

    @Test
    fun ignoresAcksThatGoBackOrPastWhatWasSent() {
        val transfer = OutgoingTransfer(id, 4 * chunk, 0)
        transfer.onAccept(0, 0)
        transfer.nextChunk()
        transfer.onAck(2 * chunk, 1)
        assertEquals(0, transfer.acked)
        transfer.onAck(chunk, 1)
        transfer.onAck(chunk / 2, 1)
        assertEquals(chunk, transfer.acked)
    }

    @Test
    fun resumesFromTheAcceptedOffset() {
        val transfer = OutgoingTransfer(id, 10 * chunk, 0)
        transfer.onAccept(3 * chunk, 0)
        assertEquals(OutgoingTransfer.Chunk(3 * chunk, chunk.toInt()), transfer.nextChunk())
        assertFalse(transfer.onAccept(11 * chunk, 0))
        assertFalse(transfer.onAccept(-1, 0))
    }

    @Test
    fun offersAgainWhenStalledAndResends() {
        val transfer = OutgoingTransfer(id, 10 * chunk, 0)
        transfer.onAccept(0, 0)
        transfer.drain()
        transfer.onAck(2 * chunk, 1_000)
        assertEquals(OutgoingTransfer.Action.NONE, transfer.tick(1_000 + STALL_MILLIS - 1))
        assertEquals(OutgoingTransfer.Action.OFFER_AGAIN, transfer.tick(1_000 + STALL_MILLIS))
        assertEquals(OutgoingTransfer.State.OFFERED, transfer.state)
        assertNull(transfer.nextChunk())
        // Not again until another stall period has gone by.
        assertEquals(OutgoingTransfer.Action.NONE, transfer.tick(1_000 + STALL_MILLIS + 1))
        // The receiver kept 3 chunks (one more than acked): the sender goes on from there.
        transfer.onAccept(3 * chunk, 40_000)
        assertEquals(OutgoingTransfer.Chunk(3 * chunk, chunk.toInt()), transfer.nextChunk())
    }

    @Test
    fun givesUpAfterTenMinutesWithoutProgress() {
        val transfer = OutgoingTransfer(id, 10 * chunk, 0)
        transfer.onAccept(0, 0)
        transfer.drain()
        transfer.onAck(chunk, 60_000)
        // Offers again in between, but the clock of progress keeps running.
        var now = 60_000L
        while (now < 60_000 + GIVE_UP_MILLIS - 1) {
            now += STALL_MILLIS
            if (now >= 60_000 + GIVE_UP_MILLIS) break
            assertEquals(OutgoingTransfer.Action.OFFER_AGAIN, transfer.tick(now))
        }
        assertEquals(OutgoingTransfer.Action.GIVE_UP, transfer.tick(60_000 + GIVE_UP_MILLIS))
        assertEquals(OutgoingTransfer.State.FAILED, transfer.state)
    }

    @Test
    fun cancelRejectAndZeroBytes() {
        val cancelled = OutgoingTransfer(id, chunk, 0)
        cancelled.onAccept(0, 0)
        cancelled.cancel()
        assertEquals(OutgoingTransfer.State.CANCELLED, cancelled.state)
        assertNull(cancelled.nextChunk())
        // Nothing moves a finished transfer.
        cancelled.onDone()
        assertEquals(OutgoingTransfer.State.CANCELLED, cancelled.state)
        assertEquals(OutgoingTransfer.Action.NONE, cancelled.tick(GIVE_UP_MILLIS * 2))

        val rejected = OutgoingTransfer(id, chunk, 0)
        rejected.onReject(Packet.REJECT_BUSY)
        assertEquals(OutgoingTransfer.State.REJECTED, rejected.state)
        assertEquals(Packet.REJECT_BUSY, rejected.rejectReason)

        val empty = OutgoingTransfer(id, 0, 0)
        assertTrue(empty.onAccept(0, 0))
        assertNull(empty.nextChunk())
        empty.onDone()
        assertEquals(OutgoingTransfer.State.DONE, empty.state)
    }

    // endregion

    // region Receiver

    @Test
    fun receiverTakesOnlyTheNextExpectedChunk() {
        val transfer = IncomingTransfer(id, "a.bin", 2 * chunk + 1, null, 0, 0)
        assertFalse(transfer.accepts(chunk, chunk.toInt()))
        assertTrue(transfer.accepts(0, chunk.toInt()))
        assertEquals(chunk, transfer.advance(chunk.toInt(), 1))
        // A duplicate (resent after a re-offer) is ignored.
        assertFalse(transfer.accepts(0, chunk.toInt()))
        assertTrue(transfer.accepts(chunk, chunk.toInt()))
        transfer.advance(chunk.toInt(), 2)
        // Past the announced size.
        assertFalse(transfer.accepts(2 * chunk, 2))
        assertFalse(transfer.accepts(2 * chunk, 0))
        assertTrue(transfer.accepts(2 * chunk, 1))
        transfer.advance(1, 3)
        assertTrue(transfer.isComplete)
    }

    @Test
    fun receiverForgetsIdleTransfers() {
        val transfer = IncomingTransfer(id, "a", 10, null, 4, 0)
        assertFalse(transfer.isIdle(IDLE_MILLIS - 1))
        transfer.touch(5_000)
        assertFalse(transfer.isIdle(IDLE_MILLIS))
        assertTrue(transfer.isIdle(5_000 + IDLE_MILLIS))
    }

    @Test
    fun offerDecisions() {
        val offer = Packet.FileOffer(id, "a", 1000, null)
        assertEquals(OfferDecision.Accept(0), decideOffer(offer, enabled = true, active = 0, partialLength = null, freeBytes = 1L shl 30))
        assertEquals(OfferDecision.Accept(400), decideOffer(offer, true, 3, 400, 1L shl 30))
        assertEquals(OfferDecision.Reject(Packet.REJECT_DISABLED), decideOffer(offer, false, 0, null, 1L shl 30))
        assertEquals(OfferDecision.Reject(Packet.REJECT_BUSY), decideOffer(offer, true, MAX_TRANSFERS_PER_DEVICE, null, 1L shl 30))
        assertEquals(OfferDecision.Reject(Packet.REJECT_NO_SPACE), decideOffer(offer, true, 0, null, 1999))
        assertEquals(OfferDecision.Accept(0), decideOffer(offer, true, 0, null, 2000))
        val huge = offer.copy(size = Packet.MAX_FILE_SIZE + 1)
        assertEquals(OfferDecision.Reject(Packet.REJECT_TOO_LARGE), decideOffer(huge, true, 0, null, Long.MAX_VALUE))
    }

    // endregion

    // region Partial files

    private val dir: File = Files.createTempDirectory("weft-partials").toFile()
    private var now = 1_000_000_000L
    private val store = PartialStore(dir) { now }
    private val device = "aa".repeat(16)

    @After
    fun cleanUp() {
        dir.deleteRecursively()
    }

    @Test
    fun partialFilesResumeOnlyTheSameFile() {
        val offer = Packet.FileOffer(id, "a.txt", 10, null)
        assertNull(store.resumable(device, offer))
        store.create(device, offer).appendBytes(byteArrayOf(1, 2, 3))
        assertEquals(3L, store.resumable(device, offer))
        // Same id, another file: what was kept goes.
        assertNull(store.resumable(device, offer.copy(size = 11)))
        assertNull(store.resumable(device, offer))
        store.create(device, offer).appendBytes(byteArrayOf(1))
        assertNull(store.resumable(device, offer.copy(name = "b.txt")))
        // Another device's transfer with the same id is apart.
        store.create(device, offer)
        assertNull(store.resumable("bb".repeat(16), offer))
        assertEquals(0L, store.resumable(device, offer))
    }

    @Test
    fun partialFilesExpireAfter24Hours() {
        val offer = Packet.FileOffer(id, "a.txt", 10, null)
        val part = store.create(device, offer)
        part.setLastModified(now)
        now += PARTIAL_LIFETIME_MILLIS - 1
        store.expire()
        assertEquals(0L, store.resumable(device, offer))
        part.setLastModified(1_000_000_000L)
        now = 1_000_000_000L + PARTIAL_LIFETIME_MILLIS
        store.expire()
        assertNull(store.resumable(device, offer))
        assertEquals(0, dir.listFiles()!!.size)
    }

    @Test
    fun deleteRemovesBothFiles() {
        val offer = Packet.FileOffer(id, "a.txt", 10, null)
        store.create(device, offer)
        store.delete(device, id)
        assertEquals(0, dir.listFiles()!!.size)
    }

    // endregion
}
