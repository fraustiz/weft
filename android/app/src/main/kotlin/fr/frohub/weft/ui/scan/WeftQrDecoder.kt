package fr.frohub.weft.ui.scan

import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.ReaderException
import com.google.zxing.common.HybridBinarizer

/**
 * Finds a Weft pairing code in a camera frame. Only QR codes whose text starts with `WEFT1:` count;
 * anything else in view is ignored. Not thread-safe: one instance per analyzer thread.
 */
class WeftQrDecoder {
    private val reader = MultiFormatReader().apply {
        setHints(
            mapOf(
                DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE),
                // Terminals often draw the code light on dark.
                DecodeHintType.ALSO_INVERTED to true,
            ),
        )
    }

    /**
     * Decodes the luminance plane of a frame: [data] holds [height] rows of [rowStride] bytes
     * (the last one may be shorter), of which the first [width] are pixels. Returns the code, or
     * null if there is no Weft code in it.
     */
    fun decode(data: ByteArray, width: Int, height: Int, rowStride: Int = width): String? {
        val source = PlanarYUVLuminanceSource(data, rowStride, height, 0, 0, width, height, false)
        val text = try {
            reader.decodeWithState(BinaryBitmap(HybridBinarizer(source))).text
        } catch (_: ReaderException) {
            return null
        } finally {
            reader.reset()
        }
        return text?.trim()?.takeIf(::isWeftCode)
    }

    companion object {
        const val PREFIX = "WEFT1:"

        fun isWeftCode(text: String): Boolean = text.startsWith(PREFIX) && text.length > PREFIX.length
    }
}
