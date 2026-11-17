// The app's state on the main actor: the core, its last snapshot, and what
// the windows show.

import AppKit
import SwiftUI
import WeftFFI
import WeftKit

@MainActor
final class AppModel: ObservableObject {
    /// How often the pasteboard is looked at: there is no notification
    /// for its changes, and reading `changeCount` costs next to nothing.
    static let pollInterval: TimeInterval = 0.5

    @Published private(set) var snapshot: CoreSnapshot?
    /// Why the core could not start, if it did not.
    @Published private(set) var startError: String?
    @Published var showingPairing = false
    @Published var selection: String?
    /// The device whose SMS sheet is open, if any, and its active thread.
    @Published var messagesDevice: String?
    @Published var selectedThread: Int64?
    /// A blocking change (relay, leaving the group) is under way.
    @Published private(set) var busy = false
    /// The device this Mac rings, and until when, to show « Stop ».
    @Published var rang: (device: String, until: Date)?
    /// The Settings tab shown.
    @Published var settingsTab = Development.settingsTab ?? "general"

    let notifier = Notifier()
    let ringer = Ringer()
    let volume = SystemVolume()
    let nowPlaying = NowPlaying()
    let remoteInput = RemoteInputController()
    /// Sparkle, started at once: it checks on its own schedule.
    let updates = Updates()
    /// The covers of the other devices' players, by device and art id,
    /// decoded once: the last `coverLimit` looked up (a failed one too).
    private var covers: [String: NSImage?] = [:]
    private var coverOrder: [String] = []
    static let coverLimit = 16
    private(set) var core: WeftCore?
    /// Devices `WEFT_DEV_SEND_FILE` was offered to.
    private var developmentFileSent: Set<String> = []
    private var hadDevices = false
    private var events: CoreEvents?
    private var timer: Timer?
    private var writtenCode: String?
    private var pairingRequested = false
    /// Set by the first view that appears: SwiftUI gives it only to views.
    var openWindow: OpenWindowAction?
    /// The same, for the Settings window.
    var openSettings: OpenSettingsAction?

    var devices: [DeviceInfo] { snapshot?.devices ?? [] }

    func start() {
        guard core == nil else { return }
        let events = CoreEvents(model: self)
        self.events = events
        let config = CoreConfig(
            stateDir: Self.stateDirectory().path,
            downloadsDir: (Development.downloadsDirectory ?? Self.downloadsDirectory()).path,
            relayUrl: Development.relayURL,
            defaultName: Host.current().localizedName
        )
        do {
            core = try WeftCore.start(config: config, delegate: events, pasteboard: MacPasteboard())
        } catch {
            startError = "\(error)"
            log.error("core did not start: \(String(describing: error), privacy: .public)")
            return
        }
        notifier.onResponse = { [weak self] response, info in self?.respond(response, info: info) }
        notifier.start()
        ringer.foundIt = { [weak self] in self?.core?.foundIt() }
        volume.onChange = { [weak self] level, muted in self?.core?.localVolume(level: level, muted: muted) }
        volume.start()
        nowPlaying.onChange = { [weak self] players in self?.core?.setLocalMedia(players: players) }
        refresh()
        hadDevices = !devices.isEmpty
        let timer = Timer(timeInterval: Self.pollInterval, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.pollPasteboard()
                if Development.fileToSend != nil, let snapshot = self?.snapshot {
                    self?.offerDevelopmentFile(snapshot)
                }
            }
        }
        timer.tolerance = 0.1
        // Common modes: it keeps running while a menu is open.
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    func stop() {
        timer?.invalidate()
        remoteInput.stop()
        nowPlaying.stop()
        core?.stop()
    }

    func refresh() {
        guard let core else { return }
        let snapshot = core.snapshot()
        self.snapshot = snapshot
        if let selection, snapshot.device(selection) == nil {
            self.selection = nil
        }
        if selection == nil {
            selection = snapshot.devices.first?.id
        }
        if let messagesDevice, !snapshot.features.sms || snapshot.device(messagesDevice) == nil {
            self.messagesDevice = nil
            self.selectedThread = nil
        }
        // Right after a first pairing: the moment to ask for notifications.
        if !hadDevices, !snapshot.devices.isEmpty {
            notifier.askIfNeeded()
        }
        hadDevices = !snapshot.devices.isEmpty
        if let rang, rang.until < .now || snapshot.device(rang.device) == nil {
            self.rang = nil
        }
        if snapshot.ringing == nil, ringer.isRinging {
            ringer.stop()
        }
        // What plays here is read only while the media feature is on.
        if snapshot.features.media != nowPlaying.isRunning {
            snapshot.features.media ? nowPlaying.start() : nowPlaying.stop()
        }
        developmentHooks(snapshot)
    }

    /// The cover `art` of one of `deviceId`'s players, once the core holds
    /// it. Asked of the core only the first time this id is seen.
    func cover(deviceId: String, art: String) -> NSImage? {
        let key = "\(deviceId)\u{0}\(art)"
        if let known = covers[key] { return known }
        guard let core else { return nil }
        let image = core.mediaArt(deviceId: deviceId, art: art).flatMap { NSImage(data: $0.data) }
        covers[key] = .some(image)
        coverOrder.append(key)
        if coverOrder.count > Self.coverLimit {
            covers[coverOrder.removeFirst()] = nil
        }
        return image
    }

    func pollPasteboard() {
        _ = core?.pollPasteboard()
    }

    // MARK: Commands

    func pair() {
        core?.startPairing()
        showingPairing = true
        showMainWindow()
    }

    func join(code: String) {
        core?.join(code: code)
    }

    func closePairing() {
        core?.cancelPairing()
        showingPairing = false
    }

    func remove(_ device: DeviceInfo) {
        do {
            try core?.removeDevice(deviceId: device.id)
        } catch {
            log.error("remove failed: \(String(describing: error), privacy: .public)")
        }
    }

    func rename(_ name: String) {
        guard case .success(let name) = DeviceName.check(name), name != snapshot?.name else { return }
        do {
            try core?.rename(name: name)
        } catch {
            log.error("rename refused: \(String(describing: error), privacy: .public)")
        }
    }

    /// Sends the pasteboard now; beeps when there was nothing to send.
    func sendClipboardNow() {
        guard let core else { return }
        let outcome = core.sendClipboardNow()
        if outcome != .sent {
            log.info("clipboard not sent: \(String(describing: outcome), privacy: .public)")
            NSSound.beep()
        }
    }

    /// Changes the relay; returns why it was refused, if it was.
    func setRelay(_ url: String) async -> String? {
        guard let core else { return nil }
        busy = true
        defer { busy = false }
        let result = await Task.detached { () -> String? in
            do {
                try core.setRelayUrl(url: url)
                return nil
            } catch CoreError.InvalidRelayUrl(let message) {
                return message
            } catch {
                return "\(error)"
            }
        }.value
        refresh()
        return result
    }

    /// Leaves the group; returns whether the other devices were told.
    func leaveGroup() async -> Bool {
        guard let core else { return false }
        busy = true
        defer { busy = false }
        let told = await Task.detached { (try? core.leaveGroup()) ?? false }.value
        refresh()
        return told
    }

    // MARK: Windows

    func showMainWindow() {
        openWindow?(id: WeftApp.mainWindow)
        NSApp.activate()
    }

    // MARK: Places

    static func stateDirectory() -> URL {
        if let dir = Development.stateDirectory { return dir }
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return support.appendingPathComponent("Weft", isDirectory: true)
    }

    static func downloadsDirectory() -> URL {
        FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
            ?? stateDirectory().appendingPathComponent("Received", isDirectory: true)
    }

    // MARK: Development

    private func developmentHooks(_ snapshot: CoreSnapshot) {
        if Development.codeFile != nil, case .showingCode(let code, _) = snapshot.pairing,
           code != writtenCode {
            writtenCode = code
            Development.writeCode(code)
        }
        if Development.startsPairing, !pairingRequested, snapshot.isConnected {
            pairingRequested = true
            pair()
        }
        offerDevelopmentFile(snapshot)
    }

    /// `WEFT_DEV_SEND_FILE`: offers the file once to each device that
    /// takes files, once it exists. Looked at with each snapshot and on
    /// the pasteboard's clock.
    private func offerDevelopmentFile(_ snapshot: CoreSnapshot) {
        if let file = Development.fileToSend, FileManager.default.fileExists(atPath: file.path) {
            for device in snapshot.devices
            where device.handles.contains("file.offer") && !developmentFileSent.contains(device.id) {
                developmentFileSent.insert(device.id)
                sendFiles([file], to: device.id)
                Development.record("offered file")
            }
        }
    }
}
