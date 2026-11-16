package fr.frohub.weft

import android.util.Log
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Receives text picked up on this device. The relay client plugs in here. */
fun interface ClipboardSender {
    fun send(text: String)
}

/** What the main screen shows about the last send: never the text itself. */
data class LastSent(val atMillis: Long, val characterCount: Int)

/**
 * Stand-in until the relay exists: keeps the last sent text in memory and logs only its length.
 */
class InMemoryClipboardSender : ClipboardSender {
    private val _lastSent = MutableStateFlow<LastSent?>(null)
    val lastSent: StateFlow<LastSent?> = _lastSent.asStateFlow()

    @Volatile
    var lastText: String? = null
        private set

    override fun send(text: String) {
        val count = text.codePointCount(0, text.length)
        lastText = text
        _lastSent.value = LastSent(System.currentTimeMillis(), count)
        Log.i(TAG, "Sent $count characters")
    }

    private companion object {
        const val TAG = "WeftSender"
    }
}
