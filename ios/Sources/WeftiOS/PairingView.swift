// Pairing sheet (`docs/protocol.md` §5): camera QR scanner (`AVCaptureSession`
// `.qr`) + paste/enter `WEFT1:` code (supporting both joining and inviting into
// an existing group), and slot-owner QR display (`CIQRCodeGenerator` crisp
// nearest-neighbor) + local-only Copy Code, with distinct group-join vs paired
// confirmation copy.

#if canImport(UIKit)
import AVFoundation
import SwiftUI
import UIKit
import WeftMobileKit

private enum PairingModeTab: String, CaseIterable {
    case scan
    case showCode
}

private enum ScanIntent: String, CaseIterable {
    case inviteToMyGroup
    case joinTheirGroup
}

struct PairingView: View {
    @ObservedObject var model: MobileAppModel

    @StateObject private var mode = Local<PairingModeTab>(.scan)
    @StateObject private var scanIntent = Local<ScanIntent>(.inviteToMyGroup)
    @StateObject private var typedCode = Local("")
    @StateObject private var copiedCode = Local(false)
    @StateObject private var cameraState = CameraPermissionModel()
    @Environment(\.dismiss) private var dismiss

    private var alreadyInGroup: Bool {
        !(model.snapshot?.peers.isEmpty ?? true)
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                switch model.snapshot?.pairing ?? .idle {
                case .joining:
                    waitingState(t("Joining group…", "Connexion au groupe…"))

                case .inviting:
                    waitingState(t("Waiting for the other device to accept…",
                                   "En attente de la réponse de l’autre appareil…"))

                case .paired(let deviceIDHex, let invited):
                    pairedConfirmation(deviceIDHex: deviceIDHex, invited: invited)

                case .failed(let failure):
                    failedState(failure)

                case .idle, .openingSlot, .showingCode:
                    activePairingTabs
                }
            }
            .background(Color(uiColor: .systemGroupedBackground).ignoresSafeArea())
            .navigationTitle(t("Pair a Device", "Associer un appareil"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(t("Close", "Fermer")) {
                        model.session?.closePairing()
                        dismiss()
                    }
                    .tint(.primary)
                }
            }
            .onAppear {
                cameraState.checkAndRequestIfNeeded()
                if !alreadyInGroup {
                    scanIntent.value = .joinTheirGroup
                }
            }
            .onChange(of: mode.value) { _, selected in
                if selected == .showCode {
                    Task { await model.session?.openPairingSlot() }
                } else {
                    model.session?.closePairing()
                }
            }
        }
    }

    private var activePairingTabs: some View {
        ScrollView {
            VStack(spacing: 20) {
                Picker(t("Method", "Méthode"), selection: $mode.value) {
                    Text(t("Scan Code", "Scanner un code")).tag(PairingModeTab.scan)
                    Text(t("My QR Code", "Mon QR code")).tag(PairingModeTab.showCode)
                }
                .pickerStyle(.segmented)

                if mode.value == .scan {
                    scanTabContent
                } else {
                    showCodeTabContent
                }
            }
            .padding(20)
        }
        .scrollDismissesKeyboard(.interactively)
    }

    // MARK: - Tab 1: Camera QR Scanner + Paste / Enter Code

    private var scanTabContent: some View {
        VStack(alignment: .leading, spacing: 16) {
            if alreadyInGroup {
                VStack(alignment: .leading, spacing: 6) {
                    Picker(t("Action", "Action"), selection: $scanIntent.value) {
                        Text(t("Add to My Group", "Ajouter à mon groupe"))
                            .tag(ScanIntent.inviteToMyGroup)
                        Text(t("Join Their Group", "Rejoindre son groupe"))
                            .tag(ScanIntent.joinTheirGroup)
                    }
                    .pickerStyle(.segmented)

                    Text(scanIntent.value == .inviteToMyGroup
                         ? t("Keeps your existing devices and invites the new computer in.",
                             "Conserve vos appareils actuels et invite le nouvel ordinateur.")
                         : t("Leaves your current group on this iPhone to join the other device’s group.",
                             "Quitte votre groupe actuel sur cet iPhone pour rejoindre celui de l’autre appareil."))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            cameraViewport

            VStack(alignment: .leading, spacing: 8) {
                Text(t("Or enter a WEFT1 code", "Ou saisissez un code WEFT1"))
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.secondary)

                HStack(spacing: 8) {
                    TextField(t("Code", "Code"), text: $typedCode.value, prompt: Text("WEFT1:…"))
                        .font(.subheadline.monospaced())
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                        .background(
                            RoundedRectangle(cornerRadius: 10, style: .continuous)
                                .fill(Color(uiColor: .secondarySystemGroupedBackground))
                        )
                        .submitLabel(.join)
                        .onSubmit {
                            submitCode(typedCode.value)
                        }

                    PasteButton(payloadType: String.self) { [typedCode] strings in
                        let clip = strings.first?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                        guard !clip.isEmpty else { return }
                        Task { @MainActor in typedCode.value = clip }
                    }
                    .labelStyle(.iconOnly)
                    .buttonBorderShape(.roundedRectangle(radius: 10))
                    .tint(.weftOrange)
                }

                Button(alreadyInGroup && scanIntent.value == .inviteToMyGroup
                       ? t("Add Device to Group", "Ajouter l’appareil au groupe")
                       : t("Join with Code", "Rejoindre avec ce code")) {
                    submitCode(typedCode.value)
                }
                .buttonStyle(WeftProminentButtonStyle(fullWidth: true, large: true))
                .disabled(typedCode.value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
    }

    @ViewBuilder
    private var cameraViewport: some View {
        let shape = RoundedRectangle(cornerRadius: 16, style: .continuous)
        ZStack {
            shape.fill(Color(uiColor: .secondarySystemGroupedBackground))
            switch cameraState.status {
            case .authorized:
                QRScannerRepresentable { scanned in
                    UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                    submitCode(scanned)
                }
            case .denied:
                VStack(spacing: 10) {
                    Image(systemName: "camera.fill")
                        .font(.system(size: 26))
                        .foregroundStyle(.secondary)
                    Text(t("Camera access is turned off", "L’accès à la caméra est désactivé"))
                        .font(.subheadline.weight(.medium))
                    Text(t("Enable Camera in Settings, or paste a WEFT1 code below.",
                           "Activez Caméra dans Réglages, ou collez un code WEFT1 ci-dessous."))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                }
                .padding(24)
            case .unavailable:
                VStack(spacing: 10) {
                    Image(systemName: "qrcode.viewfinder")
                        .font(.system(size: 28))
                        .foregroundStyle(.secondary)
                    Text(t("Point your camera at another device’s QR code, or paste its WEFT1 code below.",
                           "Pointez la caméra vers le QR code d’un autre appareil, ou collez son code WEFT1 ci-dessous."))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                }
                .padding(24)
            }
        }
        .frame(height: 240)
        .clipShape(shape)
        .overlay(shape.strokeBorder(Color.primary.opacity(0.08), lineWidth: 1))
    }

    private func submitCode(_ raw: String) {
        let clean = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return }
        typedCode.value = ""
        Task {
            if alreadyInGroup && scanIntent.value == .inviteToMyGroup {
                await model.session?.inviteWithCode(clean)
            } else {
                await model.session?.joinWithCode(clean)
            }
        }
    }

    // MARK: - Tab 2: Show QR Code (`0xB0` Slot Owner)

    @ViewBuilder
    private var showCodeTabContent: some View {
        switch model.snapshot?.pairing ?? .idle {
        case .showingCode(let code, let expiresAtMs):
            let expiresAt = Date(timeIntervalSince1970: TimeInterval(expiresAtMs) / 1000.0)
            VStack(spacing: 16) {
                Text(t(
                    "Scan this code from Weft on your computer or another phone.",
                    "Scannez ce code depuis Weft sur votre ordinateur ou un autre téléphone."
                ))
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

                CrispQRCard(code: code)

                TimelineView(.periodic(from: .now, by: 1.0)) { context in
                    let left = Int(expiresAt.timeIntervalSince(context.date).rounded(.up))
                    let relay = model.snapshot?.relayURL ?? ""
                    HStack(spacing: 6) {
                        Text(Language.current == .french
                             ? "Expire dans \(Words.countdown(secondsLeft: left))"
                             : "Expires in \(Words.countdown(secondsLeft: left))")
                            .monospacedDigit()
                        Text("·")
                        Label(
                            Words.relayHost(relay),
                            systemImage: relay.hasPrefix("wss://") ? "lock.fill" : "network"
                        )
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }

                Button(copiedCode.value
                       ? t("Copied Locally", "Copié sur cet iPhone")
                       : t("Copy WEFT1 Code", "Copier le code WEFT1")) {
                    model.copyLocalSecret(code)
                    copiedCode.value = true
                }
                .buttonStyle(WeftProminentButtonStyle(fullWidth: true, large: true))
            }
            .padding(.top, 4)

        default:
            VStack(spacing: 14) {
                ProgressView()
                Text(t("Opening pairing slot…", "Ouverture d’un créneau d’association…"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, minHeight: 220)
            .onAppear {
                if model.snapshot?.pairing == .idle {
                    Task { await model.session?.openPairingSlot() }
                }
            }
        }
    }

    // MARK: - Waiting, Paired & Failed Views

    private func waitingState(_ label: String) -> some View {
        VStack(spacing: 16) {
            Spacer()
            ProgressView()
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            Button(t("Cancel", "Annuler")) {
                model.session?.closePairing()
            }
            .tint(.primary)
            Spacer()
        }
        .padding(24)
    }

    private func pairedConfirmation(deviceIDHex: String, invited: Bool) -> some View {
        let peer = model.snapshot?.peer(deviceIDHex)
        let title: String = if invited {
            if let peer {
                Language.current == .french
                    ? "Ajouté au groupe de \(peer.displayName)"
                    : "Joined \(peer.displayName)’s group"
            } else {
                t("Joined the group", "Ajouté au groupe")
            }
        } else {
            if let peer {
                Language.current == .french
                    ? "Associé à \(peer.displayName)"
                    : "Paired with \(peer.displayName)"
            } else {
                t("Paired with new device", "Associé au nouvel appareil")
            }
        }

        return VStack(spacing: 16) {
            Spacer()
            Image(systemName: "checkmark.circle")
                .font(.system(size: 44, weight: .light))
                .foregroundStyle(.green)
            Text(title)
                .font(.title3.weight(.medium))
                .multilineTextAlignment(.center)
            Text(t(
                "Clipboard, files and media controls now travel encrypted end to end across your group.",
                "Le presse-papiers, les fichiers et les contrôles multimédia circulent désormais chiffrés de bout en bout dans votre groupe."
            ))
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: 300)

            Button(t("Done", "Terminé")) {
                model.session?.closePairing()
                dismiss()
            }
            .buttonStyle(WeftProminentButtonStyle(large: true))
            .padding(.top, 8)
            Spacer()
        }
        .padding(24)
    }

    private func failedState(_ failure: PairingFailure) -> some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "exclamationmark.triangle")
                .font(.system(size: 36, weight: .light))
                .foregroundStyle(.secondary)
            Text(t("Pairing failed", "L’association a échoué"))
                .font(.title3.weight(.medium))
            Text(failure.text)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 310)
            Button(t("Try Again", "Réessayer")) {
                model.session?.closePairing()
                if mode.value == .showCode {
                    Task { await model.session?.openPairingSlot() }
                }
            }
            .buttonStyle(WeftProminentButtonStyle(large: true))
            Spacer()
        }
        .padding(24)
    }
}

// MARK: - Crisp Nearest-Neighbor QR Card

private struct CrispQRCard: View {
    let code: String
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        let side: CGFloat = 224
        Group {
            if let qr = QRCode(code),
               let cgImage = qr.image(scale: pixelsPerModule(qr, side: side)) {
                Image(decorative: cgImage, scale: displayScale)
                    .interpolation(.none)
            } else {
                Color.white.frame(width: side, height: side)
            }
        }
        .padding(12)
        .background(
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .fill(Color.white)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .strokeBorder(Color.black.opacity(0.08), lineWidth: 1)
        )
        .accessibilityElement()
        .accessibilityLabel(t("Pairing QR code", "QR code d’association"))
    }

    private func pixelsPerModule(_ qr: QRCode, side: CGFloat) -> Int {
        let modules = qr.size + 2 * QRCode.quietZone
        return max(1, Int(side * displayScale) / modules)
    }
}

// MARK: - Camera Permission & AVCaptureSession `.qr` Scanner

@MainActor
private final class CameraPermissionModel: ObservableObject {
    enum Status {
        case unavailable
        case authorized
        case denied
    }

    @Published var status: Status = .unavailable

    func checkAndRequestIfNeeded() {
        guard AVCaptureDevice.default(for: .video) != nil else {
            status = .unavailable
            return
        }
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            status = .authorized
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { [weak self] granted in
                Task { @MainActor in
                    self?.status = granted ? .authorized : .denied
                }
            }
        case .denied, .restricted:
            status = .denied
        @unknown default:
            status = .unavailable
        }
    }
}

private struct QRScannerRepresentable: UIViewRepresentable {
    let onDetected: @MainActor (String) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(onDetected: onDetected)
    }

    func makeUIView(context: Context) -> ScannerPreviewUIView {
        let view = ScannerPreviewUIView()
        context.coordinator.attach(to: view)
        return view
    }

    func updateUIView(_ uiView: ScannerPreviewUIView, context: Context) {}

    static func dismantleUIView(_ uiView: ScannerPreviewUIView, coordinator: Coordinator) {
        coordinator.stop()
    }

    final class Coordinator: NSObject, AVCaptureMetadataOutputObjectsDelegate, @unchecked Sendable {
        private let onDetected: @MainActor (String) -> Void
        private let session = AVCaptureSession()
        private let queue = DispatchQueue(label: "fr.frohub.weft.ios.qr")
        private let lock = NSLock()
        private var hasDelivered = false

        init(onDetected: @escaping @MainActor (String) -> Void) {
            self.onDetected = onDetected
        }

        @MainActor
        func attach(to view: ScannerPreviewUIView) {
            view.previewLayer.session = session
            view.previewLayer.videoGravity = .resizeAspectFill
            let captureSession = session
            queue.async { [weak self] in
                guard let self else { return }
                guard captureSession.inputs.isEmpty,
                      let device = AVCaptureDevice.default(for: .video),
                      let input = try? AVCaptureDeviceInput(device: device),
                      captureSession.canAddInput(input) else {
                    return
                }
                captureSession.beginConfiguration()
                captureSession.addInput(input)
                let output = AVCaptureMetadataOutput()
                if captureSession.canAddOutput(output) {
                    captureSession.addOutput(output)
                    output.setMetadataObjectsDelegate(self, queue: self.queue)
                    if output.availableMetadataObjectTypes.contains(.qr) {
                        output.metadataObjectTypes = [.qr]
                    }
                }
                captureSession.commitConfiguration()
                captureSession.startRunning()
            }
        }

        func stop() {
            let captureSession = session
            queue.async {
                if captureSession.isRunning {
                    captureSession.stopRunning()
                }
            }
        }

        nonisolated func metadataOutput(
            _ output: AVCaptureMetadataOutput,
            didOutput metadataObjects: [AVMetadataObject],
            from connection: AVCaptureConnection
        ) {
            guard let match = metadataObjects
                .compactMap({ ($0 as? AVMetadataMachineReadableCodeObject)?.stringValue })
                .map({ $0.trimmingCharacters(in: .whitespacesAndNewlines) })
                .first(where: { $0.hasPrefix("WEFT1:") }) else {
                return
            }

            lock.lock()
            let already = hasDelivered
            hasDelivered = true
            lock.unlock()
            guard !already else { return }

            session.stopRunning()
            let callback = onDetected
            Task { @MainActor in
                callback(match)
            }
        }
    }
}

private final class ScannerPreviewUIView: UIView {
    override class var layerClass: AnyClass {
        AVCaptureVideoPreviewLayer.self
    }

    var previewLayer: AVCaptureVideoPreviewLayer {
        layer as! AVCaptureVideoPreviewLayer
    }
}
#endif
