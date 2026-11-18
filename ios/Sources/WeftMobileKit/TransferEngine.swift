// Outgoing and incoming chunked 128 KiB (`0x08` FileChunk) file transfers with
// an 8-chunk in-flight window, file-name sanitisation & deduplication, 24h
// partial resume in a private staging directory, and in-memory
// `clipboard.image.offer` transfers (<= 16 MiB, PNG/JPEG magic + BLAKE2b
// `WeftProtocol.contentHash` verification).

import Foundation
import WeftProtocol

// MARK: - File name sanitisation & deduplication (docs/features.md §4)

public enum FileNameSanitizer {
    public static let maxBytes = 200
    public static let maxExtensionChars = 16
    public static let fallbackName = "file"

    private static let reservedWindowsNames: Set<String> = {
        var set: Set<String> = ["con", "prn", "aux", "nul"]
        for n in 1...9 {
            set.insert("com\(n)")
            set.insert("lpt\(n)")
        }
        return set
    }()

    private static let forbiddenASCII: Set<UInt32> = Set("/\\:*?\"<>|".unicodeScalars.map(\.value))

    /// Sanitises an offered file name: no path separators, no `..`, no control
    /// or Unicode format characters, no reserved Windows device names, at most
    /// 200 UTF-8 bytes while preserving the extension, and never empty.
    public static func sanitize(_ raw: String) -> String {
        var replaced = String.UnicodeScalarView()
        replaced.reserveCapacity(raw.unicodeScalars.count)
        for scalar in raw.unicodeScalars {
            let v = scalar.value
            let isControl = v <= 0x1F || (0x7F...0x9F).contains(v)
            let isFormat = scalar.properties.generalCategory == .format
            let isForbiddenASCII = v < 0x80 && forbiddenASCII.contains(v)
            if isControl || isFormat || isForbiddenASCII {
                replaced.append("_")
            } else {
                replaced.append(scalar)
            }
        }

        // Collapse runs of 2+ dots into a single dot.
        var collapsed = ""
        collapsed.reserveCapacity(replaced.count)
        var previousWasDot = false
        for char in String(replaced) {
            if char == "." {
                if !previousWasDot {
                    collapsed.append(".")
                }
                previousWasDot = true
            } else {
                collapsed.append(char)
                previousWasDot = false
            }
        }

        var name = collapsed.trimmingCharacters(in: .whitespacesAndNewlines)
        while name.first == "." {
            name.removeFirst()
        }
        while let last = name.last, last == "." || last == " " {
            name.removeLast()
        }
        name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if name.isEmpty {
            name = fallbackName
        }

        let (base, ext) = splitExtension(name)
        var safeBase = base
        if reservedWindowsNames.contains(safeBase.lowercased()) {
            safeBase = "_\(safeBase)"
        }
        return fitName(base: safeBase, ext: ext, suffix: "")
    }

    /// Returns the first of `name`, `base (1).ext`, `base (2).ext`... for which
    /// `exists` returns `false`, always fitting within 200 UTF-8 bytes.
    public static func uniqueName(_ sanitizedName: String, exists: (String) -> Bool) -> String {
        if !exists(sanitizedName) {
            return sanitizedName
        }
        let (base, ext) = splitExtension(sanitizedName)
        var n = 1
        while true {
            let candidate = fitName(base: base, ext: ext, suffix: " (\(n))")
            if !exists(candidate) {
                return candidate
            }
            n += 1
        }
    }

    private static func splitExtension(_ name: String) -> (base: String, ext: String) {
        let chars = Array(name)
        guard let dotIndex = chars.lastIndex(of: "."),
              dotIndex > 0,
              dotIndex < chars.count - 1,
              (chars.count - 1 - dotIndex) <= maxExtensionChars else {
            return (name, "")
        }
        return (String(chars[..<dotIndex]), String(chars[dotIndex...]))
    }

    private static func fitName(base: String, ext: String, suffix: String) -> String {
        let budget = maxBytes - ext.utf8.count - suffix.utf8.count
        var cut = ""
        var used = 0
        for char in base {
            let chunk = String(char)
            let count = chunk.utf8.count
            if used + count > budget {
                break
            }
            cut.append(char)
            used += count
        }
        while let last = cut.last, last == "." || last == " " {
            cut.removeLast()
        }
        if cut.isEmpty {
            cut = fallbackName
        }
        return cut + suffix + ext
    }
}

// MARK: - Clipboard image validation

public enum ClipboardImageValidator {
    public static let maxBytes = Int(PacketConstants.maxClipboardImageSize)

    public static func hashHex(for data: Data) -> String {
        WeftProtocol.contentHash(data).hexString
    }

    public static func canOffer(data: Data, mime: String) -> Bool {
        guard (1...maxBytes).contains(data.count),
              (mime == "image/png" || mime == "image/jpeg"),
              PacketConstants.looksLikeImage(mime: mime, data: data) else {
            return false
        }
        return true
    }

    public static func verify(data: Data, expectedMime: String, expectedHash: String) -> Bool {
        guard canOffer(data: data, mime: expectedMime),
              let normalizedExpected = PacketConstants.normalizeHash16(expectedHash) else {
            return false
        }
        let actualHex = hashHex(for: data)
        return actualHex.utf8.elementsEqual(normalizedExpected.utf8)
    }
}

// MARK: - Transfer snapshots & outbound actions

public enum TransferStatus: Equatable, Sendable {
    case offered
    case transferring
    case completed(URL?)
    case rejected(String?)
    case cancelled
    case failed(String)
}

public struct TransferProgress: Equatable, Sendable, Identifiable {
    public let id: String
    public let peerHex: String
    public let name: String
    public let size: UInt64
    public var transferred: UInt64
    public let isOutgoing: Bool
    public let isClipboardImage: Bool
    public var status: TransferStatus

    public var isActive: Bool {
        switch status {
        case .offered, .transferring: true
        case .completed, .rejected, .cancelled, .failed: false
        }
    }

    public var fractionCompleted: Double {
        guard size > 0 else { return status == .offered ? 0 : 1 }
        return min(1.0, Double(transferred) / Double(size))
    }
}

public enum TransferOutputAction: Equatable, Sendable {
    case sendPacket(peerHex: String, packet: Packet)
    case sendFileChunk(peerHex: String, chunkPlaintext: Data)
    case fileReceived(peerHex: String, id: String, name: String, fileURL: URL)
    case clipboardImageReceived(peerHex: String, id: String, data: Data, mime: String)
    case clipboardImageRejected(peerHex: String, id: String, reason: String)
}

// MARK: - Partial staging store (24h resume)

private struct PartialMetadata: Codable, Equatable {
    let peerHex: String
    let id: String
    let name: String
    let size: UInt64
    var updatedAtMs: Int64
}

// MARK: - TransferEngine

public final class TransferEngine: @unchecked Sendable {
    public static let chunkSize = Weft.maxFileChunkDataLength // 128 KiB
    public static let maxUnackedChunks = 8
    public static let maxTransfersPerPeer = 4
    public static let partialLifetimeMs: Int64 = 24 * 60 * 60 * 1000
    public static let stallMs: Int64 = 30 * 1000
    public static let giveUpMs: Int64 = 10 * 60 * 1000
    public static let maxIncomingClipboardBytes: UInt64 = 32 * 1024 * 1024
    public static let maxIncomingClipboardImages = 4

    public let stagingDirectory: URL
    public let destinationDirectory: URL

    private struct OutgoingSource {
        enum Backing {
            case file(URL)
            case memory(Data)
        }
        let id: String
        let peerHex: String
        let name: String
        let mime: String?
        let size: UInt64
        let isClipboardImage: Bool
        let hashHex: String?
        let backing: Backing

        var state: TransferStatus
        var ackedOffset: UInt64
        var sentOffset: UInt64
        var inFlightChunks: [UInt64] // end-offset of each unacked chunk in order
        var lastProgressAtMs: Int64
        var lastActivityAtMs: Int64

        var isActiveState: Bool { state.isActive }
    }

    private struct IncomingSink {
        enum Backing {
            case stagedFile(partURL: URL, metaURL: URL)
            case clipboardMemory(expectedMime: String, expectedHash: String, buffer: Data)
        }
        let id: String
        let peerHex: String
        let offeredName: String
        let mime: String?
        let size: UInt64
        var receivedOffset: UInt64
        var status: TransferStatus
        var lastActivityAtMs: Int64
        var backing: Backing

        var isActiveState: Bool { status.isActive }
    }

    private let lock = NSLock()
    private var outgoing: [String: OutgoingSource] = [:] // keyed by "\(peerHex)/\(id)"
    private var ownedOutgoingFiles: [String: URL] = [:]
    private var clipboardStartedAt: [String: Int64] = [:]
    private var incoming: [String: IncomingSink] = [:]   // keyed by "\(peerHex)/\(id)"
    private var completedHistory: [(key: String, atMs: Int64)] = []
    private var freeSpaceOverride: UInt64?

    public init(
        stagingDirectory: URL,
        destinationDirectory: URL,
        freeSpaceOverride: UInt64? = nil
    ) throws {
        self.stagingDirectory = stagingDirectory
        self.destinationDirectory = destinationDirectory
        self.freeSpaceOverride = freeSpaceOverride
        let fm = FileManager.default
        if !fm.fileExists(atPath: stagingDirectory.path) {
            try fm.createDirectory(
                at: stagingDirectory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: NSNumber(value: Int16(0o700))]
            )
        }
        var resourceValues = URLResourceValues()
        resourceValues.isExcludedFromBackup = true
        var mutableStaging = stagingDirectory
        try? mutableStaging.setResourceValues(resourceValues)

        if !fm.fileExists(atPath: destinationDirectory.path) {
            try fm.createDirectory(at: destinationDirectory, withIntermediateDirectories: true)
        }
    }

    private static func key(peerHex: String, id: String) -> String {
        "\(peerHex.lowercased())/\(id.lowercased())"
    }

    public static func randomTransferIDHex() -> String {
        randomMessageID().hexString // 16 random cryptographic bytes from libsodium
    }

    // MARK: - Snapshots

    public func transfersSnapshot() -> [TransferProgress] {
        lock.lock()
        defer { lock.unlock() }
        var result: [TransferProgress] = []
        for item in outgoing.values where !item.isClipboardImage {
            result.append(TransferProgress(
                id: item.id,
                peerHex: item.peerHex,
                name: item.name,
                size: item.size,
                transferred: item.ackedOffset,
                isOutgoing: true,
                isClipboardImage: false,
                status: item.state
            ))
        }
        for item in incoming.values {
            if case .clipboardMemory = item.backing { continue }
            result.append(TransferProgress(
                id: item.id,
                peerHex: item.peerHex,
                name: FileNameSanitizer.sanitize(item.offeredName),
                size: item.size,
                transferred: item.receivedOffset,
                isOutgoing: false,
                isClipboardImage: false,
                status: item.status
            ))
        }
        return result.sorted { lhs, rhs in lhs.id < rhs.id }
    }

    // MARK: - Outgoing file & clipboard image offers

    public func offerFile(
        fileURL: URL,
        ownsFile: Bool = false,
        mime: String? = nil,
        to peerHex: String,
        transferIDHex: String = TransferEngine.randomTransferIDHex(),
        nowMs: Int64 = Int64(Date().timeIntervalSince1970 * 1000)
    ) throws -> [TransferOutputAction] {
        let attrs = try FileManager.default.attributesOfItem(atPath: fileURL.path)
        let size = (attrs[.size] as? NSNumber)?.uint64Value ?? 0
        guard size <= PacketConstants.maxFileSize else {
            throw NSError(domain: "WeftTransfer", code: 1, userInfo: [NSLocalizedDescriptionKey: "File exceeds 4 GiB limit"])
        }
        guard let id = PacketConstants.normalizeHex16(transferIDHex) else {
            throw NSError(domain: "WeftTransfer", code: 2, userInfo: [NSLocalizedDescriptionKey: "Invalid transfer ID"])
        }
        let rawName = fileURL.lastPathComponent.isEmpty ? FileNameSanitizer.fallbackName : fileURL.lastPathComponent
        let cleanPeer = peerHex.lowercased()
        let entryKey = Self.key(peerHex: cleanPeer, id: id)

        let offer = FileOfferPayload(id: id, name: rawName, size: size, mime: mime)
        lock.lock()
        if ownsFile { ownedOutgoingFiles[entryKey] = fileURL }
        outgoing[entryKey] = OutgoingSource(
            id: id,
            peerHex: cleanPeer,
            name: rawName,
            mime: mime,
            size: size,
            isClipboardImage: false,
            hashHex: nil,
            backing: .file(fileURL),
            state: .offered,
            ackedOffset: 0,
            sentOffset: 0,
            inFlightChunks: [],
            lastProgressAtMs: nowMs,
            lastActivityAtMs: nowMs
        )
        lock.unlock()

        return [.sendPacket(peerHex: cleanPeer, packet: .fileOffer(offer))]
    }

    public func offerDataAsFile(
        data: Data,
        name: String,
        mime: String? = nil,
        to peerHex: String,
        transferIDHex: String = TransferEngine.randomTransferIDHex(),
        nowMs: Int64 = Int64(Date().timeIntervalSince1970 * 1000)
    ) throws -> [TransferOutputAction] {
        guard let id = PacketConstants.normalizeHex16(transferIDHex) else {
            throw NSError(domain: "WeftTransfer", code: 2, userInfo: [NSLocalizedDescriptionKey: "Invalid transfer ID"])
        }
        let cleanPeer = peerHex.lowercased()
        let entryKey = Self.key(peerHex: cleanPeer, id: id)
        let size = UInt64(data.count)
        let offer = FileOfferPayload(id: id, name: name, size: size, mime: mime)

        lock.lock()
        outgoing[entryKey] = OutgoingSource(
            id: id,
            peerHex: cleanPeer,
            name: name,
            mime: mime,
            size: size,
            isClipboardImage: false,
            hashHex: nil,
            backing: .memory(data),
            state: .offered,
            ackedOffset: 0,
            sentOffset: 0,
            inFlightChunks: [],
            lastProgressAtMs: nowMs,
            lastActivityAtMs: nowMs
        )
        lock.unlock()

        return [.sendPacket(peerHex: cleanPeer, packet: .fileOffer(offer))]
    }

    public func offerClipboardImage(
        data: Data,
        mime: String,
        to peerHex: String,
        transferIDHex: String = TransferEngine.randomTransferIDHex(),
        nowMs: Int64 = Int64(Date().timeIntervalSince1970 * 1000)
    ) -> [TransferOutputAction] {
        guard ClipboardImageValidator.canOffer(data: data, mime: mime),
              let id = PacketConstants.normalizeHex16(transferIDHex) else {
            return []
        }
        let cleanPeer = peerHex.lowercased()
        let entryKey = Self.key(peerHex: cleanPeer, id: id)
        let hashHex = ClipboardImageValidator.hashHex(for: data)
        let size = UInt64(data.count)

        lock.lock()
        outgoing[entryKey] = OutgoingSource(
            id: id,
            peerHex: cleanPeer,
            name: mime == "image/png" ? "clipboard.png" : "clipboard.jpg",
            mime: mime,
            size: size,
            isClipboardImage: true,
            hashHex: hashHex,
            backing: .memory(data),
            state: .offered,
            ackedOffset: 0,
            sentOffset: 0,
            inFlightChunks: [],
            lastProgressAtMs: nowMs,
            lastActivityAtMs: nowMs
        )
        lock.unlock()

        let offer = ClipboardImageOfferPayload(id: id, mime: mime, size: size, hash: hashHex)
        return [.sendPacket(peerHex: cleanPeer, packet: .clipboardImageOffer(offer))]
    }

    public func inFlightChunkCount(peerHex: String, id: String) -> Int {
        lock.lock()
        defer { lock.unlock() }
        return outgoing[Self.key(peerHex: peerHex, id: id)]?.inFlightChunks.count ?? 0
    }

    // MARK: - Handling incoming packets & chunks

    public func handlePacket(
        from peerHex: String,
        packet: Packet,
        filesEnabled: Bool = true,
        clipboardImagesEnabled: Bool = true,
        nowMs: Int64 = Int64(Date().timeIntervalSince1970 * 1000)
    ) -> [TransferOutputAction] {
        let cleanPeer = peerHex.lowercased()
        lock.lock()
        defer { lock.unlock() }

        expireStaleLocked(nowMs: nowMs)

        switch packet {
        case .fileOffer(let offer):
            return handleIncomingFileOfferLocked(
                peerHex: cleanPeer,
                offer: offer,
                filesEnabled: filesEnabled,
                nowMs: nowMs
            )

        case .clipboardImageOffer(let offer):
            return handleIncomingClipboardOfferLocked(
                peerHex: cleanPeer,
                offer: offer,
                enabled: clipboardImagesEnabled,
                nowMs: nowMs
            )

        case .fileAccept(let id, let offset):
            return handleOutgoingAcceptLocked(peerHex: cleanPeer, id: id, offset: offset, expectClipboard: false, nowMs: nowMs)

        case .clipboardImageAccept(let accept):
            return handleOutgoingAcceptLocked(
                peerHex: cleanPeer,
                id: accept.id,
                offset: accept.offset ?? 0,
                expectClipboard: true,
                nowMs: nowMs
            )

        case .fileReject(let id, let reason):
            let k = Self.key(peerHex: cleanPeer, id: id)
            if var src = outgoing[k], src.isActiveState {
                src.state = .rejected(reason)
                src.inFlightChunks.removeAll()
                outgoing[k] = src
            }
            return []

        case .fileAck(let id, let offset):
            return handleOutgoingAckLocked(peerHex: cleanPeer, id: id, offset: offset, nowMs: nowMs)

        case .fileDone(let id):
            let k = Self.key(peerHex: cleanPeer, id: id)
            if var src = outgoing[k], src.isActiveState {
                src.ackedOffset = src.size
                src.inFlightChunks.removeAll()
                src.state = .completed(nil)
                outgoing[k] = src
                if src.isClipboardImage {
                    outgoing.removeValue(forKey: k)
                }
            }
            return []

        case .fileCancel(let id):
            let k = Self.key(peerHex: cleanPeer, id: id)
            if var src = outgoing[k], src.isActiveState {
                src.state = .cancelled
                src.inFlightChunks.removeAll()
                outgoing[k] = src
            }
            if let sink = incoming[k] {
                cleanupIncomingSinkLocked(sink)
                var cancelled = sink
                cancelled.status = .cancelled
                incoming[k] = cancelled
                if case .clipboardMemory = sink.backing {
                    incoming.removeValue(forKey: k)
                }
            }
            return []

        default:
            return []
        }
    }

    public func handleFileChunk(
        from peerHex: String,
        chunk: FileChunk,
        nowMs: Int64 = Int64(Date().timeIntervalSince1970 * 1000)
    ) -> [TransferOutputAction] {
        let cleanPeer = peerHex.lowercased()
        let id = chunk.transferID.hexString
        let k = Self.key(peerHex: cleanPeer, id: id)

        lock.lock()
        defer { lock.unlock() }

        guard var sink = incoming[k],
              sink.status == .transferring else {
            return []
        }
        // Only accept the exact next expected byte, never empty or past announced size.
        guard !chunk.data.isEmpty,
              chunk.offset == sink.receivedOffset,
              let endOffset = sink.receivedOffset.addingWithoutOverflow(UInt64(chunk.data.count)),
              endOffset <= sink.size else {
            return []
        }

        switch sink.backing {
        case .stagedFile(let partURL, let metaURL):
            do {
                let handle = try FileHandle(forWritingTo: partURL)
                defer { try? handle.close() }
                try handle.seekToEnd()
                try handle.write(contentsOf: chunk.data)
                sink.receivedOffset = endOffset
                sink.lastActivityAtMs = nowMs
                let meta = PartialMetadata(
                    peerHex: cleanPeer,
                    id: id,
                    name: sink.offeredName,
                    size: sink.size,
                    updatedAtMs: nowMs
                )
                if let encodedMeta = try? JSONEncoder().encode(meta) {
                    try? encodedMeta.write(to: metaURL, options: .atomic)
                }
            } catch {
                cleanupIncomingSinkLocked(sink)
                sink.status = .failed(error.localizedDescription)
                incoming[k] = sink
                return [.sendPacket(peerHex: cleanPeer, packet: .fileCancel(id: id))]
            }

            var actions: [TransferOutputAction] = [
                .sendPacket(peerHex: cleanPeer, packet: .fileAck(id: id, offset: endOffset)),
            ]
            if endOffset == sink.size {
                if let finalURL = finalizeStagedFileLocked(partURL: partURL, metaURL: metaURL, offeredName: sink.offeredName) {
                    sink.status = .completed(finalURL)
                    incoming[k] = sink
                    recordCompletedLocked(key: k, atMs: nowMs)
                    actions.append(.sendPacket(peerHex: cleanPeer, packet: .fileDone(id: id)))
                    actions.append(.fileReceived(
                        peerHex: cleanPeer,
                        id: id,
                        name: finalURL.lastPathComponent,
                        fileURL: finalURL
                    ))
                } else {
                    cleanupIncomingSinkLocked(sink)
                    sink.status = .failed("Could not save file")
                    incoming[k] = sink
                    actions.append(.sendPacket(peerHex: cleanPeer, packet: .fileCancel(id: id)))
                }
            } else {
                incoming[k] = sink
            }
            return actions

        case .clipboardMemory(let expectedMime, let expectedHash, var buffer):
            buffer.append(chunk.data)
            sink.receivedOffset = endOffset
            sink.lastActivityAtMs = nowMs
            sink.backing = .clipboardMemory(expectedMime: expectedMime, expectedHash: expectedHash, buffer: buffer)

            var actions: [TransferOutputAction] = [
                .sendPacket(peerHex: cleanPeer, packet: .fileAck(id: id, offset: endOffset)),
            ]
            if endOffset == sink.size {
                incoming.removeValue(forKey: k)
                if ClipboardImageValidator.verify(
                    data: buffer,
                    expectedMime: expectedMime,
                    expectedHash: expectedHash
                ) {
                    recordCompletedLocked(key: k, atMs: nowMs)
                    actions.append(.sendPacket(peerHex: cleanPeer, packet: .fileDone(id: id)))
                    actions.append(.clipboardImageReceived(
                        peerHex: cleanPeer,
                        id: id,
                        data: buffer,
                        mime: expectedMime
                    ))
                } else {
                    actions.append(.sendPacket(peerHex: cleanPeer, packet: .fileCancel(id: id)))
                    actions.append(.clipboardImageRejected(
                        peerHex: cleanPeer,
                        id: id,
                        reason: "hash_or_magic_mismatch"
                    ))
                }
            } else {
                incoming[k] = sink
            }
            return actions
        }
    }

    public func cancelTransfer(peerHex: String, id: String) -> [TransferOutputAction] {
        let cleanPeer = peerHex.lowercased()
        let cleanID = id.lowercased()
        let k = Self.key(peerHex: cleanPeer, id: cleanID)

        lock.lock()
        defer { lock.unlock() }

        var shouldNotify = false
        if var src = outgoing[k], src.isActiveState {
            src.state = .cancelled
            src.inFlightChunks.removeAll()
            outgoing[k] = src
            shouldNotify = true
        }
        if let sink = incoming[k], sink.isActiveState {
            cleanupIncomingSinkLocked(sink)
            var updated = sink
            updated.status = .cancelled
            incoming[k] = updated
            shouldNotify = true
        }
        return shouldNotify ? [.sendPacket(peerHex: cleanPeer, packet: .fileCancel(id: cleanID))] : []
    }

    /// Periodic maintenance: expires 24h partial files, re-offers stalled outgoing files after 30s,
    /// and gives up after 10 minutes without progress.
    public func tick(nowMs: Int64 = Int64(Date().timeIntervalSince1970 * 1000)) -> [TransferOutputAction] {
        lock.lock()
        defer { lock.unlock() }

        expireStaleLocked(nowMs: nowMs)
        var actions: [TransferOutputAction] = []

        for (key, url) in ownedOutgoingFiles where outgoing[key]?.isActiveState != true {
            try? FileManager.default.removeItem(at: url)
            ownedOutgoingFiles.removeValue(forKey: key)
        }

        for (k, var src) in outgoing where src.isActiveState {
            if nowMs - src.lastProgressAtMs >= Self.giveUpMs {
                src.state = .failed(t("Timed out without progress", "Délai dépassé sans progression"))
                src.inFlightChunks.removeAll()
                outgoing[k] = src
                actions.append(.sendPacket(peerHex: src.peerHex, packet: .fileCancel(id: src.id)))
            } else if nowMs - src.lastActivityAtMs >= Self.stallMs && !src.isClipboardImage {
                src.state = .offered
                src.inFlightChunks.removeAll()
                src.lastActivityAtMs = nowMs
                outgoing[k] = src
                let offer = FileOfferPayload(id: src.id, name: src.name, size: src.size, mime: src.mime)
                actions.append(.sendPacket(peerHex: src.peerHex, packet: .fileOffer(offer)))
            }
        }
        return actions
    }

    // MARK: - Private helpers (lock held)

    private func handleOutgoingAcceptLocked(
        peerHex: String,
        id: String,
        offset: UInt64,
        expectClipboard: Bool,
        nowMs: Int64
    ) -> [TransferOutputAction] {
        let k = Self.key(peerHex: peerHex, id: id)
        guard var src = outgoing[k],
              src.isClipboardImage == expectClipboard,
              src.isActiveState,
              offset <= src.size else {
            return []
        }
        src.state = .transferring
        src.ackedOffset = offset
        src.sentOffset = offset
        src.inFlightChunks.removeAll()
        src.lastProgressAtMs = nowMs
        src.lastActivityAtMs = nowMs
        let chunks = fillOutgoingWindowLocked(&src)
        outgoing[k] = src
        return chunks
    }

    private func handleOutgoingAckLocked(
        peerHex: String,
        id: String,
        offset: UInt64,
        nowMs: Int64
    ) -> [TransferOutputAction] {
        let k = Self.key(peerHex: peerHex, id: id)
        guard var src = outgoing[k],
              src.state == .transferring,
              offset > src.ackedOffset,
              offset <= src.sentOffset else {
            return []
        }
        src.ackedOffset = offset
        src.inFlightChunks.removeAll { $0 <= offset }
        src.lastProgressAtMs = nowMs
        src.lastActivityAtMs = nowMs
        let chunks = fillOutgoingWindowLocked(&src)
        outgoing[k] = src
        return chunks
    }

    private func fillOutgoingWindowLocked(_ src: inout OutgoingSource) -> [TransferOutputAction] {
        guard src.state == .transferring,
              let transferIDData = Data(hexString: src.id, expectedCount: Weft.transferIDLength) else {
            return []
        }
        var actions: [TransferOutputAction] = []
        while src.inFlightChunks.count < Self.maxUnackedChunks && src.sentOffset < src.size {
            let remaining = src.size - src.sentOffset
            let length = Int(min(UInt64(Self.chunkSize), remaining))
            guard let slice = readOutgoingSlice(src: src, offset: src.sentOffset, length: length),
                  slice.count == length,
                  let encoded = try? encodeFileChunk(
                      transferID: transferIDData,
                      offset: src.sentOffset,
                      data: slice
                  ) else {
                src.state = .failed(t("Could not read file", "Impossible de lire le fichier"))
                src.inFlightChunks.removeAll()
                break
            }
            let nextEnd = src.sentOffset + UInt64(length)
            src.sentOffset = nextEnd
            src.inFlightChunks.append(nextEnd)
            actions.append(.sendFileChunk(peerHex: src.peerHex, chunkPlaintext: encoded))
        }
        return actions
    }

    private func readOutgoingSlice(src: OutgoingSource, offset: UInt64, length: Int) -> Data? {
        switch src.backing {
        case .memory(let data):
            let start = Int(offset)
            let end = start + length
            guard start >= 0, end <= data.count else { return nil }
            return Data(data[start..<end])
        case .file(let url):
            guard let handle = try? FileHandle(forReadingFrom: url) else { return nil }
            defer { try? handle.close() }
            do {
                try handle.seek(toOffset: offset)
                return try handle.read(upToCount: length)
            } catch {
                return nil
            }
        }
    }

    private func handleIncomingFileOfferLocked(
        peerHex: String,
        offer: FileOfferPayload,
        filesEnabled: Bool,
        nowMs: Int64
    ) -> [TransferOutputAction] {
        let id = offer.id.lowercased()
        let k = Self.key(peerHex: peerHex, id: id)

        if !filesEnabled {
            return [.sendPacket(peerHex: peerHex, packet: .fileReject(id: id, reason: "disabled"))]
        }
        if offer.size > PacketConstants.maxFileSize {
            return [.sendPacket(peerHex: peerHex, packet: .fileReject(id: id, reason: "tooLarge"))]
        }
        if completedHistory.contains(where: { $0.key == k && nowMs - $0.atMs < Self.partialLifetimeMs }) {
            return [.sendPacket(peerHex: peerHex, packet: .fileDone(id: id))]
        }

        let activeForPeer = incoming.values.filter {
            $0.peerHex == peerHex && $0.isActiveState && $0.id != id
        }.count
        if activeForPeer >= Self.maxTransfersPerPeer {
            return [.sendPacket(peerHex: peerHex, packet: .fileReject(id: id, reason: "busy"))]
        }

        let resumeOffset = resumablePartialOffsetLocked(peerHex: peerHex, offer: offer, nowMs: nowMs) ?? 0
        let remainingBytes = offer.size > resumeOffset ? (offer.size - resumeOffset) : 0
        let availableBytes = availableDiskBytesLocked()
        if remainingBytes > 0 && availableBytes < remainingBytes.saturatingMultiply(2) && availableBytes < remainingBytes + 1024 * 1024 {
            return [.sendPacket(peerHex: peerHex, packet: .fileReject(id: id, reason: "noSpace"))]
        }

        let (partURL, metaURL) = stagingURLs(peerHex: peerHex, id: id)
        if resumeOffset == 0 {
            FileManager.default.createFile(
                atPath: partURL.path,
                contents: Data(),
                attributes: [.posixPermissions: NSNumber(value: Int16(0o600))]
            )
        }
        let meta = PartialMetadata(
            peerHex: peerHex,
            id: id,
            name: offer.name,
            size: offer.size,
            updatedAtMs: nowMs
        )
        if let metaData = try? JSONEncoder().encode(meta) {
            try? metaData.write(to: metaURL, options: .atomic)
        }

        // Zero-byte file completes immediately right after `file.accept`.
        if offer.size == 0 {
            var actions: [TransferOutputAction] = [
                .sendPacket(peerHex: peerHex, packet: .fileAccept(id: id, offset: 0)),
            ]
            if let finalURL = finalizeStagedFileLocked(partURL: partURL, metaURL: metaURL, offeredName: offer.name) {
                incoming[k] = IncomingSink(
                    id: id,
                    peerHex: peerHex,
                    offeredName: offer.name,
                    mime: offer.mime,
                    size: 0,
                    receivedOffset: 0,
                    status: .completed(finalURL),
                    lastActivityAtMs: nowMs,
                    backing: .stagedFile(partURL: partURL, metaURL: metaURL)
                )
                recordCompletedLocked(key: k, atMs: nowMs)
                actions.append(.sendPacket(peerHex: peerHex, packet: .fileDone(id: id)))
                actions.append(.fileReceived(
                    peerHex: peerHex,
                    id: id,
                    name: finalURL.lastPathComponent,
                    fileURL: finalURL
                ))
            }
            return actions
        }

        incoming[k] = IncomingSink(
            id: id,
            peerHex: peerHex,
            offeredName: offer.name,
            mime: offer.mime,
            size: offer.size,
            receivedOffset: resumeOffset,
            status: .transferring,
            lastActivityAtMs: nowMs,
            backing: .stagedFile(partURL: partURL, metaURL: metaURL)
        )
        if resumeOffset == offer.size,
           let finalURL = finalizeStagedFileLocked(partURL: partURL, metaURL: metaURL, offeredName: offer.name) {
            incoming[k]?.status = .completed(finalURL)
            recordCompletedLocked(key: k, atMs: nowMs)
            return [
                .sendPacket(peerHex: peerHex, packet: .fileAccept(id: id, offset: resumeOffset)),
                .sendPacket(peerHex: peerHex, packet: .fileDone(id: id)),
                .fileReceived(peerHex: peerHex, id: id, name: finalURL.lastPathComponent, fileURL: finalURL),
            ]
        }

        return [.sendPacket(peerHex: peerHex, packet: .fileAccept(id: id, offset: resumeOffset))]
    }

    private func handleIncomingClipboardOfferLocked(
        peerHex: String,
        offer: ClipboardImageOfferPayload,
        enabled: Bool,
        nowMs: Int64
    ) -> [TransferOutputAction] {
        guard enabled,
              offer.size >= 1,
              offer.size <= PacketConstants.maxClipboardImageSize else {
            return []
        }
        let id = offer.id.lowercased()
        let k = Self.key(peerHex: peerHex, id: id)
        guard PacketConstants.normalizeHex16(id) != nil,
              PacketConstants.normalizeHash16(offer.hash) != nil,
              ["image/png", "image/jpeg"].contains(offer.mime), incoming[k] == nil else { return [] }
        let images = incoming.values.filter { sink in
            if case .clipboardMemory = sink.backing { return true }
            return false
        }
        guard images.count < Self.maxIncomingClipboardImages,
              !images.contains(where: { $0.peerHex == peerHex }),
              images.reduce(UInt64(0), { $0 + $1.size }) + offer.size <= Self.maxIncomingClipboardBytes,
              incoming.values.filter({ $0.peerHex == peerHex && $0.isActiveState }).count < Self.maxTransfersPerPeer else {
            return [.sendPacket(peerHex: peerHex, packet: .fileReject(id: id, reason: "busy"))]
        }
        let buffer = Data()
        clipboardStartedAt[k] = nowMs
        incoming[k] = IncomingSink(
            id: id,
            peerHex: peerHex,
            offeredName: offer.mime == "image/png" ? "clipboard.png" : "clipboard.jpg",
            mime: offer.mime,
            size: offer.size,
            receivedOffset: 0,
            status: .transferring,
            lastActivityAtMs: nowMs,
            backing: .clipboardMemory(expectedMime: offer.mime, expectedHash: offer.hash, buffer: buffer)
        )
        return [.sendPacket(peerHex: peerHex, packet: .clipboardImageAccept(ClipboardImageAcceptPayload(id: id)))]
    }

    private func stagingURLs(peerHex: String, id: String) -> (part: URL, meta: URL) {
        let base = "\(peerHex)_\(id)"
        return (
            stagingDirectory.appendingPathComponent("\(base).part"),
            stagingDirectory.appendingPathComponent("\(base).meta.json")
        )
    }

    private func resumablePartialOffsetLocked(
        peerHex: String,
        offer: FileOfferPayload,
        nowMs: Int64
    ) -> UInt64? {
        let (partURL, metaURL) = stagingURLs(peerHex: peerHex, id: offer.id.lowercased())
        let fm = FileManager.default
        guard fm.fileExists(atPath: partURL.path),
              fm.fileExists(atPath: metaURL.path),
              let metaData = try? Data(contentsOf: metaURL),
              let meta = try? JSONDecoder().decode(PartialMetadata.self, from: metaData) else {
            try? fm.removeItem(at: partURL)
            try? fm.removeItem(at: metaURL)
            return nil
        }
        guard nowMs - meta.updatedAtMs < Self.partialLifetimeMs,
              meta.peerHex == peerHex,
              meta.name == offer.name,
              meta.size == offer.size,
              let attrs = try? fm.attributesOfItem(atPath: partURL.path),
              let partBytes = (attrs[.size] as? NSNumber)?.uint64Value,
              partBytes <= offer.size else {
            try? fm.removeItem(at: partURL)
            try? fm.removeItem(at: metaURL)
            return nil
        }
        return partBytes
    }

    private func finalizeStagedFileLocked(partURL: URL, metaURL: URL, offeredName: String) -> URL? {
        let fm = FileManager.default
        let sanitized = FileNameSanitizer.sanitize(offeredName)
        let unique = FileNameSanitizer.uniqueName(sanitized) { candidate in
            fm.fileExists(atPath: destinationDirectory.appendingPathComponent(candidate).path)
        }
        let targetURL = destinationDirectory.appendingPathComponent(unique)
        do {
            try fm.moveItem(at: partURL, to: targetURL)
            try? fm.removeItem(at: metaURL)
            return targetURL
        } catch {
            return nil
        }
    }

    private func cleanupIncomingSinkLocked(_ sink: IncomingSink) {
        if case .stagedFile(let partURL, let metaURL) = sink.backing {
            try? FileManager.default.removeItem(at: partURL)
            try? FileManager.default.removeItem(at: metaURL)
        }
    }

    private func expireStaleLocked(nowMs: Int64) {
        completedHistory.removeAll { nowMs - $0.atMs >= Self.partialLifetimeMs }
        for (key, started) in clipboardStartedAt {
            guard let sink = incoming[key] else {
                clipboardStartedAt.removeValue(forKey: key)
                continue
            }
            if nowMs - sink.lastActivityAtMs >= Self.stallMs || nowMs - started >= Self.giveUpMs {
                incoming.removeValue(forKey: key)
                clipboardStartedAt.removeValue(forKey: key)
            }
        }
        let fm = FileManager.default
        guard let urls = try? fm.contentsOfDirectory(at: stagingDirectory, includingPropertiesForKeys: nil) else {
            return
        }
        let activeOwnedFiles = Set(ownedOutgoingFiles.values)
        for url in urls where url.lastPathComponent.hasPrefix("outgoing_") && !activeOwnedFiles.contains(url) {
            if let modified = try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate,
               nowMs - Int64(modified.timeIntervalSince1970 * 1000) >= Self.partialLifetimeMs {
                try? fm.removeItem(at: url)
            }
        }
        for url in urls where url.pathExtension == "json" && url.lastPathComponent.hasSuffix(".meta.json") {
            guard let data = try? Data(contentsOf: url),
                  let meta = try? JSONDecoder().decode(PartialMetadata.self, from: data) else {
                continue
            }
            if nowMs - meta.updatedAtMs >= Self.partialLifetimeMs {
                let (partURL, metaURL) = stagingURLs(peerHex: meta.peerHex, id: meta.id)
                let k = Self.key(peerHex: meta.peerHex, id: meta.id)
                if incoming[k]?.isActiveState != true {
                    try? fm.removeItem(at: partURL)
                    try? fm.removeItem(at: metaURL)
                    incoming.removeValue(forKey: k)
                }
            }
        }
    }

    private func recordCompletedLocked(key: String, atMs: Int64) {
        completedHistory.removeAll { $0.key == key }
        completedHistory.append((key: key, atMs: atMs))
        if completedHistory.count > 128 {
            completedHistory.removeFirst(completedHistory.count - 128)
        }
    }

    private func availableDiskBytesLocked() -> UInt64 {
        if let freeSpaceOverride { return freeSpaceOverride }
        if let values = try? destinationDirectory.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]),
           let capacity = values.volumeAvailableCapacityForImportantUsage,
           capacity >= 0 {
            return UInt64(capacity)
        }
        if let attrs = try? FileManager.default.attributesOfFileSystem(forPath: destinationDirectory.path),
           let free = (attrs[.systemFreeSize] as? NSNumber)?.uint64Value {
            return free
        }
        return UInt64.max
    }
}

extension TransferEngine {
    /// Returns the resumable partial byte offset if a non-expired matching `.part` file exists.
    public func resumablePartialOffset(
        peerHex: String,
        offer: FileOfferPayload,
        nowMs: Int64 = Int64(Date().timeIntervalSince1970 * 1000)
    ) -> UInt64? {
        lock.lock()
        defer { lock.unlock() }
        expireStaleLocked(nowMs: nowMs)
        return resumablePartialOffsetLocked(peerHex: peerHex.lowercased(), offer: offer, nowMs: nowMs)
    }
}

private extension UInt64 {
    func addingWithoutOverflow(_ rhs: UInt64) -> UInt64? {
        let (sum, overflow) = addingReportingOverflow(rhs)
        return overflow ? nil : sum
    }

    func saturatingMultiply(_ rhs: UInt64) -> UInt64 {
        let (product, overflow) = multipliedReportingOverflow(by: rhs)
        return overflow ? UInt64.max : product
    }
}

private extension TransferStatus {
    var isActive: Bool {
        switch self {
        case .offered, .transferring: true
        case .completed, .rejected, .cancelled, .failed: false
        }
    }
}
