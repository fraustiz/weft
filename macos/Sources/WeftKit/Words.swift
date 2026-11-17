// What the app says, decided apart from the views so it can be tested.

import Foundation

public enum Words {
    /// A platform from a device's capabilities, as people name it.
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

    /// The SF Symbol of a platform; a generic device until it is known.
    public static func symbol(_ platform: String?) -> String {
        switch platform {
        case "android", "ios": "smartphone"
        case "windows": "pc"
        case "macos": "laptopcomputer"
        case "cli", "linux": "terminal"
        default: "laptopcomputer.and.iphone"
        }
    }

    /// A battery level as a figure; French puts a no-break space before
    /// the sign.
    public static func battery(level: UInt8, language: Language = .current) -> String {
        language == .french ? "\(level)\u{a0}%" : "\(level)%"
    }

    /// The state that goes with a battery level, if any.
    public static func batteryState(charging: Bool, low: Bool) -> String? {
        if charging { return t("Charging", "En charge") }
        if low { return t("Low battery", "Batterie faible") }
        return nil
    }

    /// The SF Symbol of a battery level.
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

    /// Time left on a pairing code, as `4:05`.
    public static func countdown(secondsLeft: Int) -> String {
        let seconds = max(0, secondsLeft)
        return String(format: "%d:%02d", seconds / 60, seconds % 60)
    }

    /// The host of a relay url, as it is shown: `weft.frohub.eu`.
    public static func relayHost(_ url: String) -> String {
        guard let components = URLComponents(string: url), let host = components.host else {
            return url
        }
        if let port = components.port { return "\(host):\(port)" }
        return host
    }

    /// How many other devices there are.
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
}

extension Words {
    /// A `connectivity` network as people name it; `nil` when unknown.
    public static func network(_ network: String?, language: Language = .current) -> String? {
        switch network {
        case nil, "": nil
        case "wifi": "Wi-Fi"
        case "none": language == .french ? "Pas de réseau" : "No network"
        case let other?: other
        }
    }

    /// The SF Symbol of a network: Wi-Fi, or cellular bars.
    public static func networkSymbol(_ network: String?) -> String {
        switch network {
        case "wifi": "wifi"
        case "none": "antenna.radiowaves.left.and.right.slash"
        default: "cellularbars"
        }
    }

    /// 0 to 4 bars as the fraction the cellular symbol fills.
    public static func signalFraction(_ bars: UInt8?) -> Double {
        guard let bars else { return 1 }
        return Double(min(bars, 4)) / 4
    }

    /// A size in bytes, as the Finder writes it.
    public static func size(_ bytes: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
    }
}
