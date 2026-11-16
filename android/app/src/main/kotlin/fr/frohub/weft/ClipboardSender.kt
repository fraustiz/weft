package fr.frohub.weft

/** Sends text picked up on this device to the paired devices. */
fun interface ClipboardSender {
    suspend fun send(text: String): SendResult
}

/** How a send ended; the entry points turn it into a toast. */
sealed interface SendResult {
    /** Handed to the relay for [deviceCount] devices. */
    data class Sent(val deviceCount: Int) : SendResult
    data object NotConnected : SendResult
    data object NoPairedDevice : SendResult
    /** Longer than the 256 KiB a message can carry; never truncated. */
    data object TooLarge : SendResult
}

/** What the main screen shows about the last send: never the text itself. */
data class LastSent(val atMillis: Long, val characterCount: Int)
