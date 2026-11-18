// Interface language (EN/FR) and cross-platform formatting, validation, QR
// rendering and remote-input helpers shared by WeftMobileKit and WeftiOS.

import CoreGraphics
import CoreImage
import CoreImage.CIFilterBuiltins
import Foundation
import WeftProtocol

public enum Language: Sendable, Equatable {
    case english
    case french

    /// The language of this process, decided once from the user's preferred
    /// languages. `Bundle.main.preferredLocalizations` is not used: it falls
    /// back to the development region (English) whenever the bundle declares
    /// no French localization, which made the app English on French devices.
    public static let current: Language = detect(
        forced: ProcessInfo.processInfo.environment["WEFT_LANG"],
        preferred: Locale.preferredLanguages
    )

    /// `forced` wins when it names a language; else the first preferred
    /// language that is English or French.
    public static func detect(forced: String?, preferred: [String]) -> Language {
        if let forced = forced?.lowercased() {
            if forced.hasPrefix("fr") { return .french }
            if forced.hasPrefix("en") { return .english }
        }
        for code in preferred {
            let lower = code.lowercased()
            if lower.hasPrefix("fr") { return .french }
            if lower.hasPrefix("en") { return .english }
        }
        return .english
    }
}

/// Picks the English or the French version of a text.
public func t(_ english: String, _ french: String) -> String {
    Language.current == .french ? french : english
}

// MARK: - Share extension inbox

public enum ShareInbox {
    /// The share extension queues a file as `<UUID>_<original name>`; this
    /// returns the original name, or `fileName` unchanged without that prefix.
    public static func originalName(of fileName: String) -> String {
        let parts = fileName.split(separator: "_", maxSplits: 1, omittingEmptySubsequences: false)
        guard parts.count == 2, UUID(uuidString: String(parts[0])) != nil, !parts[1].isEmpty else {
            return fileName
        }
        return String(parts[1])
    }
}

// MARK: - Hex helpers

extension Data {
    /// Lowercase hexadecimal representation of the bytes.
    public var hexString: String {
        map { String(format: "%02x", $0) }.joined()
    }

    /// Decodes an even-length hexadecimal string, optionally requiring a byte count.
    public init?(hexString: String, expectedCount: Int? = nil) {
        let utf8 = Array(hexString.utf8)
        guard utf8.count % 2 == 0 else { return nil }
        let byteCount = utf8.count / 2
        if let expectedCount, byteCount != expectedCount { return nil }
        var bytes = [UInt8]()
        bytes.reserveCapacity(byteCount)
        func nibble(_ byte: UInt8) -> UInt8? {
            switch byte {
            case 0x30...0x39: byte - 0x30
            case 0x41...0x46: byte - 0x41 + 10
            case 0x61...0x66: byte - 0x61 + 10
            default: nil
            }
        }
        var index = 0
        while index < utf8.count {
            guard let hi = nibble(utf8[index]), let lo = nibble(utf8[index + 1]) else {
                return nil
            }
            bytes.append((hi << 4) | lo)
            index += 2
        }
        self.init(bytes)
    }
}

// MARK: - Device name validation (docs/protocol.md, kind 0x06)

public enum NameProblem: Error, Sendable, Equatable {
    case empty
    case controlCharacters
    case tooLong

    public var text: String {
        switch self {
        case .empty:
            t("Enter a name", "Saisissez un nom")
        case .controlCharacters:
            t("Remove tabs and other control characters",
              "Retirez les tabulations et autres caractères de contrôle")
        case .tooLong:
            t("Too long: 64 bytes at most, and accented letters or emoji take more than one",
              "Trop long : 64 octets au plus, et une lettre accentuée ou un emoji en prend plusieurs")
        }
    }
}

public enum DeviceName {
    public static let maxBytes = Weft.maxDeviceNameLength

    public static func check(_ typed: String) -> Result<String, NameProblem> {
        let trimmed = typed.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            return .failure(.empty)
        }
        if trimmed.unicodeScalars.contains(where: isControl) {
            return .failure(.controlCharacters)
        }
        if trimmed.utf8.count > maxBytes {
            return .failure(.tooLong)
        }
        return .success(trimmed)
    }

    public static func sanitizedFallback(_ raw: String, fallback: String = "iPhone") -> String {
        let filteredScalars = raw
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .unicodeScalars
            .filter { !isControl($0) }
        var result = ""
        var used = 0
        for scalar in filteredScalars {
            let chunk = String(scalar)
            let count = chunk.utf8.count
            if used + count > maxBytes { break }
            result.append(chunk)
            used += count
        }
        let trimmed = result.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? fallback : trimmed
    }

    public static func isControl(_ scalar: Unicode.Scalar) -> Bool {
        let value = scalar.value
        return value <= 0x1F || (0x7F...0x9F).contains(value)
    }
}

// MARK: - Shareable URLs (docs/features.md, section 4)

public enum Links {
    public static let maxBytes = 2048

    public static func isShareable(_ url: String) -> Bool {
        guard !url.isEmpty, url.utf8.count <= maxBytes else { return false }
        if url.unicodeScalars.contains(where: {
            CharacterSet.whitespacesAndNewlines.contains($0) || CharacterSet.controlCharacters.contains($0)
        }) {
            return false
        }
        let lower = url.lowercased()
        let rest: Substring
        if lower.hasPrefix("https://") {
            rest = lower.dropFirst("https://".count)
        } else if lower.hasPrefix("http://") {
            rest = lower.dropFirst("http://".count)
        } else {
            return false
        }
        let authority = rest.split(omittingEmptySubsequences: false, whereSeparator: { "/?#".contains($0) })
            .first ?? ""
        let host = authority.split(separator: "@", omittingEmptySubsequences: false).last ?? ""
        return !host.isEmpty && !host.hasPrefix(":")
    }

    public static func candidate(fromClipboard text: String?) -> String? {
        guard let trimmed = text?.trimmingCharacters(in: .whitespacesAndNewlines),
              isShareable(trimmed) else { return nil }
        return trimmed
    }

    public static func openable(_ url: String) -> URL? {
        let trimmed = url.trimmingCharacters(in: .whitespacesAndNewlines)
        guard isShareable(trimmed),
              let parsed = URL(string: trimmed),
              let scheme = parsed.scheme?.lowercased(),
              scheme == "http" || scheme == "https" else {
            return nil
        }
        return parsed
    }
}

// MARK: - Media clock (docs/features.md, section 6)

public enum MediaClock {
    public static func position(
        positionMs: UInt64?,
        lengthMs: UInt64?,
        playing: Bool,
        receivedAtMs: Int64,
        nowMs: Int64
    ) -> UInt64? {
        guard let positionMs else { return nil }
        guard playing else { return positionMs }
        let elapsed = UInt64(max(0, nowMs - receivedAtMs))
        let (moved, overflow) = positionMs.addingReportingOverflow(elapsed)
        let clamped = overflow ? UInt64.max : moved
        if let lengthMs, lengthMs > 0 { return min(clamped, lengthMs) }
        return clamped
    }

    public static func time(_ ms: UInt64) -> String {
        let seconds = ms / 1000
        let hours = seconds / 3600
        let minutes = (seconds / 60) % 60
        let rest = seconds % 60
        if hours > 0 {
            return String(format: "%llu:%02llu:%02llu", hours, minutes, rest)
        }
        return String(format: "%llu:%02llu", minutes, rest)
    }

    public static func isSeek(from shownMs: UInt64?, to wantedMs: UInt64) -> Bool {
        guard let shownMs else { return true }
        let distance = shownMs > wantedMs ? shownMs - wantedMs : wantedMs - shownMs
        return distance > 1500
    }
}

// MARK: - Human-facing words

public enum Words {
    public static func platform(_ platform: String?) -> String? {
        switch platform {
        case "android": "Android"
        case "windows": "Windows"
        case "macos": "macOS"
        case "ios": "iOS"
        case "linux": "Linux"
        case "cli": t("Command line", "Ligne de commande")
        case nil, "": nil
        case let other?: other
        }
    }

    public static func symbol(_ platform: String?) -> String {
        switch platform {
        case "android", "ios": "iphone"
        case "windows": "pc"
        case "macos": "laptopcomputer"
        case "cli", "linux": "terminal"
        default: "laptopcomputer.and.iphone"
        }
    }

    public static func battery(level: UInt8, language: Language = .current) -> String {
        language == .french ? "\(level)\u{00A0}%" : "\(level)%"
    }

    public static func batterySymbol(level: UInt8, charging: Bool) -> String {
        if charging { return "battery.100percent.bolt" }
        switch level {
        case 0..<13: return "battery.0percent"
        case 13..<38: return "battery.25percent"
        case 38..<63: return "battery.50percent"
        case 63..<88: return "battery.75percent"
        default: return "battery.100percent"
        }
    }

    public static func countdown(secondsLeft: Int) -> String {
        let seconds = max(0, secondsLeft)
        return String(format: "%d:%02d", seconds / 60, seconds % 60)
    }

    public static func relayHost(_ url: String) -> String {
        guard let components = URLComponents(string: url), let host = components.host else {
            return url
        }
        if let port = components.port { return "\(host):\(port)" }
        return host
    }

    public static func deviceCount(_ count: Int, language: Language = .current) -> String {
        switch (language, count) {
        case (.english, 0): "No devices"
        case (.english, 1): "1 device"
        case (.english, _): "\(count) devices"
        case (.french, 0): "Aucun appareil"
        case (.french, 1): "1 appareil"
        case (.french, _): "\(count) appareils"
        }
    }

    public static func network(_ network: String?, language: Language = .current) -> String? {
        switch network {
        case nil, "": nil
        case "wifi": "Wi-Fi"
        case "none": language == .french ? "Pas de réseau" : "No network"
        case let other?: other
        }
    }

    public static func networkSymbol(_ network: String?) -> String {
        switch network {
        case "wifi": "wifi"
        case "none": "antenna.radiowaves.left.and.right.slash"
        default: "cellularbars"
        }
    }

    public static func signalFraction(_ bars: UInt8?) -> Double {
        guard let bars else { return 1 }
        return Double(min(bars, 4)) / 4.0
    }

    public static func size(_ bytes: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
    }

    public static func shortID(_ hex: String) -> String {
        String(hex.prefix(8))
    }
}

// MARK: - Sharp QR matrix & image renderer

public struct QRCode: Sendable, Equatable {
    public let modules: [[Bool]]

    public var size: Int { modules.count }

    public static let quietZone = 4

    public init?(_ text: String) {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        let extent = output.extent.integral
        let width = Int(extent.width)
        let height = Int(extent.height)
        guard width > 0, width == height else { return nil }
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
                let x = (column + Self.quietZone) * scale
                let y = side - (row + Self.quietZone + 1) * scale
                context.fill(CGRect(x: x, y: y, width: scale, height: scale))
            }
        }
        return context.makeImage()
    }
}

// MARK: - Remote input motion math (docs/features.md, section 8)

public enum RemoteInputMath {
    private static let halfWidthRadians = 25.0 * .pi / 180.0
    private static let halfHeightRadians = 15.0 * .pi / 180.0

    /// Velocity-sensitive pointer gain matching Android's `pointerGain`.
    public static func pointerGain(speedPointsPerMs: Double, sensitivity: Double) -> Double {
        sensitivity * (1.0 + min(max(0.0, speedPointsPerMs), 2.0))
    }

    /// Character-level diff between `old` and `new` as `(backspaces, insertedText)`
    /// in Unicode scalar count so surrogate pairs / emoji never split.
    public static func typedChange(old: String, new: String) -> (backspaces: Int, inserted: String) {
        let oldScalars = Array(old.unicodeScalars)
        let newScalars = Array(new.unicodeScalars)
        let shorter = min(oldScalars.count, newScalars.count)
        var common = 0
        while common < shorter && oldScalars[common] == newScalars[common] {
            common += 1
        }
        let backspaces = oldScalars.count - common
        var inserted = String.UnicodeScalarView()
        inserted.append(contentsOf: newScalars[common...])
        return (backspaces, String(inserted))
    }

    /// Maps attitude yaw & pitch (in radians) relative to initial `(yaw0, pitch0)`
    /// into normalized presenter coordinates `x, y` in `-1...1`.
    /// CoreMotion yaw increases counter-clockwise when looking down; turning right
    /// decreases yaw (`yaw0 - yaw > 0 -> +x`), and pitching the top of the phone up
    /// increases pitch (`pitch0 - pitch < 0 -> -y`, moving spot toward the top edge `-1`).
    public static func presenterSpot(
        yaw0: Double,
        pitch0: Double,
        yaw: Double,
        pitch: Double
    ) -> (x: Double, y: Double) {
        var turn = yaw0 - yaw
        while turn > .pi { turn -= 2.0 * .pi }
        while turn < -.pi { turn += 2.0 * .pi }
        let x = min(max(turn / halfWidthRadians, -1.0), 1.0)
        let y = min(max((pitch0 - pitch) / halfHeightRadians, -1.0), 1.0)
        return (x, y)
    }
}
