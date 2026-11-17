// SMS and contacts formatting, recipient matching, and timestamps
// (docs/features.md section 7), kept pure so WeftKitTests can check every
// edge case without the Rust core.

import Foundation

public struct SmsContact: Equatable, Sendable {
    public let name: String
    public let numbers: [String]

    public init(name: String, numbers: [String]) {
        self.name = name
        self.numbers = numbers
    }
}

public struct SmsSuggestion: Equatable, Sendable, Identifiable {
    public var id: String { label }
    public let label: String
    public let number: String

    public init(label: String, number: String) {
        self.label = label
        self.number = number
    }
}

public enum SmsFormatting {
    public static let maxSuggestions = 8
    public static let runWindowMs: Int64 = 10 * 60 * 1000
    public static let separator = " · "

    /// Keeps ASCII digits and a leading `+`.
    public static func digits(_ address: String) -> String {
        var result = ""
        for (index, scalar) in address.unicodeScalars.enumerated() {
            if (0x30...0x39).contains(scalar.value) || (index == 0 && scalar == "+") {
                result.unicodeScalars.append(scalar)
            }
        }
        return result
    }

    /// Whether two phone numbers or sender names refer to the same line:
    /// identical digits, or identical last 9 digits when both have at least 9.
    public static func sameAddress(_ a: String, _ b: String) -> Bool {
        let aDigits = String(digits(a).drop(while: { $0 == "+" }))
        let bDigits = String(digits(b).drop(while: { $0 == "+" }))
        if aDigits.isEmpty || bDigits.isEmpty {
            return a.trimmingCharacters(in: .whitespacesAndNewlines)
                .caseInsensitiveCompare(b.trimmingCharacters(in: .whitespacesAndNewlines)) == .orderedSame
        }
        if aDigits == bDigits { return true }
        let tail = 9
        return aDigits.count >= tail && bDigits.count >= tail
            && aDigits.suffix(tail) == bDigits.suffix(tail)
    }

    /// Whether two address lists name the same set of recipients.
    public static func samePeople(_ a: [String], _ b: [String]) -> Bool {
        a.count == b.count
            && a.allSatisfy { left in b.contains { right in sameAddress(left, right) } }
            && b.allSatisfy { right in a.contains { left in sameAddress(left, right) } }
    }

    /// Matching contact numbers for a recipient search query.
    public static func suggestions(
        contacts: [SmsContact],
        query: String,
        limit: Int = maxSuggestions
    ) -> [SmsSuggestion] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, limit > 0 else { return [] }
        let lower = trimmed.lowercased()
        let queryDigits = String(digits(trimmed).drop(while: { $0 == "+" }))
        let nonSpaceCount = trimmed.filter { !$0.isWhitespace }.count
        let allowsDigitMatch = !queryDigits.isEmpty && queryDigits.count * 2 >= nonSpaceCount

        var found: [SmsSuggestion] = []
        for contact in contacts {
            let nameMatches = contact.name.lowercased().contains(lower)
            for number in contact.numbers {
                let numberMatches = allowsDigitMatch && digits(number).contains(queryDigits)
                if nameMatches || numberMatches {
                    found.append(SmsSuggestion(
                        label: "\(contact.name)\(separator)\(number)",
                        number: number
                    ))
                    if found.count == limit { return found }
                }
            }
        }
        return found
    }

    /// Resolves what was typed or chosen in the recipient field to a phone number.
    public static func recipient(contacts: [SmsContact], text: String) -> String? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        if let range = trimmed.range(of: separator, options: .backwards) {
            let name = String(trimmed[..<range.lowerBound])
            let number = String(trimmed[range.upperBound...])
            if contacts.contains(where: { $0.name == name && $0.numbers.contains(number) }) {
                return number
            }
        }
        let allowed = CharacterSet(charactersIn: "0123456789 +-.()")
        let hasDigit = trimmed.unicodeScalars.contains { (0x30...0x39).contains($0.value) }
        let onlyDialChars = trimmed.unicodeScalars.allSatisfy { allowed.contains($0) }
        return (hasDigit && onlyDialChars) ? trimmed : nil
    }

    /// Formats an SMS timestamp in milliseconds since 1970.
    public static func timestamp(
        ms: Int64,
        nowMs: Int64? = nil,
        calendar: Calendar = .current,
        language: Language = .current,
        alwaysIncludeTime: Bool = false
    ) -> String {
        guard ms > 0 else { return "" }
        let now = Date(timeIntervalSince1970: TimeInterval(nowMs ?? Int64(Date.now.timeIntervalSince1970 * 1000)) / 1000)
        let date = Date(timeIntervalSince1970: TimeInterval(ms) / 1000)

        let thenParts = calendar.dateComponents([.year, .month, .day, .hour, .minute], from: date)
        let nowParts = calendar.dateComponents([.year, .month, .day], from: now)
        guard let year = thenParts.year, let month = thenParts.month, let day = thenParts.day,
              let hour = thenParts.hour, let minute = thenParts.minute else {
            return ""
        }
        let clock = String(format: "%02d:%02d", hour, minute)
        if year == nowParts.year && month == nowParts.month && day == nowParts.day {
            return clock
        }
        let monthsEn = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        let monthsFr = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."]
        let names = language == .french ? monthsFr : monthsEn
        let monthName = names[min(max(month, 1), 12) - 1]
        let dayText = year == nowParts.year ? "\(day) \(monthName)" : "\(day) \(monthName) \(year)"
        return alwaysIncludeTime ? "\(dayText) \(clock)" : dayText
    }

    /// Human-readable reason for a failed outgoing SMS (`docs/features.md` section 9).
    public static func failureText(_ reason: String?, language: Language = .current) -> String {
        let french = language == .french
        switch reason {
        case "permission":
            return french
                ? "Weft n’a pas l’autorisation d’envoyer des SMS sur le téléphone"
                : "Weft does not have SMS permission on the phone"
        case "noSms":
            return french
                ? "Ce téléphone ne peut pas envoyer de SMS"
                : "This phone cannot send SMS"
        case "noService":
            return french ? "Pas de réseau mobile" : "No mobile service"
        case "radioOff":
            return french
                ? "Le réseau mobile du téléphone est coupé"
                : "The phone’s mobile network is off"
        default:
            return french
                ? "Le téléphone n’a pas pu l’envoyer"
                : "The phone could not send it"
        }
    }

    /// Consecutive messages from the same side within `runWindowMs` share one timestamp.
    public static func isLastOfRun(
        timeMs: Int64,
        outgoing: Bool,
        nextTimeMs: Int64?,
        nextOutgoing: Bool?
    ) -> Bool {
        guard let nextTimeMs, let nextOutgoing else { return true }
        return nextOutgoing != outgoing || (nextTimeMs - timeMs) > runWindowMs
    }
}
