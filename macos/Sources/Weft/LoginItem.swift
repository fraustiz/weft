// "Open at login", through SMAppService. It needs a real app bundle; an
// app that is not signed by a developer may also need the user's approval
// in System Settings › General › Login Items. macOS reports an app that
// was never registered as "not found", so that reads as off: turning it on
// registers the app, and if macOS refuses, its reason is shown.

import Foundation
import ServiceManagement
import WeftKit

enum LoginItem {
    enum State: Equatable {
        case on
        case off
        /// Registered, waiting for the user's approval.
        case needsApproval
        /// Not available, and why.
        case unavailable(String)
    }

    static var state: State {
        guard Bundle.main.bundleURL.pathExtension == "app" else {
            return .unavailable(t(
                "Available when Weft runs from Weft.app.",
                "Disponible quand Weft est lancé depuis Weft.app."
            ))
        }
        switch SMAppService.mainApp.status {
        case .enabled: return .on
        case .requiresApproval: return .needsApproval
        case .notRegistered, .notFound: return .off
        @unknown default: return .off
        }
    }

    /// Turns it on or off; returns the error macOS gave, in words.
    static func set(_ on: Bool) -> String? {
        do {
            if on {
                try SMAppService.mainApp.register()
            } else {
                try SMAppService.mainApp.unregister()
            }
            return nil
        } catch {
            log.error("login item change failed: \(error.localizedDescription, privacy: .public)")
            return error.localizedDescription
        }
    }

    static func openSystemSettings() {
        SMAppService.openSystemSettingsLoginItems()
    }
}
