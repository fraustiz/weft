// The ring of this Mac (docs/features.md section 3): a two-tone alarm in a
// loop and a window on top of the others with « Found it », until it is
// pressed, the device stops the ring, or the core's minute is over.

import AppKit
import SwiftUI
import WeftKit

@MainActor
final class Ringer {
    private var sound: NSSound?
    private var window: NSPanel?
    /// Pressed « Found it ».
    var foundIt: (() -> Void)?

    var isRinging: Bool { window != nil }

    func start(device: String) {
        if sound == nil, let sound = NSSound(data: Self.alarm) {
            sound.loops = true
            sound.volume = 1
            sound.play()
            self.sound = sound
        }
        showWindow(device: device)
    }

    func stop() {
        sound?.stop()
        sound = nil
        window?.orderOut(nil)
        window = nil
    }

    private func showWindow(device: String) {
        if let window {
            window.orderFrontRegardless()
            return
        }
        let view = RingView(device: device) { [weak self] in self?.foundIt?() }
        let panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 360, height: 220),
            styleMask: [.titled, .nonactivatingPanel, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.isMovableByWindowBackground = true
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.hidesOnDeactivate = false
        panel.isReleasedWhenClosed = false
        panel.title = t("Weft is ringing", "Weft sonne")
        panel.contentView = NSHostingView(rootView: view.tint(.weftOrange))
        if let appearance = Brand.appearance { panel.appearance = appearance }
        panel.center()
        panel.orderFrontRegardless()
        panel.makeKey()
        window = panel
    }

    /// One second of the alarm, as the Windows app plays it: two short
    /// beeps a fifth apart, then silence. A 16-bit mono WAV in memory.
    static let alarm: Data = {
        let rate = 22_050.0
        func tone(_ frequency: Double, _ seconds: Double) -> [Int16] {
            let count = Int(rate * seconds)
            return (0..<count).map { n in
                let phase = (Double(n) * frequency / rate).truncatingRemainder(dividingBy: 1)
                let wave = min(max(sin(phase * 2 * .pi) * 3, -1), 1)
                let edge = min(Double(min(n, count - n)) / (rate * 0.005), 1)
                return Int16(wave * edge * 0.9 * Double(Int16.max))
            }
        }
        func silence(_ seconds: Double) -> [Int16] { Array(repeating: 0, count: Int(rate * seconds)) }
        let samples = tone(880, 0.15) + silence(0.05) + tone(1320, 0.15) + silence(0.65)
        var data = Data()
        func append<T: FixedWidthInteger>(_ value: T) {
            withUnsafeBytes(of: value.littleEndian) { data.append(contentsOf: $0) }
        }
        let bytes = UInt32(samples.count * 2)
        data.append(contentsOf: Array("RIFF".utf8)); append(36 + bytes)
        data.append(contentsOf: Array("WAVE".utf8))
        data.append(contentsOf: Array("fmt ".utf8)); append(UInt32(16))
        append(UInt16(1)); append(UInt16(1)); append(UInt32(rate)); append(UInt32(rate) * 2)
        append(UInt16(2)); append(UInt16(16))
        data.append(contentsOf: Array("data".utf8)); append(bytes)
        for sample in samples { append(sample) }
        return data
    }()
}

private struct RingView: View {
    let device: String
    let foundIt: () -> Void

    var body: some View {
        VStack(spacing: 14) {
            Image(systemName: "bell.and.waves.left.and.right.fill")
                .font(.system(size: 34))
                .foregroundStyle(Color.weftOrange)
                .accessibilityHidden(true)
            VStack(spacing: 4) {
                Text(Language.current == .french
                     ? "\(device) fait sonner ce Mac"
                     : "\(device) is ringing this Mac")
                    .font(.headline)
                    .multilineTextAlignment(.center)
                Text(t("It stops by itself after a minute.", "Elle s’arrête d’elle-même au bout d’une minute."))
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Button(t("Found It", "Trouvé"), action: foundIt)
                .buttonStyle(WeftProminentButtonStyle())
                .keyboardShortcut(.defaultAction)
        }
        .padding(.horizontal, 28)
        .padding(.top, 28)
        .padding(.bottom, 24)
        .frame(width: 360)
    }
}
