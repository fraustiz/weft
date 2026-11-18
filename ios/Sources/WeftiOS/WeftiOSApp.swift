// Weft for iOS 17+: SwiftUI entry point, brand tokens, macro-free `Local<T>`
// state wrapper, platform monitors (battery, network, pasteboard, URL scheme),
// and host stub so `swift build --package-path ios` succeeds on macOS too.

#if canImport(UIKit)
import Combine
import Network
import SwiftUI
import UIKit
import UniformTypeIdentifiers
import WeftMobileKit

// MARK: - Macro-free View State (`Local<Value>`)

@MainActor
final class Local<Value>: ObservableObject {
    @Published var value: Value

    init(_ value: Value) {
        self.value = value
    }
}

// MARK: - Brand Tokens & Hand-Drawn Mark (`brand/README.md`)

enum Brand {
    static let orange = UIColor(red: 0xF2 / 255.0, green: 0x6A / 255.0, blue: 0x21 / 255.0, alpha: 1.0)
    static let ink = UIColor(red: 0x17 / 255.0, green: 0x13 / 255.0, blue: 0x0F / 255.0, alpha: 1.0)
    static let cream = UIColor(red: 0xF6 / 255.0, green: 0xEF / 255.0, blue: 0xE6 / 255.0, alpha: 1.0)
}

extension Color {
    static let weftOrange = Color(uiColor: Brand.orange)
    static let weftInk = Color(uiColor: Brand.ink)
    static let weftCream = Color(uiColor: Brand.cream)
}

/// Primary filled action button: sunset orange surface with high-contrast `#17130F` ink text.
struct WeftProminentButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled
    var fullWidth = false
    var large = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(large ? .body.weight(.medium) : .subheadline.weight(.medium))
            .foregroundStyle(Color.weftInk)
            .padding(.horizontal, large ? 20 : 14)
            .padding(.vertical, large ? 12 : 8)
            .frame(maxWidth: fullWidth ? .infinity : nil)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(Color.weftOrange)
                    .brightness(configuration.isPressed ? -0.07 : 0)
            )
            .opacity(isEnabled ? 1.0 : 0.45)
            .contentShape(Rectangle())
    }
}

/// Action row label: the icon carries the brand orange, the title stays neutral.
struct WeftActionLabelStyle: LabelStyle {
    /// Every icon sits centered in the same column, so titles line up whatever
    /// the symbol's own width; it scales with Dynamic Type.
    @ScaledMetric(relativeTo: .body) private var iconWidth: CGFloat = 28

    func makeBody(configuration: Configuration) -> some View {
        // An HStack, not Label: Label pins the icon to the first line of a
        // wrapped title; here it stays centered on the whole title.
        HStack(alignment: .center, spacing: 14) {
            configuration.icon
                .foregroundStyle(Color.weftOrange)
                .frame(width: iconWidth, alignment: .center)
            configuration.title
                .foregroundStyle(.primary)
                // Row separators start under the title on every row.
                .alignmentGuide(.listRowSeparatorLeading) { $0[.leading] }
        }
        .accessibilityElement(children: .combine)
    }
}

/// Hand-drawn vector symbol from `brand/final/weft-mark.svg`: three rounded pixels
/// crossed by a needle exiting through the missing fourth pixel.
struct BrandMark: View {
    var color: Color = .weftOrange

    var body: some View {
        Canvas { context, size in
            let scale = min(size.width, size.height) / 100.0
            let place = CGAffineTransform(scaleX: scale, y: scale)
            let needle = Self.needle.applying(place)
            context.drawLayer { layer in
                for (x, y) in [(20.0, 20.0), (20.0, 52.0), (52.0, 52.0)] {
                    let pixel = Path(
                        roundedRect: CGRect(x: x, y: y, width: 28, height: 28),
                        cornerRadius: 7
                    )
                    layer.fill(pixel.applying(place), with: .color(color))
                }
                layer.blendMode = .destinationOut
                layer.stroke(
                    needle,
                    with: .color(.black),
                    style: StrokeStyle(lineWidth: 5.0 * scale, lineJoin: .round)
                )
                layer.fill(needle, with: .color(.black))
            }
            context.fill(needle, with: .color(color), style: FillStyle(eoFill: true))
        }
        .aspectRatio(1, contentMode: .fit)
        .accessibilityHidden(true)
    }

    private static let needle: Path = {
        var path = Path()
        path.move(to: .zero)
        path.addCurve(
            to: CGPoint(x: 86, y: -3.2),
            control1: CGPoint(x: 24.64, y: -2.40),
            control2: CGPoint(x: 61.60, y: -3.2)
        )
        path.addArc(
            center: CGPoint(x: 86, y: 0),
            radius: 3.2,
            startAngle: .degrees(-90),
            endAngle: .degrees(90),
            clockwise: false
        )
        path.addCurve(
            to: .zero,
            control1: CGPoint(x: 61.60, y: 3.2),
            control2: CGPoint(x: 24.64, y: 2.40)
        )
        path.closeSubpath()
        path.addRoundedRect(
            in: CGRect(x: 77, y: -0.8, width: 6, height: 1.6),
            cornerSize: CGSize(width: 0.8, height: 0.8)
        )
        let transform = CGAffineTransform(translationX: 14, y: 88)
            .rotated(by: -49.5 * .pi / 180.0)
        return path.applying(transform)
    }()
}

// MARK: - Clipboard Payload (System Paste Button or `weft://send-clipboard`)

/// What a clipboard send carries: copied text, or a PNG/JPEG image.
enum ClipboardPayload {
    case text(String)
    case image(Data, mime: String)

    /// PNG when it fits the clipboard image limit, else JPEG.
    static func encoded(_ image: UIImage) -> ClipboardPayload? {
        if let png = image.pngData(), ClipboardImageValidator.canOffer(data: png, mime: "image/png") {
            return .image(png, mime: "image/png")
        }
        if let jpg = image.jpegData(compressionQuality: 0.9),
           ClipboardImageValidator.canOffer(data: jpg, mime: "image/jpeg") {
            return .image(jpg, mime: "image/jpeg")
        }
        return nil
    }
}

// MARK: - iOS App Coordinator (`MobileAppModel`)

@MainActor
final class MobileAppModel: ObservableObject {
    @Published private(set) var snapshot: MobileSnapshot?
    @Published private(set) var startupError: String?
    @Published var showingPairing = false
    @Published var showingSettings = false
    @Published var remoteControlPeerID: String?
    @Published var receivedLinkToOpen: URL?
    @Published var pendingShareFile: URL?
    @Published var shareFailed = false
    /// Why the last "Send Clipboard" tap sent nothing, for the row's alert.
    @Published var clipboardProblem: ClipboardProblem?
    /// Whether the pasteboard holds text or an image. Reading these flags
    /// never prompts, so the "Send Clipboard" row can be dimmed when empty.
    @Published private(set) var clipboardHasContent = false

    enum ClipboardProblem: Identifiable {
        /// Nothing Weft can send: no text, PNG or JPEG.
        case empty
        /// There is content but iOS refused to paste it.
        case denied
        var id: Self { self }
    }

    private(set) var session: MobileSession?
    private let alarmPlayer = AlarmPlayer()
    private var cancellables: Set<AnyCancellable> = []
    private var pathMonitor: NWPathMonitor?
    private var statusPollTask: Task<Void, Never>?
    private var lastWrittenPasteboardCount: Int?
    private var pendingShortcutSend = false

    init() {
        bootstrap()
        refreshClipboardState()
        NotificationCenter.default.publisher(for: UIPasteboard.changedNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.refreshClipboardState() }
            .store(in: &cancellables)
    }

    /// Other apps' copies post no notification here: also called when the
    /// app becomes active.
    func refreshClipboardState() {
        let pb = UIPasteboard.general
        clipboardHasContent = pb.hasStrings || pb.hasImages
    }

    private func bootstrap() {
        let fm = FileManager.default
        let appSupport = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? fm.temporaryDirectory
        let weftSupport = appSupport.appendingPathComponent("Weft", isDirectory: true)
        let stateFile = weftSupport.appendingPathComponent("state.json")
        let stagingDir = weftSupport.appendingPathComponent("Staging", isDirectory: true)
        let documentsDir = fm.urls(for: .documentDirectory, in: .userDomainMask).first
            ?? weftSupport.appendingPathComponent("Received", isDirectory: true)
        let receivedDir = documentsDir.appendingPathComponent("Weft", isDirectory: true)

        let rawDeviceName = UIDevice.current.name
        let initialRelay = ProcessInfo.processInfo.environment["WEFT_RELAY_URL"] ?? MobileStore.defaultRelayURL

        do {
            let store = try MobileStore(
                fileURL: stateFile,
                defaultDeviceName: rawDeviceName,
                initialRelayURL: initialRelay
            )
            let engine = try TransferEngine(
                stagingDirectory: stagingDir,
                destinationDirectory: receivedDir
            )
            let createdSession = MobileSession(store: store, transferEngine: engine)
            self.session = createdSession
            self.snapshot = createdSession.snapshot

            createdSession.$snapshot
                .receive(on: DispatchQueue.main)
                .sink { [weak self] updated in
                    self?.snapshot = updated
                }
                .store(in: &cancellables)

            wireCallbacks(createdSession)
            startPlatformMonitors()
            createdSession.start()
        } catch {
            self.startupError = error.localizedDescription
        }
    }

    private func wireCallbacks(_ session: MobileSession) {
        alarmPlayer.onAutoStop = { [weak self] in
            Task { @MainActor in
                await self?.session?.dismissIncomingRing()
            }
        }

        session.onRingStarted = { [weak self] _ in
            self?.alarmPlayer.start()
            let generator = UINotificationFeedbackGenerator()
            generator.notificationOccurred(.warning)
        }

        session.onRingStopped = { [weak self] in
            self?.alarmPlayer.stop()
        }

        session.onClipboardTextReceived = { [weak self] _, text in
            guard UIApplication.shared.applicationState == .active else { return }
            UIPasteboard.general.string = text
            self?.lastWrittenPasteboardCount = UIPasteboard.general.changeCount
        }

        session.onClipboardImageReceived = { [weak self] _, data, mime in
            guard UIApplication.shared.applicationState == .active else { return }
            let typeID = mime == "image/png" ? UTType.png.identifier : UTType.jpeg.identifier
            UIPasteboard.general.setData(data, forPasteboardType: typeID)
            self?.lastWrittenPasteboardCount = UIPasteboard.general.changeCount
        }

        session.onShareURLReceived = { [weak self] _, url in
            self?.receivedLinkToOpen = url
            if UIApplication.shared.applicationState == .active {
                UIApplication.shared.open(url)
            }
        }

        session.onPingReceived = { _, _ in
            let generator = UINotificationFeedbackGenerator()
            generator.notificationOccurred(.success)
        }
    }

    private func startPlatformMonitors() {
        UIDevice.current.isBatteryMonitoringEnabled = true
        refreshBatteryState()

        statusPollTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 30_000_000_000)
                guard let self else { return }
                self.refreshBatteryState()
            }
        }

        let monitor = NWPathMonitor()
        monitor.pathUpdateHandler = { [weak self] path in
            let network = Self.networkLabel(from: path)
            Task { @MainActor in
                await self?.session?.updateLocalConnectivity(network: network)
            }
        }
        monitor.start(queue: DispatchQueue(label: "fr.frohub.weft.ios.network"))
        self.pathMonitor = monitor
    }

    nonisolated private static func networkLabel(from path: NWPath) -> String {
        guard path.status == .satisfied else { return "none" }
        if path.usesInterfaceType(.wifi) || path.usesInterfaceType(.wiredEthernet) {
            return "wifi"
        }
        if path.usesInterfaceType(.cellular) {
            return "LTE"
        }
        return "wifi"
    }

    func refreshBatteryState() {
        guard let session else { return }
        let rawLevel = UIDevice.current.batteryLevel
        let level: UInt8? = rawLevel >= 0 ? UInt8(min(100, max(0, Int((rawLevel * 100).rounded())))) : nil
        let state = UIDevice.current.batteryState
        let charging: Bool? = switch state {
        case .charging, .full: true
        case .unplugged: false
        case .unknown: nil
        @unknown default: nil
        }
        let low = ProcessInfo.processInfo.isLowPowerModeEnabled
        Task {
            await session.updateLocalBattery(level: level, charging: charging, low: low)
        }
    }

    func onSceneBecameActive() {
        refreshClipboardState()
        loadSharedItem()
        session?.start()
        refreshBatteryState()
        if pendingShortcutSend {
            pendingShortcutSend = false
            Task {
                // Allow UIPasteboard access once foregrounded.
                try? await Task.sleep(nanoseconds: 150_000_000)
                await sendClipboardNow()
            }
        }
    }

    // MARK: - URL Scheme (`weft://send-clipboard` & `weft://pair?code=...`)

    func handleIncomingURL(_ url: URL) {
        guard url.scheme?.lowercased() == "weft" else { return }
        let host = url.host?.lowercased() ?? ""
        let path = url.path.lowercased()

        if host == "send-clipboard" || path == "/send-clipboard" {
            if UIApplication.shared.applicationState == .active {
                Task { await sendClipboardNow() }
            } else {
                pendingShortcutSend = true
            }
            return
        }

        if host == "pair" || path == "/pair",
           let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
           let code = components.queryItems?.first(where: { $0.name == "code" })?.value,
           !code.isEmpty {
            showingPairing = true
            Task {
                await session?.joinWithCode(code)
            }
        }
    }

    // MARK: - One-tap Clipboard Send (Text or PNG/JPEG Image)

    func loadSharedItem() {
        guard pendingShareFile == nil,
              let group = FileManager.default.containerURL(
                forSecurityApplicationGroupIdentifier: "group.eu.frohub.weft") else { return }
        let files = (try? FileManager.default.contentsOfDirectory(
            at: group.appendingPathComponent("ShareInbox"), includingPropertiesForKeys: nil)) ?? []
        pendingShareFile = files.sorted { $0.lastPathComponent < $1.lastPathComponent }.first
    }

    func sendSharedItem(to peerHex: String) async {
        guard let file = pendingShareFile, let session else { return }
        if file.pathExtension != "wefttext" {
            // sendFiles stages a private copy before returning, so the inbox copy
            // can be removed only after that handoff succeeds.
            // The extension queues `<UUID>_<name>`; offer the original name.
            if await session.sendFiles([file], to: peerHex, offeredName: {
                ShareInbox.originalName(of: $0.lastPathComponent)
            }) == 1 {
                try? FileManager.default.removeItem(at: file)
                pendingShareFile = nil
            } else { shareFailed = true }
            return
        }
        guard
              let data = try? Data(contentsOf: file), data.count <= 256 * 1024,
              let text = String(data: data, encoding: .utf8) else { return }
        let isLink = URL(string: text).map { ["http", "https"].contains($0.scheme?.lowercased() ?? "") } ?? false
        let sent: Bool
        if isLink { sent = await session.shareLink(text, to: peerHex) }
        else { sent = await session.sendClipboardText(text, to: peerHex) }
        if sent {
            try? FileManager.default.removeItem(at: file)
            pendingShareFile = nil
        } else {
            shareFailed = true
        }
    }

    /// Sends a payload read from the pasteboard by `sendClipboardNow`.
    func sendClipboard(_ payload: ClipboardPayload?, to peerHex: String? = nil) async {
        guard let session else { return }
        let ok: Bool
        switch payload {
        case .text(let text)?:
            ok = await session.sendClipboardText(text, to: peerHex)
        case .image(let data, let mime)?:
            ok = await session.sendClipboardImage(data: data, mime: mime, to: peerHex)
        case nil:
            UINotificationFeedbackGenerator().notificationOccurred(.error)
            return
        }
        if ok { UIImpactFeedbackGenerator(style: .light).impactOccurred() }
    }

    /// "Send Clipboard" row and `weft://send-clipboard`: reads the pasteboard
    /// directly, so iOS asks the user to allow pasting unless Settings › Weft ›
    /// Paste from Other Apps is set to Allow.
    func sendClipboardNow(to peerHex: String? = nil) async {
        let pb = UIPasteboard.general
        // Reading these flags never prompts.
        let hasContent = pb.hasStrings || pb.hasImages
        guard let payload = Self.pasteboardPayload() else {
            UINotificationFeedbackGenerator().notificationOccurred(.error)
            clipboardProblem = hasContent ? .denied : .empty
            return
        }
        await sendClipboard(payload, to: peerHex)
    }

    private static func pasteboardPayload() -> ClipboardPayload? {
        let pb = UIPasteboard.general

        // 1. Prefer non-empty text if present on the pasteboard.
        if pb.hasStrings, let text = pb.string, !text.isEmpty {
            return .text(text)
        }

        // 2. Otherwise inspect PNG / JPEG / UIImage on the pasteboard.
        if let pngData = pb.data(forPasteboardType: UTType.png.identifier),
           ClipboardImageValidator.canOffer(data: pngData, mime: "image/png") {
            return .image(pngData, mime: "image/png")
        }

        if let jpegData = pb.data(forPasteboardType: UTType.jpeg.identifier),
           ClipboardImageValidator.canOffer(data: jpegData, mime: "image/jpeg") {
            return .image(jpegData, mime: "image/jpeg")
        }

        if pb.hasImages, let image = pb.image {
            return ClipboardPayload.encoded(image)
        }
        return nil
    }

    /// Writes `text` to the local pasteboard marked local-only with a 5-minute expiry
    /// so pairing codes never sync over Universal Clipboard or Weft.
    func copyLocalSecret(_ text: String) {
        let expiry = Date().addingTimeInterval(300)
        UIPasteboard.general.setItems(
            [[UTType.utf8PlainText.identifier: text]],
            options: [
                .localOnly: true,
                .expirationDate: expiry,
            ]
        )
        lastWrittenPasteboardCount = UIPasteboard.general.changeCount
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
    }

    func foundRingingPhone() {
        alarmPlayer.stop()
        Task {
            await session?.dismissIncomingRing()
        }
    }
}

// MARK: - App Entry Point

@main
struct WeftiOSApp: App {
    @StateObject private var model = MobileAppModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            DevicesView(model: model)
                .tint(.weftOrange)
                .sheet(isPresented: Binding(
                    get: { model.pendingShareFile != nil },
                    set: { if !$0 { model.pendingShareFile = nil } }
                )) {
                    NavigationStack {
                        List(model.snapshot?.peers ?? []) { peer in
                            Button(peer.displayName) {
                                Task { await model.sendSharedItem(to: peer.id) }
                            }
                        }
                        .tint(.primary)
                        .navigationTitle(t("Send to a device", "Envoyer à un appareil"))
                        .overlay {
                            if (model.snapshot?.peers ?? []).isEmpty {
                                Text(t("Pair a device in Weft before sending this share.",
                                       "Associez un appareil dans Weft avant d’envoyer ce partage."))
                                    .padding()
                            }
                        }
                        .alert(t("Could not send. Retry when the device is connected.",
                                 "Envoi impossible. Réessayez quand l’appareil est connecté."),
                            isPresented: $model.shareFailed) { Button("OK", role: .cancel) {} }
                    }
                }
                .onAppear { model.loadSharedItem() }
                .onOpenURL { url in
                    model.handleIncomingURL(url)
                }
                .onChange(of: scenePhase) { _, phase in
                    if phase == .active {
                        model.onSceneBecameActive()
                    }
                }
        }
    }
}

#else

@main
enum WeftiOSHostStub {
    static func main() {}
}

#endif
