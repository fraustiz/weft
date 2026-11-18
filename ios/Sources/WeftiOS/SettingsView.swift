// Settings sheet: device name (`UIDevice.current.name`), `weft://send-clipboard`
// Shortcuts helper, feature switches, relay URL editor, and Leave Group
// confirmation.

#if canImport(UIKit)
import SwiftUI
import UIKit
import WeftMobileKit

struct SettingsView: View {
    @ObservedObject var model: MobileAppModel

    @StateObject private var editedName = Local("")
    @StateObject private var nameProblem = Local<NameProblem?>(nil)
    @StateObject private var editedRelay = Local("")
    @StateObject private var relayError = Local<String?>(nil)
    @StateObject private var confirmingLeave = Local(false)
    @StateObject private var leaveNote = Local<String?>(nil)
    @StateObject private var copiedShortcut = Local(false)
    @FocusState private var nameFocused: Bool
    @Environment(\.dismiss) private var dismiss

    private var snapshot: MobileSnapshot? { model.snapshot }
    private var inGroup: Bool { !(snapshot?.peers.isEmpty ?? true) }

    var body: some View {
        NavigationStack {
            Form {
                deviceIdentitySection
                shortcutAutomationSection
                if let features = snapshot?.features {
                    featureSwitchesSection(features)
                }
                relaySection
                groupSection
            }
            .navigationTitle(t("Settings", "Réglages"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(t("Done", "Terminé")) {
                        commitDeviceName()
                        dismiss()
                    }
                    .tint(.primary)
                }
            }
            .onAppear {
                editedName.value = snapshot?.deviceName ?? UIDevice.current.name
                editedRelay.value = snapshot?.relayURL ?? MobileStore.defaultRelayURL
            }
            .confirmationDialog(
                t("Leave the group?", "Quitter le groupe ?"),
                isPresented: $confirmingLeave.value
            ) {
                Button(t("Leave Group", "Quitter le groupe"), role: .destructive) {
                    Task {
                        let told = await model.session?.leaveGroup(evenIfOffline: true) ?? false
                        leaveNote.value = told ? nil : t(
                            "Left locally. Your other devices were offline; remove this iPhone on them too.",
                            "Groupe quitté sur cet iPhone. Vos autres appareils étaient hors ligne : retirez aussi cet iPhone sur eux."
                        )
                    }
                }
            } message: {
                Text(t(
                    "This iPhone stops sharing with every device in the group and keeps its identity.",
                    "Cet iPhone cesse de partager avec tous les appareils du groupe et conserve son identité."
                ))
            }
        }
    }

    // MARK: - Sections

    private var deviceIdentitySection: some View {
        Section {
            TextField(t("Name", "Nom"), text: $editedName.value)
                .focused($nameFocused)
                .submitLabel(.done)
                .onSubmit {
                    commitDeviceName()
                }
                .onChange(of: editedName.value) { _, current in
                    if case .failure(let problem) = DeviceName.check(current) {
                        nameProblem.value = problem
                    } else {
                        nameProblem.value = nil
                    }
                }
                .onChange(of: nameFocused) { _, focused in
                    if !focused { commitDeviceName() }
                }

            if let snapshot {
                LabeledContent(t("Device ID", "Identifiant")) {
                    Text(snapshot.shortID)
                        .font(.body.monospaced())
                        .textSelection(.enabled)
                }
            }
        } header: {
            Text(t("This iPhone", "Cet iPhone"))
        } footer: {
            if let problem = nameProblem.value {
                Text(problem.text).foregroundStyle(.red)
            } else {
                Text(t("Shown to the other devices in your group.",
                       "Affiché sur les autres appareils de votre groupe."))
            }
        }
    }

    private var shortcutAutomationSection: some View {
        Section {
            HStack(spacing: 10) {
                Text("weft://send-clipboard")
                    .font(.footnote.monospaced())
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Spacer(minLength: 0)
                Button(copiedShortcut.value ? t("Copied", "Copié") : t("Copy URL", "Copier l’URL")) {
                    UIPasteboard.general.string = "weft://send-clipboard"
                    copiedShortcut.value = true
                    UIImpactFeedbackGenerator(style: .light).impactOccurred()
                }
                .font(.footnote.weight(.medium))
                .tint(.primary)
            }
        } header: {
            Text(t("One-Tap Automation", "Raccourci en un geste"))
        } footer: {
            Text(t(
                "Open this URL in an Apple Shortcut assigned to Back Tap, Action Button or Control Center to send your clipboard in one tap.",
                "Ouvrez cette URL dans un raccourci Apple associé au Toucher le dos, au bouton Action ou au Centre de contrôle pour envoyer le presse-papiers en un geste."
            ))
        }
    }

    private func featureSwitchesSection(_ features: FeatureSwitches) -> some View {
        Section {
            featureToggle(
                t("Receive Clipboard", "Recevoir le presse-papiers"),
                subtitle: t("Update pasteboard while Weft is open", "Met à jour le presse-papiers quand Weft est ouvert"),
                value: features.clipboardReceive
            ) { on in
                var next = features; next.clipboardReceive = on; applyFeatures(next)
            }
            featureToggle(
                t("Files", "Fichiers"),
                subtitle: t("Send files and save received ones in Files › On My iPhone › Weft",
                            "Envoyer des fichiers et enregistrer ceux reçus dans Fichiers › Sur mon iPhone › Weft"),
                value: features.files
            ) { on in
                var next = features; next.files = on; applyFeatures(next)
            }
            featureToggle(
                t("Links", "Liens"),
                subtitle: t("Share web links and open received links", "Partager et ouvrir des liens web"),
                value: features.links
            ) { on in
                var next = features; next.links = on; applyFeatures(next)
            }
            featureToggle(
                t("Battery", "Batterie"),
                subtitle: t("Share battery level with your devices", "Partager le niveau de batterie"),
                value: features.battery
            ) { on in
                var next = features; next.battery = on; applyFeatures(next)
            }
            featureToggle(
                t("Network", "Réseau"),
                subtitle: t("Share Wi-Fi or cellular status", "Partager l’état Wi-Fi ou cellulaire"),
                value: features.connectivity
            ) { on in
                var next = features; next.connectivity = on; applyFeatures(next)
            }
            featureToggle(
                t("Ring", "Sonnerie"),
                subtitle: t("Find devices and let your computers ring this iPhone",
                            "Faire sonner vos appareils et laisser vos ordinateurs faire sonner cet iPhone"),
                value: features.ring
            ) { on in
                var next = features; next.ring = on; applyFeatures(next)
            }
            featureToggle(
                t("Ping", "Ping"),
                subtitle: t("Send and receive test pings", "Envoyer et recevoir des pings"),
                value: features.ping
            ) { on in
                var next = features; next.ping = on; applyFeatures(next)
            }
            featureToggle(
                t("Media", "Média"),
                subtitle: t("See and control what plays on your computers",
                            "Voir et contrôler la lecture sur vos ordinateurs"),
                value: features.media
            ) { on in
                var next = features; next.media = on; applyFeatures(next)
            }
            featureToggle(
                t("Volume", "Volume"),
                subtitle: t("Adjust media volume on your computers", "Régler le volume multimédia de vos ordinateurs"),
                value: features.volume
            ) { on in
                var next = features; next.volume = on; applyFeatures(next)
            }
            featureToggle(
                t("Remote Control", "Contrôle à distance"),
                subtitle: t("Trackpad, keyboard and presentation laser pointer",
                            "Pavé tactile, clavier et pointeur laser de présentation"),
                value: features.remote
            ) { on in
                var next = features; next.remote = on; applyFeatures(next)
            }
            featureToggle(
                t("Computer Shortcuts", "Raccourcis ordinateur"),
                subtitle: t("Trigger shortcuts configured on your Mac or PC",
                            "Lancer les raccourcis configurés sur votre Mac ou PC"),
                value: features.commands
            ) { on in
                var next = features; next.commands = on; applyFeatures(next)
            }
        } header: {
            Text(t("Features", "Fonctions"))
        } footer: {
            Text(t(
                "Turned-off features are immediately removed from the capabilities sent to your other devices.",
                "Les fonctions désactivées sont aussitôt retirées des capacités annoncées à vos autres appareils."
            ))
        }
    }

    private func featureToggle(
        _ title: String,
        subtitle: String,
        value: Bool,
        onChange: @escaping (Bool) -> Void
    ) -> some View {
        Toggle(
            isOn: Binding(
                get: { value },
                set: { newValue in onChange(newValue) }
            )
        ) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var relaySection: some View {
        let trimmed = editedRelay.value.trimmingCharacters(in: .whitespacesAndNewlines)
        let currentRelay = snapshot?.relayURL ?? MobileStore.defaultRelayURL

        return Section {
            TextField(
                t("Relay URL", "Adresse du relais"),
                text: $editedRelay.value,
                prompt: Text(MobileStore.defaultRelayURL)
            )
            .font(.subheadline.monospaced())
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .disabled(inGroup)
            .onSubmit {
                applyRelay()
            }

            if let error = relayError.value {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(.red)
            }

            HStack {
                Button(t("Use Default", "Par défaut")) {
                    editedRelay.value = MobileStore.defaultRelayURL
                    applyRelay()
                }
                .disabled(inGroup || trimmed == MobileStore.defaultRelayURL)

                Spacer()

                Button(t("Apply", "Appliquer")) {
                    applyRelay()
                }
                .disabled(inGroup || trimmed.isEmpty || trimmed == currentRelay)
            }
            .tint(.primary)
        } header: {
            Text(t("Relay", "Relais"))
        } footer: {
            Text(inGroup
                 ? t("All devices in a group share the same relay. Leave the group to change it.",
                     "Tous les appareils d’un groupe partagent le même relais. Quittez le groupe pour le modifier.")
                 : t("The relay forwards encrypted envelopes without ever reading them.",
                     "Le relais achemine les enveloppes chiffrées sans jamais pouvoir les lire."))
        }
    }

    private var groupSection: some View {
        Section {
            LabeledContent(
                t("Paired devices", "Appareils associés"),
                value: Words.deviceCount(snapshot?.peers.count ?? 0)
            )
            Button(t("Leave Group…", "Quitter le groupe…"), role: .destructive) {
                confirmingLeave.value = true
            }
            .disabled(!inGroup)

            if let note = leaveNote.value {
                Text(note)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        } header: {
            Text(t("Group", "Groupe"))
        }
    }

    // MARK: - Actions

    private func commitDeviceName() {
        let candidate = editedName.value
        guard candidate != snapshot?.deviceName else { return }
        Task {
            nameProblem.value = await model.session?.renameDevice(candidate)
        }
    }

    private func applyFeatures(_ updated: FeatureSwitches) {
        Task {
            await model.session?.updateFeatures(updated)
        }
    }

    private func applyRelay() {
        guard !inGroup else { return }
        let trimmed = editedRelay.value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        Task {
            relayError.value = await model.session?.updateRelayURL(trimmed)
        }
    }
}
#endif
