package fr.frohub.weft.core

import fr.frohub.weft.protocol.Lan
import fr.frohub.weft.protocol.Weft
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.EOFException
import java.io.IOException
import java.net.Socket
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** A LAN v2 stream after mutual authentication. Both directions can send at the same time. */
internal class LanLink(
    private val socket: Socket,
    private val scope: CoroutineScope,
    private val timeoutMillis: Long,
) : AutoCloseable {
    private class Pending(val id: ByteArray, val received: CompletableDeferred<Boolean> = CompletableDeferred())

    private val input = DataInputStream(socket.getInputStream())
    private val output = DataOutputStream(socket.getOutputStream())
    private val writing = Mutex()
    private val pending = AtomicReference<Pending?>(null)

    /** The caller serializes sends and relays these exact bytes on false. Runs on IO. */
    suspend fun send(frame: ByteArray, id: ByteArray): Boolean {
        val receipt = Pending(id)
        check(pending.compareAndSet(null, receipt))
        var accepted = false
        try {
            accepted = deadline {
                writing.withLock {
                    output.writeInt(frame.size)
                    output.write(frame)
                }
                receipt.received.await()
            }
            return accepted
        } catch (_: IOException) {
            return false
        } finally {
            if (!accepted) close()
            pending.set(null)
        }
    }

    /** The callback queues a valid complete frame and returns its ID, or refuses it. Runs on IO. */
    suspend fun read(accept: (ByteArray) -> ByteArray?): Nothing {
        while (true) {
            // Idle streams can wait indefinitely. The rest of a record has a deadline.
            val first = input.read()
            if (first < 0) throw EOFException()
            val (length, bytes) = deadline {
                val length = (first shl 24) or (input.readUnsignedByte() shl 16) or input.readUnsignedShort()
                if (length != 0 && length !in MIN_FRAME_LENGTH..Lan.MAX_FRAME_LENGTH) {
                    throw IOException("invalid LAN size")
                }
                length to ByteArray(if (length == 0) Weft.MESSAGE_ID_LENGTH else length).also(input::readFully)
            }
            if (length == 0) {
                val receipt = pending.get()
                if (receipt == null || !receipt.id.contentEquals(bytes) || !receipt.received.complete(true)) {
                    throw IOException("unexpected LAN receipt")
                }
            } else {
                val id = accept(bytes) ?: throw IOException("LAN frame refused")
                // A zero big-endian length marks a receipt followed by the envelope's message ID.
                deadline {
                    writing.withLock {
                        output.writeInt(0)
                        output.write(id)
                    }
                }
            }
        }
    }

    /** Closing the socket interrupts blocking reads and writes as well as the receipt wait. */
    private suspend fun <T> deadline(block: suspend () -> T): T {
        val timer = scope.launch(Dispatchers.Default) { delay(timeoutMillis); close() }
        return try { block() } finally { timer.cancel() }
    }

    override fun close() {
        runCatching { socket.close() }
        pending.get()?.received?.complete(false)
    }

    private companion object {
        const val MIN_FRAME_LENGTH = Weft.HEADER_LENGTH + Weft.NONCE_LENGTH + Weft.TAG_LENGTH
    }
}
