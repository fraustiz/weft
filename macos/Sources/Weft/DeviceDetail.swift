// A device's page in the main window: how it is (battery, network), what
// this Mac can do with it (send files and links, ring, ping), what it
// plays and its volume, the files on their way, and its removal. Each
// action shows only when both sides handle it (docs/features.md section 2).

import AppKit
import SwiftUI
import WeftFFI
import WeftKit

struct DeviceDetail: View {
    @ObservedObject var model: AppModel
    let device: DeviceInfo
    @StateObject private var confirmingRemoval = Local(false)
    @StateObject private var dropTargeted = Local(false)

    private var features: FeatureSwitches? { model.snapshot?.features }
    private var takesFiles: Bool { (features?.files ?? false) && device.takes("file.offer") }

    var body: some View {
        Form {
            Section {
                DeviceHeader(device: device)
            }
            actions
            if (features?.media ?? false) && device.gives("media"), let media = device.media {
                MediaSection(model: model, device: device, media: media)
            }
            if (features?.volume ?? false) && device.takes("volume.set"),
               let volume = device.volume, volume.level != nil {
                VolumeSection(model: model, device: device, volume: volume)
            }
            if (features?.links ?? false) && device.takes("share.url") {
                LinkSection(model: model, device: device)
            }
            if device.sendsRemoteInput || device.allowedControl || device.takes("input.key") {
                RemoteControlSection(model: model, device: device)
            }
            let transfers = model.snapshot?.transfers.filter { $0.deviceId == device.id } ?? []
            if !transfers.isEmpty {
                Section(t("Files", "Fichiers")) {
                    ForEach(transfers) { transfer in
                        TransferRow(transfer: transfer) { model.cancelTransfer(transfer) }
                    }
                }
            }
            Section {
                LabeledContent(t("Device ID", "Identifiant")) {
                    Text(device.shortId)
                        .font(.body.monospaced())
                        .textSelection(.enabled)
                        .help(device.id)
                }
                Button(t("Remove from Group…", "Retirer du groupe…"), role: .destructive) {
                    confirmingRemoval.value = true
                }
            } footer: {
                Text(t(
                    "A removed device no longer gets this Mac’s clipboard. You can pair it again later.",
                    "Un appareil retiré ne reçoit plus le presse-papiers de ce Mac. Vous pourrez l’associer de nouveau."
                ))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.leading)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .formStyle(.grouped)
        .navigationTitle(device.displayName)
        .overlay {
            if dropTargeted.value {
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(Color.weftOrange, lineWidth: 2)
                    .padding(6)
                    .allowsHitTesting(false)
            }
        }
        .dropDestination(for: URL.self) { urls, _ in
            guard takesFiles else { return false }
            model.sendFiles(urls, to: device.id)
            return true
        } isTargeted: { targeted in
            dropTargeted.value = targeted && takesFiles
        }
        .confirmationDialog(
            Language.current == .french
                ? "Retirer « \(device.displayName) » du groupe ?"
                : "Remove “\(device.displayName)” from the group?",
            isPresented: $confirmingRemoval.value
        ) {
            Button(t("Remove", "Retirer"), role: .destructive) { model.remove(device) }
        } message: {
            Text(t(
                "The other devices of the group stop trusting it too.",
                "Les autres appareils du groupe cessent aussi de lui faire confiance."
            ))
        }
    }

    @ViewBuilder
    private var actions: some View {
        let ring = (features?.ring ?? false) && device.takes("ring")
        let ping = device.takes("ping")
        let messages = (features?.sms ?? false) && device.canMessage
        if device.platform == nil {
            Section {
                Text(Language.current == .french
                     ? "Les actions s’affichent quand \(device.displayName) est en ligne."
                     : "Actions show once \(device.displayName) is online.")
                    .foregroundStyle(.secondary)
            }
        } else if takesFiles || ring || ping || messages {
            Section {
                HStack(spacing: 8) {
                    if takesFiles {
                        Button(t("Send File…", "Envoyer un fichier…")) { model.pickFiles(for: device) }
                            .buttonStyle(WeftProminentButtonStyle())
                    }
                    if messages {
                        Button(t("Messages…", "Messages…")) { model.openMessagesSheet(for: device) }
                    }
                    if ring {
                        if model.rang?.device == device.id {
                            Button(t("Stop Ringing", "Arrêter la sonnerie")) { model.stopRinging(device) }
                        } else {
                            Button(t("Ring", "Faire sonner")) { model.ring(device) }
                                .help(t("Rings it loudly, even in silent mode, for a minute",
                                        "Le fait sonner fort, même en silencieux, pendant une minute"))
                        }
                    }
                    if ping {
                        Button(t("Ping", "Ping")) { model.ping(device) }
                            .help(t("Shows a notification on it", "Y affiche une notification"))
                    }
                    Spacer(minLength: 0)
                }
            } footer: {
                if takesFiles {
                    Text(t("Or drop files on this window.", "Ou déposez des fichiers sur cette fenêtre."))
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }
}

/// Icon, name, platform, then battery and network when it sent them.
private struct DeviceHeader: View {
    let device: DeviceInfo

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: device.symbol)
                .font(.system(size: 28, weight: .light))
                .foregroundStyle(.secondary)
                .frame(width: 40)
            VStack(alignment: .leading, spacing: 4) {
                Text(device.displayName).font(.title3)
                HStack(spacing: 14) {
                    Text(device.platformName ?? t("Platform not known yet", "Plateforme pas encore connue"))
                    if let battery = device.battery, let text = device.batteryText {
                        Label(text, systemImage: Words.batterySymbol(level: battery.level, charging: battery.charging))
                            .monospacedDigit()
                            .accessibilityLabel(t("Battery ", "Batterie ") + text)
                    }
                    if let network = device.networkText {
                        Label {
                            Text(network)
                        } icon: {
                            Image(systemName: Words.networkSymbol(device.connectivity?.network),
                                  variableValue: Words.signalFraction(device.connectivity?.signal))
                        }
                        .accessibilityLabel(t("Network ", "Réseau ") + network)
                    }
                }
                .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 4)
    }
}

/// A link to open on the device: the one on the clipboard, or typed.
private struct LinkSection: View {
    @ObservedObject var model: AppModel
    let device: DeviceInfo
    @StateObject private var link = Local("")
    @StateObject private var sent = Local(false)

    private var valid: Bool { Links.isShareable(link.value) }

    var body: some View {
        Section {
            HStack(spacing: 8) {
                TextField(t("Link", "Lien"), text: $link.value, prompt: Text("https://…"))
                    .labelsHidden()
                    .onSubmit(send)
                    .onChange(of: link.value) { _, _ in sent.value = false }
                Button(sent.value ? t("Sent", "Envoyé") : t("Send Link", "Envoyer le lien"), action: send)
                    .disabled(!valid || sent.value)
            }
        } header: {
            Text(t("Link", "Lien"))
        } footer: {
            Text(hint)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .onAppear {
            if link.value.isEmpty,
               let candidate = Links.candidate(fromClipboard: NSPasteboard.general.string(forType: .string)) {
                link.value = candidate
            }
        }
    }

    private var hint: String {
        let text = link.value.trimmingCharacters(in: .whitespaces)
        if text.isEmpty || valid {
            return Language.current == .french
                ? "Un lien web, ouvert sur \(device.displayName)."
                : "A web link, opened on \(device.displayName)."
        }
        return Language.current == .french
            ? "Liens http et https uniquement, \(Links.maxBytes) octets au plus."
            : "Only http and https links, up to \(Links.maxBytes) bytes."
    }

    private func send() {
        if model.shareLink(link.value.trimmingCharacters(in: .whitespaces), to: device) {
            sent.value = true
        }
    }
}

/// What the device plays: the player shown, with its controls; the
/// position moves on with the clock while it plays.
private struct MediaSection: View {
    @ObservedObject var model: AppModel
    let device: DeviceInfo
    let media: MediaInfo
    /// A position being dragged, in ms, until the bar is released.
    @StateObject private var seeking = Local<Double?>(nil)
    @StateObject private var playerVolume = Local<Double?>(nil)

    var body: some View {
        Section(t("Now Playing", "En lecture")) {
            if let player = device.currentPlayer {
                VStack(alignment: .leading, spacing: 10) {
                    HStack(spacing: 12) {
                        CoverTile(image: player.art.flatMap { model.cover(deviceId: device.id, art: $0) })
                        VStack(alignment: .leading, spacing: 2) {
                            if !player.app.trimmingCharacters(in: .whitespaces).isEmpty {
                                Text(player.app).font(.caption).foregroundStyle(.secondary)
                            }
                            Text(player.title.isEmpty ? t("Untitled", "Sans titre") : player.title)
                                .font(.headline)
                                .lineLimit(1)
                            if !player.artist.isEmpty {
                                Text(player.artist).foregroundStyle(.secondary).lineLimit(1)
                            }
                        }
                        Spacer(minLength: 0)
                    }
                    if player.canSeek, let length = player.lengthMs, length > 0 {
                        TimelineView(.periodic(from: .now, by: 1)) { context in
                            seekBar(player, length: length, now: context.date)
                        }
                    }
                    transport(player)
                    if let level = player.volume {
                        HStack(spacing: 8) {
                            Image(systemName: "speaker.fill").foregroundStyle(.secondary)
                                .accessibilityHidden(true)
                            Slider(
                                value: Binding(
                                    get: { playerVolume.value ?? Double(level) },
                                    set: { playerVolume.value = $0 }
                                ),
                                in: 0...100
                            ) { editing in
                                if !editing, let wanted = playerVolume.value {
                                    model.media(.setVolume(level: UInt8(wanted.rounded())), player: player.id, on: device)
                                }
                            }
                            .labelsHidden()
                            .accessibilityLabel(t("Player volume", "Volume du lecteur"))
                            Image(systemName: "speaker.wave.3.fill").foregroundStyle(.secondary)
                                .accessibilityHidden(true)
                        }
                        .onChange(of: level) { _, _ in playerVolume.value = nil }
                    }
                }
                .padding(.vertical, 4)
            } else {
                Text(Language.current == .french
                     ? "Rien en lecture sur \(device.displayName)."
                     : "Nothing playing on \(device.displayName).")
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func shown(_ player: PlayerInfo, now: Date) -> UInt64? {
        MediaClock.position(
            positionMs: player.positionMs, lengthMs: player.lengthMs, playing: player.playing,
            receivedAtMs: media.receivedAtMs, nowMs: Int64(now.timeIntervalSince1970 * 1000)
        )
    }

    private func seekBar(_ player: PlayerInfo, length: UInt64, now: Date) -> some View {
        let position = seeking.value.map { UInt64($0) } ?? shown(player, now: now) ?? 0
        return HStack(spacing: 10) {
            Text(MediaClock.time(position))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(minWidth: 36, alignment: .trailing)
            Slider(
                value: Binding(
                    get: { Double(min(position, length)) },
                    set: { seeking.value = $0 }
                ),
                in: 0...Double(length)
            ) { editing in
                guard !editing, let wanted = seeking.value else { return }
                let target = UInt64(max(0, wanted))
                if MediaClock.isSeek(from: shown(player, now: .now), to: target) {
                    model.media(.seek(positionMs: target), player: player.id, on: device)
                }
                // Kept a moment, until the device tells its new position.
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { seeking.value = nil }
            }
            .controlSize(.small)
            .labelsHidden()
            .accessibilityLabel(t("Position", "Position"))
            Text(MediaClock.time(length))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(minWidth: 36, alignment: .leading)
        }
    }

    private func transport(_ player: PlayerInfo) -> some View {
        HStack(spacing: 22) {
            Spacer()
            control("backward.fill", t("Previous", "Précédent"), enabled: player.canPrevious) {
                model.media(.previous, player: player.id, on: device)
            }
            if player.playing {
                control("pause.fill", t("Pause", "Pause"), enabled: player.canPause, large: true) {
                    model.media(.pause, player: player.id, on: device)
                }
            } else {
                control("play.fill", t("Play", "Lecture"), enabled: player.canPlay, large: true) {
                    model.media(.play, player: player.id, on: device)
                }
            }
            control("forward.fill", t("Next", "Suivant"), enabled: player.canNext) {
                model.media(.next, player: player.id, on: device)
            }
            Spacer()
        }
    }

    private func control(
        _ symbol: String, _ name: String, enabled: Bool, large: Bool = false, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: large ? 22 : 16))
                .frame(width: 32, height: 28)
                .contentShape(Rectangle())
        }
        .buttonStyle(.borderless)
        .foregroundStyle(.primary)
        .disabled(!enabled)
        .help(name)
        .accessibilityLabel(name)
    }
}

/// A player's cover, or a neutral tile with a note when there is none.
private struct CoverTile: View {
    let image: NSImage?
    static let side: CGFloat = 64

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 6, style: .continuous)
        ZStack {
            if let image {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fill)
            } else {
                shape.fill(Color(nsColor: .quaternaryLabelColor).opacity(0.5))
                Image(systemName: "music.note")
                    .font(.system(size: 24, weight: .regular))
                    .foregroundStyle(.secondary)
            }
        }
        .frame(width: Self.side, height: Self.side)
        .clipShape(shape)
        .overlay(shape.strokeBorder(Color(nsColor: .separatorColor), lineWidth: 1))
        .accessibilityHidden(true)
    }
}

/// The device's media volume, with mute.
private struct VolumeSection: View {
    @ObservedObject var model: AppModel
    let device: DeviceInfo
    let volume: VolumeInfo
    @StateObject private var draft = Local<Double?>(nil)

    var body: some View {
        let muted = volume.muted ?? false
        let level = draft.value ?? Double(volume.level ?? 0)
        Section(t("Media Volume", "Volume multimédia")) {
            HStack(spacing: 10) {
                Button {
                    model.setVolume(nil, muted: !muted, on: device)
                } label: {
                    Image(systemName: muted ? "speaker.slash.fill" : "speaker.wave.2.fill")
                        .frame(width: 22)
                }
                .buttonStyle(.borderless)
                .foregroundStyle(.primary)
                .help(muted ? t("Unmute", "Réactiver le son") : t("Mute", "Couper le son"))
                .accessibilityLabel(muted ? t("Unmute", "Réactiver le son") : t("Mute", "Couper le son"))
                Slider(
                    value: Binding(get: { level }, set: { draft.value = $0 }),
                    in: 0...100
                ) { editing in
                    if !editing, let wanted = draft.value {
                        model.setVolume(UInt8(wanted.rounded()), muted: nil, on: device)
                    }
                }
                .labelsHidden()
                .accessibilityLabel(t("Volume", "Volume"))
                Text("\(Int(level.rounded()))")
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .frame(width: 28, alignment: .trailing)
            }
            .opacity(muted ? 0.6 : 1)
        }
        .onChange(of: volume.level) { _, _ in draft.value = nil }
    }
}

private struct TransferRow: View {
    let transfer: TransferInfo
    let cancel: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: transfer.outgoing ? "arrow.up.circle" : "arrow.down.circle")
                .font(.title3)
                .foregroundStyle(.secondary)
                .accessibilityLabel(transfer.outgoing ? t("Sending", "Envoi") : t("Receiving", "Réception"))
            VStack(alignment: .leading, spacing: 4) {
                Text(transfer.name).lineLimit(1).truncationMode(.middle)
                if transfer.isActive {
                    if transfer.size == 0 || transfer.done == 0 {
                        ProgressView().progressViewStyle(.linear)
                    } else {
                        ProgressView(value: Double(transfer.done), total: Double(transfer.size))
                    }
                }
                Text(transfer.status)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            Spacer(minLength: 0)
            if transfer.isActive {
                Button {
                    cancel()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                }
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
                .help(t("Cancel", "Annuler"))
                .accessibilityLabel(Language.current == .french ? "Annuler \(transfer.name)" : "Cancel \(transfer.name)")
            }
        }
        .padding(.vertical, 2)
    }
}

/// Allowing the device to control this Mac's pointer, keyboard and
/// presenter, and typing on the phone from this Mac.
private struct RemoteControlSection: View {
    @ObservedObject var model: AppModel
    let device: DeviceInfo
    @StateObject private var accessibilityTrusted = Local(RemoteInputController.isTrusted)
    @StateObject private var typedText = Local("")
    @StateObject private var sentText = Local("")

    var body: some View {
        Section(t("Remote Control", "Contrôle à distance")) {
            if device.sendsRemoteInput || device.allowedControl {
                Toggle(isOn: Binding(
                    get: { device.allowedControl },
                    set: { allowed in
                        model.setControl(allowed, for: device)
                        accessibilityTrusted.value = RemoteInputController.isTrusted
                    }
                )) {
                    Text(Language.current == .french
                         ? "Autoriser \(device.displayName) à contrôler ce Mac"
                         : "Allow \(device.displayName) to control this Mac")
                    Text(t("Pointer, keyboard and presentation controls.",
                           "Pointeur, clavier et contrôle de présentation."))
                }
                if device.allowedControl && !accessibilityTrusted.value {
                    HStack(spacing: 12) {
                        Text(t(
                            "Allow Weft in Privacy & Security › Accessibility so it can move the pointer and type keys.",
                            "Autorisez Weft dans Confidentialité et sécurité › Accessibilité pour déplacer le pointeur et saisir des touches."
                        ))
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        Spacer(minLength: 0)
                        Button(t("Open System Settings", "Ouvrir Réglages Système")) {
                            RemoteInputController.openAccessibilitySettings()
                        }
                    }
                }
            }
            if device.takes("input.key") {
                VStack(alignment: .leading, spacing: 6) {
                    HStack(alignment: .bottom, spacing: 8) {
                        TextField(
                            t("Type on Phone", "Saisir sur le téléphone"),
                            text: $typedText.value,
                            prompt: Text(Language.current == .french
                                ? "Saisir sur \(device.displayName)…"
                                : "Type on \(device.displayName)…"),
                            axis: .vertical
                        )
                        .lineLimit(1...4)
                        .labelsHidden()
                        .onChange(of: typedText.value) { _, updated in
                            guard updated != sentText.value else { return }
                            model.sendTypedDiff(from: sentText.value, to: updated, on: device)
                            sentText.value = updated
                        }
                        Button(t("Clear", "Effacer")) {
                            sentText.value = ""
                            typedText.value = ""
                        }
                        .disabled(typedText.value.isEmpty)
                    }
                    Text(t(
                        "Requires the Weft keyboard to be selected on the phone. Clear empties this field only.",
                        "Nécessite que le clavier Weft soit sélectionné sur le téléphone. Effacer ne vide que ce champ."
                    ))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
                .padding(.vertical, 2)
            }
        }
        .onAppear {
            accessibilityTrusted.value = RemoteInputController.isTrusted
        }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            accessibilityTrusted.value = RemoteInputController.isTrusted
        }
        .onChange(of: device.id) { _, _ in
            sentText.value = ""
            typedText.value = ""
        }
    }
}
