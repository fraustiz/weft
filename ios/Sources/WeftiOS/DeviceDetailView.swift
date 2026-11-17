// Single-device detail screen: battery & network status, clipboard/file/link
// sharing, ring & ping, computer Now Playing card with cover art and seek
// slider, volume slider, remote control launcher, and computer shortcuts.

#if canImport(UIKit)
import SwiftUI
import UIKit
import UniformTypeIdentifiers
import WeftMobileKit

struct DeviceDetailView: View {
    @ObservedObject var model: MobileAppModel
    let peerID: String

    @StateObject private var pickingFiles = Local(false)
    @StateObject private var confirmingRemoval = Local(false)
    @Environment(\.dismiss) private var dismiss

    private var peer: PeerSnapshot? {
        model.snapshot?.peer(peerID)
    }

    var body: some View {
        Group {
            if let peer, let snapshot = model.snapshot {
                detailForm(peer: peer, snapshot: snapshot)
            } else {
                ContentUnavailableView(
                    t("Device removed", "Appareil retiré"),
                    systemImage: "iphone.slash"
                )
            }
        }
        .fileImporter(
            isPresented: $pickingFiles.value,
            allowedContentTypes: [.item],
            allowsMultipleSelection: true
        ) { result in
            if case .success(let urls) = result, !urls.isEmpty {
                Task {
                    _ = await model.session?.sendFiles(urls, to: peerID)
                }
            }
        }
    }

    private func detailForm(peer: PeerSnapshot, snapshot: MobileSnapshot) -> some View {
        let features = snapshot.features
        let takesFiles = features.files && (peer.platform == nil || peer.handles("file.offer"))
        let takesRing = features.ring && peer.handles("ring")
        let takesPing = features.ping && peer.handles("ping")
        let takesRemote = features.remote && (
            peer.handles("input.pointer") || peer.handles("presenter") || peer.handles("input.key")
        )

        return Form {
            Section {
                PeerHeaderCard(peer: peer)
            }

            Section(t("Actions", "Actions")) {
                Button {
                    Task { await model.sendClipboardNow(to: peer.id) }
                } label: {
                    Label(t("Send Clipboard", "Envoyer le presse-papiers"), systemImage: "doc.on.clipboard")
                }

                if takesFiles {
                    Button {
                        pickingFiles.value = true
                    } label: {
                        Label(t("Send File…", "Envoyer un fichier…"), systemImage: "arrow.up.doc")
                    }
                }

                if takesRemote {
                    Button {
                        model.remoteControlPeerID = peer.id
                    } label: {
                        Label(t("Remote Control…", "Contrôle à distance…"), systemImage: "hand.point.up.left")
                    }
                }

                if takesRing {
                    if snapshot.outgoingRingingPeerHex == peer.id {
                        Button {
                            Task { await model.session?.stopRingingPeer(peer.id) }
                        } label: {
                            Label(t("Stop Ringing", "Arrêter la sonnerie"), systemImage: "bell.slash")
                        }
                    } else {
                        Button {
                            Task { await model.session?.ringPeer(peer.id) }
                        } label: {
                            Label(t("Ring Device", "Faire sonner"), systemImage: "bell")
                        }
                    }
                }

                if takesPing {
                    Button {
                        Task { await model.session?.pingPeer(peer.id) }
                    } label: {
                        Label(t("Ping", "Ping"), systemImage: "wave.3.right")
                    }
                }
            }

            if features.links && (peer.platform == nil || peer.handles("share.url")) {
                ShareLinkSection(model: model, peer: peer)
            }

            if features.media && peer.sends("media") {
                PeerMediaSection(model: model, peer: peer)
            }

            if features.volume && peer.handles("volume.set"), let vol = peer.volume, vol.level != nil {
                PeerVolumeSection(model: model, peer: peer, volume: vol)
            }

            if features.commands && !peer.commands.isEmpty {
                PeerShortcutsSection(model: model, peer: peer)
            }

            let peerTransfers = snapshot.transfers.filter { $0.peerHex == peer.id }
            if !peerTransfers.isEmpty {
                Section(t("Files", "Fichiers")) {
                    ForEach(peerTransfers) { transfer in
                        TransferProgressRow(transfer: transfer) {
                            Task {
                                await model.session?.cancelTransfer(peerHex: transfer.peerHex, id: transfer.id)
                            }
                        }
                    }
                }
            }

            Section {
                LabeledContent(t("Device ID", "Identifiant")) {
                    Text(peer.shortID)
                        .font(.body.monospaced())
                        .textSelection(.enabled)
                }
                Button(t("Remove from Group…", "Retirer du groupe…"), role: .destructive) {
                    confirmingRemoval.value = true
                }
            } footer: {
                Text(t(
                    "Removing this device stops encrypted sharing with the entire group until paired again.",
                    "Retirer cet appareil interrompt le partage chiffré avec tout le groupe jusqu’à une nouvelle association."
                ))
            }
        }
        .navigationTitle(peer.displayName)
        .navigationBarTitleDisplayMode(.inline)
        .confirmationDialog(
            Language.current == .french
                ? "Retirer « \(peer.displayName) » du groupe ?"
                : "Remove “\(peer.displayName)” from the group?",
            isPresented: $confirmingRemoval.value
        ) {
            Button(t("Remove", "Retirer"), role: .destructive) {
                Task {
                    let removed = await model.session?.removePeer(peer.id) ?? false
                    if removed { dismiss() }
                }
            }
        } message: {
            Text(t(
                "The other devices in the group stop trusting it too.",
                "Les autres appareils du groupe cessent aussi de lui faire confiance."
            ))
        }
    }
}

private struct PeerHeaderCard: View {
    let peer: PeerSnapshot

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: peer.symbol)
                .font(.system(size: 26, weight: .light))
                .foregroundStyle(.secondary)
                .frame(width: 40)
            VStack(alignment: .leading, spacing: 4) {
                Text(peer.displayName)
                    .font(.title3.weight(.medium))
                HStack(spacing: 12) {
                    Text(peer.platformName ?? t("Waiting for status", "En attente d’état"))
                    if let bat = peer.battery, let level = bat.level, let text = peer.batteryText {
                        Label(
                            text,
                            systemImage: Words.batterySymbol(level: level, charging: bat.charging == true)
                        )
                        .monospacedDigit()
                    }
                    if let netText = peer.networkText {
                        Label {
                            Text(netText)
                        } icon: {
                            Image(
                                systemName: Words.networkSymbol(peer.connectivity?.network),
                                variableValue: Words.signalFraction(peer.connectivity?.signal)
                            )
                        }
                    }
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 4)
    }
}

private struct ShareLinkSection: View {
    @ObservedObject var model: MobileAppModel
    let peer: PeerSnapshot
    @StateObject private var typedLink = Local("")
    @StateObject private var justSent = Local(false)

    private var isValid: Bool {
        Links.isShareable(typedLink.value.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    var body: some View {
        Section {
            HStack(spacing: 8) {
                TextField(t("Link", "Lien"), text: $typedLink.value, prompt: Text("https://…"))
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .onSubmit { sendLink() }
                    .onChange(of: typedLink.value) { _, _ in
                        justSent.value = false
                    }
                Button(justSent.value ? t("Sent", "Envoyé") : t("Send", "Envoyer")) {
                    sendLink()
                }
                .buttonStyle(WeftProminentButtonStyle())
                .disabled(!isValid || justSent.value)
            }
        } header: {
            Text(t("Share Link", "Partager un lien"))
        } footer: {
            Text(Language.current == .french
                 ? "Ouvre un lien http ou https sur \(peer.displayName)."
                 : "Opens an http or https link on \(peer.displayName).")
        }
        .onAppear {
            if typedLink.value.isEmpty,
               UIPasteboard.general.hasURLs || UIPasteboard.general.hasStrings,
               let candidate = Links.candidate(fromClipboard: UIPasteboard.general.string) {
                typedLink.value = candidate
            }
        }
    }

    private func sendLink() {
        let url = typedLink.value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard Links.isShareable(url) else { return }
        Task {
            let ok = await model.session?.shareLink(url, to: peer.id) ?? false
            if ok {
                justSent.value = true
            }
        }
    }
}

private struct PeerMediaSection: View {
    @ObservedObject var model: MobileAppModel
    let peer: PeerSnapshot
    @StateObject private var seekingMs = Local<Double?>(nil)

    var body: some View {
        Section(t("Now Playing", "En lecture")) {
            if let player = peer.currentPlayer {
                VStack(alignment: .leading, spacing: 12) {
                    HStack(spacing: 12) {
                        CoverArtTile(data: player.art.flatMap { peer.covers[$0] })
                        VStack(alignment: .leading, spacing: 2) {
                            if !player.app.isEmpty {
                                Text(player.app)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Text(player.title.isEmpty ? t("Untitled", "Sans titre") : player.title)
                                .font(.headline)
                                .lineLimit(1)
                            if !player.artist.isEmpty {
                                Text(player.artist)
                                    .font(.subheadline)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                        }
                        Spacer(minLength: 0)
                    }

                    if player.canSeek, let length = player.length, length > 0 {
                        TimelineView(.periodic(from: .now, by: 1.0)) { context in
                            seekSlider(player: player, lengthMs: length, now: context.date)
                        }
                    }

                    HStack(spacing: 28) {
                        Spacer()
                        Button {
                            Task {
                                await model.session?.sendMediaCommand(.previous, playerID: player.id, to: peer.id)
                            }
                        } label: {
                            Image(systemName: "backward.fill")
                                .font(.system(size: 18))
                                .frame(width: 36, height: 32)
                        }
                        .buttonStyle(.borderless)
                        .foregroundStyle(.primary)
                        .disabled(!player.canPrevious)
                        .accessibilityLabel(t("Previous", "Précédent"))

                        Button {
                            let action: MediaCommandAction = player.playing ? .pause : .play
                            Task {
                                await model.session?.sendMediaCommand(action, playerID: player.id, to: peer.id)
                            }
                        } label: {
                            Image(systemName: player.playing ? "pause.fill" : "play.fill")
                                .font(.system(size: 24))
                                .frame(width: 44, height: 36)
                        }
                        .buttonStyle(.borderless)
                        .foregroundStyle(.primary)
                        .disabled(player.playing ? !player.canPause : !player.canPlay)
                        .accessibilityLabel(player.playing ? t("Pause", "Pause") : t("Play", "Lecture"))

                        Button {
                            Task {
                                await model.session?.sendMediaCommand(.next, playerID: player.id, to: peer.id)
                            }
                        } label: {
                            Image(systemName: "forward.fill")
                                .font(.system(size: 18))
                                .frame(width: 36, height: 32)
                        }
                        .buttonStyle(.borderless)
                        .foregroundStyle(.primary)
                        .disabled(!player.canNext)
                        .accessibilityLabel(t("Next", "Suivant"))
                        Spacer()
                    }
                }
                .padding(.vertical, 4)
            } else {
                Text(Language.current == .french
                     ? "Rien en lecture sur \(peer.displayName)"
                     : "Nothing playing on \(peer.displayName)")
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func extrapolatedPosition(_ player: MediaPlayerPayload, now: Date) -> UInt64? {
        MediaClock.position(
            positionMs: player.position,
            lengthMs: player.length,
            playing: player.playing,
            receivedAtMs: peer.mediaReceivedAtMs,
            nowMs: Int64(now.timeIntervalSince1970 * 1000)
        )
    }

    private func seekSlider(player: MediaPlayerPayload, lengthMs: UInt64, now: Date) -> some View {
        let current = seekingMs.value.map { UInt64(max(0, $0)) } ?? extrapolatedPosition(player, now: now) ?? 0
        return HStack(spacing: 10) {
            Text(MediaClock.time(min(current, lengthMs)))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(minWidth: 38, alignment: .trailing)
            Slider(
                value: Binding(
                    get: { Double(min(current, lengthMs)) },
                    set: { value in seekingMs.value = value }
                ),
                in: 0...Double(lengthMs)
            ) { editing in
                guard !editing, let wanted = seekingMs.value else { return }
                let target = UInt64(max(0, wanted))
                if MediaClock.isSeek(from: extrapolatedPosition(player, now: .now), to: target) {
                    Task {
                        await model.session?.sendMediaCommand(.seek(positionMs: target), playerID: player.id, to: peer.id)
                    }
                }
                Task { @MainActor in
                    try? await Task.sleep(nanoseconds: 1_200_000_000)
                    seekingMs.value = nil
                }
            }
            Text(MediaClock.time(lengthMs))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(minWidth: 38, alignment: .leading)
        }
    }
}

private struct CoverArtTile: View {
    let data: Data?

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 8, style: .continuous)
        ZStack {
            if let data, let uiImage = UIImage(data: data) {
                Image(uiImage: uiImage)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fill)
            } else {
                shape.fill(Color.secondary.opacity(0.14))
                Image(systemName: "music.note")
                    .font(.system(size: 22))
                    .foregroundStyle(.secondary)
            }
        }
        .frame(width: 60, height: 60)
        .clipShape(shape)
        .overlay(shape.strokeBorder(Color.primary.opacity(0.08), lineWidth: 1))
        .accessibilityHidden(true)
    }
}

private struct PeerVolumeSection: View {
    @ObservedObject var model: MobileAppModel
    let peer: PeerSnapshot
    let volume: VolumePayload
    @StateObject private var draftLevel = Local<Double?>(nil)

    var body: some View {
        let muted = volume.muted ?? false
        let shownLevel = draftLevel.value ?? Double(volume.level ?? 0)
        Section(t("Media Volume", "Volume multimédia")) {
            HStack(spacing: 12) {
                Button {
                    Task {
                        await model.session?.setRemoteVolume(level: nil, muted: !muted, to: peer.id)
                    }
                } label: {
                    Image(systemName: muted ? "speaker.slash.fill" : "speaker.wave.2.fill")
                        .frame(width: 24)
                }
                .buttonStyle(.borderless)
                .foregroundStyle(.primary)
                .accessibilityLabel(muted ? t("Unmute", "Réactiver le son") : t("Mute", "Couper le son"))

                Slider(
                    value: Binding(
                        get: { shownLevel },
                        set: { value in draftLevel.value = value }
                    ),
                    in: 0...100
                ) { editing in
                    if !editing, let wanted = draftLevel.value {
                        Task {
                            await model.session?.setRemoteVolume(
                                level: UInt8(wanted.rounded()),
                                muted: nil,
                                to: peer.id
                            )
                        }
                    }
                }
                Text("\(Int(shownLevel.rounded()))")
                    .font(.subheadline.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .frame(width: 30, alignment: .trailing)
            }
            .opacity(muted ? 0.65 : 1.0)
        }
        .onChange(of: volume.level) { _, _ in
            draftLevel.value = nil
        }
    }
}

private struct PeerShortcutsSection: View {
    @ObservedObject var model: MobileAppModel
    let peer: PeerSnapshot
    @StateObject private var lastTriggeredID = Local<String?>(nil)

    var body: some View {
        Section(t("Shortcuts", "Raccourcis")) {
            ForEach(peer.commands) { item in
                Button {
                    lastTriggeredID.value = item.id
                    UIImpactFeedbackGenerator(style: .light).impactOccurred()
                    Task {
                        await model.session?.runShortcut(id: item.id, on: peer.id)
                        try? await Task.sleep(nanoseconds: 1_500_000_000)
                        if lastTriggeredID.value == item.id {
                            lastTriggeredID.value = nil
                        }
                    }
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: "terminal")
                            .foregroundStyle(.secondary)
                            .frame(width: 22)
                        Text(item.name)
                            .foregroundStyle(.primary)
                        Spacer(minLength: 0)
                        if lastTriggeredID.value == item.id {
                            Text(t("Ran", "Lancé"))
                                .font(.caption)
                                .foregroundStyle(Color.weftOrange)
                        } else {
                            Image(systemName: "play.fill")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
    }
}
#endif
