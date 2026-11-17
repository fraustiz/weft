// How the phone's notifications, its calls and the Mac's own notices
// become macOS notifications (docs/features.md section 5), decided apart
// from UserNotifications so it can be tested: the titles, the buttons and
// what a press on each one means.

import Foundation

/// A button of a notification.
public struct NotificationButton: Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        /// One of the phone's actions, by its position in `actions`.
        case action(Int)
        /// The inline reply: a text field.
        case reply
        /// Dismisses the notification on the phone.
        case dismiss
        /// Silences the ringing call.
        case muteCall
        /// Shows a received file in the Finder.
        case showInFinder
    }

    /// The action identifier macOS hands back on a press.
    public let identifier: String
    public let title: String
    public let kind: Kind

    public init(identifier: String, title: String, kind: Kind) {
        self.identifier = identifier
        self.title = title
        self.kind = kind
    }
}

/// What to show, before it becomes a `UNNotificationContent`.
public struct NotificationPlan: Equatable, Sendable {
    /// Stable for one phone notification, so an update replaces it and a
    /// removal finds it.
    public var identifier: String
    public var title: String
    public var subtitle: String
    public var body: String
    /// Notifications of one device are grouped.
    public var threadIdentifier: String
    /// The category carrying `buttons`; the same buttons give the same one.
    public var categoryIdentifier: String
    public var buttons: [NotificationButton]
    /// Plays the default sound when shown.
    public var sound: Bool
    /// Pops up as a banner; `false` goes straight to Notification Center.
    public var banner: Bool
}

/// What a press on a notification asks for.
public enum NotificationResponse: Equatable, Sendable {
    case open
    case action(String)
    case reply(String)
    case dismiss
    case muteCall
    case showInFinder
    /// Closed without a choice, or a button this version does not know.
    case nothing
}

public enum PhoneNotifications {
    /// The phone's actions shown, at most: macOS lists them in a menu.
    public static let maxActions = 3
    static let replyIdentifier = "reply"
    static let dismissIdentifier = "dismiss"
    static let muteIdentifier = "mute"
    static let finderIdentifier = "finder"
    static let actionPrefix = "action."

    /// The identifier of a phone notification on this Mac.
    public static func identifier(deviceId: String, id: String) -> String {
        "n.\(deviceId).\(id)"
    }

    /// The identifier of a device's ringing call: one at a time.
    public static func callIdentifier(deviceId: String) -> String {
        "call.\(deviceId)"
    }

    /// A phone notification: « <app> · <device> » as the title, the
    /// notification's title as subtitle and its text as body.
    public static func notification(
        deviceId: String,
        device: String,
        id: String,
        app: String,
        title: String,
        text: String,
        actions: [String],
        reply: Bool,
        dismissable: Bool,
        silent: Bool,
        language: Language = .current
    ) -> NotificationPlan {
        let app = app.trimmingCharacters(in: .whitespacesAndNewlines)
        var buttons = actions.prefix(maxActions).enumerated().map { index, label in
            NotificationButton(identifier: "\(actionPrefix)\(index)", title: label, kind: .action(index))
        }
        if reply {
            buttons.append(NotificationButton(
                identifier: replyIdentifier,
                title: language == .french ? "Répondre" : "Reply",
                kind: .reply
            ))
        }
        if dismissable {
            buttons.append(NotificationButton(
                identifier: dismissIdentifier,
                title: language == .french ? "Effacer sur le téléphone" : "Dismiss on Phone",
                kind: .dismiss
            ))
        }
        return NotificationPlan(
            identifier: identifier(deviceId: deviceId, id: id),
            title: app.isEmpty ? device : "\(app) · \(device)",
            subtitle: title,
            body: text,
            threadIdentifier: deviceId,
            categoryIdentifier: category(for: buttons),
            buttons: buttons,
            sound: !silent,
            banner: !silent
        )
    }

    /// A call: `ringing` with « Mute », `missed` that stays in
    /// Notification Center; `nil` for the other events, which only end a
    /// ringing one.
    public static func call(
        deviceId: String,
        device: String,
        event: String,
        caller: String?,
        language: Language = .current
    ) -> NotificationPlan? {
        let french = language == .french
        let caller = caller?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty
        switch event {
        case "ringing":
            let mute = NotificationButton(
                identifier: muteIdentifier, title: french ? "Silence" : "Mute", kind: .muteCall
            )
            let title = switch (caller, french) {
            case (let caller?, false): "Incoming call: \(caller)"
            case (let caller?, true): "Appel entrant : \(caller)"
            case (nil, false): "Incoming call"
            case (nil, true): "Appel entrant"
            }
            return NotificationPlan(
                identifier: callIdentifier(deviceId: deviceId),
                title: title, subtitle: "", body: device,
                threadIdentifier: deviceId,
                categoryIdentifier: category(for: [mute]),
                buttons: [mute],
                // The phone rings already.
                sound: false, banner: true
            )
        case "missed":
            let title = switch (caller, french) {
            case (let caller?, false): "Missed call: \(caller)"
            case (let caller?, true): "Appel manqué : \(caller)"
            case (nil, false): "Missed call"
            case (nil, true): "Appel manqué"
            }
            return NotificationPlan(
                identifier: "missed.\(deviceId).\(UUID().uuidString)",
                title: title, subtitle: "", body: device,
                threadIdentifier: deviceId,
                categoryIdentifier: category(for: []),
                buttons: [],
                sound: true, banner: true
            )
        default:
            return nil
        }
    }

    /// « Received <name> », with « Show in Finder ».
    public static func fileReceived(device: String, fileName: String, language: Language = .current) -> NotificationPlan {
        let french = language == .french
        let finder = NotificationButton(
            identifier: finderIdentifier,
            title: french ? "Afficher dans le Finder" : "Show in Finder",
            kind: .showInFinder
        )
        return NotificationPlan(
            identifier: "file.\(UUID().uuidString)",
            title: french ? "\(fileName) reçu" : "Received \(fileName)",
            subtitle: "",
            body: french ? "De \(device), dans Téléchargements" : "From \(device), in Downloads",
            threadIdentifier: "weft",
            categoryIdentifier: category(for: [finder]),
            buttons: [finder],
            sound: true, banner: true
        )
    }

    /// An incoming SMS on the phone: « <sender> · <device> » with an inline
    /// reply action.
    public static func smsReceived(
        deviceId: String,
        device: String,
        thread: Int64,
        sender: String,
        text: String,
        language: Language = .current
    ) -> NotificationPlan {
        let sender = sender.trimmingCharacters(in: .whitespacesAndNewlines)
        let reply = NotificationButton(
            identifier: replyIdentifier,
            title: language == .french ? "Répondre" : "Reply",
            kind: .reply
        )
        return NotificationPlan(
            identifier: "sms.\(deviceId).\(thread).\(UUID().uuidString)",
            title: sender.isEmpty ? device : "\(sender) · \(device)",
            subtitle: "",
            body: text,
            threadIdentifier: "sms.\(deviceId).\(thread)",
            categoryIdentifier: category(for: [reply]),
            buttons: [reply],
            sound: true,
            banner: true
        )
    }

    /// A shortcut triggered by another device.
    public static func commandRan(
        device: String,
        name: String,
        language: Language = .current
    ) -> NotificationPlan {
        let title = language == .french ? "Raccourci lancé : \(name)" : "Shortcut ran: \(name)"
        return plain(title: title, body: device)
    }

    /// A plain notice of this Mac: a ping, a link, a failed transfer.
    public static func plain(title: String, body: String) -> NotificationPlan {
        NotificationPlan(
            identifier: "notice.\(UUID().uuidString)",
            title: title, subtitle: "", body: body,
            threadIdentifier: "weft",
            categoryIdentifier: category(for: []),
            buttons: [],
            sound: true, banner: true
        )
    }

    /// The category of a set of buttons: the same buttons, the same
    /// category, so categories do not pile up.
    public static func category(for buttons: [NotificationButton]) -> String {
        guard !buttons.isEmpty else { return "weft.plain" }
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for button in buttons {
            for byte in "\(button.identifier)\u{1f}\(button.title)\u{1e}".utf8 {
                hash ^= UInt64(byte)
                hash = hash &* 0x0100_0000_01b3
            }
        }
        return "weft.\(String(hash, radix: 16))"
    }

    /// What a press means. `actionIdentifier` is macOS's; `actions` the
    /// labels the phone sent, kept with the notification.
    public static func response(
        actionIdentifier: String,
        replyText: String?,
        actions: [String]
    ) -> NotificationResponse {
        switch actionIdentifier {
        case "com.apple.UNNotificationDefaultActionIdentifier":
            return .open
        case "com.apple.UNNotificationDismissActionIdentifier":
            return .nothing
        case replyIdentifier:
            guard let text = replyText?.trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty else {
                return .nothing
            }
            return .reply(text)
        case dismissIdentifier:
            return .dismiss
        case muteIdentifier:
            return .muteCall
        case finderIdentifier:
            return .showInFinder
        default:
            guard actionIdentifier.hasPrefix(actionPrefix),
                  let index = Int(actionIdentifier.dropFirst(actionPrefix.count)),
                  actions.indices.contains(index) else { return .nothing }
            return .action(actions[index])
        }
    }
}

extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
