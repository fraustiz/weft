package fr.frohub.weft.core

import fr.frohub.weft.protocol.ControlError

/** A trusted device, as shown to the user. */
data class Device(
    /** 16-byte device id in hex. */
    val id: String,
    /** Name given when pairing, if any. */
    val name: String?,
    /** When this device started trusting it, in epoch millis, if known. */
    val addedAt: Long? = null,
) {
    /** The first 4 bytes of the id, to tell devices apart. */
    val shortId: String get() = shortId(id)
}

/** How [fr.frohub.weft.core.WeftCore.join] ended. */
sealed interface JoinResult {
    /**
     * Joined the group of the device that showed the code. [devices] is every device now
     * trusted. [replacedGroup] is true if this device was in another group before: those
     * devices are no longer trusted, since the relay moved this device to the new group.
     */
    data class Joined(val devices: List<Device>, val replacedGroup: Boolean) : JoinResult

    /** Not a valid `WEFT1:` code. */
    data object InvalidCode : JoinResult

    /** The code was shown by this device. */
    data object OwnCode : JoinResult

    /** The code's relay uses `ws://`, which this build does not allow. */
    data object InsecureRelay : JoinResult

    /**
     * Could not connect to the relay in time. [reason] is for logs, [problem] for display.
     */
    data class ConnectionFailed(
        val reason: String,
        val problem: ConnectionProblem = ConnectionProblem.UNREACHABLE,
    ) : JoinResult

    /** No Welcome from the device that showed the code within 30 s. */
    data object TimedOut : JoinResult

    /** The relay refused the request (expired code, rate limit, full group…). */
    data class Refused(val error: ControlError) : JoinResult

    /** Another join is already running. */
    data object Busy : JoinResult
}

/** Progress of the join started with [fr.frohub.weft.core.WeftCore.startJoin], for the UI. */
sealed interface JoinState {
    data object Idle : JoinState
    data object Joining : JoinState
    data class Done(val result: JoinResult) : JoinState
}

/** How [fr.frohub.weft.core.WeftCore.remove] or [fr.frohub.weft.core.WeftCore.leave] ended. */
sealed interface GroupChangeResult {
    data object Done : GroupChangeResult

    /** The relay could not be reached in time; nothing was changed. */
    data object NotConnected : GroupChangeResult

    /** The device is not trusted (any more). */
    data object UnknownDevice : GroupChangeResult

    /** A pairing or another group change is running. */
    data object Busy : GroupChangeResult
}
