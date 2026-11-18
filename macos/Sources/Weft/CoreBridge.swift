// The two interfaces the Rust core calls back: the pasteboard, and the
// delegate that reports snapshots, notices and received texts.

import AppKit
import os
import WeftFFI
import WeftKit

let log = Logger(subsystem: "fr.frohub.weft.mac", category: "app")

/// `NSPasteboard.general` as the core's clipboard rules need it.
///
/// The core calls it only from `poll_pasteboard` and `send_clipboard_now`,
/// which the app calls on the main thread, so it never runs concurrently.
final class MacPasteboard: WeftFFI.Pasteboard, @unchecked Sendable {
    private var board: NSPasteboard { .general }
    private var receivedImageFingerprint: Data?

    func changeCount() -> Int64 {
        Int64(board.changeCount)
    }

    func readText() -> PasteboardContent {
        let types = board.types?.map(\.rawValue) ?? []
        switch PasteboardPolicy.decide(types: types) {
        case .excluded:
            return .excluded
        case .noText:
            for type in [NSPasteboard.PasteboardType.png,
                         NSPasteboard.PasteboardType("public.jpeg"), .tiff] {
                guard board.availableType(from: [type]) != nil else { continue }
                guard let data = board.data(forType: type) else { continue }
                guard let image = ClipboardImages.prepare(data) else { return .tooLarge }
                if image.fingerprint == receivedImageFingerprint { return .excluded }
                receivedImageFingerprint = nil
                return .image(mime: image.mime, bytes: image.bytes)
            }
            return .noText
        case .readText:
            receivedImageFingerprint = nil
            guard let text = board.string(forType: .string) else { return .noText }
            return .text(text: text)
        }
    }

    func writeImage(mime: String, bytes: Data) -> Int64? {
        guard let image = ClipboardImages.prepare(bytes, mime: mime) else { return nil }
        let item = NSPasteboardItem()
        let type = NSPasteboard.PasteboardType(image.mime == "image/png" ? "public.png" : "public.jpeg")
        guard item.setData(image.bytes, forType: type) else { return nil }
        if let bundle = Bundle.main.bundleIdentifier {
            item.setString(bundle, forType: NSPasteboard.PasteboardType(PasteboardPolicy.source))
        }
        board.clearContents()
        guard board.writeObjects([item]) else { return nil }
        receivedImageFingerprint = image.fingerprint
        return Int64(board.changeCount)
    }

    func writeText(text: String, private: Bool) -> Int64? {
        board.clearContents()
        var types: [NSPasteboard.PasteboardType] = [.string]
        types.append(NSPasteboard.PasteboardType(PasteboardPolicy.source))
        if `private` {
            types.append(NSPasteboard.PasteboardType(PasteboardPolicy.concealed))
        }
        board.declareTypes(types, owner: nil)
        guard board.setString(text, forType: .string) else { return nil }
        if let bundle = Bundle.main.bundleIdentifier {
            board.setString(bundle, forType: NSPasteboard.PasteboardType(PasteboardPolicy.source))
        }
        if `private` {
            board.setData(Data(), forType: NSPasteboard.PasteboardType(PasteboardPolicy.concealed))
        }
        receivedImageFingerprint = nil
        return Int64(board.changeCount)
    }
}

/// Receives the core's calls on Rust threads and hands them to the model
/// on the main actor. Snapshot changes are coalesced: one refresh waits at
/// a time, whatever the number of generations behind it.
final class CoreEvents: CoreDelegate, @unchecked Sendable {
    private weak var model: AppModel?
    private let refreshQueued = OSAllocatedUnfairLock(initialState: false)

    init(model: AppModel) {
        self.model = model
    }

    func snapshotChanged(generation: UInt64) {
        let alreadyQueued = refreshQueued.withLock { queued in
            defer { queued = true }
            return queued
        }
        if alreadyQueued { return }
        Task { @MainActor [weak self] in
            self?.refreshQueued.withLock { $0 = false }
            self?.model?.refresh()
        }
    }

    func notice(notice: CoreNotice) {
        Task { @MainActor [weak self] in
            self?.model?.handle(notice)
        }
    }

    func input(input: RemoteInput) {
        Task { @MainActor [weak self] in
            self?.model?.handleInput(input)
        }
    }

    func clipboardWaiting() {
        Task { @MainActor [weak self] in
            self?.model?.pollPasteboard()
        }
    }
}

extension CoreNotice {
    /// The packet family, for logs.
    var kind: String {
        switch self {
        case .clipboardImage: "clipboardImage"
        case .ring: "ring"
        case .ringEnded: "ringEnded"
        case .ping: "ping"
        case .link: "link"
        case .fileReceived: "fileReceived"
        case .transferFailed: "transferFailed"
        case .notification: "notification"
        case .notificationRemoved: "notificationRemoved"
        case .call: "call"
        case .mediaCommand: "mediaCommand"
        case .setVolume: "setVolume"
        case .smsReceived: "smsReceived"
        case .commandRan: "commandRan"
        }
    }
}
