// What the user does to the other devices, and what the app does with the
// core's notices: rings, pings, links, files, the phone's notifications
// and calls, and this Mac's volume.

import AppKit
import WeftFFI
import WeftKit

extension AppModel {
    // MARK: Commands to a device

    private func attempt(_ what: String, _ call: () throws -> Void) {
        do {
            try call()
        } catch {
            log.error("\(what, privacy: .public) refused: \(String(describing: error), privacy: .public)")
        }
    }

    func ring(_ device: DeviceInfo) {
        attempt("ring") { try core?.ring(deviceId: device.id) }
        rang = (device.id, Date.now.addingTimeInterval(60))
    }

    func stopRinging(_ device: DeviceInfo) {
        attempt("stop ringing") { try core?.stopRinging(deviceId: device.id) }
        rang = nil
    }

    func ping(_ device: DeviceInfo) {
        attempt("ping") { try core?.ping(deviceId: device.id) }
    }

    /// Sends a link; returns whether it was one to send.
    func shareLink(_ url: String, to device: DeviceInfo) -> Bool {
        guard Links.isShareable(url) else { return false }
        attempt("link") { try core?.shareUrl(deviceId: device.id, url: url) }
        return true
    }

    /// Offers regular files to a device; folders and the like are left out.
    func sendFiles(_ urls: [URL], to deviceId: String) {
        let paths = urls.filter { url in
            url.isFileURL && ((try? url.resourceValues(forKeys: [.isRegularFileKey]))?.isRegularFile ?? false)
        }.map(\.path)
        guard !paths.isEmpty else {
            NSSound.beep()
            return
        }
        attempt("send files") { try core?.sendFiles(deviceId: deviceId, paths: paths) }
    }

    /// Lets the user pick files for a device.
    func pickFiles(for device: DeviceInfo) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = true
        panel.prompt = t("Send", "Envoyer")
        panel.message = Language.current == .french
            ? "Choisissez les fichiers à envoyer à \(device.displayName)"
            : "Choose the files to send to \(device.displayName)"
        NSApp.activate()
        guard panel.runModal() == .OK else { return }
        sendFiles(panel.urls, to: device.id)
    }

    func cancelTransfer(_ transfer: TransferInfo) {
        core?.cancelTransfer(transferId: transfer.id)
    }

    func media(_ control: MediaControl, player: String, on device: DeviceInfo) {
        attempt("media command") {
            try core?.mediaCommand(deviceId: device.id, player: player, control: control)
        }
    }

    func setVolume(_ level: UInt8?, muted: Bool?, on device: DeviceInfo) {
        attempt("volume") { try core?.setDeviceVolume(deviceId: device.id, level: level, muted: muted) }
    }

    func setFeatures(_ switches: FeatureSwitches) {
        core?.setFeatures(switches: switches)
    }

    func setCommands(_ commands: [CommandItem]) {
        core?.setCommands(commands: commands)
    }

    func openMessagesSheet(for device: DeviceInfo, thread: Int64? = nil) {
        attempt("open messages") { try core?.openMessages(deviceId: device.id) }
        if let thread {
            attempt("load messages") { try core?.loadMessages(deviceId: device.id, thread: thread, before: nil) }
        }
        selectedThread = thread
        messagesDevice = device.id
    }

    func closeMessagesSheet() {
        messagesDevice = nil
        selectedThread = nil
    }

    func openThread(_ thread: Int64, on device: DeviceInfo) {
        selectedThread = thread
        attempt("load messages") { try core?.loadMessages(deviceId: device.id, thread: thread, before: nil) }
    }

    func loadEarlierMessages(thread: Int64, before: Int64?, on device: DeviceInfo) {
        attempt("load earlier messages") { try core?.loadMessages(deviceId: device.id, thread: thread, before: before) }
    }

    func sendSms(addresses: [String], text: String, thread: Int64?, on device: DeviceInfo) {
        attempt("send sms") { try core?.sendSms(deviceId: device.id, addresses: addresses, text: text, thread: thread) }
    }

    func setControl(_ allowed: Bool, for device: DeviceInfo) {
        attempt("set control") { try core?.setDeviceControl(deviceId: device.id, allowed: allowed) }
        if allowed && !RemoteInputController.isTrusted {
            RemoteInputController.promptAccessibility()
        }
    }

    func sendTypedDiff(from old: String, to new: String, on device: DeviceInfo) {
        for step in RemoteTyping.diff(from: old, to: new) {
            switch step {
            case .backspace(let count):
                for _ in 0..<count {
                    attempt("backspace") { try core?.sendSpecialKey(deviceId: device.id, key: .backspace) }
                }
            case .text(let chunk):
                attempt("type text") { try core?.typeText(deviceId: device.id, text: chunk) }
            case .enter:
                attempt("enter") { try core?.sendSpecialKey(deviceId: device.id, key: .enter) }
            }
        }
    }

    func sendSpecialKey(_ key: SpecialKeyKind, on device: DeviceInfo) {
        attempt("special key") { try core?.sendSpecialKey(deviceId: device.id, key: key) }
    }

    func handleInput(_ input: RemoteInput) {
        remoteInput.apply(input)
    }

    // MARK: Notices

    private func named(_ device: String?) -> String {
        device ?? t("a device", "un appareil")
    }

    /// Acts on what the core reports. Only the kind is logged, never what
    /// it says.
    func handle(_ notice: CoreNotice) {
        log.info("notice: \(notice.kind, privacy: .public)")
        Development.record("notice \(notice.kind)")
        let french = Language.current == .french
        switch notice {
        case .ring(_, let device):
            ringer.start(device: named(device))
        case .ringEnded:
            ringer.stop()
        case .ping(_, let device, let message):
            let device = named(device)
            notifier.post(PhoneNotifications.plain(
                title: french ? "Ping de \(device)" : "Ping from \(device)",
                body: message ?? ""
            ))
        case .link(_, let device, let url):
            let device = named(device)
            guard let link = Links.openable(url) else { return }
            if Development.opensLinks {
                NSWorkspace.shared.open(link)
            } else {
                Development.record("link not opened (development)")
            }
            notifier.post(PhoneNotifications.plain(title: french ? "Lien de \(device)" : "Link from \(device)", body: url))
        case .fileReceived(_, let device, let path):
            let name = URL(fileURLWithPath: path).lastPathComponent
            notifier.post(
                PhoneNotifications.fileReceived(device: named(device), fileName: name),
                info: ["kind": "file", "path": path]
            )
        case .transferFailed(_, let name, let outgoing, let failure):
            let title = switch (outgoing, french) {
            case (true, false): "\(name) was not sent"
            case (true, true): "\(name) n’a pas été envoyé"
            case (false, false): "\(name) was not received"
            case (false, true): "\(name) n’a pas été reçu"
            }
            notifier.post(PhoneNotifications.plain(title: title, body: failure.text))
        case .notification(let from, let device, let id, let app, let title, let text,
                           let dismissable, let actions, let reply, let silent):
            let plan = PhoneNotifications.notification(
                deviceId: from, device: named(device), id: id, app: app, title: title, text: text,
                actions: actions, reply: reply, dismissable: dismissable, silent: silent
            )
            notifier.post(plan, info: [
                "kind": "phone", "device": from, "id": id,
                "actions": Array(actions.prefix(PhoneNotifications.maxActions)),
            ])
        case .notificationRemoved(let from, let id):
            notifier.remove(identifier: PhoneNotifications.identifier(deviceId: from, id: id))
        case .call(let from, let device, let event, let number, let name):
            // Whatever happened, the phone no longer rings.
            notifier.remove(identifier: PhoneNotifications.callIdentifier(deviceId: from))
            let caller = [name, number].compactMap { $0 }.first { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
            if let plan = PhoneNotifications.call(deviceId: from, device: named(device), event: event, caller: caller) {
                notifier.post(plan, info: ["kind": "call", "device": from])
            }
        case .setVolume(_, let level, let muted):
            volume.set(level: level, muted: muted)
        case .mediaCommand(_, let player, let control):
            // To the player this Mac announced (NowPlaying.swift).
            if snapshot?.features.media ?? false {
                nowPlaying.command(control, player: player)
            }
        case .smsReceived(let from, let device, let thread, let sender, let text, let addresses):
            let plan = PhoneNotifications.smsReceived(
                deviceId: from,
                device: named(device),
                thread: thread,
                sender: sender,
                text: text
            )
            notifier.post(plan, info: [
                "kind": "sms",
                "device": from,
                "thread": thread,
                "addresses": addresses,
            ])
        case .commandRan(_, let device, let name):
            notifier.post(PhoneNotifications.commandRan(device: named(device), name: name))
        }
    }

    /// A press on one of Weft's notifications.
    func respond(_ response: NotificationResponse, info: [AnyHashable: Any]) {
        let device = info["device"] as? String
        let id = info["id"] as? String
        switch response {
        case .open:
            if let device { selection = device }
            let kind = info["kind"] as? String
            if kind == "file", let path = info["path"] as? String {
                NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
            } else if kind == "sms", let deviceId = device, let peer = snapshot?.device(deviceId) {
                let thread = (info["thread"] as? NSNumber)?.int64Value ?? (info["thread"] as? Int64)
                showMainWindow()
                openMessagesSheet(for: peer, thread: thread)
            } else {
                showMainWindow()
            }
        case .action(let label):
            guard let device, let id else { return }
            attempt("notification action") {
                try core?.notificationAction(deviceId: device, id: id, action: label)
            }
        case .reply(let text):
            if info["kind"] as? String == "sms" {
                guard let device, let addresses = info["addresses"] as? [String], !addresses.isEmpty else { return }
                let thread = (info["thread"] as? NSNumber)?.int64Value ?? (info["thread"] as? Int64)
                attempt("sms reply") {
                    try core?.sendSms(deviceId: device, addresses: addresses, text: text, thread: thread)
                }
                return
            }
            guard let device, let id else { return }
            attempt("notification reply") {
                try core?.replyToNotification(deviceId: device, id: id, text: text)
            }
        case .dismiss:
            guard let device, let id else { return }
            attempt("notification dismiss") { try core?.dismissNotification(deviceId: device, id: id) }
        case .muteCall:
            guard let device else { return }
            attempt("call mute") { try core?.muteCall(deviceId: device) }
        case .showInFinder:
            if let path = info["path"] as? String {
                NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
            }
        case .nothing:
            break
        }
    }
}
