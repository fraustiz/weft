// Interface language: English, or French when the app runs in French.
//
// A String Catalog needs Xcode to compile, and a SwiftPM resource bundle
// (`Bundle.module`) is looked for at the root of a hand-assembled .app,
// where it breaks the code signature. So the strings live here, English
// and French side by side as in the Windows app (windows/src/i18n.rs),
// checked by the compiler. `WEFT_LANG=en|fr` forces one.

import Foundation

public enum Language: Sendable, Equatable {
    case english
    case french

    /// The language of this process, decided once.
    public static let current: Language = detect(
        forced: ProcessInfo.processInfo.environment["WEFT_LANG"],
        preferred: Bundle.main.preferredLocalizations + Locale.preferredLanguages
    )

    /// `forced` wins when it names a language; else the first preferred
    /// language that is English or French.
    public static func detect(forced: String?, preferred: [String]) -> Language {
        if let forced = forced?.lowercased() {
            if forced.hasPrefix("fr") { return .french }
            if forced.hasPrefix("en") { return .english }
        }
        for code in preferred {
            let code = code.lowercased()
            if code.hasPrefix("fr") { return .french }
            if code.hasPrefix("en") { return .english }
        }
        return .english
    }
}

/// Picks the English or the French version of a text.
public func t(_ english: String, _ french: String) -> String {
    Language.current == .french ? french : english
}
