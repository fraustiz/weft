// The Settings window: this Mac's name, opening at login and the group in
// General; the relay in Advanced.

import AppKit
import SwiftUI
import WeftFFI
import WeftKit

struct SettingsView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        TabView {
            GeneralSettings(model: model)
                .tabItem { Label(t("General", "Général"), systemImage: "gearshape") }
            AdvancedSettings(model: model)
                .tabItem { Label(t("Advanced", "Avancé"), systemImage: "network") }
        }
        .frame(width: 480, height: 500)
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
