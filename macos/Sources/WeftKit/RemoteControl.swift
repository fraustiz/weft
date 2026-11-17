// Pure helpers for remote input, presenter geometry, macOS virtual keycodes,
// and live typing on a phone (docs/features.md section 8).

import CoreGraphics
import Foundation

/// Virtual keycodes on macOS (`Events.h` / `HIToolbox`).
public enum MacKey: Sendable, Equatable, CaseIterable {
    case enter
    case backspace
    case tab
    case escape
    case left
    case right
    case down
    case up
    case home
    case end
    case pageUp
    case pageDown
    case delete
    case f5

    public var keyCode: UInt16 {
        switch self {
        case .enter: 36
        case .backspace: 51
        case .tab: 48
        case .escape: 53
        case .left: 123
        case .right: 124
        case .down: 125
        case .up: 126
        case .home: 115
        case .end: 119
        case .pageUp: 116
        case .pageDown: 121
        case .delete: 117
        case .f5: 96
        }
    }
}

public struct PointerStep: Equatable, Sendable {
    public let dx: Int32
    public let dy: Int32

    public init(dx: Int32, dy: Int32) {
        self.dx = dx
        self.dy = dy
    }
}

/// Accumulates sub-pixel pointer moves or fractional scroll steps so small
/// deltas add up instead of being truncated away.
public struct PointerRemainder: Equatable, Sendable {
    private var x: Double = 0
    private var y: Double = 0

    public init() {}

    public mutating func take(dx: Double, dy: Double) -> PointerStep {
        let totalX = x + dx
        let wholeX = totalX.rounded(.towardZero)
        x = totalX - wholeX

        let totalY = y + dy
        let wholeY = totalY.rounded(.towardZero)
        y = totalY - wholeY

        return PointerStep(
            dx: Int32(clamping: Int64(wholeX)),
            dy: Int32(clamping: Int64(wholeY))
        )
    }
}

/// Positions the presenter highlight panel on an AppKit `NSScreen` frame
/// (`origin` at bottom-left, `y` upwards) from wire coordinates (`-1...1`,
/// `y` downwards).
public enum SpotGeometry {
    public static let diameter: CGFloat = 24

    public static func origin(
        x: Double,
        y: Double,
        in screenFrame: CGRect,
        diameter: CGFloat = diameter
    ) -> CGPoint {
        let clampedX = min(max(x, -1), 1)
        let clampedY = min(max(y, -1), 1)
        let px = screenFrame.minX + CGFloat((clampedX + 1) / 2) * screenFrame.width - diameter / 2
        let py = screenFrame.maxY - CGFloat((clampedY + 1) / 2) * screenFrame.height - diameter / 2
        return CGPoint(x: px.rounded(), y: py.rounded())
    }
}

public enum RemoteTypingStep: Equatable, Sendable {
    case backspace(Int)
    case text(String)
    case enter
}

public enum RemoteTyping {
    /// Computes the key packets needed to turn `old` into `new` when the
    /// phone's cursor stays at the end of the text.
    public static func diff(from old: String, to new: String) -> [RemoteTypingStep] {
        let oldChars = Array(normalizeLineBreaks(old))
        let newChars = Array(normalizeLineBreaks(new))
        var common = 0
        while common < oldChars.count && common < newChars.count && oldChars[common] == newChars[common] {
            common += 1
        }
        var steps: [RemoteTypingStep] = []
        let removed = oldChars.count - common
        if removed > 0 {
            steps.append(.backspace(removed))
        }
        var buffer = ""
        for character in newChars[common...] {
            if character == "\n" {
                if !buffer.isEmpty {
                    steps.append(.text(buffer))
                    buffer.removeAll(keepingCapacity: true)
                }
                steps.append(.enter)
            } else {
                buffer.append(character)
            }
        }
        if !buffer.isEmpty {
            steps.append(.text(buffer))
        }
        return steps
    }

    private static func normalizeLineBreaks(_ text: String) -> String {
        text.replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
    }
}
