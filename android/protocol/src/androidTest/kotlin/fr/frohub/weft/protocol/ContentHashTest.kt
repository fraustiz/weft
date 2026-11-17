package fr.frohub.weft.protocol

import org.junit.Assert.assertEquals
import org.junit.Test

class ContentHashTest {
    @Test fun matchesBlake2b128Vectors() {
        fun hex(bytes: ByteArray) = bytes.joinToString("") { "%02x".format(it.toInt() and 255) }
        assertEquals("cae66941d9efbd404e4d88758ea67670", hex(contentHash(byteArrayOf())))
        assertEquals("cf4ab791c62b8d2b2109c90275287816", hex(contentHash("abc".toByteArray())))
    }
}
