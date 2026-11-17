// What the app says about the core's snapshot.

import Foundation
import WeftFFI
import WeftKit

extension CoreSnapshot {
    var isConnected: Bool {
        if case .connected = status { return true }
        return false
    }

    /// One line on where the connection stands.
    var statusTitle: String {
        if paused { return t("Paused", "En pause") }
        switch status {
        case .connecting:
            return t("Connecting…", "Connexion…")
        case .connected:
            return t("Connected", "Connecté")
        case .offline:
            return t("Reconnecting…", "Reconnexion…")
        case .stopped:
            return t("Stopped", "Arrêté")
        }
    }

    /// Why it is not connected, as the core put it, when it is not.
    var statusReason: String? {
        switch status {
        case .offline(let reason, _), .stopped(let reason):
            return reason
        case .connecting, .connected:
            return nil
        }
    }

    func device(_ id: String?) -> DeviceInfo? {
        devices.first { $0.id == id }
    }
}

extension DeviceInfo: Identifiable {}

extension DeviceInfo {
    var displayName: String {
        name ?? t("Unnamed device", "Appareil sans nom")
    }

    var platformName: String? { Words.platform(platform) }

    var symbol: String { Words.symbol(platform) }

    /// "87%", or "87% · Charging".
    var batteryText: String? {
        guard let battery else { return nil }
        let figure = Words.battery(level: battery.level)
        guard let state = Words.batteryState(charging: battery.charging, low: battery.low) else {
            return figure
        }
        return "\(figure) · \(state)"
    }
}

extension LastActivity {
    /// "Last text sent 2 minutes ago", without the text.
    func sentence(now: Date = .now) -> String {
        let at = Date(timeIntervalSince1970: TimeInterval(atMs) / 1000)
        let what = direction == .sent
            ? t("Last text sent", "Dernier texte envoyé")
            : t("Last text received", "Dernier texte reçu")
        if now.timeIntervalSince(at) < 60 {
            return "\(what) \(t("just now", "à l’instant"))"
        }
        let formatter = RelativeDateTimeFormatter()
        formatter.locale = Locale(identifier: Language.current == .french ? "fr_FR" : "en_US")
        formatter.unitsStyle = .full
        return "\(what) \(formatter.localizedString(for: at, relativeTo: now))"
    }
}

extension PairingFailureKind {
    /// Why a pairing failed, as a sentence the user can act on.
    var text: String {
        switch self {
        case .invalidCode:
            t("This is not a Weft pairing code. Check that it was copied whole.",
              "Ce n’est pas un code d’association Weft. Vérifiez qu’il a été copié en entier.")
        case .expired:
            t("This code has expired: a code lasts 5 minutes.",
              "Ce code a expiré : un code dure 5 minutes.")
        case .ownCode:
            t("This code was made on this Mac. Enter it on the other device instead.",
              "Ce code a été créé sur ce Mac. Saisissez-le plutôt sur l’autre appareil.")
        case .otherRelay:
            t("This code is for a device on another relay. Both devices must use the same relay.",
              "Ce code vient d’un appareil sur un autre relais. Les deux appareils doivent utiliser le même relais.")
        case .groupFull:
            t("There are already as many devices as Weft allows. Remove one to add another.",
              "Weft a déjà autant d’appareils qu’il en accepte. Retirez-en un pour en ajouter un autre.")
        case .notConnected:
            t("Weft is not connected to the relay. Check the network, then try again.",
              "Weft n’est pas connecté au relais. Vérifiez le réseau, puis réessayez.")
        case .connectionLost:
            t("The connection to the relay was lost during pairing.",
              "La connexion au relais a été perdue pendant l’association.")
        case .noAnswer:
            t("The other device did not answer in time. Check that it still shows the code.",
              "L’autre appareil n’a pas répondu à temps. Vérifiez qu’il affiche toujours le code.")
        case .other(let message):
            message
        }
    }
}
