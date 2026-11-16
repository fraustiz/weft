package fr.frohub.weft.core

import android.os.Build
import fr.frohub.weft.protocol.Weft
import fr.frohub.weft.protocol.WeftError
import fr.frohub.weft.protocol.WeftException
import fr.frohub.weft.protocol.encodeDeviceName

/** Name used when the model name gives nothing usable. */
const val FALLBACK_DEVICE_NAME = "Android phone"

/** Why a name cannot be a Device name (docs/protocol.md, section 3). */
enum class NameProblem {
    /** No characters at all. */
    EMPTY,

    /** More than [Weft.MAX_DEVICE_NAME_LENGTH] bytes of UTF-8. */
    TOO_LONG,

    /** A control character (line break, tab…) or a broken one. */
    INVALID_CHARACTER,
}

/** Null if [name] is a valid Device name: 1 to 64 bytes of UTF-8, no control characters. */
fun deviceNameProblem(name: String): NameProblem? = try {
    encodeDeviceName(name)
    null
} catch (e: WeftException) {
    when (val error = e.error) {
        is WeftError.DeviceNameLength -> if (error.length == 0) NameProblem.EMPTY else NameProblem.TOO_LONG
        else -> NameProblem.INVALID_CHARACTER
    }
}

/**
 * Makes [raw] a valid Device name, as the CLI does with the host name: control characters
 * removed, surrounding whitespace trimmed, cut to 64 bytes without splitting a character.
 * Null if nothing is left.
 */
fun sanitizeDeviceName(raw: String): String? {
    val cleaned = buildString {
        var i = 0
        while (i < raw.length) {
            val codePoint = raw.codePointAt(i)
            i += Character.charCount(codePoint)
            if (Character.getType(codePoint) == Character.CONTROL.toInt()) continue
            // An unpaired surrogate cannot be encoded.
            if (codePoint in Character.MIN_SURROGATE.code..Character.MAX_SURROGATE.code) continue
            appendCodePoint(codePoint)
        }
    }.trim()
    var name = cleaned
    if (name.toByteArray(Charsets.UTF_8).size > Weft.MAX_DEVICE_NAME_LENGTH) {
        val cut = StringBuilder()
        var bytes = 0
        var i = 0
        while (i < name.length) {
            val codePoint = name.codePointAt(i)
            val size = String(Character.toChars(codePoint)).toByteArray(Charsets.UTF_8).size
            if (bytes + size > Weft.MAX_DEVICE_NAME_LENGTH) break
            cut.appendCodePoint(codePoint)
            bytes += size
            i += Character.charCount(codePoint)
        }
        name = cut.toString().trimEnd()
    }
    return name.takeIf { deviceNameProblem(it) == null }
}

/** This phone's name until the user picks one: its model, or [FALLBACK_DEVICE_NAME]. */
fun defaultDeviceName(): String = sanitizeDeviceName(Build.MODEL.orEmpty()) ?: FALLBACK_DEVICE_NAME

/** How [WeftCore.rename] ended. */
sealed interface RenameResult {
    /** Saved, and sent to [sentTo] devices (0 when this phone is not paired). */
    data class Sent(val sentTo: Int) : RenameResult

    /** Saved on this phone, but the relay could not be reached: the other devices keep the old name. */
    data object NotSent : RenameResult

    /** Not a valid name; nothing changed. */
    data class Invalid(val problem: NameProblem) : RenameResult
}
