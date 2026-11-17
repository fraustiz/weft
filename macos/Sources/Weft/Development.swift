// Switches for development and the end-to-end check, read from the
// environment. None is set in normal use: the app then keeps its state in
// ~/Library/Application Support/Weft and a new identity goes to the public
// relay, wss://weft.frohub.eu/v1. macos/scripts/e2e.sh shows them in use.
//
// | Variable                  | Effect                                                  |
// | ------------------------- | ------------------------------------------------------- |
// | `WEFT_DEV_STATE`          | State directory, instead of Application Support         |
// | `WEFT_DEV_RELAY`          | Relay a new identity is created on (ws:// on loopback ok) |
// | `WEFT_DEV_CODE_FILE`      | File the shown pairing code is written to (mode 0600)    |
// | `WEFT_DEV_START_PAIRING`  | `1`: opens the pairing sheet once connected              |
// | `WEFT_DEV_SHOW`           | `main`, `settings` or `menu`: what to open at launch     |
// | `WEFT_LANG`               | `en` or `fr`: forces the interface language              |
// | `WEFT_LOG`                | Level of the core's logs on stderr (`info` by default)   |

import Foundation

enum Development {
    private static let environment = ProcessInfo.processInfo.environment

    /// `WEFT_DEV_STATE`: the state directory, instead of Application Support.
    static var stateDirectory: URL? {
        environment["WEFT_DEV_STATE"].map { URL(fileURLWithPath: $0, isDirectory: true) }
    }

    /// `WEFT_DEV_RELAY`: the relay a new identity is created on, such as
    /// `ws://127.0.0.1:8095/v1`. An existing identity keeps its relay.
    static var relayURL: String? { environment["WEFT_DEV_RELAY"] }

    /// `WEFT_DEV_CODE_FILE`: where each pairing code shown is written, so a
    /// script can pair a command-line client without a camera. The code is
    /// a secret: this is the only place it is ever written out, and only
    /// when this variable is set.
    static var codeFile: URL? {
        environment["WEFT_DEV_CODE_FILE"].map { URL(fileURLWithPath: $0) }
    }

    /// `WEFT_DEV_START_PAIRING=1`: opens the pairing sheet once connected.
    static var startsPairing: Bool { environment["WEFT_DEV_START_PAIRING"] == "1" }

    /// `WEFT_DEV_SHOW=main|settings|menu`: what to open at launch, for
    /// screenshots.
    static var show: String? { environment["WEFT_DEV_SHOW"] }

    /// Writes `code` to the code file, readable by this user only.
    static func writeCode(_ code: String) {
        guard let file = codeFile else { return }
        let created = FileManager.default.createFile(
            atPath: file.path,
            contents: Data(code.utf8),
            attributes: [.posixPermissions: 0o600]
        )
        if !created {
            log.error("could not write the development code file")
        }
    }
}
