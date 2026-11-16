package fr.frohub.weft.core

import android.util.Log
import fr.frohub.weft.ClipboardSender
import fr.frohub.weft.LastSent
import fr.frohub.weft.SendResult
import fr.frohub.weft.protocol.Header
import fr.frohub.weft.protocol.Identity
import fr.frohub.weft.protocol.Kind
import fr.frohub.weft.protocol.Weft
import fr.frohub.weft.protocol.randomMessageId
import fr.frohub.weft.protocol.seal
import fr.frohub.weft.protocol.sessionKeys
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * Seals clipboard text for every trusted device and hands it to the relay.
 *
 * The send counters of all recipients are incremented and saved before the first envelope
 * leaves, so a crash can never make a counter go back. Sends are serialized, so envelopes to one
 * device leave in counter order.
 */
class RelayClipboardSender(
    private val repository: StateRepository,
    private val identity: suspend () -> Identity,
    private val sink: FrameSink,
    /** Waits up to the given time for the relay connection; true once connected. */
    private val awaitConnected: suspend (Duration) -> Boolean,
    private val clock: () -> Long = System::currentTimeMillis,
) : ClipboardSender {
    private val mutex = Mutex()
    private val _lastSent = MutableStateFlow<LastSent?>(null)

    val lastSent: StateFlow<LastSent?> = _lastSent.asStateFlow()

    override suspend fun send(text: String): SendResult = mutex.withLock {
        val plaintext = text.toByteArray(Charsets.UTF_8)
        if (plaintext.size > Weft.MAX_CLIPBOARD_TEXT_LENGTH) return SendResult.TooLarge
        val devices = repository.load().devices
        if (devices.isEmpty()) return SendResult.NoPairedDevice
        if (!sink.isConnected && !awaitConnected(CONNECT_WAIT)) return SendResult.NotConnected

        val sent = sealAndSend(devices.map { it.deviceIdHex }, Kind.CLIPBOARD_TEXT, plaintext)
        if (sent == 0) return SendResult.NotConnected
        _lastSent.value = LastSent(clock(), text.codePointCount(0, text.length))
        Log.i(TAG, "sent ${plaintext.size} bytes to $sent device(s)")
        SendResult.Sent(sent)
    }

    /**
     * Seals [plaintext] as a [kind] envelope for each of [deviceIds] that is still trusted, with
     * the same counter rules as clipboard text. Returns how many envelopes the sink took.
     */
    suspend fun sendEnvelope(deviceIds: List<String>, kind: Kind, plaintext: ByteArray): Int =
        mutex.withLock { sealAndSend(deviceIds, kind, plaintext) }

    private suspend fun sealAndSend(deviceIds: List<String>, kind: Kind, plaintext: ByteArray): Int {
        val local = identity()
        val (devices, counters) = repository.update { state ->
            val devices = deviceIds.mapNotNull { state.device(it) }
            val (next, counters) = state.nextCounters(devices.map { it.deviceIdHex })
            next to (devices to counters)
        }
        var sent = 0
        for (device in devices) {
            val keys = sessionKeys(local, device.deviceId, device.kxPublicKey)
            val header = Header(
                kind = kind,
                sender = local.deviceId,
                recipient = device.deviceId,
                messageId = randomMessageId(),
                counter = counters.getValue(device.deviceIdHex),
            )
            if (sink.send(seal(header, plaintext, keys.tx))) sent++
        }
        return sent
    }

    private companion object {
        const val TAG = "WeftSender"
        /** How long a send waits for the connection, e.g. right after a cold start. */
        val CONNECT_WAIT = 10.seconds
    }
}
