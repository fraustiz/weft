// Async @MainActor session coordinator driving WebSocket relay authentication,
// exponential-backoff reconnect, QR pairing & group invitations, group
// membership envelopes, clipboard sync, packets, and chunked file transfers.

import Combine
import Foundation
import WeftProtocol

// MARK: - Transport abstraction (URLSessionWebSocketTask in app, FakeRelay in tests)

public protocol RelaySocket: Sendable {
    func send(_ data: Data) async throws
    func receive() async throws -> Data
    func close() async
}

public protocol RelaySocketFactory: Sendable {
    func connect(url: URL) async throws -> any RelaySocket
}

public final class URLSessionRelaySocket: RelaySocket {
    private let task: URLSessionWebSocketTask

    public init(task: URLSessionWebSocketTask) {
        self.task = task
        task.maximumMessageSize = 512 * 1024
        task.resume()
    }

    public func send(_ data: Data) async throws {
        try await task.send(.data(data))
    }

    public func receive() async throws -> Data {
        while true {
            let message = try await task.receive()
            switch message {
            case .data(let bytes):
                return bytes
            case .string:
                // Weft wire protocol uses binary WebSocket frames only; ignore text frames.
                continue
            @unknown default:
                continue
            }
        }
    }

    public func close() async {
        task.cancel(with: .normalClosure, reason: nil)
    }
}

public struct URLSessionRelaySocketFactory: RelaySocketFactory {
    private let session: URLSession

    public init() {
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 15
        config.waitsForConnectivity = false
        self.session = URLSession(configuration: config)
    }

    public func connect(url: URL) async throws -> any RelaySocket {
        let task = session.webSocketTask(with: url)
        return URLSessionRelaySocket(task: task)
    }
}

// MARK: - Connection & Pairing Snapshot Models

public enum ConnectionProblem: Equatable, Sendable {
    case unreachable
    case timedOut
    case wrongRelayName
    case rejected
    case insecure
    case protocolError
    case lost

    public var text: String {
        switch self {
        case .unreachable:
            t("Relay unreachable", "Relais injoignable")
        case .timedOut:
            t("Connection timed out", "Délai de connexion dépassé")
        case .wrongRelayName:
            t("Relay name does not match address", "Le nom du relais ne correspond pas à l’adresse")
        case .rejected:
            t("Authentication refused by relay", "Authentification refusée par le relais")
        case .insecure:
            t("TLS (wss://) is required for remote relays", "TLS (wss://) est requis pour un relais distant")
        case .protocolError:
            t("Unexpected relay frame", "Trame inattendue du relais")
        case .lost:
            t("Connection lost, reconnecting", "Connexion perdue, reconnexion")
        }
    }
}

public enum ConnectionState: Equatable, Sendable {
    case disconnected
    case connecting
    case connected
    case reconnecting(problem: ConnectionProblem, attempt: Int)

    public var isConnected: Bool {
        self == .connected
    }

    public var statusTitle: String {
        switch self {
        case .disconnected:
            t("Offline", "Hors ligne")
        case .connecting:
            t("Connecting…", "Connexion…")
        case .connected:
            t("Connected", "Connecté")
        case .reconnecting(let problem, _):
            problem.text
        }
    }
}

public enum PairingFailure: Equatable, Sendable {
    case invalidCode
    case ownCode
    case insecureRelay
    case groupFull
    case relayError(ControlError)
    case timedOut
    case noAnswerToInvite
    case noGroupForInvite
    case otherRelayForInvite
    case connectionFailed(ConnectionProblem)
    case storageFailed

    public var text: String {
        switch self {
        case .invalidCode:
            t("That code is not a valid Weft pairing code.",
              "Ce code n’est pas un code d’association Weft valide.")
        case .ownCode:
            t("That code belongs to this iPhone.",
              "Ce code appartient à cet iPhone.")
        case .insecureRelay:
            t("The relay in this code does not use TLS (wss://).",
              "Le relais de ce code n’utilise pas TLS (wss://).")
        case .groupFull, .relayError(.groupFull):
            t("The group already has 10 devices, the maximum.",
              "Le groupe compte déjà 10 appareils, le maximum.")
        case .relayError(.unknownPairing):
            t("That pairing code has expired or was already used.",
              "Ce code d’association a expiré ou a déjà été utilisé.")
        case .relayError(.notAllowed):
            t("The relay refused the pairing confirmation.",
              "Le relais a refusé la confirmation d’association.")
        case .relayError(.rateLimited):
            t("Too many pairing attempts. Wait a moment and try again.",
              "Trop de tentatives d’association. Patientez un instant avant de réessayer.")
        case .timedOut:
            t("The other device did not finish pairing in time.",
              "L’autre appareil n’a pas terminé l’association à temps.")
        case .noAnswerToInvite:
            t("The other device did not answer. If it is already in a group, scan from that group instead.",
              "L’autre appareil n’a pas répondu. S’il appartient déjà à un groupe, associez depuis ce groupe.")
        case .noGroupForInvite:
            t("Pair with a first device before inviting another into a group.",
              "Associez un premier appareil avant d’en inviter un autre dans le groupe.")
        case .otherRelayForInvite:
            t("That device uses another relay. Join its group instead, or change its relay first.",
              "Cet appareil utilise un autre relais. Rejoignez son groupe ou changez d’abord son relais.")
        case .connectionFailed(let problem):
            problem.text
        case .storageFailed:
            t("Could not save device trust. Try again.", "Impossible d’enregistrer l’association. Réessayez.")
        }
    }
}

public enum PairingState: Equatable, Sendable {
    case idle
    case openingSlot
    case showingCode(code: String, expiresAtMs: Int64)
    case joining
    case inviting
    case paired(deviceIDHex: String, invited: Bool)
    case failed(PairingFailure)
}

public struct PeerSnapshot: Equatable, Sendable, Identifiable {
    public let id: String
    public let shortID: String
    public let displayName: String
    public let addedAtMs: Int64?
    public let capabilities: CapabilitiesPayload?
    public let battery: BatteryPayload?
    public let connectivity: ConnectivityPayload?
    public let players: [MediaPlayerPayload]
    public let mediaReceivedAtMs: Int64
    public let covers: [String: Data]
    public let volume: VolumePayload?
    public let commands: [CommandItemPayload]

    public var platform: String? { capabilities?.platform }
    public var platformName: String? { Words.platform(platform) }
    public var symbol: String { Words.symbol(platform) }

    public var currentPlayer: MediaPlayerPayload? {
        players.first(where: \.playing) ?? players.first
    }

    public func handles(_ packetType: String) -> Bool {
        capabilities?.handles(packetType) == true
    }

    public func sends(_ packetType: String) -> Bool {
        capabilities?.sends(packetType) == true
    }

    public var batteryText: String? {
        guard let level = battery?.level else { return nil }
        return Words.battery(level: level)
    }

    public var networkText: String? {
        Words.network(connectivity?.network)
    }
}

public struct BannerNotice: Equatable, Sendable, Identifiable {
    public let id: UUID
    public let text: String
    public let createdAtMs: Int64

    public init(id: UUID = UUID(), text: String, createdAtMs: Int64 = Int64(Date().timeIntervalSince1970 * 1000)) {
        self.id = id
        self.text = text
        self.createdAtMs = createdAtMs
    }
}

public struct MobileSnapshot: Equatable, Sendable {
    public let deviceIDHex: String
    public let shortID: String
    public let deviceName: String
    public let relayURL: String
    public let connection: ConnectionState
    public let peers: [PeerSnapshot]
    public let transfers: [TransferProgress]
    public let pairing: PairingState
    public let features: FeatureSwitches
    public let pointerSpeed: Double
    public let ringingFromPeerHex: String?
    public let ringingFromPeerName: String?
    public let outgoingRingingPeerHex: String?
    public let lastBanner: BannerNotice?

    public func peer(_ idHex: String?) -> PeerSnapshot? {
        guard let idHex else { return nil }
        let clean = idHex.lowercased()
        return peers.first { $0.id == clean }
    }
}

// MARK: - Coalescing Remote Input Queue (60 Hz)

public final class RemoteInputQueue: @unchecked Sendable {
    private let lock = NSLock()
    private var queued: [Packet] = []

    public init() {}

    public func move(dx: Double, dy: Double) {
        guard dx != 0 || dy != 0 else { return }
        lock.lock()
        defer { lock.unlock() }
        if let last = queued.last,
           case .inputPointer(let prev) = last,
           prev.dx != nil || prev.dy != nil,
           prev.scrollX == nil && prev.scrollY == nil {
            queued[queued.count - 1] = .inputPointer(InputPointerPayload(
                dx: (prev.dx ?? 0) + dx,
                dy: (prev.dy ?? 0) + dy
            ))
        } else {
            queued.append(.inputPointer(InputPointerPayload(dx: dx, dy: dy)))
        }
    }

    public func scroll(scrollX: Double, scrollY: Double) {
        guard scrollX != 0 || scrollY != 0 else { return }
        lock.lock()
        defer { lock.unlock() }
        if let last = queued.last,
           case .inputPointer(let prev) = last,
           prev.scrollX != nil || prev.scrollY != nil,
           prev.dx == nil && prev.dy == nil {
            queued[queued.count - 1] = .inputPointer(InputPointerPayload(
                scrollX: (prev.scrollX ?? 0) + scrollX,
                scrollY: (prev.scrollY ?? 0) + scrollY
            ))
        } else {
            queued.append(.inputPointer(InputPointerPayload(scrollX: scrollX, scrollY: scrollY)))
        }
    }

    public func point(_ pointer: PresenterPointer) {
        let next = Packet.presenter(PresenterPayload(action: nil, pointer: pointer))
        lock.lock()
        defer { lock.unlock() }
        if let last = queued.last,
           case .presenter(let prev) = last,
           prev.action == nil {
            queued[queued.count - 1] = next
        } else {
            queued.append(next)
        }
    }

    public func add(_ packet: Packet) {
        lock.lock()
        defer { lock.unlock() }
        queued.append(packet)
    }

    public func drain() -> [Packet] {
        lock.lock()
        defer { lock.unlock() }
        let items = queued
        queued.removeAll(keepingCapacity: true)
        return items
    }
}

// MARK: - MobileSession

@MainActor
public final class MobileSession: ObservableObject {
    public static let slotLifetimeMs: Int64 = 5 * 60 * 1000
    public static let joinTimeoutNanoseconds: UInt64 = 30_000_000_000
    public static let handshakeTimeoutNanoseconds: UInt64 = 10_000_000_000

    public let store: MobileStore
    public let transferEngine: TransferEngine
    private let socketFactory: any RelaySocketFactory

    @Published public private(set) var snapshot: MobileSnapshot

    // Callbacks into the platform UI layer
    public var onClipboardTextReceived: ((_ senderName: String, _ text: String) -> Void)?
    public var onClipboardImageReceived: ((_ senderName: String, _ data: Data, _ mime: String) -> Void)?
    public var onShareURLReceived: ((_ senderName: String, _ url: URL) -> Void)?
    public var onRingStarted: ((_ senderName: String) -> Void)?
    public var onRingStopped: (() -> Void)?
    public var onPingReceived: ((_ senderName: String, _ message: String?) -> Void)?
    public var onFileReceived: ((_ senderName: String, _ fileName: String, _ fileURL: URL) -> Void)?

    // Runtime per-peer state (ephemeral, rebuilt per session)
    private struct PeerRuntime {
        var capabilities: CapabilitiesPayload?
        var battery: BatteryPayload?
        var connectivity: ConnectivityPayload?
        var players: [MediaPlayerPayload] = []
        var mediaReceivedAtMs: Int64 = 0
        var coverOrder: [String] = []
        var covers: [String: Data] = [:]
        var volume: VolumePayload?
        var commands: [CommandItemPayload] = []

        mutating func insertCover(id: String, data: Data) {
            coverOrder.removeAll { $0 == id }
            coverOrder.append(id)
            covers[id] = data
            while coverOrder.count > PacketConstants.maxCoversKept {
                let oldest = coverOrder.removeFirst()
                covers.removeValue(forKey: oldest)
            }
        }
    }

    private struct ActiveSlot {
        let code: PairingCode
        let encoded: String
        let expiresAtMs: Int64
    }

    private struct PendingJoinContext {
        let ownerKeys: PeerKeys
        let ownerHex: String
        let relayURL: String
        let buffer: JoinFrameBuffer
        var timeoutTask: Task<Void, Never>?
    }

    private struct PendingAcceptedInviteContext {
        let inviterKeys: PeerKeys
        let inviterHex: String
        let buffer: JoinFrameBuffer
        var timeoutTask: Task<Void, Never>?
    }

    private struct PendingSentInviteContext {
        let inviteeKeys: PeerKeys
        let inviteeHex: String
        let wasAlreadyTrusted: Bool
        var timeoutTask: Task<Void, Never>?
    }

    private var peerRuntime: [String: PeerRuntime] = [:]
    private var localBattery = BatteryPayload()
    private var localConnectivity = ConnectivityPayload()

    private var desiredConnection = false
    private var overrideRelayURL: String?
    private var connectionLoopTask: Task<Void, Never>?
    private var maintenanceTask: Task<Void, Never>?
    private var activeSocket: (any RelaySocket)?
    private var connectionState: ConnectionState = .disconnected
    private var connectionWaiters: [UUID: CheckedContinuation<Bool, Never>] = [:]

    private var pairingState: PairingState = .idle
    private var activeSlot: ActiveSlot?
    private var pendingJoin: PendingJoinContext?
    private var pendingAcceptedInvite: PendingAcceptedInviteContext?
    private var pendingSentInvite: PendingSentInviteContext?

    private var ringingFromPeerHex: String?
    private var outgoingRingingPeerHex: String?
    private var lastBanner: BannerNotice?

    public init(
        store: MobileStore,
        transferEngine: TransferEngine,
        socketFactory: any RelaySocketFactory = URLSessionRelaySocketFactory()
    ) {
        self.store = store
        self.transferEngine = transferEngine
        self.socketFactory = socketFactory
        self.snapshot = Self.buildSnapshot(
            store: store,
            transferEngine: transferEngine,
            peerRuntime: [:],
            connectionState: .disconnected,
            pairingState: .idle,
            ringingFromPeerHex: nil,
            outgoingRingingPeerHex: nil,
            lastBanner: nil
        )
    }

    // MARK: - Lifecycle

    public func start() {
        desiredConnection = true
        ensureConnectionLoop(forceRestart: false)
        if maintenanceTask == nil {
            maintenanceTask = Task { [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(nanoseconds: 5_000_000_000)
                    guard let self else { return }
                    await self.runPeriodicMaintenance()
                }
            }
        }
    }

    public func stop() {
        desiredConnection = false
        maintenanceTask?.cancel()
        maintenanceTask = nil
        connectionLoopTask?.cancel()
        connectionLoopTask = nil
        let sock = activeSocket
        activeSocket = nil
        if let sock {
            Task { await sock.close() }
        }
        connectionState = .disconnected
        resolveConnectionWaiters(false)
        publishSnapshot()
    }

    private func targetRelayURL() -> String? {
        if let overrideRelayURL { return overrideRelayURL }
        if activeSlot != nil || pendingJoin != nil || pendingAcceptedInvite != nil || pendingSentInvite != nil {
            return store.relayURL
        }
        if desiredConnection {
            return store.relayURL
        }
        return nil
    }

    private func ensureConnectionLoop(forceRestart: Bool) {
        if forceRestart {
            connectionLoopTask?.cancel()
            connectionLoopTask = nil
            let old = activeSocket
            activeSocket = nil
            if let old {
                Task { await old.close() }
            }
        }
        guard let urlString = targetRelayURL() else {
            connectionLoopTask?.cancel()
            connectionLoopTask = nil
            connectionState = .disconnected
            publishSnapshot()
            return
        }
        if connectionLoopTask == nil {
            connectionLoopTask = Task { [weak self] in
                await self?.runConnectionSupervisor(initialURL: urlString)
            }
        }
    }

    private func runConnectionSupervisor(initialURL: String) async {
        var attempt = 0
        var backoffSeconds: Double = 1.0

        while !Task.isCancelled {
            guard let currentURLString = targetRelayURL() else {
                connectionState = .disconnected
                publishSnapshot()
                return
            }
            if attempt == 0 {
                connectionState = .connecting
                publishSnapshot()
            }

            let outcome = await runSingleSession(urlString: currentURLString)
            let failureProblem = outcome.problem
            if outcome.didAuthenticate {
                attempt = 0
                backoffSeconds = 1.0
            }

            if Task.isCancelled || targetRelayURL() == nil {
                break
            }
            if targetRelayURL() != currentURLString {
                // Relay URL switched intentionally (e.g. joining a code on a different relay); reconnect immediately.
                attempt = 0
                backoffSeconds = 1.0
                continue
            }

            if outcome.didAuthenticate {
                attempt = 1
                backoffSeconds = 1.0
            } else {
                attempt += 1
            }

            connectionState = .reconnecting(problem: failureProblem, attempt: attempt)
            resolveConnectionWaiters(false)
            publishSnapshot()

            let sleepNs = UInt64(backoffSeconds * 1_000_000_000)
            try? await Task.sleep(nanoseconds: sleepNs)
            backoffSeconds = min(backoffSeconds * 2.0, 30.0)
        }
    }

    private enum HandshakeError: Error {
        case problem(ConnectionProblem)
    }

    private func runSingleSession(
        urlString: String
    ) async -> (problem: ConnectionProblem, didAuthenticate: Bool) {
        guard let endpoint = RelayEndpoint.parse(urlString) else {
            return (.protocolError, false)
        }
        guard endpoint.isAllowed else {
            return (.insecure, false)
        }
        guard let url = URL(string: endpoint.url) else {
            return (.protocolError, false)
        }

        let socket: any RelaySocket
        do {
            socket = try await socketFactory.connect(url: url)
        } catch {
            return (.unreachable, false)
        }

        var wasAuthenticated = false
        defer {
            if activeSocket as AnyObject? === socket as AnyObject? {
                activeSocket = nil
            }
            Task { await socket.close() }
        }

        do {
            let identity = store.identity
            try await withThrowingTimeout(nanoseconds: Self.handshakeTimeoutNanoseconds) {
                let challengeFrame = try await socket.receive()
                let challenge = try Challenge(decoding: challengeFrame)
                try checkRelayName(challenge: challenge, expectedHost: endpoint.host)
                let authResponse = signAuth(identity: identity, challenge: challenge)
                try await socket.send(authResponse.encode())
                let accepted = try await socket.receive()
                guard accepted.count == 1, accepted[0] == Weft.authAccepted else {
                    throw HandshakeError.problem(.rejected)
                }
            }

            guard !Task.isCancelled, targetRelayURL() == urlString else {
                return (.lost, false)
            }

            activeSocket = socket
            wasAuthenticated = true
            connectionState = .connected
            resolveConnectionWaiters(true)
            publishSnapshot()
            onRelayAuthenticated()

            while !Task.isCancelled {
                let frame = try await socket.receive()
                await handleIncomingFrame(frame)
            }
            return (.lost, wasAuthenticated)
        } catch let error as WeftError {
            if error == .relayNameMismatch {
                return (.wrongRelayName, wasAuthenticated)
            }
            return (wasAuthenticated ? .lost : .protocolError, wasAuthenticated)
        } catch HandshakeError.problem(let p) {
            return (p, wasAuthenticated)
        } catch is TimeoutMarkError {
            return (.timedOut, wasAuthenticated)
        } catch {
            return (wasAuthenticated ? .lost : .unreachable, wasAuthenticated)
        }
    }

    private struct TimeoutMarkError: Error {}

    private func withThrowingTimeout<T: Sendable>(
        nanoseconds: UInt64,
        operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        try await withThrowingTaskGroup(of: T.self) { group in
            group.addTask {
                try await operation()
            }
            group.addTask {
                try await Task.sleep(nanoseconds: nanoseconds)
                throw TimeoutMarkError()
            }
            let first = try await group.next()!
            group.cancelAll()
            return first
        }
    }

    private func awaitConnected(timeoutNanoseconds: UInt64 = 10_000_000_000) async -> Bool {
        if connectionState.isConnected && activeSocket != nil {
            return true
        }
        ensureConnectionLoop(forceRestart: false)
        let waiterID = UUID()
        return await withTaskGroup(of: Bool.self) { group in
            group.addTask { @MainActor [weak self] in
                guard let self else { return false }
                if self.connectionState.isConnected && self.activeSocket != nil {
                    return true
                }
                return await withCheckedContinuation { cont in
                    self.connectionWaiters[waiterID] = cont
                }
            }
            group.addTask {
                try? await Task.sleep(nanoseconds: timeoutNanoseconds)
                return false
            }
            let first = await group.next() ?? false
            group.cancelAll()
            if let leftover = connectionWaiters.removeValue(forKey: waiterID) {
                leftover.resume(returning: first)
            }
            return first
        }
    }

    private func resolveConnectionWaiters(_ connected: Bool) {
        let waiters = connectionWaiters
        connectionWaiters.removeAll()
        for (_, cont) in waiters {
            cont.resume(returning: connected)
        }
    }

    private func onRelayAuthenticated() {
        let nowMs = Self.nowMillis()
        if let slot = activeSlot, slot.expiresAtMs > nowMs {
            Task {
                _ = await sendControl(.pairOpen(pairingID: slot.code.pairingID))
            }
        }
        let peerIDs = store.trustedPeers.map(\.deviceIDHex)
        guard !peerIDs.isEmpty else { return }
        Task {
            await announceSelfAndState(to: peerIDs)
            await sendPacket(.stateRequest, to: peerIDs)
        }
    }

    // MARK: - Outbound helpers

    @discardableResult
    private func sendRawFrame(_ frame: Data) async -> Bool {
        guard let socket = activeSocket, connectionState.isConnected else {
            return false
        }
        do {
            try await socket.send(frame)
            return true
        } catch {
            return false
        }
    }

    @discardableResult
    private func sendControl(_ control: ControlFrame) async -> Bool {
        guard let encoded = try? control.encode() else { return false }
        return await sendRawFrame(encoded)
    }

    @discardableResult
    private func sendEnvelope(kind: Kind, plaintext: Data, to recipientHexes: [String]) async -> Int {
        guard connectionState.isConnected, activeSocket != nil else { return 0 }
        var sentCount = 0
        for hex in recipientHexes {
            guard let frame = try? store.sealEnvelope(to: hex, kind: kind, plaintext: plaintext) else {
                continue
            }
            if await sendRawFrame(frame) {
                sentCount += 1
            }
        }
        return sentCount
    }

    @discardableResult
    private func sendPacket(_ packet: Packet, to recipientHexes: [String]) async -> Int {
        guard !recipientHexes.isEmpty,
              let json = try? packet.encodeJSONData() else {
            return 0
        }
        return await sendEnvelope(kind: .packet, plaintext: json, to: recipientHexes)
    }

    private func announceSelfAndState(to recipientHexes: [String]) async {
        guard !recipientHexes.isEmpty else { return }
        if let namePlaintext = try? encodeDeviceName(store.deviceName) {
            _ = await sendEnvelope(kind: .deviceName, plaintext: namePlaintext, to: recipientHexes)
        }
        let features = store.features
        _ = await sendPacket(.capabilities(features.capabilitiesPayload()), to: recipientHexes)
        if features.battery && (localBattery.level != nil || localBattery.charging != nil) {
            _ = await sendPacket(.battery(localBattery), to: recipientHexes)
        }
        if features.connectivity && (localConnectivity.network != nil || localConnectivity.signal != nil) {
            _ = await sendPacket(.connectivity(localConnectivity), to: recipientHexes)
        }
    }

    // MARK: - Incoming Frame Dispatch

    /// Internal/public entry point for incoming binary WebSocket frames.
    public func handleIncomingFrame(_ frame: Data) async {
        switch classify(frame) {
        case .control:
            await handleControlFrame(frame)
        case .envelope:
            await handleEnvelopeFrame(frame)
        case .unknown:
            break
        }
    }

    private func handleControlFrame(_ frame: Data) async {
        guard let control = try? ControlFrame(decoding: frame) else { return }
        switch control {
        case .pairRequest(let request):
            await handleIncomingPairRequest(request)

        case .pairInvite(let request):
            await handleIncomingPairInvite(request)

        case .error(let code):
            handleControlError(code)

        case .pairOpen, .groupAdd, .groupRemove, .groupJoin:
            break
        }
    }

    private func handleControlError(_ code: ControlError) {
        if let pj = pendingJoin {
            pj.timeoutTask?.cancel()
            _ = pj.buffer.settleAndDrain()
            pendingJoin = nil
            overrideRelayURL = nil
            pairingState = .failed(.relayError(code))
            ensureConnectionLoop(forceRestart: true)
            publishSnapshot()
            return
        }
        if let pai = pendingAcceptedInvite {
            pai.timeoutTask?.cancel()
            _ = pai.buffer.settleAndDrain()
            pendingAcceptedInvite = nil
            guard rollbackTrust(pai.inviterHex) else { return }
            pairingState = .failed(.relayError(code))
            publishSnapshot()
            return
        }
        if let psi = pendingSentInvite {
            psi.timeoutTask?.cancel()
            pendingSentInvite = nil
            if !psi.wasAlreadyTrusted {
                guard rollbackTrust(psi.inviteeHex) else { return }
            }
            pairingState = .failed(.relayError(code))
            publishSnapshot()
            return
        }
        if activeSlot != nil {
            activeSlot = nil
            pairingState = .failed(.relayError(code))
            publishSnapshot()
        }
    }

    // MARK: - Slot Owner QR Pairing (`0xB0`, `0xB1` -> `0xB2`, `0xB5` -> `0xB4`)

    public func openPairingSlot() async {
        cancelPendingPairingOperations(resetState: false)
        if store.trustedPeers.count + 1 >= Weft.maxGroupSize {
            pairingState = .failed(.groupFull)
            publishSnapshot()
            return
        }
        pairingState = .openingSlot
        publishSnapshot()

        guard let code = try? PairingCode(
            relayURL: store.relayURL,
            pairingID: randomPairingID(),
            signPublicKey: store.identity.signPublicKey,
            kxPublicKey: store.identity.kxPublicKey,
            secret: randomPairingSecret()
        ), let encoded = try? code.encode() else {
            pairingState = .failed(.invalidCode)
            publishSnapshot()
            return
        }

        let expiresAtMs = Self.nowMillis() + Self.slotLifetimeMs
        activeSlot = ActiveSlot(code: code, encoded: encoded, expiresAtMs: expiresAtMs)

        let connected = await awaitConnected()
        guard activeSlot?.code.pairingID == code.pairingID else { return }
        let opened = connected ? await sendControl(.pairOpen(pairingID: code.pairingID)) : false
        if !opened {
            activeSlot = nil
            pairingState = .failed(.connectionFailed(.timedOut))
            publishSnapshot()
            return
        }

        pairingState = .showingCode(code: encoded, expiresAtMs: expiresAtMs)
        publishSnapshot()
    }

    public func closePairing() {
        cancelPendingPairingOperations(resetState: true)
        publishSnapshot()
    }

    private func cancelPendingPairingOperations(resetState: Bool) {
        activeSlot = nil
        if let pj = pendingJoin {
            pj.timeoutTask?.cancel()
            _ = pj.buffer.settleAndDrain()
            pendingJoin = nil
            if overrideRelayURL != nil {
                overrideRelayURL = nil
                ensureConnectionLoop(forceRestart: true)
            }
        }
        if let pai = pendingAcceptedInvite {
            pai.timeoutTask?.cancel()
            _ = pai.buffer.settleAndDrain()
            pendingAcceptedInvite = nil
        }
        if let psi = pendingSentInvite {
            psi.timeoutTask?.cancel()
            if !psi.wasAlreadyTrusted {
                guard rollbackTrust(psi.inviteeHex) else { return }
            }
            pendingSentInvite = nil
        }
        if resetState {
            pairingState = .idle
        }
    }

    private func handleIncomingPairRequest(_ request: PairRequest) async {
        let nowMs = Self.nowMillis()
        guard let slot = activeSlot, slot.expiresAtMs > nowMs else { return }
        guard request.pairingID == slot.code.pairingID else { return }
        guard (try? verifyPairRequest(secret: slot.code.secret, request: request)) == true else {
            // Relay closes the slot when forwarding; reopen so the QR code remains usable.
            _ = await sendControl(.pairOpen(pairingID: slot.code.pairingID))
            return
        }
        guard request.signPublicKey != store.identity.signPublicKey,
              let joinerKeys = try? PeerKeys(
                  signPublicKey: request.signPublicKey,
                  kxPublicKey: request.kxPublicKey
              ) else {
            _ = await sendControl(.pairOpen(pairingID: slot.code.pairingID))
            return
        }

        let joinerHex = joinerKeys.deviceID.hexString
        let existingOthers = store.trustedPeers.filter { $0.deviceIDHex != joinerHex }
        if existingOthers.count + 2 > Weft.maxGroupSize {
            activeSlot = nil
            pairingState = .failed(.groupFull)
            publishSnapshot()
            return
        }

        activeSlot = nil
        do { _ = try store.trustPeer(joinerKeys, nowMs: nowMs) }
        catch { pairingState = .failed(.storageFailed); publishSnapshot(); return }
        _ = await sendControl(.groupAdd(deviceID: joinerKeys.deviceID))

        // Notify existing members first so they trust the joiner before its DeviceName arrives.
        let otherHexes = existingOthers.map(\.deviceIDHex)
        if !otherHexes.isEmpty {
            _ = await sendEnvelope(
                kind: .deviceAdded,
                plaintext: encodeDeviceAdded(joinerKeys),
                to: otherHexes
            )
        }
        if let welcomePlaintext = try? encodeWelcome(existingOthers.map(\.peerKeys)) {
            _ = await sendEnvelope(kind: .welcome, plaintext: welcomePlaintext, to: [joinerHex])
        }
        await announceSelfAndState(to: [joinerHex])

        pairingState = .paired(deviceIDHex: joinerHex, invited: false)
        publishSnapshot()
    }

    private func handleIncomingPairInvite(_ request: PairRequest) async {
        let nowMs = Self.nowMillis()
        guard let slot = activeSlot, slot.expiresAtMs > nowMs else { return }
        guard request.pairingID == slot.code.pairingID else { return }
        guard (try? verifyInviteRequest(secret: slot.code.secret, request: request)) == true else {
            _ = await sendControl(.pairOpen(pairingID: slot.code.pairingID))
            return
        }
        guard request.signPublicKey != store.identity.signPublicKey else {
            _ = await sendControl(.pairOpen(pairingID: slot.code.pairingID))
            return
        }
        // Spec §5 ("Invitation"): a device already in a group MUST ignore `0xB5` invitations
        // and reopen its slot so scanning its QR code can never pull it out of its group.
        guard store.trustedPeers.isEmpty else {
            _ = await sendControl(.pairOpen(pairingID: slot.code.pairingID))
            return
        }
        guard let inviterKeys = try? PeerKeys(
            signPublicKey: request.signPublicKey,
            kxPublicKey: request.kxPublicKey
        ) else {
            return
        }

        activeSlot = nil
        let inviterHex = inviterKeys.deviceID.hexString
        let buffer = JoinFrameBuffer(ownerHex: inviterHex)
        do { _ = try store.trustPeer(inviterKeys, nowMs: nowMs) }
        catch { pairingState = .failed(.storageFailed); publishSnapshot(); return }

        var context = PendingAcceptedInviteContext(
            inviterKeys: inviterKeys,
            inviterHex: inviterHex,
            buffer: buffer,
            timeoutTask: nil
        )
        context.timeoutTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: Self.joinTimeoutNanoseconds)
            guard !Task.isCancelled, let self else { return }
            await self.failAcceptedInviteTimeout(inviterHex: inviterHex)
        }
        pendingAcceptedInvite = context
        pairingState = .joining
        publishSnapshot()

        if !(await sendControl(.groupJoin(deviceID: inviterKeys.deviceID))) {
            await failAcceptedInviteTimeout(inviterHex: inviterHex)
            return
        }
        // Sending our DeviceName signals to the inviter that we moved into its group.
        await announceSelfAndState(to: [inviterHex])
    }

    private func failAcceptedInviteTimeout(inviterHex: String) async {
        guard let pai = pendingAcceptedInvite, pai.inviterHex == inviterHex else { return }
        pai.timeoutTask?.cancel()
        _ = pai.buffer.settleAndDrain()
        pendingAcceptedInvite = nil
        guard rollbackTrust(inviterHex) else { return }
        _ = await sendControl(.groupRemove(deviceID: store.identity.deviceID))
        pairingState = .failed(.timedOut)
        publishSnapshot()
    }

    // MARK: - Scanner / Code Entry (`joinWithCode` `0xB1` & `inviteWithCode` `0xB5`)

    /// Joins the group of the device showing `rawCode` (`0xB1` `pairRequest`).
    public func joinWithCode(_ rawCode: String) async {
        cancelPendingPairingOperations(resetState: false)
        let trimmed = rawCode.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let pairing = try? PairingCode(decoding: trimmed) else {
            pairingState = .failed(.invalidCode)
            publishSnapshot()
            return
        }
        guard let endpoint = RelayEndpoint.parse(pairing.relayURL) else {
            pairingState = .failed(.invalidCode)
            publishSnapshot()
            return
        }
        guard endpoint.isAllowed else {
            pairingState = .failed(.insecureRelay)
            publishSnapshot()
            return
        }
        guard pairing.signPublicKey != store.identity.signPublicKey,
              let ownerKeys = try? PeerKeys(
                  signPublicKey: pairing.signPublicKey,
                  kxPublicKey: pairing.kxPublicKey
              ),
              let request = try? PairRequest(
                  secret: pairing.secret,
                  pairingID: pairing.pairingID,
                  signPublicKey: store.identity.signPublicKey,
                  kxPublicKey: store.identity.kxPublicKey
              ) else {
            pairingState = .failed(.ownCode)
            publishSnapshot()
            return
        }

        let ownerHex = ownerKeys.deviceID.hexString
        let buffer = JoinFrameBuffer(ownerHex: ownerHex)
        pairingState = .joining
        publishSnapshot()

        let needsRelaySwitch = store.relayURL != endpoint.url
        if needsRelaySwitch {
            overrideRelayURL = endpoint.url
            ensureConnectionLoop(forceRestart: true)
        }

        var context = PendingJoinContext(
            ownerKeys: ownerKeys,
            ownerHex: ownerHex,
            relayURL: endpoint.url,
            buffer: buffer,
            timeoutTask: nil
        )
        context.timeoutTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: Self.joinTimeoutNanoseconds)
            guard !Task.isCancelled, let self else { return }
            self.failPendingJoin(ownerHex: ownerHex, failure: .timedOut)
        }
        pendingJoin = context

        let connected = await awaitConnected()
        guard pendingJoin?.ownerHex == ownerHex else { return }
        let requested = connected ? await sendControl(.pairRequest(request)) : false
        if !requested {
            failPendingJoin(ownerHex: ownerHex, failure: .connectionFailed(.timedOut))
        }
    }

    private func failPendingJoin(ownerHex: String, failure: PairingFailure) {
        guard let pj = pendingJoin, pj.ownerHex == ownerHex else { return }
        pj.timeoutTask?.cancel()
        _ = pj.buffer.settleAndDrain()
        pendingJoin = nil
        if overrideRelayURL != nil {
            overrideRelayURL = nil
            ensureConnectionLoop(forceRestart: true)
        }
        pairingState = .failed(failure)
        publishSnapshot()
    }

    /// Invites the device showing `rawCode` into this iPhone's existing group (`0xB5` `pairInvite`).
    public func inviteWithCode(_ rawCode: String) async {
        cancelPendingPairingOperations(resetState: false)
        let trimmed = rawCode.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let pairing = try? PairingCode(decoding: trimmed) else {
            pairingState = .failed(.invalidCode)
            publishSnapshot()
            return
        }
        guard pairing.signPublicKey != store.identity.signPublicKey,
              let inviteeKeys = try? PeerKeys(
                  signPublicKey: pairing.signPublicKey,
                  kxPublicKey: pairing.kxPublicKey
              ) else {
            pairingState = .failed(.ownCode)
            publishSnapshot()
            return
        }

        let existingPeers = store.trustedPeers
        guard !existingPeers.isEmpty else {
            pairingState = .failed(.noGroupForInvite)
            publishSnapshot()
            return
        }
        guard pairing.relayURL == store.relayURL else {
            pairingState = .failed(.otherRelayForInvite)
            publishSnapshot()
            return
        }

        let inviteeHex = inviteeKeys.deviceID.hexString
        let wasTrusted = existingPeers.contains { $0.deviceIDHex == inviteeHex }
        if !wasTrusted && existingPeers.count + 1 >= Weft.maxGroupSize {
            pairingState = .failed(.groupFull)
            publishSnapshot()
            return
        }
        guard let request = try? PairRequest.invite(
            secret: pairing.secret,
            pairingID: pairing.pairingID,
            signPublicKey: store.identity.signPublicKey,
            kxPublicKey: store.identity.kxPublicKey
        ) else {
            pairingState = .failed(.invalidCode)
            publishSnapshot()
            return
        }

        pairingState = .inviting
        publishSnapshot()

        let connected = await awaitConnected()
        if !connected {
            pairingState = .failed(.connectionFailed(.timedOut))
            publishSnapshot()
            return
        }

        do { _ = try store.trustPeer(inviteeKeys, nowMs: Self.nowMillis()) }
        catch { pairingState = .failed(.storageFailed); publishSnapshot(); return }
        var context = PendingSentInviteContext(
            inviteeKeys: inviteeKeys,
            inviteeHex: inviteeHex,
            wasAlreadyTrusted: wasTrusted,
            timeoutTask: nil
        )
        context.timeoutTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: Self.joinTimeoutNanoseconds)
            guard !Task.isCancelled, let self else { return }
            self.failSentInvite(inviteeHex: inviteeHex, failure: .noAnswerToInvite)
        }
        pendingSentInvite = context

        if !(await sendControl(.pairInvite(request))) {
            failSentInvite(inviteeHex: inviteeHex, failure: .connectionFailed(.lost))
        }
    }

    private func failSentInvite(inviteeHex: String, failure: PairingFailure) {
        guard let psi = pendingSentInvite, psi.inviteeHex == inviteeHex else { return }
        psi.timeoutTask?.cancel()
        pendingSentInvite = nil
        if !psi.wasAlreadyTrusted {
            guard rollbackTrust(inviteeHex) else { return }
        }
        pairingState = .failed(failure)
        publishSnapshot()
    }

    // MARK: - Envelope Processing & Deferred Replay

    /// Stop synchronization if cancelling provisional trust cannot be persisted.
    private func rollbackTrust(_ peerHex: String) -> Bool {
        do { try store.untrustPeer(peerHex); return true }
        catch {
            pairingState = .failed(.storageFailed)
            lastBanner = BannerNotice(text: PairingFailure.storageFailed.text)
            stop()
            return false
        }
    }

    private func handleEnvelopeFrame(_ frame: Data) async {
        guard frame.count >= Weft.headerLength,
              let header = try? Header(decoding: frame.prefix(Weft.headerLength)),
              header.recipient == store.identity.deviceID else {
            return
        }

        let senderHex = header.sender.hexString

        // 1. Pending `0xB1` join buffering until Welcome from slot owner arrives
        if let pj = pendingJoin {
            let isTrusted = store.trustedPeer(senderHex) != nil
            switch pj.buffer.onEnvelope(senderHex: senderHex, kind: header.kind, isTrusted: isTrusted, frame: frame) {
            case .welcome:
                await settlePendingJoinWithWelcome(pj, frame: frame)
                return
            case .deferred:
                return
            case .processNow:
                break
            }
        }

        // 2. Pending `0xB4` accepted-invitation buffering until Welcome from inviter arrives
        if let pai = pendingAcceptedInvite {
            // Note: inviter is already trusted so we could send it our DeviceName, while other
            // members listed in the upcoming Welcome are NOT trusted yet.
            let isTrusted = store.trustedPeer(senderHex) != nil
            switch pai.buffer.onEnvelope(senderHex: senderHex, kind: header.kind, isTrusted: isTrusted, frame: frame) {
            case .welcome:
                await settleAcceptedInviteWithWelcome(pai, frame: frame)
                return
            case .deferred:
                return
            case .processNow:
                break
            }
        }

        await processAuthenticatedEnvelope(frame)
    }

    private func settlePendingJoinWithWelcome(_ pj: PendingJoinContext, frame: Data) async {
        let nowMs = Self.nowMillis()
        guard let outcome = try? store.acceptJoinWelcome(
            frame: frame,
            ownerKeys: pj.ownerKeys,
            newRelayURL: pj.relayURL,
            nowMs: nowMs
        ) else {
            return
        }

        pj.timeoutTask?.cancel()
        let deferred = pj.buffer.settleAndDrain()
        pendingJoin = nil
        overrideRelayURL = nil

        if outcome.replacedPreviousGroup {
            let kept = Set(outcome.peers.map(\.deviceIDHex))
            peerRuntime = peerRuntime.filter { kept.contains($0.key) }
        }

        for buffered in deferred {
            await processAuthenticatedEnvelope(buffered)
        }

        let allHexes = store.trustedPeers.map(\.deviceIDHex)
        await announceSelfAndState(to: allHexes)
        _ = await sendPacket(.stateRequest, to: allHexes)

        pairingState = .paired(deviceIDHex: pj.ownerHex, invited: true)
        publishSnapshot()
    }

    private func settleAcceptedInviteWithWelcome(_ pai: PendingAcceptedInviteContext, frame: Data) async {
        guard let opened = try? store.openEnvelope(frame: frame, nowMs: Self.nowMillis()),
              case .welcome(let newlyTrusted) = opened.event else {
            return
        }

        pai.timeoutTask?.cancel()
        let deferred = pai.buffer.settleAndDrain()
        pendingAcceptedInvite = nil

        for buffered in deferred {
            await processAuthenticatedEnvelope(buffered)
        }

        let otherNewMembers = newlyTrusted.filter { $0 != pai.inviterHex }
        if !otherNewMembers.isEmpty {
            await announceSelfAndState(to: otherNewMembers)
            _ = await sendPacket(.stateRequest, to: otherNewMembers)
        }
        _ = await sendPacket(.stateRequest, to: [pai.inviterHex])

        pairingState = .paired(deviceIDHex: pai.inviterHex, invited: true)
        publishSnapshot()
    }

    private func processAuthenticatedEnvelope(_ frame: Data) async {
        let nowMs = Self.nowMillis()
        guard let opened = try? store.openEnvelope(frame: frame, nowMs: nowMs) else {
            return
        }
        let senderHex = opened.senderHex
        let senderName = store.trustedPeer(senderHex)?.displayName ?? Words.shortID(senderHex)

        switch opened.event {
        case .clipboardText(let text):
            guard !text.isEmpty, store.features.clipboardReceive else { return }
            lastBanner = BannerNotice(
                text: Language.current == .french
                    ? "Presse-papiers reçu de \(senderName)"
                    : "Clipboard received from \(senderName)"
            )
            onClipboardTextReceived?(senderName, text)
            publishSnapshot()

        case .ping:
            if store.features.ping {
                lastBanner = BannerNotice(
                    text: Language.current == .french
                        ? "Ping reçu de \(senderName)"
                        : "Ping from \(senderName)"
                )
                onPingReceived?(senderName, nil)
                publishSnapshot()
            }

        case .welcome(let newlyTrusted):
            await announceSelfAndState(to: newlyTrusted)
            publishSnapshot()

        case .deviceAdded(_, let newlyTrusted):
            if !newlyTrusted.isEmpty {
                await announceSelfAndState(to: newlyTrusted)
            }
            publishSnapshot()

        case .deviceRemoved(let removedHex):
            peerRuntime.removeValue(forKey: removedHex)
            if ringingFromPeerHex == removedHex {
                stopIncomingRingLocally()
            }
            publishSnapshot()

        case .deviceName:
            if let psi = pendingSentInvite, psi.inviteeHex == senderHex {
                await completeSentInviteAfterInviteeNamed(psi)
            } else {
                publishSnapshot()
            }

        case .packet(let jsonData):
            await handleDecodedPacket(from: senderHex, senderName: senderName, jsonData: jsonData, nowMs: nowMs)

        case .fileChunk(let chunkPlaintext):
            if let chunk = try? decodeFileChunk(chunkPlaintext) {
                let actions = transferEngine.handleFileChunk(from: senderHex, chunk: chunk, nowMs: nowMs)
                await executeTransferActions(actions)
                publishSnapshot()
            }
        }
    }

    private func completeSentInviteAfterInviteeNamed(_ psi: PendingSentInviteContext) async {
        psi.timeoutTask?.cancel()
        pendingSentInvite = nil

        let inviteeHex = psi.inviteeHex
        let others = store.trustedPeers.filter { $0.deviceIDHex != inviteeHex }
        let otherHexes = others.map(\.deviceIDHex)

        if !otherHexes.isEmpty {
            _ = await sendEnvelope(
                kind: .deviceAdded,
                plaintext: encodeDeviceAdded(psi.inviteeKeys),
                to: otherHexes
            )
        }
        if let welcomePlaintext = try? encodeWelcome(others.map(\.peerKeys)) {
            _ = await sendEnvelope(kind: .welcome, plaintext: welcomePlaintext, to: [inviteeHex])
        }
        await announceSelfAndState(to: [inviteeHex])
        _ = await sendPacket(.stateRequest, to: [inviteeHex])

        pairingState = .paired(deviceIDHex: inviteeHex, invited: false)
        publishSnapshot()
    }

    // MARK: - Packet Handling (`0x07`)

    private func handleDecodedPacket(
        from senderHex: String,
        senderName: String,
        jsonData: Data,
        nowMs: Int64
    ) async {
        guard case .known(let packet) = Packet.parse(jsonData: jsonData) else {
            return
        }
        let features = store.features
        guard features.allowsIncomingPacket(packet.packetType) else {
            if case .fileOffer(let offer) = packet {
                _ = await sendPacket(.fileReject(id: offer.id, reason: "disabled"), to: [senderHex])
            }
            return
        }

        switch packet {
        case .capabilities(let cap):
            let hadCapabilitiesBefore = peerRuntime[senderHex]?.capabilities != nil
            var rt = peerRuntime[senderHex] ?? PeerRuntime()
            rt.capabilities = cap
            peerRuntime[senderHex] = rt
            publishSnapshot()
            if !hadCapabilitiesBefore {
                await announceSelfAndState(to: [senderHex])
            }

        case .stateRequest:
            await announceSelfAndState(to: [senderHex])

        case .battery(let bat):
            var rt = peerRuntime[senderHex] ?? PeerRuntime()
            rt.battery = bat
            peerRuntime[senderHex] = rt
            publishSnapshot()

        case .connectivity(let conn):
            var rt = peerRuntime[senderHex] ?? PeerRuntime()
            rt.connectivity = conn
            peerRuntime[senderHex] = rt
            publishSnapshot()

        case .ring:
            ringingFromPeerHex = senderHex
            onRingStarted?(senderName)
            publishSnapshot()

        case .ringStop:
            if ringingFromPeerHex == senderHex || ringingFromPeerHex != nil {
                stopIncomingRingLocally()
                publishSnapshot()
            }

        case .ping(let message):
            let display: String
            if let message, !message.isEmpty {
                display = "\(senderName): \(message)"
            } else {
                display = Language.current == .french ? "Ping de \(senderName)" : "Ping from \(senderName)"
            }
            lastBanner = BannerNotice(text: display, createdAtMs: nowMs)
            onPingReceived?(senderName, message)
            publishSnapshot()

        case .shareURL(let rawURL):
            if let url = Links.openable(rawURL) {
                lastBanner = BannerNotice(
                    text: Language.current == .french
                        ? "Lien reçu de \(senderName)"
                        : "Link received from \(senderName)",
                    createdAtMs: nowMs
                )
                onShareURLReceived?(senderName, url)
                publishSnapshot()
            }

        case .fileOffer, .fileAccept, .fileReject, .fileAck, .fileDone, .fileCancel,
             .clipboardImageOffer, .clipboardImageAccept:
            let actions = transferEngine.handlePacket(
                from: senderHex,
                packet: packet,
                filesEnabled: features.files,
                clipboardImagesEnabled: features.clipboardReceive,
                nowMs: nowMs
            )
            await executeTransferActions(actions)
            publishSnapshot()

        case .media(let players):
            var rt = peerRuntime[senderHex] ?? PeerRuntime()
            rt.players = players
            rt.mediaReceivedAtMs = nowMs
            peerRuntime[senderHex] = rt
            publishSnapshot()

        case .mediaArt(let art):
            var rt = peerRuntime[senderHex] ?? PeerRuntime()
            rt.insertCover(id: art.art, data: art.data)
            peerRuntime[senderHex] = rt
            publishSnapshot()

        case .volume(let vol):
            var rt = peerRuntime[senderHex] ?? PeerRuntime()
            rt.volume = vol
            peerRuntime[senderHex] = rt
            publishSnapshot()

        case .commandList(let commands):
            var rt = peerRuntime[senderHex] ?? PeerRuntime()
            rt.commands = commands
            peerRuntime[senderHex] = rt
            publishSnapshot()

        case .mediaCommand, .volumeSet, .inputPointer, .inputClick, .inputKey, .presenter, .commandRun:
            break
        }
    }

    private func executeTransferActions(_ actions: [TransferOutputAction]) async {
        for action in actions {
            switch action {
            case .sendPacket(let peerHex, let packet):
                _ = await sendPacket(packet, to: [peerHex])

            case .sendFileChunk(let peerHex, let plaintext):
                _ = await sendEnvelope(kind: .fileChunk, plaintext: plaintext, to: [peerHex])

            case .fileReceived(let peerHex, _, let name, let fileURL):
                let peerName = store.trustedPeer(peerHex)?.displayName ?? Words.shortID(peerHex)
                lastBanner = BannerNotice(
                    text: Language.current == .french ? "Reçu : \(name)" : "Received \(name)"
                )
                onFileReceived?(peerName, name, fileURL)

            case .clipboardImageReceived(let peerHex, _, let data, let mime):
                guard store.features.clipboardReceive else { continue }
                let peerName = store.trustedPeer(peerHex)?.displayName ?? Words.shortID(peerHex)
                lastBanner = BannerNotice(
                    text: Language.current == .french
                        ? "Image copiée depuis \(peerName)"
                        : "Image copied from \(peerName)"
                )
                onClipboardImageReceived?(peerName, data, mime)

            case .clipboardImageRejected:
                break
            }
        }
    }

    private func stopIncomingRingLocally() {
        ringingFromPeerHex = nil
        onRingStopped?()
    }

    private func runPeriodicMaintenance() async {
        let actions = transferEngine.tick(nowMs: Self.nowMillis())
        if !actions.isEmpty {
            await executeTransferActions(actions)
            publishSnapshot()
        }
    }

    // MARK: - User Actions (Clipboard, Files, Links, Ring, Ping, Media, Remote, Settings)

    public func dismissIncomingRing() async {
        let sender = ringingFromPeerHex
        stopIncomingRingLocally()
        publishSnapshot()
        if let sender {
            _ = await sendPacket(.ringStop, to: [sender])
        }
    }

    public func clearBanner() {
        lastBanner = nil
        publishSnapshot()
    }

    /// Sends UTF-8 clipboard text (`0x01`) to `targetPeerHex` (or every trusted peer when `nil`).
    @discardableResult
    public func sendClipboardText(_ text: String, to targetPeerHex: String? = nil) async -> Bool {
        guard !text.isEmpty else { return false }
        let bytes = Data(text.utf8)
        guard bytes.count <= Weft.maxClipboardTextLength else {
            lastBanner = BannerNotice(
                text: t("Clipboard text exceeds 256 KiB", "Texte du presse-papiers supérieur à 256 Kio")
            )
            publishSnapshot()
            return false
        }
        let recipients = targetPeerHex.map { [$0.lowercased()] } ?? store.trustedPeers.map(\.deviceIDHex)
        guard !recipients.isEmpty else { return false }

        guard await awaitConnected() else {
            lastBanner = BannerNotice(text: t("Could not reach the relay", "Impossible de joindre le relais"))
            publishSnapshot()
            return false
        }

        let count = await sendEnvelope(kind: .clipboardText, plaintext: bytes, to: recipients)
        if count > 0 {
            lastBanner = BannerNotice(text: t("Clipboard sent", "Presse-papiers envoyé"))
            publishSnapshot()
            return true
        }
        return false
    }

    /// Sends a PNG or JPEG clipboard image (`<= 16 MiB`) via `clipboard.image.offer` + `0x08` chunks.
    @discardableResult
    public func sendClipboardImage(data: Data, mime: String, to targetPeerHex: String? = nil) async -> Bool {
        guard ClipboardImageValidator.canOffer(data: data, mime: mime) else {
            lastBanner = BannerNotice(
                text: t("Clipboard image must be PNG or JPEG up to 16 MiB",
                        "L’image doit être au format PNG ou JPEG jusqu’à 16 Mio")
            )
            publishSnapshot()
            return false
        }

        let candidates = targetPeerHex.map { [$0.lowercased()] } ?? store.trustedPeers.map(\.deviceIDHex)
        let recipients = candidates.filter { hex in
            guard let cap = peerRuntime[hex]?.capabilities else { return true }
            return cap.handles("clipboard.image.offer")
        }
        guard !recipients.isEmpty else {
            lastBanner = BannerNotice(
                text: t("No connected device accepts clipboard images",
                        "Aucun appareil connecté n’accepte les images du presse-papiers")
            )
            publishSnapshot()
            return false
        }

        guard await awaitConnected() else { return false }

        var offeredAny = false
        for hex in recipients {
            let actions = transferEngine.offerClipboardImage(data: data, mime: mime, to: hex)
            if !actions.isEmpty {
                offeredAny = true
                await executeTransferActions(actions)
            }
        }
        if offeredAny {
            lastBanner = BannerNotice(text: t("Sending clipboard image…", "Envoi de l’image du presse-papiers…"))
            publishSnapshot()
        }
        return offeredAny
    }

    @discardableResult
    public func sendFiles(_ urls: [URL], to peerHex: String) async -> Int {
        guard store.features.files, await awaitConnected() else { return 0 }
        var count = 0
        for url in urls {
            let scoped = url.startAccessingSecurityScopedResource()
            // Always stage a private copy: an extension inbox item may be removed
            // immediately after this method accepts the transfer.
            let localCopyURL: URL
            do {
                let stagedCopy = transferEngine.stagingDirectory
                    .appendingPathComponent("outgoing_\(UUID().uuidString)_\(url.lastPathComponent)")
                do {
                    try FileManager.default.copyItem(at: url, to: stagedCopy)
                    localCopyURL = stagedCopy
                } catch {
                    if scoped { url.stopAccessingSecurityScopedResource() }
                    continue
                }
                if scoped { url.stopAccessingSecurityScopedResource() }
            }

            if let actions = try? transferEngine.offerFile(
                fileURL: localCopyURL,
                ownsFile: true,
                to: peerHex,
                nowMs: Self.nowMillis()
            ) {
                // Keep the original display file name when staged from a security-scoped URL.
                await executeTransferActions(actions.map { action in
                    if case .sendPacket(let p, .fileOffer(var offer)) = action {
                        offer.name = url.lastPathComponent
                        return .sendPacket(peerHex: p, packet: .fileOffer(offer))
                    }
                    return action
                })
                count += 1
            } else {
                try? FileManager.default.removeItem(at: localCopyURL)
            }
        }
        publishSnapshot()
        return count
    }

    public func cancelTransfer(peerHex: String, id: String) async {
        let actions = transferEngine.cancelTransfer(peerHex: peerHex, id: id)
        await executeTransferActions(actions)
        publishSnapshot()
    }

    @discardableResult
    public func shareLink(_ rawURL: String, to peerHex: String) async -> Bool {
        let trimmed = rawURL.trimmingCharacters(in: .whitespacesAndNewlines)
        guard store.features.links, Links.isShareable(trimmed), await awaitConnected() else {
            return false
        }
        return await sendPacket(.shareURL(url: trimmed), to: [peerHex]) > 0
    }

    public func ringPeer(_ peerHex: String) async {
        guard store.features.ring, await awaitConnected() else { return }
        if await sendPacket(.ring, to: [peerHex]) > 0 {
            outgoingRingingPeerHex = peerHex.lowercased()
            publishSnapshot()
        }
    }

    public func stopRingingPeer(_ peerHex: String) async {
        if outgoingRingingPeerHex == peerHex.lowercased() {
            outgoingRingingPeerHex = nil
            publishSnapshot()
        }
        guard await awaitConnected() else { return }
        _ = await sendPacket(.ringStop, to: [peerHex])
    }

    public func pingPeer(_ peerHex: String, message: String? = nil) async {
        guard store.features.ping, await awaitConnected() else { return }
        if await sendPacket(.ping(message: message), to: [peerHex]) > 0 {
            let name = store.trustedPeer(peerHex)?.displayName ?? Words.shortID(peerHex)
            lastBanner = BannerNotice(
                text: Language.current == .french ? "Ping envoyé à \(name)" : "Ping sent to \(name)"
            )
            publishSnapshot()
        }
    }

    public func sendMediaCommand(_ action: MediaCommandAction, playerID: String, to peerHex: String) async {
        guard store.features.media, connectionState.isConnected else { return }
        let payload = MediaCommandPayload(player: playerID, command: action)
        _ = await sendPacket(.mediaCommand(payload), to: [peerHex])
    }

    public func setRemoteVolume(level: UInt8?, muted: Bool?, to peerHex: String) async {
        guard store.features.volume, connectionState.isConnected else { return }
        _ = await sendPacket(.volumeSet(VolumeSetPayload(level: level, muted: muted)), to: [peerHex])
    }

    public func runShortcut(id: String, on peerHex: String) async {
        guard store.features.commands, await awaitConnected() else { return }
        _ = await sendPacket(.commandRun(id: id), to: [peerHex])
    }

    public func sendRemotePackets(_ packets: [Packet], to peerHex: String) async {
        guard store.features.remote, connectionState.isConnected, !packets.isEmpty else { return }
        for packet in packets {
            _ = await sendPacket(packet, to: [peerHex])
        }
    }

    // MARK: - Local Device Status & Settings Updates

    public func updateLocalBattery(level: UInt8?, charging: Bool?, low: Bool?) async {
        let updated = BatteryPayload(level: level, charging: charging, low: low)
        guard updated != localBattery else { return }
        localBattery = updated
        guard store.features.battery, connectionState.isConnected else { return }
        let peers = store.trustedPeers.map(\.deviceIDHex)
        _ = await sendPacket(.battery(updated), to: peers)
    }

    public func updateLocalConnectivity(network: String?, signal: UInt8? = nil) async {
        let updated = ConnectivityPayload(network: network, signal: signal)
        guard updated != localConnectivity else { return }
        localConnectivity = updated
        guard store.features.connectivity, connectionState.isConnected else { return }
        let peers = store.trustedPeers.map(\.deviceIDHex)
        _ = await sendPacket(.connectivity(updated), to: peers)
    }

    public func renameDevice(_ typed: String) async -> NameProblem? {
        let saved: String
        do {
            saved = try store.setDeviceName(typed)
        } catch MobileStoreError.invalidDeviceName(let problem) {
            return problem
        } catch {
            return .empty
        }
        publishSnapshot()
        let peers = store.trustedPeers.map(\.deviceIDHex)
        if !peers.isEmpty, await awaitConnected(), let plaintext = try? encodeDeviceName(saved) {
            _ = await sendEnvelope(kind: .deviceName, plaintext: plaintext, to: peers)
            _ = await sendPacket(.capabilities(store.features.capabilitiesPayload()), to: peers)
        }
        return nil
    }

    public func updateFeatures(_ updated: FeatureSwitches) async {
        try? store.setFeatures(updated)
        publishSnapshot()
        let peers = store.trustedPeers.map(\.deviceIDHex)
        if !peers.isEmpty, connectionState.isConnected {
            await announceSelfAndState(to: peers)
        }
    }

    public func updatePointerSpeed(_ speed: Double) {
        try? store.setPointerSpeed(speed)
        publishSnapshot()
    }

    public func updateRelayURL(_ typed: String) async -> String? {
        do {
            _ = try store.setRelayURL(typed)
        } catch MobileStoreError.relayChangeWhileInGroup {
            return t("Leave the group first before changing the relay.",
                     "Quittez d’abord le groupe avant de changer de relais.")
        } catch {
            return t("Enter a valid wss:// relay URL (up to 255 bytes).",
                     "Saisissez une adresse de relais wss:// valide (255 octets au plus).")
        }
        ensureConnectionLoop(forceRestart: true)
        publishSnapshot()
        return nil
    }

    @discardableResult
    public func removePeer(_ peerHex: String) async -> Bool {
        let clean = peerHex.lowercased()
        guard let peer = store.trustedPeer(clean) else { return false }
        do { try store.untrustPeer(clean) }
        catch { return false }
        _ = await sendControl(.groupRemove(deviceID: peer.deviceID))
        peerRuntime.removeValue(forKey: clean)
        let remaining = store.trustedPeers.map(\.deviceIDHex)
        if !remaining.isEmpty, let removedPlaintext = try? encodeDeviceRemoved(deviceID: peer.deviceID) {
            _ = await sendEnvelope(kind: .deviceRemoved, plaintext: removedPlaintext, to: remaining)
        }
        publishSnapshot()
        return true
    }

    /// Leaves the group, notifying all group members and the relay if reachable;
    /// always clears local group trust when `evenIfOffline` is true.
    public func leaveGroup(evenIfOffline: Bool = true) async -> Bool {
        let members = store.trustedPeers.map(\.deviceIDHex)
        guard !members.isEmpty else { return true }

        let connected = await awaitConnected()
        guard connected || evenIfOffline else { return false }
        // Persist every counter and the trust removal before sending any notice.
        var notices: [Data] = []
        do {
            if connected {
                let removed = try encodeDeviceRemoved(deviceID: store.identity.deviceID)
                for member in members {
                    notices.append(try store.sealEnvelope(to: member, kind: .deviceRemoved, plaintext: removed))
                }
            }
            try store.clearGroup()
        } catch { return false }
        peerRuntime.removeAll()
        stopIncomingRingLocally()
        outgoingRingingPeerHex = nil
        publishSnapshot()
        for notice in notices { _ = await sendRawFrame(notice) }
        if connected { _ = await sendControl(.groupRemove(deviceID: store.identity.deviceID)) }
        return true
    }

    // MARK: - Snapshot Builder

    private func publishSnapshot() {
        snapshot = Self.buildSnapshot(
            store: store,
            transferEngine: transferEngine,
            peerRuntime: peerRuntime,
            connectionState: connectionState,
            pairingState: pairingState,
            ringingFromPeerHex: ringingFromPeerHex,
            outgoingRingingPeerHex: outgoingRingingPeerHex,
            lastBanner: lastBanner
        )
    }

    private static func buildSnapshot(
        store: MobileStore,
        transferEngine: TransferEngine,
        peerRuntime: [String: PeerRuntime],
        connectionState: ConnectionState,
        pairingState: PairingState,
        ringingFromPeerHex: String?,
        outgoingRingingPeerHex: String?,
        lastBanner: BannerNotice?
    ) -> MobileSnapshot {
        let trusted = store.trustedPeers
        let peers: [PeerSnapshot] = trusted.map { peer in
            let rt = peerRuntime[peer.deviceIDHex]
            return PeerSnapshot(
                id: peer.deviceIDHex,
                shortID: peer.shortID,
                displayName: peer.displayName,
                addedAtMs: peer.addedAtMs,
                capabilities: rt?.capabilities,
                battery: rt?.battery,
                connectivity: rt?.connectivity,
                players: rt?.players ?? [],
                mediaReceivedAtMs: rt?.mediaReceivedAtMs ?? 0,
                covers: rt?.covers ?? [:],
                volume: rt?.volume,
                commands: rt?.commands ?? []
            )
        }
        let ringingName = ringingFromPeerHex.flatMap { hex in
            trusted.first(where: { $0.deviceIDHex == hex })?.displayName ?? Words.shortID(hex)
        }
        return MobileSnapshot(
            deviceIDHex: store.deviceIDHex,
            shortID: store.shortID,
            deviceName: store.deviceName,
            relayURL: store.relayURL,
            connection: connectionState,
            peers: peers,
            transfers: transferEngine.transfersSnapshot(),
            pairing: pairingState,
            features: store.features,
            pointerSpeed: store.pointerSpeed,
            ringingFromPeerHex: ringingFromPeerHex,
            ringingFromPeerName: ringingName,
            outgoingRingingPeerHex: outgoingRingingPeerHex,
            lastBanner: lastBanner
        )
    }

    private static func nowMillis() -> Int64 {
        Int64(Date().timeIntervalSince1970 * 1000)
    }
}
