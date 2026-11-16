package fr.frohub.weft.features

import fr.frohub.weft.protocol.Weft

/** At most this many chunks without acknowledgement (docs/features.md, section 4). */
const val MAX_UNACKED_CHUNKS = 8

/** At most this many transfers per direction and per device. */
const val MAX_TRANSFERS_PER_DEVICE = 4

/** A sender gives up after this long without progress. */
const val GIVE_UP_MILLIS = 10 * 60_000L

/**
 * Without progress for this long, a sender offers the file again: the receiver answers with the
 * offset it has, so a transfer resumes after a lost connection or a receiver that restarted.
 */
const val STALL_MILLIS = 30_000L

/** A receiver forgets (but keeps on disk) a transfer that sent nothing for this long. */
const val IDLE_MILLIS = GIVE_UP_MILLIS

/** A partial file is deleted this long after its last write. */
const val PARTIAL_LIFETIME_MILLIS = 24 * 60 * 60_000L

/**
 * The sending side of one transfer: which bytes to send next, the acknowledgement window, resume
 * and timeouts. Pure: the caller does the I/O and passes the time in. Not thread-safe.
 */
class OutgoingTransfer(
    val id: String,
    val size: Long,
    nowMillis: Long,
    private val chunkSize: Int = Weft.MAX_FILE_CHUNK_DATA_LENGTH,
    private val window: Int = MAX_UNACKED_CHUNKS,
) {
    enum class State { OFFERED, SENDING, DONE, REJECTED, CANCELLED, FAILED }

    /** A piece of the file to read and send. */
    data class Chunk(val offset: Long, val length: Int)

    /** What [tick] asks the caller to do. */
    enum class Action { NONE, OFFER_AGAIN, GIVE_UP }

    var state = State.OFFERED
        private set

    /** Bytes the receiver has confirmed. */
    var acked = 0L
        private set

    /** The next byte to send. */
    var next = 0L
        private set

    /** The receiver's reason, once [State.REJECTED]. */
    var rejectReason: String? = null
        private set

    private var lastProgress = nowMillis
    private var lastOffer = nowMillis

    val isActive: Boolean get() = state == State.OFFERED || state == State.SENDING

    /** Chunks sent and not acknowledged yet. */
    val inFlight: Int get() = ((next - acked + chunkSize - 1) / chunkSize).toInt()

    /** The receiver has [offset] bytes: send from there. False (and ignored) if out of range. */
    fun onAccept(offset: Long, nowMillis: Long): Boolean {
        if (!isActive || offset !in 0..size) return false
        if (offset > acked) lastProgress = nowMillis
        state = State.SENDING
        acked = offset
        next = offset
        return true
    }

    /** The next chunk to send, or null while the window is full or everything is out. */
    fun nextChunk(): Chunk? {
        if (state != State.SENDING || next >= size || inFlight >= window) return null
        val length = minOf(chunkSize.toLong(), size - next).toInt()
        return Chunk(next, length).also { next += length }
    }

    /** The receiver has written [offset] bytes. Acks that go back or past what was sent are ignored. */
    fun onAck(offset: Long, nowMillis: Long) {
        if (state != State.SENDING || offset <= acked || offset > next) return
        acked = offset
        lastProgress = nowMillis
    }

    fun onDone() {
        if (isActive) {
            acked = size
            state = State.DONE
        }
    }

    fun onReject(reason: String?) {
        if (isActive) {
            rejectReason = reason
            state = State.REJECTED
        }
    }

    fun cancel() {
        if (isActive) state = State.CANCELLED
    }

    /**
     * Back to [State.OFFERED], to send the offer again (after a reconnect, or when [tick] says
     * so): the receiver's `file.accept` tells where to resume.
     */
    fun offerAgain(nowMillis: Long) {
        if (!isActive) return
        state = State.OFFERED
        next = acked
        lastOffer = nowMillis
    }

    /** Checks the timeouts. [Action.GIVE_UP] leaves the transfer [State.FAILED]. */
    fun tick(nowMillis: Long): Action {
        if (!isActive) return Action.NONE
        if (nowMillis - lastProgress >= GIVE_UP_MILLIS) {
            state = State.FAILED
            return Action.GIVE_UP
        }
        if (nowMillis - lastProgress >= STALL_MILLIS && nowMillis - lastOffer >= STALL_MILLIS) {
            offerAgain(nowMillis)
            return Action.OFFER_AGAIN
        }
        return Action.NONE
    }
}

/** The receiving side of one transfer: which chunk comes next. Pure. Not thread-safe. */
class IncomingTransfer(
    val id: String,
    val name: String,
    val size: Long,
    val mime: String?,
    written: Long,
    nowMillis: Long,
) {
    /** Bytes written to the partial file. */
    var written = written
        private set

    private var lastActivity = nowMillis

    val isComplete: Boolean get() = written == size

    /** True for the chunk that starts exactly at the next expected byte and fits in the file. */
    fun accepts(offset: Long, length: Int): Boolean = offset == written && length > 0 && length <= size - written

    /** Records a chunk of [length] bytes written; returns the offset to acknowledge. */
    fun advance(length: Int, nowMillis: Long): Long {
        written += length
        lastActivity = nowMillis
        return written
    }

    /** The sender offered it again: it is alive. */
    fun touch(nowMillis: Long) {
        lastActivity = nowMillis
    }

    fun isIdle(nowMillis: Long): Boolean = nowMillis - lastActivity >= IDLE_MILLIS
}

/** How a receiver answers a `file.offer`. */
sealed interface OfferDecision {
    data class Accept(val offset: Long) : OfferDecision
    data class Reject(val reason: String) : OfferDecision
}

/**
 * The answer to a new offer (one that matches no transfer in progress): [enabled] is the Files
 * switch, [active] how many transfers from that device are in progress, [partialLength] the bytes
 * kept from an earlier attempt with the same id, name and size (null if none), [freeBytes] the
 * space left where partial files go.
 */
fun decideOffer(offer: Packet.FileOffer, enabled: Boolean, active: Int, partialLength: Long?, freeBytes: Long): OfferDecision {
    if (!enabled) return OfferDecision.Reject(Packet.REJECT_DISABLED)
    if (offer.size > Packet.MAX_FILE_SIZE) return OfferDecision.Reject(Packet.REJECT_TOO_LARGE)
    if (active >= MAX_TRANSFERS_PER_DEVICE) return OfferDecision.Reject(Packet.REJECT_BUSY)
    val have = partialLength?.coerceIn(0, offer.size) ?: 0
    // Room for the rest, plus as much again to copy it into Downloads.
    if (freeBytes < offer.size - have + offer.size) return OfferDecision.Reject(Packet.REJECT_NO_SPACE)
    return OfferDecision.Accept(have)
}
