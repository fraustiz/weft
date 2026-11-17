package fr.frohub.weft.core

import fr.frohub.weft.protocol.ControlError

/** A trusted device, as shown to the user. */
data class Device(
    /** 16-byte device id in hex. */
    val id: String,
    /** The name the device announced, if any yet. */
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

/**
 * How [fr.frohub.weft.core.WeftCore.invite] ended: this phone, already in a group, added the
 * device that showed the code to it (docs/protocol.md section 5, « Invitation »).
 */
sealed interface InviteResult {
    /** [device] is in the group now, with the name it announced. */
    data class Added(val device: Device) : InviteResult

    /** Not a valid `WEFT1:` code. */
    data object InvalidCode : InviteResult

    /** The code was shown by this device. */
    data object OwnCode : InviteResult

    /** This phone trusts no device: there is no group to add to, join the other device instead. */
    data object NoGroup : InviteResult

    /** The code is for another relay than this phone's group: it cannot be added from there. */
    data object OtherRelay : InviteResult

    /** The group already has [fr.frohub.weft.protocol.Weft.MAX_GROUP_SIZE] devices. */
    data object GroupFull : InviteResult

    /**
     * The device did not answer within 30 s. It may already belong to a group, and then
     * ignores invitations: it has to be paired from that group instead. It is not trusted.
     */
    data object NoAnswer : InviteResult

    /** Could not reach the relay in time. [reason] is for logs, [problem] for display. */
    data class ConnectionFailed(
        val reason: String,
        val problem: ConnectionProblem = ConnectionProblem.UNREACHABLE,
    ) : InviteResult

    /** The relay refused the invitation (expired code, rate limit…). */
    data class Refused(val error: ControlError) : InviteResult

    /** A pairing or a group change is already running. */
    data object Busy : InviteResult
}

/** Progress of the invitation started with [fr.frohub.weft.core.WeftCore.startInvite], for the UI. */
sealed interface InviteState {
    data object Idle : InviteState
    data object Inviting : InviteState
    data class Done(val result: InviteResult) : InviteState
}
