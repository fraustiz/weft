package fr.frohub.weft.core

private const val DIGITS = "0123456789abcdef"

/** Lowercase hex, as in the state file and the CLI. */
internal fun ByteArray.toHex(): String = buildString(size * 2) {
    for (byte in this@toHex) {
        val v = byte.toInt() and 0xff
        append(DIGITS[v ushr 4])
        append(DIGITS[v and 0x0f])
    }
}

/** Parses hex (either case) of exactly [length] bytes, or returns null. */
internal fun String.hexToBytesOrNull(length: Int): ByteArray? {
    if (this.length != length * 2) return null
    val out = ByteArray(length)
    for (i in 0 until length) {
        val hi = Character.digit(this[2 * i], 16)
        val lo = Character.digit(this[2 * i + 1], 16)
        if (hi < 0 || lo < 0) return null
        out[i] = ((hi shl 4) or lo).toByte()
    }
    return out
}

/** The first 4 bytes of a device id in hex: enough to tell devices apart in logs. */
internal fun shortId(deviceIdHex: String): String = deviceIdHex.take(8)
