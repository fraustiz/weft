// Bounded ImageIO handling for clipboard images. Never uses NSImage's
// eager convenience decoder on bytes supplied by another device.
import CoreGraphics
import CryptoKit
import Foundation
import ImageIO
import UniformTypeIdentifiers

public struct ClipboardImageValue {
    public let mime: String
    public let bytes: Data
    public let fingerprint: Data
}

public enum ClipboardImages {
    public static let maxBytes = 16 * 1024 * 1024
    public static let maxPixels = 16 * 1024 * 1024

    /// TIFF is a native pasteboard fallback; only PNG/JPEG travel on the wire.
    /// Inspect dimensions before decoding or converting any compressed image.
    public static func prepare(_ data: Data, mime: String? = nil) -> ClipboardImageValue? {
        // A declared MIME denotes wire input. TIFF is only a local fallback.
        if let mime, mime != "image/png" && mime != "image/jpeg" { return nil }
        guard !data.isEmpty, data.count <= maxBytes,
              let source = CGImageSourceCreateWithData(data as CFData,
                [kCGImageSourceShouldCache: false] as CFDictionary),
              CGImageSourceGetCount(source) == 1,
              let type = CGImageSourceGetType(source) as String?,
              [UTType.png.identifier, UTType.jpeg.identifier, UTType.tiff.identifier].contains(type),
              let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = props[kCGImagePropertyPixelWidth] as? NSNumber,
              let height = props[kCGImagePropertyPixelHeight] as? NSNumber,
              dimensionsAllowed(width: width.int64Value, height: height.int64Value) else { return nil }
        let actualMime = type == UTType.png.identifier ? "image/png"
            : type == UTType.jpeg.identifier ? "image/jpeg" : "image/tiff"
        if let mime, actualMime != mime { return nil }

        guard let image = thumbnail(source, size: max(width.intValue, height.intValue)),
              let fingerprint = fingerprint(image) else { return nil }
        if actualMime != "image/tiff" {
            return ClipboardImageValue(mime: actualMime, bytes: data, fingerprint: fingerprint)
        }
        // Conversion is bounded by the inspected 16-megapixel dimensions.
        let output = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(output, UTType.png.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination), output.length <= maxBytes else { return nil }
        return ClipboardImageValue(mime: "image/png", bytes: output as Data, fingerprint: fingerprint)
    }

    static func dimensionsAllowed(width: Int64, height: Int64) -> Bool {
        width > 0 && height > 0 && width <= Int64(maxPixels) && height <= Int64(maxPixels)
            && width * height <= Int64(maxPixels)
    }

    private static func thumbnail(_ source: CGImageSource, size: Int) -> CGImage? {
        CGImageSourceCreateThumbnailAtIndex(source, 0, [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: size,
            kCGImageSourceShouldCacheImmediately: true,
        ] as CFDictionary)
    }

    /// Full-resolution sRGB pixels, independent of PNG/JPEG/TIFF encoding.
    /// Keeping every pixel prevents two similar screenshots from suppressing each other.
    private static func fingerprint(_ image: CGImage) -> Data? {
        let width = image.width
        let height = image.height
        guard width > 0, height > 0, width <= maxPixels, height <= maxPixels,
              width * height <= maxPixels,
              let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width * 4, space: colorSpace,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue),
              let pixels = context.data else { return nil }
        let bounds = CGRect(x: 0, y: 0, width: width, height: height)
        context.clear(bounds)
        context.interpolationQuality = .none
        context.draw(image, in: bounds)
        var digest = SHA256()
        let dimensions = [UInt32(width).littleEndian, UInt32(height).littleEndian]
        dimensions.withUnsafeBufferPointer { digest.update(data: Data(buffer: $0)) }
        // Hash in place while the context owns the buffer, without another 64 MiB copy.
        digest.update(data: Data(bytesNoCopy: pixels, count: width * height * 4, deallocator: .none))
        return Data(digest.finalize())
    }
}
