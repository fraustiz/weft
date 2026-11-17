// macOS notifications through UserNotifications: the phone's
// notifications and calls, and what this Mac tells about itself (a ping,
// a received file). What each one shows and what its buttons mean is
// decided in WeftKit/PhoneNotifications.swift; this side posts them and
// turns a press into a command for the core.
//
// UserNotifications needs a real app bundle (build/Weft.app, ad-hoc
// signed is enough); run outside one, as by `swift run`, nothing is
// posted.

import AppKit
@preconcurrency import UserNotifications
import WeftKit

@MainActor
final class Notifier: NSObject, ObservableObject {
    enum Permission: Equatable {
        /// Not looked at yet, or no app bundle.
        case unknown
        case notAsked
        case denied
        case allowed
    }

    @Published private(set) var permission = Permission.unknown
    /// What a press asks for, with what the notification carried.
    var onResponse: ((NotificationResponse, [AnyHashable: Any]) -> Void)?

    private var registered: [String: UNNotificationCategory] = [:]
    private var asking = false

    /// UserNotifications aborts the process without an app bundle.
    private let available: Bool = Bundle.main.bundleIdentifier != nil
        && Bundle.main.bundleURL.pathExtension == "app"

    private var center: UNUserNotificationCenter? {
        available ? UNUserNotificationCenter.current() : nil
    }

    func start() {
        guard let center else {
            log.info("not in an app bundle: notifications are off")
            return
        }
        center.delegate = self
        refreshPermission()
    }

    func refreshPermission() {
        guard let center else { return }
        Task {
            let status = await center.notificationSettings().authorizationStatus
            permission = switch status {
            case .notDetermined: .notAsked
            case .denied: .denied
            default: .allowed
            }
            Development.record("permission \(permission)")
        }
    }

    /// Asks once, the first time it matters: after a pairing, or with the
    /// first notification to show.
    func askIfNeeded() {
        guard let center, permission == .notAsked, !asking else { return }
        asking = true
        Task {
            var granted = false
            do {
                granted = try await center.requestAuthorization(options: [.alert, .sound])
            } catch {
                log.error("notification permission: \(error.localizedDescription, privacy: .public)")
            }
            asking = false
            permission = granted ? .allowed : .denied
            Development.record("permission \(granted ? "allowed" : "denied")")
        }
    }

    /// Opens System Settings › Notifications at Weft.
    func openSystemSettings() {
        let id = Bundle.main.bundleIdentifier ?? ""
        let urls = [
            "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=\(id)",
            "x-apple.systempreferences:com.apple.preference.notifications",
        ]
        for string in urls {
            if let url = URL(string: string), NSWorkspace.shared.open(url) { return }
        }
    }

    /// Shows `plan`. `info` comes back with a press on it.
    func post(_ plan: NotificationPlan, info: [String: Any] = [:]) {
        guard let center else { return }
        if permission == .notAsked { askIfNeeded() }
        let content = UNMutableNotificationContent()
        content.title = plan.title
        content.subtitle = plan.subtitle
        content.body = plan.body
        content.threadIdentifier = plan.threadIdentifier
        content.categoryIdentifier = plan.categoryIdentifier
        content.sound = plan.sound ? .default : nil
        if !plan.banner { content.interruptionLevel = .passive }
        var userInfo = info
        userInfo["banner"] = plan.banner
        content.userInfo = userInfo
        let request = UNNotificationRequest(identifier: plan.identifier, content: content, trigger: nil)
        Task {
            await register(plan)
            do {
                try await center.add(request)
                Development.record("posted \(plan.categoryIdentifier == "weft.plain" ? "plain" : "with buttons")")
            } catch {
                log.error("notification not shown: \(error.localizedDescription, privacy: .public)")
                Development.record("not posted")
            }
        }
    }

    /// Removes a delivered notification.
    func remove(identifier: String) {
        center?.removeDeliveredNotifications(withIdentifiers: [identifier])
        center?.removePendingNotificationRequests(withIdentifiers: [identifier])
    }

    /// Makes sure the category of `plan` is known before it is posted:
    /// macOS shows a notification without its buttons otherwise.
    private func register(_ plan: NotificationPlan) async {
        guard let center, registered[plan.categoryIdentifier] == nil else { return }
        let actions: [UNNotificationAction] = plan.buttons.map { button in
            switch button.kind {
            case .reply:
                UNTextInputNotificationAction(
                    identifier: button.identifier, title: button.title, options: [],
                    textInputButtonTitle: t("Send", "Envoyer"), textInputPlaceholder: ""
                )
            case .dismiss:
                UNNotificationAction(identifier: button.identifier, title: button.title, options: [.destructive])
            case .showInFinder:
                UNNotificationAction(identifier: button.identifier, title: button.title, options: [.foreground])
            case .action, .muteCall:
                UNNotificationAction(identifier: button.identifier, title: button.title, options: [])
            }
        }
        registered[plan.categoryIdentifier] = UNNotificationCategory(
            identifier: plan.categoryIdentifier, actions: actions, intentIdentifiers: [], options: []
        )
        center.setNotificationCategories(Set(registered.values))
        // Set asynchronously: wait until it is in, a moment at most.
        for _ in 0..<10 {
            let known = await center.notificationCategories()
            if known.contains(where: { $0.identifier == plan.categoryIdentifier }) { return }
            try? await Task.sleep(for: .milliseconds(50))
        }
    }
}

extension Notifier: UNUserNotificationCenterDelegate {
    /// Shown while Weft is the active app too, unless it is an update.
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        let banner = notification.request.content.userInfo["banner"] as? Bool ?? true
        completionHandler(banner ? [.banner, .list, .sound] : [.list])
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        let info = response.notification.request.content.userInfo
        let actions = info["actions"] as? [String] ?? []
        let reply = (response as? UNTextInputNotificationResponse)?.userText
        let meaning = PhoneNotifications.response(
            actionIdentifier: response.actionIdentifier, replyText: reply, actions: actions
        )
        let sendable = SendableInfo(info: info)
        Task { @MainActor [weak self] in
            self?.onResponse?(meaning, sendable.info)
        }
        completionHandler()
    }
}

/// userInfo holds plists only (strings, arrays, booleans): safe to hand
/// to the main actor.
private struct SendableInfo: @unchecked Sendable {
    let info: [AnyHashable: Any]
}
