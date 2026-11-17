// In-app updates with Sparkle 2: an appcast on GitHub Releases, its
// archives checked with an EdDSA key (Info.plist SUPublicEDKey), once a day
// by default.
//
// Only an app built by scripts/build-app.sh with a public key checks for
// updates: `swift run`, and the development bundle (WEFT_DEV_BUNDLE=1,
// written without SUFeedURL), never do, so they cannot replace anything.

import AppKit
import Combine
import Foundation
import Sparkle
import WeftKit

@MainActor
final class Updates: NSObject, ObservableObject {
    /// Whether this build updates itself at all.
    let available: Bool
    /// The updater is ready for « Check for Updates… » (not already busy).
    @Published private(set) var canCheck = false
    /// Mirrors the updater's setting, kept in this app's preferences.
    @Published private(set) var checksAutomatically = false

    private var controller: SPUStandardUpdaterController?
    private var observers: Set<AnyCancellable> = []

    override init() {
        available = Self.configured
        super.init()
        guard available else { return }
        let controller = SPUStandardUpdaterController(
            startingUpdater: false, updaterDelegate: self, userDriverDelegate: self
        )
        self.controller = controller
        let updater = controller.updater
        updater.publisher(for: \.canCheckForUpdates)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] can in self?.canCheck = can }
            .store(in: &observers)
        updater.publisher(for: \.automaticallyChecksForUpdates)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] on in self?.checksAutomatically = on }
            .store(in: &observers)
        controller.startUpdater()
        if Development.installsUpdateNow {
            updater.checkForUpdatesInBackground()
        }
    }

    /// « 1.2.3 (456) », from Info.plist.
    static var version: String {
        let info = Bundle.main.infoDictionary ?? [:]
        let short = info["CFBundleShortVersionString"] as? String
        let build = info["CFBundleVersion"] as? String
        switch (short, build) {
        case let (short?, build?): return "\(short) (\(build))"
        case let (short?, nil): return short
        default: return t("Development build", "Version de développement")
        }
    }

    func check() {
        controller?.checkForUpdates(nil)
    }

    func setChecksAutomatically(_ on: Bool) {
        controller?.updater.automaticallyChecksForUpdates = on
    }

    /// A bundle (not `swift run`) whose Info.plist has a feed and a public
    /// key: what scripts/build-app.sh writes for Weft.app only.
    private static var configured: Bool {
        let bundle = Bundle.main
        guard bundle.bundleURL.pathExtension == "app" else { return false }
        let feed = bundle.object(forInfoDictionaryKey: "SUFeedURL") as? String ?? ""
        let key = bundle.object(forInfoDictionaryKey: "SUPublicEDKey") as? String ?? ""
        return !feed.isEmpty && !key.isEmpty
    }
}

extension Updates: SPUUpdaterDelegate {
    /// An update downloaded in the background waits for the app to quit;
    /// with `WEFT_DEV_UPDATE=now` it is installed at once and the app
    /// relaunched, so the whole update can be tested without a click.
    nonisolated func updater(
        _ updater: SPUUpdater,
        willInstallUpdateOnQuit item: SUAppcastItem,
        immediateInstallationBlock immediateInstallHandler: @escaping () -> Void
    ) -> Bool {
        guard Development.installsUpdateNow else { return false }
        immediateInstallHandler()
        return true
    }
}

extension Updates: SPUStandardUserDriverDelegate {
    /// A menu bar app has no Dock icon to bounce: Sparkle asks it to make
    /// a scheduled update noticed itself.
    nonisolated var supportsGentleScheduledUpdateReminders: Bool { true }

    /// A daily check found an update: its window is brought to the front,
    /// over the other apps.
    nonisolated func standardUserDriverWillHandleShowingUpdate(
        _ handleShowingUpdate: Bool,
        forUpdate update: SUAppcastItem,
        state: SPUUserUpdateState
    ) {
        guard handleShowingUpdate, !state.userInitiated else { return }
        MainActor.assumeIsolated { NSApp.activate() }
    }
}
