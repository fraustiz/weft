// The rule a device name follows on the wire (docs/protocol.md, kind 0x06):
// UTF-8, 1 to 64 bytes, no control characters (U+0000–U+001F,
// U+007F–U+009F). Surrounding spaces are dropped, as the core does.

import Foundation

public enum NameProblem: Error, Sendable, Equatable {
    case empty
    case controlCharacters
    case tooLong

    public var text: String {
        switch self {
        case .empty:
            t("Enter a name", "Saisissez un nom")
        case .controlCharacters:
            t("Remove the tabs and other control characters",
              "Retirez les tabulations et autres caractères de contrôle")
        case .tooLong:
            t("Too long: 64 bytes at most, and accented letters or emoji take more than one",
              "Trop long : 64 octets au plus, et une lettre accentuée ou un emoji en prend plusieurs")
        }
    }
}

public enum DeviceName {
    public static let maxBytes = 64

    /// The name to save for what was typed, or why it cannot be.
    public static func check(_ typed: String) -> Result<String, NameProblem> {
        let name = typed.trimmingCharacters(in: .whitespacesAndNewlines)
        if name.isEmpty {
            return .failure(.empty)
        }
        if name.unicodeScalars.contains(where: isControl) {
            return .failure(.controlCharacters)
        }
        if name.utf8.count > maxBytes {
            return .failure(.tooLong)
        }
        return .success(name)
    }

    static func isControl(_ scalar: Unicode.Scalar) -> Bool {
        let value = scalar.value
        return value <= 0x1F || (0x7F...0x9F).contains(value)
    }
}
