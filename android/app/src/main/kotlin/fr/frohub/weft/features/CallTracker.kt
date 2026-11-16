package fr.frohub.weft.features

/** The phone's call state, as Android reports it. */
enum class PhoneState { IDLE, RINGING, OFFHOOK }

/** A `call` event to send, with the number when it is known. */
data class CallEvent(val event: String, val number: String?)

/**
 * Turns call state changes into `call` events (docs/features.md, section 5): ringing, talking
 * once answered (or for an outgoing call), missed when a ringing call stops unanswered, ended
 * after talking. The number is kept for the whole call, whichever report carried it. Not
 * thread-safe.
 */
class CallTracker {
    private var state = PhoneState.IDLE

    /** The number of the current call, once known. */
    var number: String? = null
        private set

    val isRinging: Boolean get() = state == PhoneState.RINGING

    /** Remembers a number reported apart from the state (the phone state broadcast). */
    fun onNumber(number: String?) {
        if (!number.isNullOrBlank()) this.number = number
    }

    /** The event for a state change, or null when the state did not change. */
    fun onState(next: PhoneState, number: String? = null): CallEvent? {
        onNumber(number)
        val previous = state
        if (previous == next) return null
        state = next
        val event = when (next) {
            PhoneState.RINGING -> Packet.CALL_RINGING
            PhoneState.OFFHOOK -> Packet.CALL_TALKING
            PhoneState.IDLE -> if (previous == PhoneState.RINGING) Packet.CALL_MISSED else Packet.CALL_ENDED
        }
        val known = this.number
        if (next == PhoneState.IDLE) this.number = null
        return CallEvent(event, known)
    }
}
