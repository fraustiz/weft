package fr.frohub.weft.ui.scan

import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Looks for a Weft code in each frame, on the analysis thread, and reports the first one found
 * once. Frames are YUV_420_888: the Y plane alone is the luminance ZXing needs, and a QR code
 * reads in any rotation, so frames are not rotated.
 */
class QrAnalyzer(private val onCode: (String) -> Unit) : ImageAnalysis.Analyzer {
    private val decoder = WeftQrDecoder()
    private val found = AtomicBoolean(false)
    private var buffer = ByteArray(0)

    override fun analyze(image: ImageProxy) {
        image.use {
            if (found.get()) return
            val plane = image.planes[0]
            val pixels = plane.buffer.apply { rewind() }
            if (buffer.size != pixels.remaining()) buffer = ByteArray(pixels.remaining())
            pixels.get(buffer)
            val code = decoder.decode(buffer, image.width, image.height, plane.rowStride) ?: return
            if (found.compareAndSet(false, true)) onCode(code)
        }
    }
}
