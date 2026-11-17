// Ready-made macOS shortcuts for the Shortcuts tab (docs/features.md
// section 8, `command.list` and `command.run`). Only the shortcut's name
// goes to the other devices; the command line stays on this Mac.

import Foundation

public struct ShortcutPreset: Equatable, Sendable, Identifiable {
    public let id: String
    public let name: String
    public let line: String

    public init(id: String, name: String, line: String) {
        self.id = id
        self.name = name
        self.line = line
    }
}

public struct CleanedShortcut: Equatable, Sendable {
    public let name: String
    public let line: String

    public init(name: String, line: String) {
        self.name = name
        self.line = line
    }
}

public enum ShortcutPresets {
    public static func all(language: Language = .current) -> [ShortcutPreset] {
        let french = language == .french
        return [
            ShortcutPreset(
                id: "lock-screen",
                name: french ? "Verrouiller l’écran" : "Lock Screen",
                line: "pmset displaysleepnow"
            ),
            ShortcutPreset(
                id: "sleep",
                name: french ? "Mettre en veille" : "Sleep",
                line: "pmset sleepnow"
            ),
            ShortcutPreset(
                id: "mute",
                name: french ? "Couper ou réactiver le son" : "Mute / Unmute",
                line: "osascript -e 'set volume output muted not (output muted of (get volume settings))'"
            ),
        ]
    }

    /// Trims surrounding whitespace; `nil` when either field is blank.
    public static func clean(name: String, line: String) -> CleanedShortcut? {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let line = line.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, !line.isEmpty else { return nil }
        return CleanedShortcut(name: name, line: line)
    }

    /// A fresh 16-character lowercase hex identifier for a new shortcut.
    public static func makeId() -> String {
        var value = UInt64.random(in: 0...UInt64.max)
        return withUnsafeBytes(of: &value) { bytes in
            bytes.map { String(format: "%02x", $0) }.joined()
        }
    }
}
