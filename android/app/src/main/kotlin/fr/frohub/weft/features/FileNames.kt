package fr.frohub.weft.features

/** Largest received file name, in bytes of UTF-8 (docs/features.md, section 4). */
const val MAX_FILE_NAME_LENGTH = 200

private const val FALLBACK_NAME = "file"

/** Names Windows reserves, with or without an extension. */
private val RESERVED = setOf("con", "prn", "aux", "nul") +
    (1..9).flatMap { listOf("com$it", "lpt$it") }

private val DOT_RUN = Regex("\\.{2,}")

/** Longest extension kept apart when the name is cut. */
private const val MAX_EXTENSION_LENGTH = 16

/**
 * Makes a received file name safe to save: path separators, control characters and characters
 * Windows forbids become `_`, runs of dots become one and leading dots go (so no `..` and no
 * hidden file), reserved Windows names get a `_` prefix, and the name is cut to 200 bytes of UTF-8 with
 * its extension kept. Never empty.
 */
fun sanitizeFileName(raw: String): String {
    val replaced = buildString {
        var i = 0
        while (i < raw.length) {
            val codePoint = raw.codePointAt(i)
            i += Character.charCount(codePoint)
            val unsafe = Character.isISOControl(codePoint) ||
                Character.getType(codePoint) == Character.FORMAT.toInt() ||
                codePoint in Character.MIN_SURROGATE.code..Character.MAX_SURROGATE.code ||
                codePoint.toChar() in "/\\:*?\"<>|" && codePoint < 0x80
            if (unsafe) append('_') else appendCodePoint(codePoint)
        }
    }
    // Windows also drops trailing dots and spaces; a leading dot hides the file (and makes `..`).
    var name = replaced.replace(DOT_RUN, ".").trim().trimStart('.').trimEnd('.', ' ').trim()
    if (name.isEmpty()) name = FALLBACK_NAME
    val (base, extension) = splitExtension(name)
    var safeBase = base
    if (safeBase.lowercase() in RESERVED) safeBase = "_$safeBase"
    return fitName(safeBase, extension)
}

/**
 * The first of `name`, `name (1).ext`, `name (2).ext`… for which [exists] is false, so a
 * received file never replaces another. [name] must already be sanitised.
 */
fun uniqueFileName(name: String, exists: (String) -> Boolean): String {
    if (!exists(name)) return name
    val (base, extension) = splitExtension(name)
    var n = 1
    while (true) {
        val candidate = fitName(base, extension, " ($n)")
        if (!exists(candidate)) return candidate
        n++
    }
}

/** `("report", ".pdf")`; no extension for a name without a dot or one that only starts with it. */
private fun splitExtension(name: String): Pair<String, String> {
    val dot = name.lastIndexOf('.')
    if (dot <= 0 || name.length - dot > MAX_EXTENSION_LENGTH + 1 || dot == name.length - 1) return name to ""
    return name.substring(0, dot) to name.substring(dot)
}

/** `base + suffix + extension`, with [base] cut so the whole fits in 200 bytes. */
private fun fitName(base: String, extension: String, suffix: String = ""): String {
    val budget = MAX_FILE_NAME_LENGTH - utf8Length(extension) - utf8Length(suffix)
    val cut = StringBuilder()
    var bytes = 0
    var i = 0
    while (i < base.length) {
        val codePoint = base.codePointAt(i)
        val size = utf8Length(String(Character.toChars(codePoint)))
        if (bytes + size > budget) break
        cut.appendCodePoint(codePoint)
        bytes += size
        i += Character.charCount(codePoint)
    }
    val trimmed = cut.toString().trimEnd('.', ' ').ifEmpty { FALLBACK_NAME }
    return trimmed + suffix + extension
}

private fun utf8Length(text: String) = text.toByteArray(Charsets.UTF_8).size
