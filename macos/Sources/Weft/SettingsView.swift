// The Settings window: this Mac's name, opening at login, the group and
// updates in General; the feature switches in Features; the relay in Advanced.

import AppKit
import SwiftUI
import WeftFFI
import WeftKit

struct SettingsView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        TabView(selection: $model.settingsTab) {
            GeneralSettings(model: model)
                .tabItem { Label(t("General", "Général"), systemImage: "gearshape") }
                .tag("general")
            FeatureSettings(model: model, notifier: model.notifier, nowPlaying: model.nowPlaying)
                .tabItem { Label(t("Features", "Fonctions"), systemImage: "switch.2") }
                .tag("features")
            AdvancedSettings(model: model)
                .tabItem { Label(t("Advanced", "Avancé"), systemImage: "network") }
                .tag("advanced")
        }
        .frame(width: 500, height: 560)
        .onAppear { NSApp.activate() }
    }
}

private struct GeneralSettings: View {
    @ObservedObject var model: AppModel
    @StateObject private var name = Local("")
    @StateObject private var loginItem = Local(LoginItem.state)
    @StateObject private var loginError = Local<String?>(nil)
    @StateObject private var confirmingLeave = Local(false)
    @StateObject private var leaveResult = Local<String?>(nil)
    @FocusState private var editingName: Bool

    private var problem: NameProblem? {
        if case .failure(let problem) = DeviceName.check(name.value) { return problem }
        return nil
    }

    var body: some View {
        Form {
            Section(t("This Mac", "Ce Mac")) {
                TextField(t("Name", "Nom"), text: $name.value)
                    .focused($editingName)
                    .onSubmit(saveName)
                    .onChange(of: editingName) { _, editing in
                        if !editing { saveName() }
                    }
                if let problem {
                    Text(problem.text)
                        .font(.callout)
                        .foregroundStyle(.red)
                } else {
                    Text(t("Shown on your other devices.", "Affiché sur vos autres appareils."))
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                if let snapshot = model.snapshot {
                    LabeledContent(t("Device ID", "Identifiant")) {
                        Text(snapshot.shortId)
                            .font(.body.monospaced())
                            .textSelection(.enabled)
                            .help(snapshot.deviceId)
                    }
                }
            }
            Section(t("Startup", "Démarrage")) {
                loginToggle
            }
            UpdateSettings(updates: model.updates)
            Section {
                LabeledContent(t("Paired devices", "Appareils associés"),
                               value: Words.deviceCount(model.devices.count))
                Button(t("Leave Group…", "Quitter le groupe…"), role: .destructive) {
                    confirmingLeave.value = true
                }
                .disabled(model.devices.isEmpty || model.busy)
                if let leaveResult = leaveResult.value {
                    Text(leaveResult).font(.callout).foregroundStyle(.secondary)
                }
            } header: {
                Text(t("Group", "Groupe"))
            } footer: {
                Text(t(
                    "This Mac stops sharing with every device of the group, and keeps its identity.",
                    "Ce Mac cesse de partager avec tous les appareils du groupe, et garde son identité."
                ))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.leading)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .formStyle(.grouped)
        .onAppear {
            name.value = model.snapshot?.name ?? ""
            loginItem.value = LoginItem.state
        }
        .onChange(of: model.snapshot?.name) { _, saved in
            if !editingName, let saved { name.value = saved }
        }
        .confirmationDialog(
            t("Leave the group?", "Quitter le groupe ?"),
            isPresented: $confirmingLeave.value
        ) {
            Button(t("Leave Group", "Quitter le groupe"), role: .destructive) {
                Task {
                    let told = await model.leaveGroup()
                    leaveResult.value = told ? nil : t(
                        "Left. The other devices could not be told: remove this Mac on them too.",
                        "Groupe quitté. Les autres appareils n’ont pas pu être prévenus : retirez aussi ce Mac sur eux."
                    )
                }
            }
        } message: {
            Text(t(
                "To share again, pair a device anew.",
                "Pour partager de nouveau, associez un appareil."
            ))
        }
    }

    @ViewBuilder
    private var loginToggle: some View {
        switch loginItem.value {
        case .unavailable(let why):
            Toggle(t("Open at Login", "Ouvrir à l’ouverture de session"), isOn: .constant(false))
                .disabled(true)
            Text(why).font(.callout).foregroundStyle(.secondary)
        case .on, .off, .needsApproval:
            Toggle(t("Open at Login", "Ouvrir à l’ouverture de session"), isOn: Binding(
                get: { loginItem.value != .off },
                set: { on in
                    loginError.value = LoginItem.set(on)
                    loginItem.value = LoginItem.state
                }
            ))
            if loginItem.value == .needsApproval {
                HStack {
                    Text(t("Allow Weft in Login Items.", "Autorisez Weft dans Ouverture."))
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button(t("Open System Settings", "Ouvrir Réglages Système"),
                           action: LoginItem.openSystemSettings)
                }
            }
            if let loginError = loginError.value {
                Text(loginError).font(.callout).foregroundStyle(.red)
            }
        }
    }

    private func saveName() {
        model.rename(name.value)
    }
}

/// The version, and Sparkle's daily check.
private struct UpdateSettings: View {
    @ObservedObject var updates: Updates

    var body: some View {
        Section {
            LabeledContent(t("Version", "Version")) {
                Text(Updates.version).textSelection(.enabled)
            }
            if updates.available {
                Toggle(t("Automatically check for updates", "Rechercher automatiquement les mises à jour"),
                       isOn: Binding(get: { updates.checksAutomatically }, set: { updates.setChecksAutomatically($0) }))
                HStack {
                    Spacer()
                    Button(t("Check for Updates…", "Rechercher les mises à jour…"), action: updates.check)
                        .disabled(!updates.canCheck)
                }
            }
        } header: {
            Text(t("Updates", "Mises à jour"))
        } footer: {
            if !updates.available {
                Text(t(
                    "This build does not update itself.",
                    "Cette version ne se met pas à jour toute seule."
                ))
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}

/// The switches of docs/features.md section 10 the Mac has. A switched-off
/// feature leaves the capabilities sent to the other devices at once.
private struct FeatureSettings: View {
    @ObservedObject var model: AppModel
    @ObservedObject var notifier: Notifier
    @ObservedObject var nowPlaying: NowPlaying

    private struct Switch {
        let path: WritableKeyPath<FeatureSwitches, Bool>
        let title: String
        let caption: String
    }

    private let switches: [Switch] = [
        Switch(path: \.files, title: t("Files", "Fichiers"),
               caption: t("Send files to your devices, and receive theirs in Downloads.",
                          "Envoyez des fichiers à vos appareils et recevez les leurs dans Téléchargements.")),
        Switch(path: \.links, title: t("Links", "Liens"),
               caption: t("Links your devices share open in your browser.",
                          "Les liens partagés par vos appareils s’ouvrent dans votre navigateur.")),
        Switch(path: \.notifications, title: t("Notifications", "Notifications"),
               caption: t("Your phone’s notifications show on this Mac, and you can answer them.",
                          "Les notifications du téléphone s’affichent sur ce Mac, et vous pouvez y répondre.")),
        Switch(path: \.calls, title: t("Calls", "Appels"),
               caption: t("Incoming and missed calls of your phone show here.",
                          "Les appels entrants et manqués du téléphone s’affichent ici.")),
        Switch(path: \.battery, title: t("Battery and Network", "Batterie et réseau"),
               caption: t("Show the battery and network of your devices.",
                          "Affichez la batterie et le réseau de vos appareils.")),
        Switch(path: \.ring, title: t("Ring", "Sonnerie"),
               caption: t("Ring your devices to find them, and let them ring this Mac.",
                          "Faites sonner vos appareils pour les retrouver, et laissez-les faire sonner ce Mac.")),
        Switch(path: \.media, title: t("Media", "Média"),
               caption: t("See and control what plays on your devices.",
                          "Voyez et contrôlez ce qui joue sur vos appareils.")),
        Switch(path: \.volume, title: t("Volume", "Volume"),
               caption: t("Change the volume of your devices, and let them change this Mac’s.",
                          "Changez le volume de vos appareils, et laissez-les changer celui de ce Mac.")),
    ]

    var body: some View {
        Form {
            if let features = model.snapshot?.features {
                // A problem first: without permission, half of these
                // cannot show anything.
                if features.notifications || features.calls || features.files {
                    notificationPermission
                }
                Section {
                    ForEach(switches, id: \.title) { item in
                        Toggle(isOn: Binding(
                            get: { features[keyPath: item.path] },
                            set: { on in
                                var changed = features
                                changed[keyPath: item.path] = on
                                model.setFeatures(changed)
                            }
                        )) {
                            Text(item.title)
                            Text(item.caption)
                        }
                        if item.path == \.media, features.media, nowPlaying.status == .unavailable {
                            // What plays on this Mac cannot be read: the
                            // rest of the feature still works.
                            Text(t(
                                "This Mac cannot read what plays on this version of macOS.",
                                "Ce Mac ne peut pas lire ce qui joue sur cette version de macOS."
                            ))
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                        }
                    }
                } footer: {
                    Text(t(
                        "A feature turned off here is hidden from your other devices too.",
                        "Une fonction coupée ici est aussi masquée sur vos autres appareils."
                    ))
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .formStyle(.grouped)
        .onAppear { notifier.refreshPermission() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            notifier.refreshPermission()
        }
    }

    @ViewBuilder
    private var notificationPermission: some View {
        switch notifier.permission {
        case .denied:
            Section(t("macOS Notifications", "Notifications de macOS")) {
                Text(t(
                    "Notifications are off for Weft: the phone’s notifications, its calls and received files cannot show. Turn them on in System Settings › Notifications.",
                    "Les notifications sont coupées pour Weft : celles du téléphone, ses appels et les fichiers reçus ne peuvent pas s’afficher. Activez-les dans Réglages Système › Notifications."
                ))
                .foregroundStyle(.secondary)
                Button(t("Open System Settings", "Ouvrir Réglages Système"), action: notifier.openSystemSettings)
            }
        case .notAsked:
            Section(t("macOS Notifications", "Notifications de macOS")) {
                Text(t(
                    "Weft needs your permission to show notifications.",
                    "Weft a besoin de votre accord pour afficher des notifications."
                ))
                .foregroundStyle(.secondary)
                Button(t("Allow Notifications…", "Autoriser les notifications…"), action: notifier.askIfNeeded)
            }
        case .allowed, .unknown:
            EmptyView()
        }
    }
}

private struct AdvancedSettings: View {
    @ObservedObject var model: AppModel
    @StateObject private var url = Local("")
    @StateObject private var refusal = Local<String?>(nil)

    private var current: String { model.snapshot?.relayUrl ?? "" }
    private var inGroup: Bool { !model.devices.isEmpty }
    private var edited: String { url.value.trimmingCharacters(in: .whitespacesAndNewlines) }

    var body: some View {
        Form {
            Section {
                TextField(t("Relay URL", "Adresse du relais"), text: $url.value, prompt: Text(defaultRelayUrl()))
                    .font(.body.monospaced())
                    .disabled(inGroup || model.busy)
                    .onSubmit(apply)
                if let refusal = refusal.value {
                    Text(refusal).font(.callout).foregroundStyle(.red)
                }
                HStack {
                    Button(t("Use Default", "Par défaut")) { url.value = defaultRelayUrl() }
                        .disabled(inGroup || model.busy || edited == defaultRelayUrl())
                    Spacer()
                    if model.busy { ProgressView().controlSize(.small) }
                    Button(t("Use This Relay", "Utiliser ce relais"), action: apply)
                        .disabled(inGroup || model.busy || edited.isEmpty || edited == current)
                }
            } header: {
                Text(t("Relay", "Relais"))
            } footer: {
                Text(inGroup
                     ? t("Every device of a group uses the same relay. Leave the group to change it.",
                         "Tous les appareils d’un groupe utilisent le même relais. Quittez le groupe pour le changer.")
                     : t("The relay passes encrypted messages between your devices and never reads them. Anyone can run one.",
                         "Le relais transmet les messages chiffrés entre vos appareils sans jamais les lire. Chacun peut héberger le sien."))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.leading)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .formStyle(.grouped)
        .onAppear { url.value = current }
        .onChange(of: current) { _, saved in url.value = saved }
    }

    private func apply() {
        guard !inGroup, !edited.isEmpty, edited != current else { return }
        refusal.value = nil
        Task { refusal.value = await model.setRelay(edited) }
    }
}
