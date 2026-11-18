// Weft's own colours instead of the system accent (brand/README.md):
// sunset orange as a rare accent, ink for text on it, neutral surfaces.
//
// Without Xcode there is no asset catalog, so `NSAccentColorName` cannot
// name a colour. Instead:
// - SwiftUI controls take `.tint(.weftOrange)` from each scene's root:
//   toggles, sliders, progress, links;
// - what SwiftUI cannot tint (filled buttons, the sidebar selection) is
//   drawn here, with ink text on orange: white on #F26A21 is about 3:1,
//   ink #17130F about 6:1;
// - what AppKit draws alone (focus rings, the Settings tab icons, text
//   selection) follows the app's accent setting, set to the system orange,
//   the nearest of the accents macOS offers (`Brand.applyAccent`).

import AppKit
import SwiftUI

enum Brand {
    static let orange = NSColor(srgbRed: 0xF2 / 255, green: 0x6A / 255, blue: 0x21 / 255, alpha: 1)
    static let ink = NSColor(srgbRed: 0x17 / 255, green: 0x13 / 255, blue: 0x0F / 255, alpha: 1)

    /// The accent of what AppKit draws by itself, for this app only: the
    /// orange of System Settings › Appearance, without touching the Mac's
    /// own setting. Read by AppKit when it starts, so call it first.
    static func applyAccent() {
        let defaults = UserDefaults.standard
        var arguments = defaults.volatileDomain(forName: UserDefaults.argumentDomain)
        arguments["AppleAccentColor"] = 1
        arguments["AppleHighlightColor"] = "1.000000 0.874510 0.701961 Orange"
        defaults.setVolatileDomain(arguments, forName: UserDefaults.argumentDomain)
    }

    /// `WEFT_DEV_APPEARANCE`, for screenshots in both appearances.
    @MainActor static var appearance: NSAppearance? {
        switch Development.appearance {
        case "light": NSAppearance(named: .aqua)
        case "dark": NSAppearance(named: .darkAqua)
        default: nil
        }
    }
}

extension Color {
    static let weftOrange = Color(nsColor: Brand.orange)
    static let weftInk = Color(nsColor: Brand.ink)
}

/// The one filled button of a view: orange, ink label.
struct WeftProminentButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled
    var large = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(large ? .body.weight(.medium) : .body)
            .lineLimit(1)
            .fixedSize()
            .foregroundStyle(Color.weftInk)
            .padding(.horizontal, large ? 18 : 12)
            .padding(.vertical, large ? 7 : 4)
            .background(
                RoundedRectangle(cornerRadius: large ? 8 : 6, style: .continuous)
                    .fill(Color.weftOrange)
                    .brightness(configuration.isPressed ? -0.08 : 0)
            )
            .opacity(isEnabled ? 1 : 0.45)
            .contentShape(Rectangle())
    }
}

/// The quiet companion of `WeftProminentButtonStyle`: same font, padding and
/// radius, so a row of actions shares one height, on a neutral fill with
/// neutral text. Orange stays for the one primary action.
struct WeftSecondaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.body)
            .lineLimit(1)
            .fixedSize()
            .foregroundStyle(Color.primary)
            .padding(.horizontal, 12)
            .padding(.vertical, 4)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(Color.primary.opacity(configuration.isPressed ? 0.14 : 0.08))
            )
            .opacity(isEnabled ? 1 : 0.45)
            .contentShape(Rectangle())
    }
}

/// The sidebar's own selection: an orange row with ink text, in place of
/// the system's, which only follows the Mac's accent.
struct SelectionHighlight: ViewModifier {
    let selected: Bool

    func body(content: Content) -> some View {
        content
            .foregroundStyle(selected ? Color.weftInk : Color.primary)
            .listRowBackground(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(selected ? Color.weftOrange : Color.clear)
                    .padding(.horizontal, 10)
            )
    }
}

/// Turns off the system's selection drawing of the table under a SwiftUI
/// `List`, so `SelectionHighlight` is the only one. Selection itself,
/// keyboard included, stays the table's.
struct PlainTableSelection: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async { Self.apply(from: view) }
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        DispatchQueue.main.async { Self.apply(from: view) }
    }

    private static func apply(from view: NSView) {
        guard let root = view.window?.contentView else { return }
        for table in tables(in: root) where table.selectionHighlightStyle != .none {
            table.selectionHighlightStyle = .none
        }
    }

    private static func tables(in view: NSView) -> [NSTableView] {
        if let table = view as? NSTableView { return [table] }
        return view.subviews.flatMap(tables(in:))
    }
}
