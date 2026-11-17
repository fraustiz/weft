// The main window: the devices of the group in a sidebar and the selected
// one in detail, or, before any pairing, a single invitation to pair.

import SwiftUI
import WeftFFI
import WeftKit

struct MainWindow: View {
    @ObservedObject var model: AppModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Group {
            if let snapshot = model.snapshot {
                if snapshot.devices.isEmpty {
                    EmptyState(model: model)
                } else {
                    devices(snapshot)
                }
            } else if let error = model.startError {
                ContentUnavailableView(
                    t("Weft could not start", "Weft n’a pas pu démarrer"),
                    systemImage: "exclamationmark.triangle",
                    description: Text(error)
                )
            } else {
                ProgressView()
            }
        }
        .frame(minWidth: 560, minHeight: 500)
        .sheet(isPresented: $model.showingPairing, onDismiss: model.closePairing) {
            PairingSheet(model: model)
        }
        .onAppear { model.openWindow = openWindow }
    }

    private func devices(_ snapshot: CoreSnapshot) -> some View {
        NavigationSplitView {
            List(selection: $model.selection) {
                Section(t("Devices", "Appareils")) {
                    ForEach(snapshot.devices) { device in
                        DeviceRow(device: device, selected: model.selection == device.id)
                            .tag(device.id)
                    }
                }
            }
            .background(PlainTableSelection())
            .safeAreaInset(edge: .bottom) {
                StatusFooter(snapshot: snapshot)
            }
            .navigationSplitViewColumnWidth(min: 200, ideal: 220, max: 300)
        } detail: {
            if let device = snapshot.device(model.selection) {
                DeviceDetail(model: model, device: device)
            } else {
                ContentUnavailableView(
                    t("No device selected", "Aucun appareil sélectionné"),
                    systemImage: "sidebar.left"
                )
            }
        }
        .toolbar {
            ToolbarItem {
                Button(action: model.pair) {
                    Label(t("Pair a Device…", "Associer un appareil…"), systemImage: "plus")
                }
                .help(t("Pair a device", "Associer un appareil"))
            }
        }
    }
}

private struct DeviceRow: View {
    let device: DeviceInfo
    let selected: Bool

    var body: some View {
        Label {
            VStack(alignment: .leading, spacing: 2) {
                Text(device.displayName)
                Text(caption)
                    .font(.caption)
                    .opacity(selected ? 0.75 : 1)
                    .foregroundStyle(selected ? AnyShapeStyle(Color.weftInk) : AnyShapeStyle(.secondary))
                    .monospacedDigit()
            }
        } icon: {
            Image(systemName: device.symbol)
                .foregroundStyle(selected ? Color.weftInk : Color.secondary)
        }
        .padding(.vertical, 2)
        .modifier(SelectionHighlight(selected: selected))
    }

    /// "Android · 87%", or the short id until the platform is known.
    private var caption: String {
        var parts = [device.platformName ?? device.shortId]
        if let battery = device.battery {
            parts.append(Words.battery(level: battery.level))
        }
        return parts.joined(separator: " · ")
    }
}

/// Where the connection stands, under the device list.
struct StatusFooter: View {
    let snapshot: CoreSnapshot

    var body: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(snapshot.isConnected ? Color.weftOrange : Color.secondary.opacity(0.5))
                .frame(width: 8, height: 8)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 0) {
                Text(snapshot.statusTitle)
                Text(snapshot.statusReason ?? Words.relayHost(snapshot.relayUrl))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .font(.caption)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .help(snapshot.statusReason ?? snapshot.relayUrl)
    }
}

/// Before any pairing: what Weft does, and the one thing to do.
private struct EmptyState: View {
    @ObservedObject var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            BrandMark()
                .frame(width: 64, height: 64)
                .padding(.bottom, 20)
            Text(t("Pair your first device", "Associez un premier appareil"))
                .font(.title2)
                .padding(.bottom, 8)
            Text(t(
                "Weft keeps this Mac’s clipboard in step with your phone and your other computers, encrypted end to end.",
                "Weft accorde le presse-papiers de ce Mac avec votre téléphone et vos autres ordinateurs, chiffré de bout en bout."
            ))
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: 360)
            .padding(.bottom, 24)
            Button(t("Pair a Device…", "Associer un appareil…"), action: model.pair)
                .buttonStyle(WeftProminentButtonStyle(large: true))
                .keyboardShortcut(.defaultAction)
            if let snapshot = model.snapshot {
                StatusFooter(snapshot: snapshot)
                    .frame(maxWidth: 280)
                    .padding(.top, 32)
            }
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
