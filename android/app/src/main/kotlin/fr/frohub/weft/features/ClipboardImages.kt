package fr.frohub.weft.features

import fr.frohub.weft.core.hexToBytesOrNull
import fr.frohub.weft.core.toHex
import fr.frohub.weft.protocol.decodeFileChunk
import fr.frohub.weft.protocol.encodeFileChunk
import java.util.UUID
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

const val MAX_CLIPBOARD_IMAGE_BYTES = 16 * 1024 * 1024
private const val IMAGE_LIFETIME = 30_000L

fun isClipboardImage(mime: String, bytes: ByteArray): Boolean = bytes.size in 1..MAX_CLIPBOARD_IMAGE_BYTES && when (mime) {
    "image/png" -> bytes.size >= 8 && bytes.copyOfRange(0, 8).contentEquals(byteArrayOf(137.toByte(), 80, 78, 71, 13, 10, 26, 10))
    "image/jpeg" -> bytes.size >= 3 && bytes[0] == 255.toByte() && bytes[1] == 216.toByte() && bytes[2] == 255.toByte()
    else -> false
}

/** Ephemeral, bounded clipboard transfers. All state and callbacks are serialized. */
class ClipboardImages(
    private val digest: (ByteArray) -> String,
    private val send: suspend (String, Packet) -> Boolean,
    private val chunk: suspend (String, ByteArray) -> Boolean,
    private val apply: suspend (String, ByteArray) -> Unit,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    private data class Incoming(val offer: Packet.ClipboardImageOffer, val bytes: ByteArray, val started: Long, var offset: Int = 0)
    private data class Outgoing(val mime: String, val bytes: ByteArray, val transfer: OutgoingTransfer, val started: Long)
    private val lock = Mutex()
    private val incoming = mutableMapOf<Pair<String, String>, Incoming>()
    private val outgoing = mutableMapOf<Pair<String, String>, Outgoing>()
    suspend fun clear() = lock.withLock { incoming.clear(); outgoing.clear() }

    suspend fun offer(devices: List<String>, mime: String, bytes: ByteArray): Int = lock.withLock {
        if (!isClipboardImage(mime, bytes)) return 0
        val hash = digest(bytes)
        outgoing.clear()
        val data = bytes.copyOf()
        var sent = 0
        for (device in devices) {
            val id = UUID.randomUUID().toString().replace("-", "")
            outgoing[device to id] = Outgoing(mime, data, OutgoingTransfer(id, data.size.toLong(), clock()), clock())
            if (send(device, Packet.ClipboardImageOffer(id, mime, data.size.toLong(), hash))) sent++
            else outgoing.remove(device to id)
        }
        sent
    }

    suspend fun expire() = lock.withLock {
        val now = clock()
        val expired = incoming.filterValues { now - it.started >= IMAGE_LIFETIME }.keys +
            outgoing.filterValues { now - it.started >= IMAGE_LIFETIME }.keys
        for (key in expired) {
            incoming.remove(key); outgoing.remove(key)
            send(key.first, Packet.FileCancel(key.second))
        }
    }

    suspend fun packet(peer: String, packet: Packet): Boolean = lock.withLock {
        if (packet is Packet.ClipboardImageOffer) {
            val key = peer to packet.id
            if (!packet.id.matches(Regex("[0-9a-f]{32}")) || packet.size !in 1..MAX_CLIPBOARD_IMAGE_BYTES.toLong() ||
                packet.mime !in setOf("image/png", "image/jpeg") ||
                !packet.hash.matches(Regex("[0-9a-f]{32}"))) return true
            incoming.keys.filter { it.first == peer }.forEach { incoming.remove(it) }
            if (incoming.values.sumOf { it.offer.size } + packet.size > 2L * MAX_CLIPBOARD_IMAGE_BYTES) {
                send(peer, Packet.FileReject(packet.id, Packet.REJECT_BUSY)); return true
            }
            incoming[key] = Incoming(packet, ByteArray(packet.size.toInt()), clock())
            send(peer, Packet.ClipboardImageAccept(packet.id))
            return true
        }
        val id = when (packet) {
            is Packet.ClipboardImageAccept -> packet.id
            is Packet.FileAck -> packet.id
            is Packet.FileDone -> packet.id
            is Packet.FileReject -> packet.id
            is Packet.FileCancel -> packet.id
            else -> return false
        }
        val key = peer to id
        if (packet is Packet.FileCancel && incoming.remove(key) != null) return true
        val source = outgoing[key] ?: return packet is Packet.ClipboardImageAccept
        when (packet) {
            is Packet.ClipboardImageAccept -> source.transfer.onAccept(0, clock())
            is Packet.FileAck -> source.transfer.onAck(packet.offset, clock())
            is Packet.FileDone, is Packet.FileReject, is Packet.FileCancel -> { outgoing.remove(key); return true }
        }
        while (true) {
            val next = source.transfer.nextChunk() ?: break
            if (!chunk(peer, encodeFileChunk(id.hexToBytesOrNull(16)!!, next.offset.toULong(),
                source.bytes.copyOfRange(next.offset.toInt(), next.offset.toInt() + next.length)))) {
                outgoing.remove(key)
                break
            }
        }
        true
    }

    suspend fun receive(peer: String, plaintext: ByteArray): Boolean = lock.withLock {
        val piece = decodeFileChunk(plaintext)
        val key = peer to piece.transferId.toHex()
        val target = incoming[key] ?: return false
        val data = piece.data
        if (clock() - target.started >= IMAGE_LIFETIME || piece.offset != target.offset.toULong() ||
            data.isEmpty() || data.size > target.bytes.size - target.offset) {
            incoming.remove(key); send(peer, Packet.FileCancel(key.second)); return true
        }
        data.copyInto(target.bytes, target.offset)
        target.offset += data.size
        if (target.offset == target.bytes.size) {
            incoming.remove(key)
            if (!isClipboardImage(target.offer.mime, target.bytes) || digest(target.bytes) != target.offer.hash) {
                send(peer, Packet.FileCancel(key.second)); return true
            }
            try { apply(target.offer.mime, target.bytes) }
            catch (_: java.io.IOException) { send(peer, Packet.FileCancel(key.second)); return true }
            catch (_: SecurityException) { send(peer, Packet.FileCancel(key.second)); return true }
            send(peer, Packet.FileAck(key.second, target.offset.toLong()))
            send(peer, Packet.FileDone(key.second))
        } else send(peer, Packet.FileAck(key.second, target.offset.toLong()))
        true
    }
}
