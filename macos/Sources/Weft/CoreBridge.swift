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

    func changeCount() -> Int64 {
        Int64(board.changeCount)
    }

    func readText() -> PasteboardContent {
        let types = board.types?.map(\.rawValue) ?? []
        switch PasteboardPolicy.decide(types: types) {
        case .excluded:
            return .excluded
        case .noText:
            return .noText
        case .readText:
            guard let text = board.string(forType: .string) else { return .noText }
            return .text(text: text)
        }
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
        // This version shows none of them yet. Only the kind is logged,
        // never what it says.
        log.info("notice not handled yet: \(notice.kind, privacy: .public)")
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
