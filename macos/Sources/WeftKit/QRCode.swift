// QR codes for pairing codes, drawn module by module: CoreImage gives the
// matrix, and each module becomes a square of whole pixels, so the code
// stays sharp at any size, with a quiet zone of four modules.

import CoreGraphics
import Foundation
import CoreImage
import CoreImage.CIFilterBuiltins

public struct QRCode: Sendable, Equatable {
    /// Dark modules, row by row from the top, without any margin.
    public let modules: [[Bool]]

    public var size: Int { modules.count }

    /// The quiet zone the standard asks for, in modules.
    public static let quietZone = 4

    /// The matrix of `text`, at error correction level M (15 %), or `nil`
    /// if the text does not fit in a QR code.
    public init?(_ text: String) {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        let extent = output.extent.integral
        let width = Int(extent.width), height = Int(extent.height)
        guard width > 0, width == height else { return nil }
        // One byte per module, read straight from CoreImage's output.
        var pixels = [UInt8](repeating: 255, count: width * height)
        let context = CIContext(options: [.workingColorSpace: NSNull()])
        context.render(
            output,
            toBitmap: &pixels,
            rowBytes: width,
            bounds: extent,
            format: .L8,
            colorSpace: nil
        )
        var rows = (0..<height).map { row in
            (0..<width).map { column in pixels[row * width + column] < 128 }
        }
        // CoreImage adds a white border of its own: drop it, so the quiet
        // zone is exactly the one drawn.
        while let first = rows.first, !first.contains(true) { rows.removeFirst() }
        while let last = rows.last, !last.contains(true) { rows.removeLast() }
        let used = rows.reduce(into: (Int.max, -1)) { range, row in
            if let first = row.firstIndex(of: true) { range.0 = min(range.0, first) }
            if let last = row.lastIndex(of: true) { range.1 = max(range.1, last) }
        }
        guard used.1 >= used.0 else { return nil }
        modules = rows.map { Array($0[used.0...used.1]) }
        guard modules.count == modules.first?.count else { return nil }
    }

    /// Black modules on white, `scale` pixels per module, with the quiet
    /// zone. Opaque, in the sRGB space.
    public func image(scale: Int) -> CGImage? {
        let scale = max(1, scale)
        let side = (size + 2 * Self.quietZone) * scale
        guard
            let space = CGColorSpace(name: CGColorSpace.sRGB),
            let context = CGContext(
                data: nil,
                width: side,
                height: side,
                bitsPerComponent: 8,
                bytesPerRow: 0,
                space: space,
                bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
            )
        else { return nil }
        context.setShouldAntialias(false)
        context.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: side, height: side))
        context.setFillColor(CGColor(red: 0, green: 0, blue: 0, alpha: 1))
        for (row, line) in modules.enumerated() {
            for (column, dark) in line.enumerated() where dark {
                // Core Graphics counts rows from the bottom.
                let x = (column + Self.quietZone) * scale
                let y = side - (row + Self.quietZone + 1) * scale
                context.fill(CGRect(x: x, y: y, width: scale, height: scale))
            }
        }
        return context.makeImage()
    }
}
