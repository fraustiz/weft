// Home screen: connection status pill, one-tap Send Clipboard row, paired
// devices list, active transfers, empty onboarding state, and incoming ring
// overlay.

#if canImport(UIKit)
import SwiftUI
import WeftMobileKit

struct DevicesView: View {
    @ObservedObject var model: MobileAppModel

    var body: some View {
        NavigationStack {
            Group {
                if let snapshot = model.snapshot {
                    if snapshot.peers.isEmpty {
                        EmptyDevicesState(model: model, snapshot: snapshot)
                    } else {
                        pairedList(snapshot)
                    }
                } else if let error = model.startupError {
                    ContentUnavailableView(
                        t("Weft could not start", "Weft n’a pas pu démarrer"),
                        systemImage: "exclamationmark.triangle",
                        description: Text(error)
                    )
                } else {
                    ProgressView()
                }
            }
            .navigationTitle("Weft")
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button {
                        model.showingSettings = true
                    } label: {
                        Image(systemName: "gearshape")
                            .accessibilityLabel(t("Settings", "Réglages"))
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        model.showingPairing = true
                    } label: {
                        Image(systemName: "plus")
                            .accessibilityLabel(t("Pair a device", "Associer un appareil"))
                    }
                }
            }
            .overlay {
                if let ringingName = model.snapshot?.ringingFromPeerName {
                    IncomingRingOverlay(peerName: ringingName) {
                        model.foundRingingPhone()
                    }
                }
            }
            .sheet(
                isPresented: $model.showingPairing,
                onDismiss: { model.session?.closePairing() }
            ) {
                PairingView(model: model)
                    .tint(.weftOrange)
            }
            .sheet(isPresented: $model.showingSettings) {
                SettingsView(model: model)
                    .tint(.weftOrange)
            }
            .sheet(
                isPresented: Binding(
                    get: { model.remoteControlPeerID != nil },
                    set: { shown in
                        if !shown { model.remoteControlPeerID = nil }
                    }
                )
            ) {
                if let peerID = model.remoteControlPeerID,
                   let peer = model.snapshot?.peer(peerID) {
                    RemotePadView(model: model, peer: peer)
                        .tint(.weftOrange)
                }
            }
        }
        // Outside the stack so a banner (file received, ping sent…) also
        // shows on a pushed device screen, not only on the list.
        .safeAreaInset(edge: .bottom) {
            if let banner = model.snapshot?.lastBanner {
                BannerPill(notice: banner) {
                    model.session?.clearBanner()
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 8)
            }
        }
    }

    private func pairedList(_ snapshot: MobileSnapshot) -> some View {
        List {
            Section {
                ConnectionStatusRow(snapshot: snapshot)
                SendClipboardRow(model: model, peerHex: nil)
            } footer: {
                Text(t(
                    "Sends copied text or a PNG/JPEG image to your group. Incoming copies update automatically while Weft is open.",
                    "Envoie le texte copié ou une image PNG/JPEG à votre groupe. Les copies reçues s’appliquent quand Weft est ouvert."
                ))
            }

            Section(t("Devices", "Appareils")) {
                ForEach(snapshot.peers) { peer in
                    NavigationLink {
                        DeviceDetailView(model: model, peerID: peer.id)
                    } label: {
                        PeerRow(peer: peer)
                    }
                }
            }

            let activeTransfers = snapshot.transfers.filter(\.isActive)
            if !activeTransfers.isEmpty {
                Section(t("Transfers", "Transferts")) {
                    ForEach(activeTransfers) { transfer in
                        TransferProgressRow(transfer: transfer) {
                            Task {
                                await model.session?.cancelTransfer(peerHex: transfer.peerHex, id: transfer.id)
                            }
                        }
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
    }
}

/// "Send Clipboard" row: the whole row is the button. It reads the pasteboard,
/// so iOS asks to allow pasting unless Weft is allowed in Settings; when it
/// sends nothing, an alert says why. `nil` peer sends to the whole group.
struct SendClipboardRow: View {
    @ObservedObject var model: MobileAppModel
    let peerHex: String?

    var body: some View {
        Button {
            Task { await model.sendClipboardNow(to: peerHex) }
        } label: {
            Label(t("Send Clipboard", "Envoyer le presse-papiers"), systemImage: "doc.on.clipboard")
                .labelStyle(WeftActionLabelStyle())
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
                // The plain style keeps explicit colors when disabled: dim them.
                .opacity(model.clipboardHasContent ? 1 : 0.35)
        }
        // Like the other action rows: neutral title, orange icon.
        .buttonStyle(.plain)
        .disabled(!model.clipboardHasContent)
        .accessibilityHint(model.clipboardHasContent ? "" : t("The clipboard is empty", "Le presse-papiers est vide"))
        .alert(item: $model.clipboardProblem) { problem in
            switch problem {
            case .empty:
                Alert(title: Text(t("Nothing to send", "Rien à envoyer")),
                      message: Text(t("Copy some text or a PNG or JPEG image first.",
                                      "Copiez d’abord du texte ou une image PNG ou JPEG.")))
            case .denied:
                Alert(title: Text(t("Pasting is not allowed", "Collage non autorisé")),
                      message: Text(t("In Settings › Weft › Paste from Other Apps, choose Allow so Weft can send your clipboard without asking.",
                                      "Dans Réglages › Weft › Coller depuis d’autres apps, choisissez Autoriser pour que Weft envoie le presse-papiers sans demander.")),
                      primaryButton: .default(Text(t("Open Settings", "Ouvrir Réglages"))) {
                          if let url = URL(string: UIApplication.openSettingsURLString) {
                              UIApplication.shared.open(url)
                          }
                      },
                      secondaryButton: .cancel(Text(t("Not Now", "Plus tard"))))
            }
        }
    }
}

struct ConnectionStatusRow: View {
    let snapshot: MobileSnapshot

    var body: some View {
        HStack(spacing: 10) {
            Circle()
                .fill(snapshot.connection.isConnected ? Color.weftOrange : Color.secondary.opacity(0.45))
                .frame(width: 8, height: 8)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(snapshot.connection.statusTitle)
                    .font(.subheadline.weight(.medium))
                Text(Words.relayHost(snapshot.relayURL))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 0)
            Text(Words.deviceCount(snapshot.peers.count))
                .font(.caption)
                .foregroundStyle(.secondary)
                .monospacedDigit()
        }
        .padding(.vertical, 2)
    }
}

private struct PeerRow: View {
    let peer: PeerSnapshot

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: peer.symbol)
                .font(.system(size: 18, weight: .regular))
                .foregroundStyle(.secondary)
                .frame(width: 28, alignment: .center)
            VStack(alignment: .leading, spacing: 2) {
                Text(peer.displayName)
                    .font(.body)
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            Spacer(minLength: 0)
            if let player = peer.currentPlayer, player.playing {
                Image(systemName: "waveform")
                    .font(.caption)
                    .foregroundStyle(Color.weftOrange)
                    .accessibilityLabel(t("Playing media", "Lecture en cours"))
            }
        }
        .padding(.vertical, 2)
    }

    private var subtitle: String {
        var parts: [String] = [peer.platformName ?? peer.shortID]
        if let bat = peer.batteryText {
            parts.append(bat)
        }
        if let net = peer.networkText {
            parts.append(net)
        }
        return parts.joined(separator: " · ")
    }
}

struct TransferProgressRow: View {
    let transfer: TransferProgress
    let onCancel: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: transfer.isOutgoing ? "arrow.up.circle" : "arrow.down.circle")
                .font(.system(size: 18))
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 4) {
                Text(transfer.name)
                    .font(.subheadline)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if transfer.isActive {
                    ProgressView(value: transfer.fractionCompleted)
                }
                Text(statusCaption)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            Spacer(minLength: 0)
            if transfer.isActive {
                Button {
                    onCancel()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.borderless)
                .accessibilityLabel(t("Cancel transfer", "Annuler le transfert"))
            }
        }
        .padding(.vertical, 2)
    }

    private var statusCaption: String {
        switch transfer.status {
        case .offered:
            return t("Waiting for acceptance…", "En attente d’acceptation…")
        case .transferring:
            return "\(Words.size(transfer.transferred)) / \(Words.size(transfer.size))"
        case .completed:
            return t("Completed", "Terminé")
        case .rejected(let reason):
            return reason.map { "\(t("Refused", "Refusé")) (\($0))" } ?? t("Refused", "Refusé")
        case .cancelled:
            return t("Cancelled", "Annulé")
        case .failed(let why):
            return why
        }
    }
}

private struct EmptyDevicesState: View {
    @ObservedObject var model: MobileAppModel
    let snapshot: MobileSnapshot

    var body: some View {
        VStack(spacing: 0) {
            Spacer()
            BrandMark()
                .frame(width: 64, height: 64)
                .padding(.bottom, 20)
            Text(t("Pair your first device", "Associez un premier appareil"))
                .font(.title2.weight(.medium))
                .padding(.bottom, 8)
            Text(t(
                "Share your clipboard, files, links and media controls with your Mac, Windows PC or Android phone, encrypted end to end.",
                "Partagez votre presse-papiers, vos fichiers, vos liens et vos contrôles multimédia avec votre Mac, PC Windows ou téléphone Android, chiffrés de bout en bout."
            ))
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: 320)
            .padding(.bottom, 24)

            Button(t("Pair a Device", "Associer un appareil")) {
                model.showingPairing = true
            }
            .buttonStyle(WeftProminentButtonStyle(large: true))

            Spacer()

            HStack(spacing: 8) {
                Circle()
                    .fill(snapshot.connection.isConnected ? Color.weftOrange : Color.secondary.opacity(0.45))
                    .frame(width: 8, height: 8)
                Text("\(snapshot.connection.statusTitle) · \(Words.relayHost(snapshot.relayURL))")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .padding(.bottom, 16)
        }
        .padding(.horizontal, 24)
    }
}

private struct BannerPill: View {
    let notice: BannerNotice
    let onDismiss: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Circle()
                .fill(Color.weftOrange)
                .frame(width: 6, height: 6)
            Text(notice.text)
                .font(.footnote)
                .lineLimit(2)
            Spacer(minLength: 0)
            Button {
                onDismiss()
            } label: {
                Image(systemName: "xmark")
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(Color(uiColor: .secondarySystemGroupedBackground))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
        )
    }
}

private struct IncomingRingOverlay: View {
    let peerName: String
    let onFoundIt: () -> Void

    var body: some View {
        ZStack {
            Color.black.opacity(0.45)
                .ignoresSafeArea()
            VStack(spacing: 16) {
                Image(systemName: "bell.and.waves.left.and.right.fill")
                    .font(.system(size: 36, weight: .regular))
                    .foregroundStyle(Color.weftOrange)
                    .accessibilityHidden(true)
                VStack(spacing: 6) {
                    Text(Language.current == .french
                         ? "\(peerName) fait sonner cet iPhone"
                         : "\(peerName) is ringing this iPhone")
                        .font(.headline)
                        .multilineTextAlignment(.center)
                    Text(t("Stops automatically after one minute.",
                           "S’arrête automatiquement au bout d’une minute."))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                }
                Button(t("Found It", "Trouvé")) {
                    onFoundIt()
                }
                .buttonStyle(WeftProminentButtonStyle(fullWidth: true, large: true))
            }
            .padding(24)
            .frame(maxWidth: 320)
            .background(
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .fill(Color(uiColor: .systemBackground))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
            )
            .padding(24)
        }
    }
}
#endif
