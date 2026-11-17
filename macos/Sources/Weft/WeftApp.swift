// Weft for macOS: a menu bar app (LSUIElement) over the Rust client core.
//
// Threads: the main one runs SwiftUI and polls the pasteboard; the core's
// worker talks to the relay on its own thread, and its callbacks hop to
// the main actor (CoreBridge.swift).

import AppKit
import SwiftUI
import WeftKit

@main
struct WeftApp: App {
    static let mainWindow = "main"

    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @StateObject private var model = AppModel()

    var body: some Scene {
        MenuBarExtra {
            MenuContent(model: model)
        } label: {
            MenuBarIcon(model: model)
        }

        Window("Weft", id: Self.mainWindow) {
            MainWindow(model: model)
                .onAppear { delegate.model = model }
        }
        .defaultSize(width: 720, height: 520)
        .windowResizability(.contentMinSize)

        Settings {
            SettingsView(model: model)
        }
    }

    /// At launch: the main window on a first run (nothing paired yet), or
    /// what `WEFT_DEV_SHOW` asks for; otherwise only the menu bar icon.
    @MainActor static func launchShow(model: AppModel) {
        AppDelegate.shared?.model = model
        switch Development.show {
        case "main":
            model.showMainWindow()
        case "settings":
            NSApp.activate()
            model.openSettings?()
        case "menu":
            AppDelegate.openStatusMenu()
        default:
            if model.devices.isEmpty || Development.startsPairing {
                model.showMainWindow()
            }
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    @MainActor static weak var shared: AppDelegate?
    @MainActor weak var model: AppModel?

    @MainActor func applicationDidFinishLaunching(_ notification: Notification) {
        Self.shared = self
    }

    @MainActor func applicationWillTerminate(_ notification: Notification) {
        model?.stop()
    }

    /// Closing the last window leaves the menu bar icon.
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    /// Clicks the menu bar icon, for screenshots (`WEFT_DEV_SHOW=menu`).
    @MainActor static func openStatusMenu() {
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
            for window in NSApp.windows {
                guard let root = window.contentView?.superview ?? window.contentView,
                      let button = statusButton(in: root) else { continue }
                button.performClick(nil)
                return
            }
            FileHandle.standardError.write(Data("no status item found to open\n".utf8))
        }
    }

    @MainActor private static func statusButton(in view: NSView) -> NSStatusBarButton? {
        if let button = view as? NSStatusBarButton { return button }
        for child in view.subviews {
            if let button = statusButton(in: child) { return button }
        }
        return nil
    }
}
