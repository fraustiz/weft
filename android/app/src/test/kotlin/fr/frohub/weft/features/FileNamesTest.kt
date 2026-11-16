package fr.frohub.weft.features

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FileNamesTest {
    private fun utf8(text: String) = text.toByteArray(Charsets.UTF_8).size

    @Test
    fun keepsOrdinaryNames() {
        assertEquals("photo.jpg", sanitizeFileName("photo.jpg"))
        assertEquals("Été 2026 (final).pdf", sanitizeFileName("Été 2026 (final).pdf"))
        assertEquals("archive.tar.gz", sanitizeFileName("archive.tar.gz"))
    }

    @Test
    fun noPathsNoDotDot() {
        assertEquals("_etc_passwd", sanitizeFileName("/etc/passwd"))
        assertEquals("file", sanitizeFileName(".."))
        assertEquals("bashrc", sanitizeFileName(".bashrc"))
        for (name in listOf("../x", "..", ".", "...", "a/../b", "..\\..\\x")) {
            val safe = sanitizeFileName(name)
            assertFalse(safe, safe.contains('/') || safe.contains('\\') || safe.contains(".."))
            assertFalse(safe, safe.startsWith("."))
            assertTrue(safe.isNotEmpty())
        }
    }

    @Test
    fun noControlOrForbiddenCharacters() {
        assertEquals("a_b_c.txt", sanitizeFileName("a\u0000b\nc.txt"))
        assertEquals("a_b_.txt", sanitizeFileName("a:b?.txt"))
        // Right-to-left override, used to fake an extension.
        assertEquals("invoice_fdp.exe", sanitizeFileName("invoice‮fdp.exe"))
        assertEquals("file", sanitizeFileName("   "))
        assertEquals("file", sanitizeFileName(""))
        assertEquals("name", sanitizeFileName("name. . "))
    }

    @Test
    fun reservedWindowsNames() {
        assertEquals("_CON.txt", sanitizeFileName("CON.txt"))
        assertEquals("_lpt1", sanitizeFileName("lpt1"))
        assertEquals("console.txt", sanitizeFileName("console.txt"))
    }

    @Test
    fun cutTo200BytesKeepingTheExtension() {
        val long = sanitizeFileName("é".repeat(300) + ".jpeg")
        assertTrue(utf8(long) <= MAX_FILE_NAME_LENGTH)
        assertTrue(long.endsWith(".jpeg"))
        assertTrue(long.startsWith("éé"))
        // An emoji is never cut in half.
        val emoji = sanitizeFileName("😀".repeat(100) + ".png")
        assertTrue(utf8(emoji) <= MAX_FILE_NAME_LENGTH)
        assertEquals(0, emoji.removeSuffix(".png").length % 2)
    }

    @Test
    fun uniqueNamesNeverOverwrite() {
        val taken = mutableSetOf("report.pdf", "report (1).pdf", "notes")
        assertEquals("other.pdf", uniqueFileName("other.pdf", taken::contains))
        assertEquals("report (2).pdf", uniqueFileName("report.pdf", taken::contains))
        assertEquals("notes (1)", uniqueFileName("notes", taken::contains))
        val long = sanitizeFileName("a".repeat(250) + ".txt")
        val next = uniqueFileName(long) { it == long }
        assertTrue(next.endsWith(" (1).txt"))
        assertTrue(utf8(next) <= MAX_FILE_NAME_LENGTH)
    }
}
