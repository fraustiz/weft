package fr.frohub.weft.ui

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import fr.frohub.weft.ui.scan.WeftQrDecoder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith

/** The scanner's decoding, on frames drawn from QR codes encoded here. */
@RunWith(AndroidJUnit4::class)
class QrDecoderTest {
    private val decoder = WeftQrDecoder()

    @Test
    fun readsAWeftCode() {
        val frame = frameOf(CODE)
        assertEquals(CODE, decoder.decode(frame.data, frame.width, frame.height, frame.rowStride))
    }

    @Test
    fun readsALightOnDarkCode() {
        // As a dark terminal draws it.
        val frame = frameOf(CODE, inverted = true)
        assertEquals(CODE, decoder.decode(frame.data, frame.width, frame.height, frame.rowStride))
    }

    @Test
    fun ignoresOtherCodes() {
        for (text in listOf("https://example.com/WEFT1:", "weft1:abc", "WEFT1:")) {
            val frame = frameOf(text)
            assertNull(text, decoder.decode(frame.data, frame.width, frame.height, frame.rowStride))
        }
    }

    @Test
    fun ignoresAFrameWithoutCode() {
        val width = 640
        val height = 480
        assertNull(decoder.decode(ByteArray(width * height) { (it % 251).toByte() }, width, height))
    }

    private class Frame(val data: ByteArray, val width: Int, val height: Int, val rowStride: Int)

    /**
     * A 640x480 luminance plane with the code drawn in the middle, as the camera's Y plane
     * would hold it: each row padded to a wider stride, the last row unpadded.
     */
    private fun frameOf(text: String, inverted: Boolean = false): Frame {
        val width = 640
        val height = 480
        val rowStride = 704
        val size = 360
        val matrix = QRCodeWriter().encode(
            text,
            BarcodeFormat.QR_CODE,
            size,
            size,
            mapOf(EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M, EncodeHintType.MARGIN to 4),
        )
        val dark: Byte = if (inverted) 0xE0.toByte() else 0x20
        val light: Byte = if (inverted) 0x20 else 0xE0.toByte()
        val data = ByteArray(rowStride * (height - 1) + width) { light }
        val left = (width - matrix.width) / 2
        val top = (height - matrix.height) / 2
        for (y in 0 until matrix.height) {
            for (x in 0 until matrix.width) {
                if (matrix[x, y]) data[(top + y) * rowStride + left + x] = dark
            }
        }
        return Frame(data, width, height, rowStride)
    }

    private companion object {
        // Shaped like a real code (base64url, no padding); the scanner only checks the prefix.
        const val CODE = "WEFT1:F3dzczovL3dlZnQuZXhhbXBsZS5ldS92MQABAgMEBQYHCAkKCwwNDg8QERITFBUWFxgZGhscHR4fICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj9AQUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVpbXF1eX2BhYmNkZWZnaGlqa2xtbm9w"
    }
}
