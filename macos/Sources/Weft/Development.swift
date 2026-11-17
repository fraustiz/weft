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
// | `WEFT_DEV_SHOW`           | `main`, `settings`, `main+settings` or `menu`: what to open at launch |
// | `WEFT_DEV_SETTINGS_TAB`   | `general`, `features` or `advanced`: the Settings tab    |
// | `WEFT_DEV_APPEARANCE`     | `light` or `dark`: this app's appearance, for screenshots |
// | `WEFT_DEV_DOWNLOADS`      | Where received files go, instead of ~/Downloads          |
// | `WEFT_DEV_LOG`            | File the kind of each notice is appended to (no content) |
// | `WEFT_DEV_NO_OPEN`        | `1`: received links are not opened in the browser        |
// | `WEFT_DEV_SEND_FILE`      | Offered once to each device that takes files, as soon as this file exists |
// | `WEFT_DEV_UPDATE`         | `now`: checks for updates at launch, installs one at once and relaunches |
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

    /// `WEFT_DEV_SHOW=main|settings|main+settings|menu`: what to open at launch, for
    /// screenshots.
    static var show: String? { environment["WEFT_DEV_SHOW"] }

    /// `WEFT_DEV_SETTINGS_TAB`: the Settings tab shown first.
    static var settingsTab: String? { environment["WEFT_DEV_SETTINGS_TAB"] }

    /// `WEFT_DEV_APPEARANCE=light|dark`: this app's appearance, whatever
    /// the system's, so screenshots of both need no change to the Mac.
    static var appearance: String? { environment["WEFT_DEV_APPEARANCE"] }

    /// `WEFT_DEV_DOWNLOADS`: where received files go.
    static var downloadsDirectory: URL? {
        environment["WEFT_DEV_DOWNLOADS"].map { URL(fileURLWithPath: $0, isDirectory: true) }
    }

    /// `WEFT_DEV_NO_OPEN=1`: a received link is logged, not opened.
    static var opensLinks: Bool { environment["WEFT_DEV_NO_OPEN"] != "1" }

    /// `WEFT_DEV_SEND_FILE`: a file offered once to each device that takes
    /// files, as soon as it exists, so a script can test sending.
    static var fileToSend: URL? {
        environment["WEFT_DEV_SEND_FILE"].map { URL(fileURLWithPath: $0) }
    }

    /// `WEFT_DEV_UPDATE=now`: checks for updates at launch, and installs a
    /// downloaded one at once, relaunching the app, instead of when it
    /// quits (Updates.swift). For the end-to-end test of updates only: it
    /// still takes an appcast and an archive signed with the app's key.
    static var installsUpdateNow: Bool { environment["WEFT_DEV_UPDATE"] == "now" }

    /// Appends `line` to the `WEFT_DEV_LOG` file, if set. Only what kind of
    /// thing happened goes there: never a text, a link or a name.
    static func record(_ line: String) {
        guard let path = environment["WEFT_DEV_LOG"] else { return }
        let url = URL(fileURLWithPath: path)
        let data = Data("\(line)\n".utf8)
        if let handle = try? FileHandle(forWritingTo: url) {
            defer { try? handle.close() }
            _ = try? handle.seekToEnd()
            try? handle.write(contentsOf: data)
        } else {
            FileManager.default.createFile(atPath: path, contents: data, attributes: [.posixPermissions: 0o600])
        }
    }

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
