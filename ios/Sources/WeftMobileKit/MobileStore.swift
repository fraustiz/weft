// Persistent mobile device identity, trusted peers, monotonic send counters,
// sliding replay windows, feature switches, and atomic file persistence.

import Foundation
import WeftProtocol

public struct RelayEndpoint: Equatable, Sendable {
    public let url: String
    public let isTLS: Bool
    /// Lowercased host without port or IPv6 brackets (docs/protocol.md, section 4).
    public let host: String

    public var isAllowed: Bool {
        if isTLS { return true }
        if host == "localhost" || host == "127.0.0.1" || host == "::1" {
            return true
        }
        return ProcessInfo.processInfo.environment["WEFT_ALLOW_INSECURE_WS"] == "1"
    }

    public static func parse(_ rawURL: String) -> RelayEndpoint? {
        let trimmed = rawURL.trimmingCharacters(in: .whitespacesAndNewlines)
        let utf8Count = trimmed.utf8.count
        guard (1...Weft.maxRelayURLLength).contains(utf8Count),
              !trimmed.unicodeScalars.contains(where: {
                  CharacterSet.whitespacesAndNewlines.contains($0) || CharacterSet.controlCharacters.contains($0)
              }),
              let components = URLComponents(string: trimmed),
              let rawScheme = components.scheme?.lowercased(),
              let rawHost = components.host else {
            return nil
        }
        let isTLS: Bool
        switch rawScheme {
        case "wss": isTLS = true
        case "ws": isTLS = false
        default: return nil
        }
        let cleanHost = rawHost
            .trimmingCharacters(in: CharacterSet(charactersIn: "[]"))
            .lowercased()
        guard !cleanHost.isEmpty else { return nil }
        return RelayEndpoint(url: trimmed, isTLS: isTLS, host: cleanHost)
    }
}

public struct FeatureSwitches: Codable, Equatable, Sendable {
    public var clipboardReceive: Bool
    public var files: Bool
    public var links: Bool
    public var battery: Bool
    public var connectivity: Bool
    public var ring: Bool
    public var ping: Bool
    public var media: Bool
    public var volume: Bool
    public var remote: Bool
    public var commands: Bool

    public init(
        clipboardReceive: Bool = true,
        files: Bool = true,
        links: Bool = true,
        battery: Bool = true,
        connectivity: Bool = true,
        ring: Bool = true,
        ping: Bool = true,
        media: Bool = true,
        volume: Bool = true,
        remote: Bool = true,
        commands: Bool = true
    ) {
        self.clipboardReceive = clipboardReceive
        self.files = files
        self.links = links
        self.battery = battery
        self.connectivity = connectivity
        self.ring = ring
        self.ping = ping
        self.media = media
        self.volume = volume
        self.remote = remote
        self.commands = commands
    }

    /// Builds this iPhone's `capabilities` packet (`platform: "ios"`).
    public func capabilitiesPayload() -> CapabilitiesPayload {
        var incoming: Set<String> = ["capabilities", "state.request"]
        var outgoing: Set<String> = [
            "capabilities",
            "state.request",
            "clipboard.image.offer",
            "file.cancel",
        ]

        if clipboardReceive {
            incoming.formUnion([
                "clipboard.image.offer",
                "clipboard.image.accept",
                "file.ack",
                "file.done",
                "file.cancel",
            ])
            outgoing.formUnion([
                "clipboard.image.accept",
                "file.ack",
                "file.done",
            ])
        } else {
            incoming.formUnion([
                "clipboard.image.accept",
                "file.ack",
                "file.done",
                "file.cancel",
            ])
        }

        if files {
            let fileTypes: [String] = [
                "file.offer",
                "file.accept",
                "file.reject",
                "file.ack",
                "file.done",
                "file.cancel",
            ]
            incoming.formUnion(fileTypes)
            outgoing.formUnion(fileTypes)
        }
        if links {
            incoming.insert("share.url")
            outgoing.insert("share.url")
        }
        if battery {
            incoming.insert("battery")
            outgoing.insert("battery")
        }
        if connectivity {
            incoming.insert("connectivity")
            outgoing.insert("connectivity")
        }
        if ring {
            incoming.formUnion(["ring", "ring.stop"])
            outgoing.formUnion(["ring", "ring.stop"])
        }
        if ping {
            incoming.insert("ping")
            outgoing.insert("ping")
        }
        if media {
            incoming.formUnion(["media", "media.art"])
            outgoing.insert("media.command")
        }
        if volume {
            incoming.insert("volume")
            outgoing.insert("volume.set")
        }
        if remote {
            outgoing.formUnion(["input.pointer", "input.click", "input.key", "presenter"])
        }
        if commands {
            incoming.insert("command.list")
            outgoing.insert("command.run")
        }

        return CapabilitiesPayload(
            platform: PacketConstants.platformIOS,
            incoming: incoming.sorted(),
            outgoing: outgoing.sorted()
        )
    }

    public func allowsIncomingPacket(_ packetType: String) -> Bool {
        switch packetType {
        case "capabilities", "state.request":
            return true
        case "clipboard.image.offer":
            return clipboardReceive
        case "clipboard.image.accept":
            return true
        case "file.offer", "file.accept", "file.reject":
            return files
        case "file.ack", "file.done", "file.cancel":
            return files || clipboardReceive || true
        case "share.url":
            return links
        case "battery":
            return battery
        case "connectivity":
            return connectivity
        case "ring", "ring.stop":
            return ring
        case "ping":
            return ping
        case "media", "media.art", "media.command":
            return media
        case "volume", "volume.set":
            return volume
        case "command.list":
            return commands
        default:
            return false
        }
    }
}

public struct TrustedPeer: Codable, Equatable, Sendable, Identifiable {
    public let deviceIDHex: String
    public let signPublicKeyHex: String
    public var kxPublicKeyHex: String
    public var name: String?
    public var addedAtMs: Int64?

    public var id: String { deviceIDHex }

    public var shortID: String { Words.shortID(deviceIDHex) }

    public var displayName: String {
        if let name, !name.isEmpty { return name }
        return shortID
    }

    public var deviceID: Data {
        Data(hexString: deviceIDHex, expectedCount: Weft.deviceIDLength)!
    }

    public var signPublicKey: Data {
        Data(hexString: signPublicKeyHex, expectedCount: 32)!
    }

    public var kxPublicKey: Data {
        Data(hexString: kxPublicKeyHex, expectedCount: 32)!
    }

    public var peerKeys: PeerKeys {
        try! PeerKeys(signPublicKey: signPublicKey, kxPublicKey: kxPublicKey)
    }

    public init(keys: PeerKeys, name: String? = nil, addedAtMs: Int64? = nil) {
        self.deviceIDHex = keys.deviceID.hexString
        self.signPublicKeyHex = keys.signPublicKey.hexString
        self.kxPublicKeyHex = keys.kxPublicKey.hexString
        self.name = name
        self.addedAtMs = addedAtMs
    }

    private enum CodingKeys: String, CodingKey {
        case signPublicKeyHex = "sign_pk"
        case kxPublicKeyHex = "kx_pk"
        case name
        case addedAtMs = "added_at"
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let signHex = try container.decode(String.self, forKey: .signPublicKeyHex).lowercased()
        let kxHex = try container.decode(String.self, forKey: .kxPublicKeyHex).lowercased()
        guard let signData = Data(hexString: signHex, expectedCount: 32),
              let kxData = Data(hexString: kxHex, expectedCount: 32),
              let keys = try? PeerKeys(signPublicKey: signData, kxPublicKey: kxData) else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(codingPath: decoder.codingPath, debugDescription: "Invalid peer public keys")
            )
        }
        let rawName = try container.decodeIfPresent(String.self, forKey: .name)
        let validName: String?
        if let rawName, case .success(let checked) = DeviceName.check(rawName) {
            validName = checked
        } else {
            validName = nil
        }
        let added = try container.decodeIfPresent(Int64.self, forKey: .addedAtMs)
        self.init(keys: keys, name: validName, addedAtMs: added)
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(signPublicKeyHex, forKey: .signPublicKeyHex)
        try container.encode(kxPublicKeyHex, forKey: .kxPublicKeyHex)
        try container.encodeIfPresent(name, forKey: .name)
        try container.encodeIfPresent(addedAtMs, forKey: .addedAtMs)
    }
}

public struct PeerCounterState: Codable, Equatable, Sendable {
    public var txCounter: UInt64
    public var replayWindow: ReplayWindow

    public init(txCounter: UInt64 = 0, replayWindow: ReplayWindow = ReplayWindow()) {
        self.txCounter = txCounter
        self.replayWindow = replayWindow
    }

    private enum CodingKeys: String, CodingKey {
        case txCounter = "send_counter"
        case recvHighest = "recv_highest"
        case recvSeen = "recv_seen"
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        if let str = try? container.decode(String.self, forKey: .txCounter),
           let parsed = UInt64(str) {
            self.txCounter = parsed
        } else {
            self.txCounter = (try? container.decode(UInt64.self, forKey: .txCounter)) ?? 0
        }
        self.replayWindow = try ReplayWindow(from: decoder)
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(String(txCounter), forKey: .txCounter)
        try replayWindow.encode(to: encoder)
    }
}

public enum MobileStoreError: Error, Equatable, Sendable {
    case invalidDeviceName(NameProblem)
    case invalidRelayURL
    case relayChangeWhileInGroup
    case peerNotTrusted(String)
    case wrongRecipient
    case replayRejected(ReplayError)
    case protocolError(WeftError)
    case ioError(String)
}

public enum OpenedEnvelopeEvent: Equatable, Sendable {
    case clipboardText(String)
    case ping
    case welcome(newlyTrustedHex: [String])
    case deviceAdded(peerHex: String, newlyTrustedHex: [String])
    case deviceRemoved(peerHex: String)
    case deviceName(String)
    case packet(Data)
    case fileChunk(Data)
}

public struct OpenedEnvelope: Equatable, Sendable {
    public let senderHex: String
    public let header: Header
    public let event: OpenedEnvelopeEvent
}

private struct PersistedState: Codable {
    static let currentFormat = 1

    var format: Int = currentFormat
    var signSeedHex: String
    var kxSeedHex: String
    var deviceName: String
    var relayURL: String
    var devices: [TrustedPeer]
    var peers: [String: PeerCounterState]
    var features: FeatureSwitches
    var pointerSpeed: Double
}

public final class MobileStore: @unchecked Sendable {
    public static let defaultRelayURL = "wss://weft.frohub.eu/v1"
    public static let defaultPointerSpeed = 1.4
    public static let minPointerSpeed = 0.5
    public static let maxPointerSpeed = 3.0

    public let fileURL: URL
    public let identity: Identity

    private let lock = NSLock()
    private var persisted: PersistedState

    public init(
        fileURL: URL,
        defaultDeviceName: String = "iPhone",
        initialRelayURL: String = MobileStore.defaultRelayURL
    ) throws {
        self.fileURL = fileURL
        let cleanDefaultName = DeviceName.sanitizedFallback(defaultDeviceName, fallback: "iPhone")
        let cleanDefaultRelay = RelayEndpoint.parse(initialRelayURL)?.url ?? Self.defaultRelayURL

        let fm = FileManager.default
        if fm.fileExists(atPath: fileURL.path) {
            var raw = try Data(contentsOf: fileURL)
            #if os(iOS)
            raw = try ProtectedState.open(raw)
            #endif
            let decoded = try JSONDecoder().decode(PersistedState.self, from: raw)
            guard decoded.format == PersistedState.currentFormat,
                  let signSeed = Data(hexString: decoded.signSeedHex, expectedCount: 32),
                  let kxSeed = Data(hexString: decoded.kxSeedHex, expectedCount: 32) else {
                throw MobileStoreError.ioError("Corrupted or unsupported mobile store format")
            }
            self.identity = try Identity(signSeed: signSeed, kxSeed: kxSeed)
            var normalized = decoded
            if case .failure = DeviceName.check(normalized.deviceName) {
                normalized.deviceName = cleanDefaultName
            }
            if RelayEndpoint.parse(normalized.relayURL) == nil {
                normalized.relayURL = cleanDefaultRelay
            }
            normalized.pointerSpeed = min(max(normalized.pointerSpeed, Self.minPointerSpeed), Self.maxPointerSpeed)
            self.persisted = normalized
            #if os(iOS)
            try Self.writeAtomically(persisted, to: fileURL)
            #endif
        } else {
            let signSeed = Self.secureRandomSeed32()
            let kxSeed = Self.secureRandomSeed32()
            self.identity = try Identity(signSeed: signSeed, kxSeed: kxSeed)
            self.persisted = PersistedState(
                signSeedHex: signSeed.hexString,
                kxSeedHex: kxSeed.hexString,
                deviceName: cleanDefaultName,
                relayURL: cleanDefaultRelay,
                devices: [],
                peers: [:],
                features: FeatureSwitches(),
                pointerSpeed: Self.defaultPointerSpeed
            )
            try Self.writeAtomically(persisted, to: fileURL)
        }
    }

    private static func secureRandomSeed32() -> Data {
        var rng = SystemRandomNumberGenerator()
        var bytes = [UInt8](repeating: 0, count: 32)
        for index in bytes.indices {
            bytes[index] = UInt8.random(in: .min ... .max, using: &rng)
        }
        return Data(bytes)
    }

    private static func writeAtomically(_ state: PersistedState, to destination: URL) throws {
        let fm = FileManager.default
        var directory = destination.deletingLastPathComponent()
        if !fm.fileExists(atPath: directory.path) {
            try fm.createDirectory(
                at: directory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: NSNumber(value: Int16(0o700))]
            )
        }
        var resourceValues = URLResourceValues()
        resourceValues.isExcludedFromBackup = true
        try? directory.setResourceValues(resourceValues)

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        var payload = try encoder.encode(state)
        #if os(iOS)
        payload = try ProtectedState.seal(payload)
        #endif

        let tempURL = directory.appendingPathComponent(".\(destination.lastPathComponent).tmp.\(UUID().uuidString)")
        do {
            #if os(iOS)
            try payload.write(to: tempURL, options: [.completeFileProtectionUntilFirstUserAuthentication])
            try fm.setAttributes([
                .posixPermissions: NSNumber(value: Int16(0o600)),
                .protectionKey: FileProtectionType.completeUntilFirstUserAuthentication,
            ], ofItemAtPath: tempURL.path)
            #else
            try payload.write(to: tempURL)
            try fm.setAttributes([
                .posixPermissions: NSNumber(value: Int16(0o600)),
            ], ofItemAtPath: tempURL.path)
            #endif

            if fm.fileExists(atPath: destination.path) {
                _ = try fm.replaceItemAt(destination, withItemAt: tempURL)
            } else {
                try fm.moveItem(at: tempURL, to: destination)
            }
            #if os(iOS)
            try? fm.setAttributes([
                .posixPermissions: NSNumber(value: Int16(0o600)),
                .protectionKey: FileProtectionType.completeUntilFirstUserAuthentication,
            ], ofItemAtPath: destination.path)
            #else
            try? fm.setAttributes([
                .posixPermissions: NSNumber(value: Int16(0o600)),
            ], ofItemAtPath: destination.path)
            #endif
        } catch {
            try? fm.removeItem(at: tempURL)
            throw MobileStoreError.ioError(error.localizedDescription)
        }
    }

    // MARK: - Snapshots & getters

    public var deviceIDHex: String {
        identity.deviceID.hexString
    }

    public var shortID: String {
        Words.shortID(deviceIDHex)
    }

    public var deviceName: String {
        lock.lock()
        defer { lock.unlock() }
        return persisted.deviceName
    }

    public var relayURL: String {
        lock.lock()
        defer { lock.unlock() }
        return persisted.relayURL
    }

    public var trustedPeers: [TrustedPeer] {
        lock.lock()
        defer { lock.unlock() }
        return persisted.devices
    }

    public var features: FeatureSwitches {
        lock.lock()
        defer { lock.unlock() }
        return persisted.features
    }

    public var pointerSpeed: Double {
        lock.lock()
        defer { lock.unlock() }
        return persisted.pointerSpeed
    }

    public func trustedPeer(_ deviceIDHex: String) -> TrustedPeer? {
        lock.lock()
        defer { lock.unlock() }
        let key = deviceIDHex.lowercased()
        return persisted.devices.first { $0.deviceIDHex == key }
    }

    public func peerCounterState(_ deviceIDHex: String) -> PeerCounterState {
        lock.lock()
        defer { lock.unlock() }
        return persisted.peers[deviceIDHex.lowercased()] ?? PeerCounterState()
    }

    // MARK: - Settings & group mutations

    public func setDeviceName(_ typed: String) throws -> String {
        let checked: String
        switch DeviceName.check(typed) {
        case .success(let valid):
            checked = valid
        case .failure(let problem):
            throw MobileStoreError.invalidDeviceName(problem)
        }
        lock.lock()
        defer { lock.unlock() }
        var next = persisted
        next.deviceName = checked
        try Self.writeAtomically(next, to: fileURL)
        persisted = next
        return checked
    }

    public func setRelayURL(_ typed: String, allowWhileInGroup: Bool = false) throws -> RelayEndpoint {
        guard let endpoint = RelayEndpoint.parse(typed), endpoint.isAllowed else {
            throw MobileStoreError.invalidRelayURL
        }
        lock.lock()
        defer { lock.unlock() }
        if !allowWhileInGroup && !persisted.devices.isEmpty && persisted.relayURL != endpoint.url {
            throw MobileStoreError.relayChangeWhileInGroup
        }
        var next = persisted
        next.relayURL = endpoint.url
        try Self.writeAtomically(next, to: fileURL)
        persisted = next
        return endpoint
    }

    public func setFeatures(_ updated: FeatureSwitches) throws {
        lock.lock()
        defer { lock.unlock() }
        var next = persisted
        next.features = updated
        try Self.writeAtomically(next, to: fileURL)
        persisted = next
    }

    public func setPointerSpeed(_ speed: Double) throws {
        let clamped = min(max(speed, Self.minPointerSpeed), Self.maxPointerSpeed)
        lock.lock()
        defer { lock.unlock() }
        var next = persisted
        next.pointerSpeed = clamped
        try Self.writeAtomically(next, to: fileURL)
        persisted = next
    }

    @discardableResult
    public func trustPeer(_ keys: PeerKeys, nowMs: Int64 = Int64(Date().timeIntervalSince1970 * 1000)) throws -> TrustedPeer {
        lock.lock()
        defer { lock.unlock() }
        var next = persisted
        let peer = Self.upsertPeer(keys, in: &next.devices, nowMs: nowMs)
        try Self.writeAtomically(next, to: fileURL)
        persisted = next
        return peer
    }

    public func untrustPeer(_ deviceIDHex: String) throws {
        lock.lock()
        defer { lock.unlock() }
        let key = deviceIDHex.lowercased()
        var next = persisted
        next.devices.removeAll { $0.deviceIDHex == key }
        try Self.writeAtomically(next, to: fileURL)
        persisted = next
    }

    public func clearGroup() throws {
        lock.lock()
        defer { lock.unlock() }
        var next = persisted
        next.devices.removeAll()
        // Counter states (`next.peers`) are intentionally preserved so counters
        // continue increasing if a device is paired again later.
        try Self.writeAtomically(next, to: fileURL)
        persisted = next
    }

    // MARK: - Envelope sealing & opening

    /// Increments `txCounter`, persists atomically BEFORE returning the frame, and seals `plaintext`.
    public func sealEnvelope(
        to recipientHex: String,
        kind: Kind,
        plaintext: Data
    ) throws -> Data {
        lock.lock()
        defer { lock.unlock() }

        let targetHex = recipientHex.lowercased()
        guard let peer = persisted.devices.first(where: { $0.deviceIDHex == targetHex }) else {
            throw MobileStoreError.peerNotTrusted(targetHex)
        }
        let keys: SessionKeys
        do {
            keys = try sessionKeys(local: identity, peerDeviceID: peer.deviceID, peerKxPublicKey: peer.kxPublicKey)
        } catch {
            if let failure = error as? WeftError { throw MobileStoreError.protocolError(failure) }
            throw MobileStoreError.ioError(error.localizedDescription)
        }

        var next = persisted
        var counterState = next.peers[targetHex] ?? PeerCounterState()
        counterState.txCounter += 1
        let counter = counterState.txCounter
        next.peers[targetHex] = counterState

        let header: Header
        let frame: Data
        do {
            header = try Header(
                kind: kind,
                sender: identity.deviceID,
                recipient: peer.deviceID,
                messageID: randomMessageID(),
                counter: counter
            )
            frame = try seal(header: header, plaintext: plaintext, tx: keys.tx)
        } catch {
            if let failure = error as? WeftError { throw MobileStoreError.protocolError(failure) }
            throw MobileStoreError.ioError(error.localizedDescription)
        }

        try Self.writeAtomically(next, to: fileURL)
        persisted = next
        return frame
    }

    /// Decrypts, authenticates, replay-checks, and applies a received envelope from a trusted peer.
    public func openEnvelope(
        frame: Data,
        nowMs: Int64 = Int64(Date().timeIntervalSince1970 * 1000)
    ) throws -> OpenedEnvelope {
        guard frame.count >= Weft.headerLength else {
            throw MobileStoreError.protocolError(.frameTooShort(frame.count))
        }
        let outerHeader: Header
        do {
            outerHeader = try Header(decoding: frame.prefix(Weft.headerLength))
        } catch {
            if let failure = error as? WeftError { throw MobileStoreError.protocolError(failure) }
            throw MobileStoreError.ioError(error.localizedDescription)
        }
        guard outerHeader.recipient == identity.deviceID else {
            throw MobileStoreError.wrongRecipient
        }

        lock.lock()
        defer { lock.unlock() }

        let senderHex = outerHeader.sender.hexString
        guard let senderPeer = persisted.devices.first(where: { $0.deviceIDHex == senderHex }) else {
            throw MobileStoreError.peerNotTrusted(senderHex)
        }

        let keys: SessionKeys
        let header: Header
        let plaintext: Data
        do {
            keys = try sessionKeys(local: identity, peerDeviceID: senderPeer.deviceID, peerKxPublicKey: senderPeer.kxPublicKey)
            (header, plaintext) = try open(frame: frame, rx: keys.rx)
        } catch {
            if let failure = error as? WeftError { throw MobileStoreError.protocolError(failure) }
            throw MobileStoreError.ioError(error.localizedDescription)
        }

        var next = persisted
        var counterState = next.peers[senderHex] ?? PeerCounterState()
        do {
            try counterState.replayWindow.accept(counter: header.counter, messageID: header.messageID)
        } catch {
            if let failure = error as? ReplayError { throw MobileStoreError.replayRejected(failure) }
            throw MobileStoreError.ioError(error.localizedDescription)
        }
        next.peers[senderHex] = counterState

        let ownSignPK = identity.signPublicKey
        let event: OpenedEnvelopeEvent

        do {
            switch header.kind {
            case .clipboardText:
                let text = String(decoding: plaintext, as: UTF8.self)
                event = .clipboardText(text)

            case .ping:
                event = .ping

            case .welcome:
                let members = try decodeWelcome(plaintext)
                var newlyTrusted: [String] = []
                for member in members {
                    if member.signPublicKey == ownSignPK { continue }
                    let memberHex = member.deviceID.hexString
                    let alreadyTrusted = next.devices.contains { $0.deviceIDHex == memberHex }
                    _ = Self.upsertPeer(member, in: &next.devices, nowMs: nowMs)
                    if !alreadyTrusted {
                        newlyTrusted.append(memberHex)
                    }
                }
                event = .welcome(newlyTrustedHex: [senderHex] + newlyTrusted)

            case .deviceAdded:
                let added = try decodeDeviceAdded(plaintext)
                let addedHex = added.deviceID.hexString
                if added.signPublicKey == ownSignPK {
                    event = .deviceAdded(peerHex: addedHex, newlyTrustedHex: [])
                } else {
                    _ = Self.upsertPeer(added, in: &next.devices, nowMs: nowMs)
                    event = .deviceAdded(peerHex: addedHex, newlyTrustedHex: [addedHex])
                }

            case .deviceRemoved:
                let removedID = try decodeDeviceRemoved(plaintext)
                let removedHex = removedID.hexString
                next.devices.removeAll { $0.deviceIDHex == removedHex }
                event = .deviceRemoved(peerHex: removedHex)

            case .deviceName:
                let name = try decodeDeviceName(plaintext)
                if let index = next.devices.firstIndex(where: { $0.deviceIDHex == senderHex }) {
                    next.devices[index].name = name
                }
                event = .deviceName(name)

            case .packet:
                event = .packet(plaintext)

            case .fileChunk:
                event = .fileChunk(plaintext)
            }
        } catch {
            if let failure = error as? WeftError { throw MobileStoreError.protocolError(failure) }
            throw MobileStoreError.ioError(error.localizedDescription)
        }

        try Self.writeAtomically(next, to: fileURL)
        persisted = next
        return OpenedEnvelope(senderHex: senderHex, header: header, event: event)
    }

    /// Used during `0xB1` join before the slot owner is trusted: decrypts and validates a
    /// `Welcome` (0x03) from `ownerKeys`, then atomically replaces the trusted group with
    /// `ownerKeys` + `welcome.members` on `newRelayURL`.
    public func acceptJoinWelcome(
        frame: Data,
        ownerKeys: PeerKeys,
        newRelayURL: String,
        nowMs: Int64 = Int64(Date().timeIntervalSince1970 * 1000)
    ) throws -> (header: Header, peers: [TrustedPeer], replacedPreviousGroup: Bool) {
        guard let endpoint = RelayEndpoint.parse(newRelayURL), endpoint.isAllowed else {
            throw MobileStoreError.invalidRelayURL
        }

        lock.lock()
        defer { lock.unlock() }

        let ownerHex = ownerKeys.deviceID.hexString
        let keys: SessionKeys
        let header: Header
        let plaintext: Data
        let members: [PeerKeys]
        do {
            keys = try sessionKeys(
                local: identity,
                peerDeviceID: ownerKeys.deviceID,
                peerKxPublicKey: ownerKeys.kxPublicKey
            )
            (header, plaintext) = try open(frame: frame, rx: keys.rx)
            guard header.recipient == identity.deviceID, header.sender == ownerKeys.deviceID else {
                throw MobileStoreError.wrongRecipient
            }
            guard header.kind == .welcome else {
                throw MobileStoreError.protocolError(.unexpectedFrameType(header.kind.rawValue))
            }
            members = try decodeWelcome(plaintext)
        } catch let error as MobileStoreError {
            throw error
        } catch let error as WeftError {
            throw MobileStoreError.protocolError(error)
        }

        var counterState = persisted.peers[ownerHex] ?? PeerCounterState()
        do {
            try counterState.replayWindow.accept(counter: header.counter, messageID: header.messageID)
        } catch {
            if let failure = error as? ReplayError { throw MobileStoreError.replayRejected(failure) }
            throw MobileStoreError.ioError(error.localizedDescription)
        }

        let previousByHex = Dictionary(uniqueKeysWithValues: persisted.devices.map { ($0.deviceIDHex, $0) })
        var newDevices: [TrustedPeer] = []
        func appendKeepingMetadata(_ peerKeys: PeerKeys) {
            let hex = peerKeys.deviceID.hexString
            if let prior = previousByHex[hex] {
                newDevices.append(TrustedPeer(keys: peerKeys, name: prior.name, addedAtMs: prior.addedAtMs ?? nowMs))
            } else {
                newDevices.append(TrustedPeer(keys: peerKeys, name: nil, addedAtMs: nowMs))
            }
        }

        appendKeepingMetadata(ownerKeys)
        let ownSignPK = identity.signPublicKey
        for member in members where member.signPublicKey != ownSignPK {
            if !newDevices.contains(where: { $0.deviceIDHex == member.deviceID.hexString }) {
                appendKeepingMetadata(member)
            }
        }

        let newHexSet = Set(newDevices.map(\.deviceIDHex))
        let replaced = persisted.devices.contains { !newHexSet.contains($0.deviceIDHex) }

        var next = persisted
        next.relayURL = endpoint.url
        next.devices = newDevices
        next.peers[ownerHex] = counterState

        try Self.writeAtomically(next, to: fileURL)
        persisted = next
        return (header: header, peers: newDevices, replacedPreviousGroup: replaced)
    }

    private static func upsertPeer(
        _ keys: PeerKeys,
        in devices: inout [TrustedPeer],
        nowMs: Int64
    ) -> TrustedPeer {
        let hex = keys.deviceID.hexString
        if let index = devices.firstIndex(where: { $0.deviceIDHex == hex }) {
            devices[index].kxPublicKeyHex = keys.kxPublicKey.hexString
            return devices[index]
        }
        let created = TrustedPeer(keys: keys, name: nil, addedAtMs: nowMs)
        devices.append(created)
        return created
    }
}
