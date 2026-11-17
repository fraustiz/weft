// The menu of the menu bar icon: where the sync stands, the devices, and
// the few things done from here.

import AppKit
import SwiftUI
import WeftFFI
import WeftKit

struct MenuContent: View {
    @ObservedObject var model: AppModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        if let snapshot = model.snapshot {
            status(snapshot)
            Divider()
            devices(snapshot)
            Divider()
            Button(t("Send Clipboard Now", "Envoyer le presse-papiers")) {
                model.sendClipboardNow()
            }
            .disabled(!snapshot.isConnected || snapshot.devices.isEmpty)
            sendFile(snapshot)
            Button(t("Pair a Device…", "Associer un appareil…")) {
                model.openWindow = openWindow
                model.pair()
            }
        } else if let error = model.startError {
            Text(t("Weft could not start", "Weft n’a pas pu démarrer"))
            Text(error)
        }
        Divider()
        Button(t("Open Weft", "Ouvrir Weft")) {
            model.openWindow = openWindow
            model.showMainWindow()
        }
        SettingsLink {
            Text(t("Settings…", "Réglages…"))
        }
        .keyboardShortcut(",")
        Divider()
        Button(t("Quit Weft", "Quitter Weft")) {
            NSApp.terminate(nil)
        }
        .keyboardShortcut("q")
    }

    @ViewBuilder
    private func status(_ snapshot: CoreSnapshot) -> some View {
        if snapshot.isConnected {
            Text("\(snapshot.statusTitle) · \(Words.relayHost(snapshot.relayUrl))")
        } else {
            Text(snapshot.statusTitle)
        }
        if let reason = snapshot.statusReason {
            Text(reason)
        }
        if let activity = snapshot.lastActivity {
            Text(activity.sentence())
        }
    }

    @ViewBuilder
    private func devices(_ snapshot: CoreSnapshot) -> some View {
        if snapshot.devices.isEmpty {
            Text(t("No devices paired", "Aucun appareil associé"))
        } else {
            Section(t("Devices", "Appareils")) {
                ForEach(snapshot.devices) { device in
                    Button {
                        model.selection = device.id
                        model.openWindow = openWindow
                        model.showMainWindow()
                    } label: {
                        Label(label(device), systemImage: device.symbol)
                    }
                }
            }
        }
    }

    /// « Send File to » the devices that take files.
    @ViewBuilder
    private func sendFile(_ snapshot: CoreSnapshot) -> some View {
        let takers = snapshot.features.files ? snapshot.devices.filter { $0.takes("file.offer") } : []
        if takers.count == 1, let device = takers.first {
            Button(Language.current == .french
                   ? "Envoyer un fichier à \(device.displayName)…"
                   : "Send File to \(device.displayName)…") {
                model.pickFiles(for: device)
            }
        } else if !takers.isEmpty {
            Menu(t("Send File to", "Envoyer un fichier à")) {
                ForEach(takers) { device in
                    Button(device.displayName) { model.pickFiles(for: device) }
                }
            }
        }
    }

    private func label(_ device: DeviceInfo) -> String {
        guard let battery = device.battery else { return device.displayName }
        return "\(device.displayName) — \(Words.battery(level: battery.level))"
    }
}

/// The icon in the menu bar: the template image of the mark, so macOS
/// tints it like its own icons.
struct MenuBarIcon: View {
    @ObservedObject var model: AppModel
    @Environment(\.openWindow) private var openWindow
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        Image(nsImage: Self.image)
            .accessibilityLabel("Weft")
            .onAppear {
                // The first view up: it lends the model a way to open
                // windows, and starts the core.
                model.openWindow = openWindow
                model.openSettings = openSettings
                model.start()
                WeftApp.launchShow(model: model)
            }
    }

    /// MenuBarIcon.png (+ @2x), made by scripts/build-app.sh from
    /// brand/final/weft-mark.svg; drawn from the same shapes otherwise.
    @MainActor static var image: NSImage {
        if let cached { return cached }
        let image = loadImage()
        cached = image
        return image
    }

    @MainActor private static var cached: NSImage?

    @MainActor private static func loadImage() -> NSImage {
        if let image = NSImage(named: "MenuBarIcon") {
            image.isTemplate = true
            image.size = NSSize(width: 18, height: 18)
            return image
        }
        let renderer = ImageRenderer(content: BrandMark(color: .black).frame(width: 18, height: 18))
        renderer.scale = 2
        let image = renderer.nsImage ?? NSImage()
        image.isTemplate = true
        return image
    }
}
