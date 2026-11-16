// Bounds of the packet (0x07) and file chunk (0x08) plaintexts, as in
// crates/weft-protocol/tests/features.rs. Pure Kotlin, no libsodium: runs on the JVM.

package fr.frohub.weft.protocol

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

private fun errorOf(block: () -> Unit): WeftError? = try {
    block()
    null
} catch (e: WeftException) {
    e.error
}

class FeatureBoundsTests {
    @Test
    fun kindBytes() {
        assertEquals(Kind.PACKET, Kind.fromValue(0x07u))
        assertEquals(Kind.FILE_CHUNK, Kind.fromValue(0x08u))
        assertNull(Kind.fromValue(0x09u))
    }

    @Test
    fun packetIs1To256KibOfUtf8() {
        assertEquals(262144, Weft.MAX_PACKET_LENGTH)
        assertNull(errorOf { Kind.PACKET.checkPlaintext("""{"type":"ping","message":"héllo"}""".toByteArray()) })
        assertNull(errorOf { Kind.PACKET.checkPlaintext(ByteArray(Weft.MAX_PACKET_LENGTH) { 'a'.code.toByte() }) })
        // The JSON itself is not checked at this layer.
        assertNull(errorOf { Kind.PACKET.checkPlaintext("not json".toByteArray()) })

        assertEquals(WeftError.InvalidPlaintextLength(0), errorOf { Kind.PACKET.checkPlaintext(ByteArray(0)) })
        assertEquals(
            WeftError.PlaintextTooLarge(Weft.MAX_PACKET_LENGTH + 1),
            errorOf { Kind.PACKET.checkPlaintextLength(Weft.MAX_PACKET_LENGTH + 1) },
        )
        val badUtf8 = byteArrayOf('{'.code.toByte(), 0xff.toByte(), '}'.code.toByte())
        assertEquals(WeftError.InvalidUtf8, errorOf { Kind.PACKET.checkPlaintext(badUtf8) })
    }

    @Test
    fun fileChunkRoundTrip() {
        val id = ByteArray(16) { 0x42 }
        val plaintext = encodeFileChunk(id, 0x0102030405060708uL, "data".toByteArray())
        assertArrayEquals(id, plaintext.copyOfRange(0, 16))
        assertArrayEquals(byteArrayOf(1, 2, 3, 4, 5, 6, 7, 8), plaintext.copyOfRange(16, 24))
        assertArrayEquals("data".toByteArray(), plaintext.copyOfRange(24, plaintext.size))
        assertEquals(FileChunk(id, 0x0102030405060708uL, "data".toByteArray()), decodeFileChunk(plaintext))
        // The offset is unsigned.
        assertEquals(ULong.MAX_VALUE, decodeFileChunk(encodeFileChunk(id, ULong.MAX_VALUE, byteArrayOf(0))).offset)
    }

    @Test
    fun fileChunkDataIs1To128Kib() {
        assertEquals(131072, Weft.MAX_FILE_CHUNK_DATA_LENGTH)
        val id = ByteArray(16) { 1 }
        val max = ByteArray(Weft.MAX_FILE_CHUNK_DATA_LENGTH) { 0xab.toByte() }
        val plaintext = encodeFileChunk(id, 0uL, max)
        assertEquals(24 + Weft.MAX_FILE_CHUNK_DATA_LENGTH, plaintext.size)
        assertArrayEquals(max, decodeFileChunk(plaintext).data)
        assertNull(errorOf { Kind.FILE_CHUNK.checkPlaintext(plaintext) })

        assertEquals(WeftError.InvalidPlaintextLength(24), errorOf { encodeFileChunk(id, 0uL, ByteArray(0)) })
        val tooLarge = 24 + Weft.MAX_FILE_CHUNK_DATA_LENGTH + 1
        assertEquals(
            WeftError.PlaintextTooLarge(tooLarge),
            errorOf { encodeFileChunk(id, 0uL, ByteArray(Weft.MAX_FILE_CHUNK_DATA_LENGTH + 1)) },
        )
        for (length in listOf(0, 1, 23, 24)) {
            assertEquals(WeftError.InvalidPlaintextLength(length), errorOf { decodeFileChunk(ByteArray(length)) })
        }
        assertEquals(WeftError.PlaintextTooLarge(tooLarge), errorOf { decodeFileChunk(ByteArray(tooLarge)) })
        assertEquals(
            WeftError.InvalidLength("transferId", 16, 15),
            errorOf { encodeFileChunk(ByteArray(15), 0uL, byteArrayOf(0)) },
        )
    }
}
