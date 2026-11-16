package fr.frohub.weft.protocol

/** Everything that can go wrong while handling protocol data. */
sealed interface WeftError {
    /** A fixed-size input (seed, key, id, nonce, signature) has the wrong size. */
    data class InvalidLength(val field: String, val expected: Int, val actual: Int) : WeftError
    data class HeaderLength(val length: Int) : WeftError
    data class FrameTooShort(val length: Int) : WeftError
    data class UnsupportedVersion(val version: UByte) : WeftError
    data class UnknownKind(val kind: UByte) : WeftError
    data class PlaintextTooLarge(val length: Int) : WeftError
    data object PingNotEmpty : WeftError
    data object InvalidUtf8 : WeftError
    data object DecryptionFailed : WeftError
    data object SameDeviceId : WeftError
    data object KeyExchangeFailed : WeftError
    data class UnexpectedFrameType(val type: UByte) : WeftError
    data object MalformedAuthFrame : WeftError
    data object RelayNameTooLong : WeftError
    data object InvalidSignature : WeftError
}

/** The only exception the protocol API throws on bad input; [error] says why. */
class WeftException(val error: WeftError) : Exception(error.toString())

internal fun fail(error: WeftError): Nothing = throw WeftException(error)
