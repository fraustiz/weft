import CoreGraphics
import Foundation
import ImageIO
import Testing
import UniformTypeIdentifiers
@testable import WeftKit

private func encodedImage(_ type: UTType, gray: CGFloat = 1) throws -> Data {
    let space = try #require(CGColorSpace(name: CGColorSpace.sRGB))
    let context = try #require(CGContext(data: nil, width: 4, height: 3, bitsPerComponent: 8,
        bytesPerRow: 16, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
    context.setFillColor(CGColor(gray: gray, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: 4, height: 3))
    let image = try #require(context.makeImage())
    let data = NSMutableData()
    let destination = try #require(CGImageDestinationCreateWithData(data, type.identifier as CFString, 1, nil))
    CGImageDestinationAddImage(destination, image, nil)
    #expect(CGImageDestinationFinalize(destination))
    return data as Data
}

@Test func clipboardImagesValidateTypeAndBoundEncodedInput() throws {
    let png = try encodedImage(.png)
    let tiff = try encodedImage(.tiff)
    #expect(ClipboardImages.prepare(png, mime: "image/png")?.bytes == png)
    #expect(ClipboardImages.prepare(png, mime: "image/jpeg") == nil)
    #expect(ClipboardImages.prepare(png, mime: "image/tiff") == nil)
    #expect(ClipboardImages.prepare(tiff, mime: "image/tiff") == nil)
    #expect(ClipboardImages.prepare(Data([0, 1, 2])) == nil)
    #expect(ClipboardImages.prepare(Data(repeating: 0, count: ClipboardImages.maxBytes + 1)) == nil)
}

@Test func clipboardJpegFingerprintSurvivesPngAndTiffRewrites() throws {
    let jpeg = try encodedImage(.jpeg)
    let original = try #require(ClipboardImages.prepare(jpeg, mime: "image/jpeg"))
    let source = try #require(CGImageSourceCreateWithData(jpeg as CFData, nil))
    let decoded = try #require(CGImageSourceCreateImageAtIndex(source, 0, nil))
    for type in [UTType.png, .tiff] {
        let data = NSMutableData()
        let destination = try #require(CGImageDestinationCreateWithData(data, type.identifier as CFString, 1, nil))
        CGImageDestinationAddImage(destination, decoded, nil)
        #expect(CGImageDestinationFinalize(destination))
        let rewritten = try #require(ClipboardImages.prepare(data as Data))
        #expect(rewritten.fingerprint == original.fingerprint)
    }
}

@Test func clipboardImagePrivacyMarkersWinOverImageTypes() {
    for marker in PasteboardPolicy.excludedTypes {
        for type in ["public.png", "public.jpeg", "public.tiff"] {
            #expect(PasteboardPolicy.decide(types: [type, marker]) == .excluded)
        }
    }
}

@Test func clipboardFingerprintsKeepDifferentImagesDistinct() throws {
    let white = try #require(ClipboardImages.prepare(encodedImage(.png)))
    let black = try #require(ClipboardImages.prepare(encodedImage(.png, gray: 0)))
    #expect(white.fingerprint != black.fingerprint)
}

@Test func clipboardImageFingerprintSurvivesLosslessNativeConversion() throws {
    let png = try #require(ClipboardImages.prepare(encodedImage(.png)))
    let tiff = try #require(ClipboardImages.prepare(encodedImage(.tiff)))
    #expect(tiff.mime == "image/png")
    #expect(ClipboardImages.prepare(tiff.bytes)?.fingerprint == png.fingerprint)
    let jpeg = try #require(ClipboardImages.prepare(encodedImage(.jpeg), mime: "image/jpeg"))
    #expect(jpeg.mime == "image/jpeg")
}

@Test func clipboardDimensionsRejectBombsWithoutDecoding() {
    #expect(ClipboardImages.dimensionsAllowed(width: 5120, height: 2880))
    #expect(!ClipboardImages.dimensionsAllowed(width: 8192, height: 8192))
    #expect(!ClipboardImages.dimensionsAllowed(width: Int64.max, height: Int64.max))
    #expect(!ClipboardImages.dimensionsAllowed(width: 0, height: 1))
    #expect(!ClipboardImages.dimensionsAllowed(width: 1, height: -1))
}
