// Remote pointer, keyboard and presenter input on this Mac
// (docs/features.md section 8): checks Accessibility permission, posts
// CGEvents when trusted, and moves the click-through presenter highlight
// panel across spaces and full-screen slideshows.

import AppKit
import ApplicationServices
import CoreGraphics
import SwiftUI
import WeftFFI
import WeftKit

@MainActor
final class RemoteInputController {
    private var motion = PointerRemainder()
    private var wheel = PointerRemainder()
    private var heldButton: MouseButton?
    private let overlay = PresenterOverlay()

    /// Whether macOS Accessibility allows Weft to post input events.
    static var isTrusted: Bool {
        AXIsProcessTrusted()
    }

    /// Asks macOS to show the Accessibility prompt for Weft.
    @discardableResult
    static func promptAccessibility() -> Bool {
        let options = ["AXTrustedCheckOptionPrompt": true] as CFDictionary
        return AXIsProcessTrustedWithOptions(options)
    }

    /// Opens System Settings › Privacy & Security › Accessibility.
    static func openAccessibilitySettings() {
        let urls = [
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            "x-apple.systempreferences:com.apple.preference.security",
        ]
        for candidate in urls {
            if let url = URL(string: candidate), NSWorkspace.shared.open(url) {
                return
            }
        }
    }

    func stop() {
        heldButton = nil
        overlay.hide()
    }

    func apply(_ input: RemoteInput) {
        switch input {
        case .presenterSpot(let point):
            overlay.update(point: point)
        case .move(let dx, let dy):
            guard Self.isTrusted else { return }
            let step = motion.take(dx: dx, dy: dy)
            guard step.dx != 0 || step.dy != 0 else { return }
            movePointer(dx: step.dx, dy: step.dy)
        case .scroll(let dx, let dy):
            guard Self.isTrusted else { return }
            // Positive `dy` turns the wheel up; positive `dx` scrolls right.
            let step = wheel.take(dx: dx * 30, dy: dy * 30)
            guard step.dx != 0 || step.dy != 0 else { return }
            if let event = CGEvent(
                scrollWheelEvent2Source: nil,
                units: .pixel,
                wheelCount: 2,
                wheel1: step.dy,
                wheel2: -step.dx,
                wheel3: 0
            ) {
                event.post(tap: .cghidEventTap)
            }
        case .click(let button, let phase):
            guard Self.isTrusted else { return }
            postClick(button: button, phase: phase)
        case .keyText(let text):
            guard Self.isTrusted else { return }
            postText(text)
        case .specialKey(let key, let ctrl, let alt, let shift, let meta):
            guard Self.isTrusted else { return }
            let flags = eventFlags(ctrl: ctrl, alt: alt, shift: shift, meta: meta)
            postKey(macKey(key).keyCode, flags: flags)
        case .presenter(let action):
            guard Self.isTrusted else { return }
            let key: MacKey = switch action {
            case .next: .right
            case .previous: .left
            case .start: .f5
            case .stop: .escape
            }
            postKey(key.keyCode, flags: [])
        }
    }

    // MARK: Pointer

    private func currentLocation() -> CGPoint {
        CGEvent(source: nil)?.location ?? .zero
    }

    private func clampedLocation(from current: CGPoint, dx: Int32, dy: Int32) -> CGPoint {
        var next = CGPoint(x: current.x + CGFloat(dx), y: current.y + CGFloat(dy))
        let mainHeight = CGDisplayBounds(CGMainDisplayID()).height
        let rects: [CGRect] = NSScreen.screens.map { screen in
            CGRect(
                x: screen.frame.minX,
                y: mainHeight - screen.frame.maxY,
                width: screen.frame.width,
                height: screen.frame.height
            )
        }
        guard !rects.isEmpty else { return next }
        if rects.contains(where: { $0.contains(next) }) {
            return next
        }
        let active = rects.first(where: { $0.contains(current) }) ?? rects[0]
        next.x = min(max(next.x, active.minX), max(active.minX, active.maxX - 1))
        next.y = min(max(next.y, active.minY), max(active.minY, active.maxY - 1))
        return next
    }

    private func movePointer(dx: Int32, dy: Int32) {
        let current = currentLocation()
        let next = clampedLocation(from: current, dx: dx, dy: dy)
        let (type, button): (CGEventType, CGMouseButton) = switch heldButton {
        case .left: (.leftMouseDragged, .left)
        case .right: (.rightMouseDragged, .right)
        case .middle: (.otherMouseDragged, .center)
        case nil: (.mouseMoved, .left)
        }
        guard let event = CGEvent(
            mouseEventSource: nil,
            mouseType: type,
            mouseCursorPosition: next,
            mouseButton: button
        ) else { return }
        event.setIntegerValueField(.mouseEventDeltaX, value: Int64(dx))
        event.setIntegerValueField(.mouseEventDeltaY, value: Int64(dy))
        event.post(tap: .cghidEventTap)
    }

    private func postClick(button: MouseButton, phase: ClickPhase) {
        let position = currentLocation()
        let (downType, upType, cgButton): (CGEventType, CGEventType, CGMouseButton) = switch button {
        case .left: (.leftMouseDown, .leftMouseUp, .left)
        case .right: (.rightMouseDown, .rightMouseUp, .right)
        case .middle: (.otherMouseDown, .otherMouseUp, .center)
        }
        func post(_ type: CGEventType, clickCount: Int64) {
            guard let event = CGEvent(
                mouseEventSource: nil,
                mouseType: type,
                mouseCursorPosition: position,
                mouseButton: cgButton
            ) else { return }
            event.setIntegerValueField(.mouseEventClickState, value: clickCount)
            event.post(tap: .cghidEventTap)
        }
        switch phase {
        case .click:
            heldButton = nil
            post(downType, clickCount: 1)
            post(upType, clickCount: 1)
        case .double:
            heldButton = nil
            post(downType, clickCount: 1)
            post(upType, clickCount: 1)
            post(downType, clickCount: 2)
            post(upType, clickCount: 2)
        case .down:
            heldButton = button
            post(downType, clickCount: 1)
        case .up:
            heldButton = nil
            post(upType, clickCount: 1)
        }
    }

    // MARK: Keyboard

    private func postKey(_ keyCode: CGKeyCode, flags: CGEventFlags) {
        guard let down = CGEvent(keyboardEventSource: nil, virtualKey: keyCode, keyDown: true),
              let up = CGEvent(keyboardEventSource: nil, virtualKey: keyCode, keyDown: false) else {
            return
        }
        if !flags.isEmpty {
            down.flags = flags
            up.flags = flags
        }
        down.post(tap: .cghidEventTap)
        up.post(tap: .cghidEventTap)
    }

    private func postText(_ raw: String) {
        let normalized = raw
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
        for character in normalized {
            if character == "\n" {
                postKey(MacKey.enter.keyCode, flags: [])
                continue
            }
            if character == "\t" {
                postKey(MacKey.tab.keyCode, flags: [])
                continue
            }
            if character.unicodeScalars.allSatisfy({ $0.value < 0x20 || $0.value == 0x7F }) {
                continue
            }
            let units = Array(String(character).utf16)
            guard !units.isEmpty,
                  let down = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: true),
                  let up = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: false) else {
                continue
            }
            units.withUnsafeBufferPointer { buffer in
                down.keyboardSetUnicodeString(stringLength: buffer.count, unicodeString: buffer.baseAddress)
                up.keyboardSetUnicodeString(stringLength: buffer.count, unicodeString: buffer.baseAddress)
            }
            down.post(tap: .cghidEventTap)
            up.post(tap: .cghidEventTap)
        }
    }

    private func macKey(_ key: SpecialKeyKind) -> MacKey {
        switch key {
        case .enter: .enter
        case .backspace: .backspace
        case .tab: .tab
        case .escape: .escape
        case .left: .left
        case .right: .right
        case .up: .up
        case .down: .down
        case .home: .home
        case .end: .end
        case .pageUp: .pageUp
        case .pageDown: .pageDown
        case .delete: .delete
        }
    }

    private func eventFlags(ctrl: Bool, alt: Bool, shift: Bool, meta: Bool) -> CGEventFlags {
        var flags: CGEventFlags = []
        if ctrl { flags.insert(.maskControl) }
        if alt { flags.insert(.maskAlternate) }
        if shift { flags.insert(.maskShift) }
        if meta { flags.insert(.maskCommand) }
        return flags
    }
}

/// Borderless, click-through highlight spot shown on `NSScreen.main` during
/// a presentation.
@MainActor
final class PresenterOverlay {
    static let panelSide: CGFloat = 32
    private var panel: NSPanel?

    func update(point: SpotPoint?) {
        guard let point, let screen = NSScreen.main ?? NSScreen.screens.first else {
            hide()
            return
        }
        let panel = ensurePanel()
        let origin = SpotGeometry.origin(
            x: point.x,
            y: point.y,
            in: screen.frame,
            diameter: Self.panelSide
        )
        panel.setFrameOrigin(origin)
        if !panel.isVisible {
            panel.orderFrontRegardless()
        }
    }

    func hide() {
        panel?.orderOut(nil)
    }

    private func ensurePanel() -> NSPanel {
        if let panel { return panel }
        let created = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: Self.panelSide, height: Self.panelSide),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        created.ignoresMouseEvents = true
        created.isOpaque = false
        created.backgroundColor = .clear
        created.hasShadow = false
        created.level = .screenSaver
        created.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        created.hidesOnDeactivate = false
        created.isReleasedWhenClosed = false
        created.contentView = NSHostingView(rootView: PresenterSpotView())
        panel = created
        return created
    }
}

private struct PresenterSpotView: View {
    var body: some View {
        Circle()
            .fill(Color(red: 0xE8 / 255, green: 0x3B / 255, blue: 0x24 / 255).opacity(0.9))
            .overlay(
                Circle()
                    .strokeBorder(Color.white.opacity(0.9), lineWidth: 2)
            )
            .frame(width: SpotGeometry.diameter, height: SpotGeometry.diameter)
            .frame(width: PresenterOverlay.panelSide, height: PresenterOverlay.panelSide)
            .accessibilityHidden(true)
    }
}
