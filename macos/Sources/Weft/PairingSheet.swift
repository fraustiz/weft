// The pairing sheet. This Mac owns the pairing slot: it shows the code as
// a QR code for a phone to scan (docs/protocol.md, section 5), with the
// relay it belongs to and how long it lasts. A code from another device
// can be entered instead.

import AppKit
import SwiftUI
import WeftFFI
import WeftKit

struct PairingSheet: View {
    @ObservedObject var model: AppModel
    @StateObject private var enteringCode = Local(false)
    @StateObject private var typedCode = Local("")

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                BrandMark().frame(width: 20, height: 20)
                Text(t("Pair a Device", "Associer un appareil")).font(.headline)
            }
            .padding(.bottom, 16)
            content
        }
        .padding(24)
        .frame(width: 400)
    }

    @ViewBuilder
    private var content: some View {
        switch model.snapshot?.pairing ?? .idle {
        case .showingCode(let code, let expiresAtMs):
            showing(code: code, expiresAt: Date(timeIntervalSince1970: TimeInterval(expiresAtMs) / 1000))
        case .joining:
            waiting(t("Waiting for the other device to answer…",
                      "En attente de la réponse de l’autre appareil…"))
        case .paired(let deviceId):
            paired(model.snapshot?.device(deviceId))
        case .failed(let failure):
            failed(failure)
        case .idle:
            waiting(t("Getting a code…", "Création d’un code…"))
        }
    }

    private func showing(code: String, expiresAt: Date) -> some View {
        VStack(spacing: 0) {
            Text(t(
                "Scan this code with Weft on your phone.",
                "Scannez ce code avec Weft sur votre téléphone."
            ))
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .padding(.bottom, 16)
            QRCard(code: code)
                .padding(.bottom, 12)
            TimelineView(.periodic(from: .now, by: 1)) { context in
                let left = Int(expiresAt.timeIntervalSince(context.date).rounded(.up))
                HStack(spacing: 6) {
                    Text(Language.current == .french
                         ? "Expire dans \(Words.countdown(secondsLeft: left))"
                         : "Expires in \(Words.countdown(secondsLeft: left))")
                        .monospacedDigit()
                    Text("·")
                    let relay = model.snapshot?.relayUrl ?? ""
                    // A padlock only for a TLS relay: ws:// is for a relay
                    // on this Mac, and is not encrypted on the way.
                    Label(Words.relayHost(relay),
                          systemImage: relay.hasPrefix("wss://") ? "lock.fill" : "network")
                        .labelStyle(.titleAndIcon)
                }
                .font(.callout)
                .foregroundStyle(.secondary)
            }
            .padding(.bottom, 20)
            joinField
            HStack {
                Button(t("Copy Code", "Copier le code")) { copy(code) }
                    .help(t("For a computer without a camera", "Pour un ordinateur sans caméra"))
                Spacer()
                Button(t("Cancel", "Annuler"), action: model.closePairing)
                    .keyboardShortcut(.cancelAction)
            }
            .padding(.top, 20)
        }
    }

    /// The second way in: the code another device shows.
    private var joinField: some View {
        DisclosureGroup(
            t("Enter a code from another device", "Saisir le code d’un autre appareil"),
            isExpanded: $enteringCode.value
        ) {
            HStack {
                TextField(t("Code", "Code"), text: $typedCode.value, prompt: Text("WEFT1:…"))
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(join)
                Button(t("Join", "Rejoindre"), action: join)
                    .disabled(typedCode.value.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            .padding(.top, 8)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func waiting(_ text: String) -> some View {
        VStack(spacing: 16) {
            ProgressView().controlSize(.small)
            Text(text).foregroundStyle(.secondary)
            Button(t("Cancel", "Annuler"), action: model.closePairing)
                .keyboardShortcut(.cancelAction)
        }
        .frame(maxWidth: .infinity, minHeight: 200)
    }

    private func paired(_ device: DeviceInfo?) -> some View {
        VStack(spacing: 12) {
            Image(systemName: "checkmark.circle")
                .font(.system(size: 40, weight: .light))
                .foregroundStyle(.green)
            Text(device.map {
                Language.current == .french ? "Associé à \($0.displayName)" : "Paired with \($0.displayName)"
            } ?? t("Paired with a new device", "Associé à un nouvel appareil"))
            .font(.title3)
            Text(t(
                "What you copy on one now reaches the other.",
                "Ce que vous copiez sur l’un arrive maintenant sur l’autre."
            ))
            .foregroundStyle(.secondary)
            Button(t("Done", "Terminé"), action: model.closePairing)
                .keyboardShortcut(.defaultAction)
                .padding(.top, 8)
        }
        .frame(maxWidth: .infinity, minHeight: 200)
    }

    private func failed(_ failure: PairingFailureKind) -> some View {
        VStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle")
                .font(.system(size: 32, weight: .light))
                .foregroundStyle(.secondary)
            Text(t("Pairing failed", "L’association a échoué")).font(.title3)
            Text(failure.text)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                Button(t("Cancel", "Annuler"), action: model.closePairing)
                    .keyboardShortcut(.cancelAction)
                Button(t("Try Again", "Réessayer"), action: model.pair)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.top, 8)
        }
        .frame(maxWidth: .infinity, minHeight: 200)
    }

    private func join() {
        let code = typedCode.value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty else { return }
        typedCode.value = ""
        model.join(code: code)
    }

    /// Copies the code for this Mac only: marked concealed, so neither
    /// Weft nor clipboard managers send it anywhere.
    private func copy(_ code: String) {
        _ = MacPasteboard().writeText(text: code, private: true)
    }
}

/// The code as a QR code: black on white whatever the appearance, since a
/// camera reads dark modules on a light ground, at whole pixels per module.
private struct QRCard: View {
    let code: String
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        let side: CGFloat = 216
        Group {
            if let qr = QRCode(code), let image = qr.image(scale: pixelsPerModule(qr, side: side)) {
                // At its own size: every module is the same whole number
                // of pixels.
                Image(decorative: image, scale: displayScale)
                    .interpolation(.none)
            } else {
                Color.white.frame(width: side, height: side)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(Color.primary.opacity(0.1))
        )
        .accessibilityElement()
        .accessibilityLabel(t("Pairing QR code", "QR code d’association"))
    }

    /// The most whole pixels per module that fit in `side` points.
    private func pixelsPerModule(_ qr: QRCode, side: CGFloat) -> Int {
        let modules = qr.size + 2 * QRCode.quietZone
        return max(1, Int(side * displayScale) / modules)
    }
}
