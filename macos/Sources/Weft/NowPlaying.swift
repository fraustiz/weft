// What plays on this Mac, for the other devices (docs/features.md section
// 6, `media`), and their commands to it.
//
// macOS has no public API for it: the private MediaRemote framework only
// answers entitled processes, so mediaremote-adapter (BSD-3-Clause,
// bundled by scripts/build-app.sh, notice in macos/THIRD_PARTY_NOTICES.md)
// has /usr/bin/perl, which is entitled, load a small framework that prints
// the now-playing state as JSON lines. Its `test` command says at start
// whether that still works on this macOS; when it does not, this Mac
// simply announces no player.
//
// Threads: the adapter's output is read and parsed on a thread of its own,
// where the cover is also scaled; the state then goes to the main actor,
// which decides what to send to the core.

import AppKit
import Foundation
import WeftFFI
import WeftKit

/// The adapter's files in the app bundle.
struct MediaAdapter: Sendable {
    let script: URL
    let framework: URL
    let testClient: URL

    /// The adapter bundled with this app, if it is (not with `swift run`).
    static func bundled() -> MediaAdapter? {
        let contents = Bundle.main.bundleURL.appendingPathComponent("Contents", isDirectory: true)
        let adapter = MediaAdapter(
            script: contents.appendingPathComponent("Resources/MediaRemoteAdapter/mediaremote-adapter.pl"),
            framework: contents.appendingPathComponent("Frameworks/MediaRemoteAdapter.framework", isDirectory: true),
            testClient: contents.appendingPathComponent("Helpers/MediaRemoteAdapterTestClient")
        )
        let files = [adapter.script, adapter.framework, adapter.testClient]
        return files.allSatisfy { FileManager.default.fileExists(atPath: $0.path) } ? adapter : nil
    }

    /// Stops the adapters an earlier run of this app left running when it
    /// was killed or crashed (WeftKit/OrphanedHelpers.swift). Blocks a
    /// moment when there are some: call it off the main thread.
    static func stopOrphans() {
        guard let adapter = bundled() else { return }
        let count = OrphanedHelpers.stop(running: adapter.script.path)
        if count > 0 {
            log.info("stopped \(count, privacy: .public) media adapter(s) left by an earlier run")
        }
    }

    /// A process running the adapter with `arguments` after its paths.
    func process(_ arguments: [String]) -> Process {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/perl")
        process.arguments = [script.path, framework.path, testClient.path] + arguments
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        return process
    }

    /// Runs a short command and waits for it, `timeout` at most; returns
    /// its exit status (`nil`: it could not run or took too long). Blocks:
    /// call it off the main thread.
    func run(_ arguments: [String], timeout: TimeInterval = 15) -> Int32? {
        let process = process(arguments)
        let done = DispatchSemaphore(value: 0)
        process.terminationHandler = { _ in done.signal() }
        do {
            try process.run()
        } catch {
            return nil
        }
        if done.wait(timeout: .now() + timeout) == .timedOut {
            process.terminate()
            _ = done.wait(timeout: .now() + 2)
            return nil
        }
        return process.terminationStatus
    }
}

/// One running `stream`: reads its lines on its own thread and hands each
/// new state over, with its cover made ready.
private final class AdapterStream: @unchecked Sendable {
    typealias Update = @Sendable (_ track: NowPlayingTrack?, _ cover: Data?) -> Void

    private let process: Process
    private let pipe = Pipe()

    init(adapter: MediaAdapter, update: @escaping Update, exited: @escaping @Sendable () -> Void) {
        // Diffs, so the cover comes once per change and not with each line;
        // `--debounce` merges the bursts some players make on a new track.
        process = adapter.process(["stream", "--micros", "--debounce=100"])
        process.standardOutput = pipe
        process.terminationHandler = { _ in exited() }
        let handle = pipe.fileHandleForReading
        let thread = Thread {
            var stream = NowPlayingStream()
            var buffer = Data()
            var artwork: String?
            var cover: Data?
            var last: NowPlayingTrack??
            while true {
                let chunk = handle.availableData
                if chunk.isEmpty { break }
                buffer.append(chunk)
                var changed = false
                while let end = buffer.firstIndex(of: 0x0A) {
                    let line = buffer[buffer.startIndex..<end]
                    buffer.removeSubrange(buffer.startIndex...end)
                    if stream.apply(line: Data(line)) { changed = true }
                }
                guard changed else { continue }
                let track = stream.track
                // Scaled again only when the image itself changed.
                if track?.artwork != artwork {
                    artwork = track?.artwork
                    cover = artwork.flatMap(CoverArt.jpeg(fromBase64:))
                }
                if let last, last == track { continue }
                last = .some(track)
                update(track, cover)
            }
        }
        thread.name = "Weft now playing"
        thread.qualityOfService = .utility
        do {
            try process.run()
            thread.start()
        } catch {
            log.error("media adapter did not start: \(String(describing: error), privacy: .public)")
            exited()
        }
    }

    func stop() {
        process.terminationHandler = nil
        // The reader thread ends by itself once the adapter's output closes.
        if process.isRunning { process.terminate() }
    }
}

@MainActor
final class NowPlaying: ObservableObject {
    enum Status { case off, starting, working, unavailable }

    /// Whether this Mac can tell what plays; `unavailable` once the
    /// adapter's own test failed (or it is not in the bundle).
    @Published private(set) var status = Status.off
    /// Called with the players to announce, already coalesced.
    var onChange: (([LocalPlayer]) -> Void)?

    private let adapter = MediaAdapter.bundled()
    /// The adapter's test, once per launch.
    private var tested: Bool?
    private var running = false
    /// Bumped on each start and stop, so late callbacks of an older run
    /// are ignored.
    private var generation = 0
    private var stream: AdapterStream?
    private var streamStarted = Date.distantPast
    private var backoff: TimeInterval = 1
    private var track: NowPlayingTrack?
    private var cover: Data?
    private var sent: NowPlayingSendPolicy.Sent?
    private var coalescer = Coalescer(windowMs: 250)
    private var appNames: [String: String] = [:]

    var isRunning: Bool { running }

    func start() {
        guard !running else { return }
        running = true
        generation += 1
        guard let adapter else {
            log.info("media adapter not bundled: no local player")
            become(.unavailable)
            return
        }
        if let tested {
            tested ? startStream() : become(.unavailable)
            return
        }
        status = .starting
        let run = generation
        Task.detached(priority: .utility) {
            let works = adapter.run(["test"]) == 0
            await MainActor.run { [weak self] in self?.tested(works, run: run) }
        }
    }

    func stop() {
        guard running else { return }
        running = false
        generation += 1
        stream?.stop()
        stream = nil
        status = .off
        track = nil
        cover = nil
        coalescer.reset()
        flush()
    }

    /// A command from another device to one of this Mac's players.
    func command(_ control: MediaControl, player: String) {
        guard running, status == .working, let adapter, let track, track.bundleIdentifier == player else {
            return
        }
        let arguments: [String]
        switch control {
        case .play: arguments = ["send", "0"]
        case .pause: arguments = ["send", "1"]
        case .playPause: arguments = ["send", "2"]
        case .next: arguments = ["send", "4"]
        case .previous: arguments = ["send", "5"]
        case .seek(let positionMs):
            let (micros, overflow) = positionMs.multipliedReportingOverflow(by: 1000)
            arguments = ["seek", String(overflow ? UInt64.max : micros)]
        case .setVolume:
            // The system's players have no volume of their own here.
            return
        }
        Development.record("media adapter \(arguments[0])")
        Task.detached(priority: .userInitiated) {
            if adapter.run(arguments, timeout: 5) != 0 {
                log.error("media adapter command failed")
            }
        }
    }

    // MARK: The adapter

    private func tested(_ works: Bool, run: Int) {
        tested = works
        guard run == generation, running else { return }
        if works {
            startStream()
        } else {
            log.error("media adapter test failed: this macOS does not let it read what plays")
            become(.unavailable)
        }
    }

    private func become(_ status: Status) {
        self.status = status
        Development.record("now playing \(status)")
        if status == .unavailable {
            track = nil
            cover = nil
            changed()
        }
    }

    private func startStream() {
        guard let adapter else { return }
        let run = generation
        streamStarted = .now
        if status != .working { become(.working) }
        stream = AdapterStream(
            adapter: adapter,
            update: { track, cover in
                Task { @MainActor [weak self] in self?.received(track, cover: cover, run: run) }
            },
            exited: {
                Task { @MainActor [weak self] in self?.streamExited(run: run) }
            }
        )
    }

    /// The stream ended without being asked: nothing is known to play
    /// until it runs again, after a wait that grows while it keeps dying.
    private func streamExited(run: Int) {
        guard run == generation, running else { return }
        stream = nil
        if Date.now.timeIntervalSince(streamStarted) > 60 { backoff = 1 }
        let wait = backoff
        backoff = min(backoff * 2, 60)
        log.error("media adapter stream ended; again in \(wait, privacy: .public) s")
        track = nil
        cover = nil
        changed()
        DispatchQueue.main.asyncAfter(deadline: .now() + wait) { [weak self] in
            guard let self, run == self.generation, self.running, self.stream == nil else { return }
            self.startStream()
        }
    }

    private func received(_ track: NowPlayingTrack?, cover: Data?, run: Int) {
        guard run == generation, running else { return }
        self.track = track
        self.cover = track == nil ? nil : cover
        changed()
    }

    // MARK: Sending

    private static func nowMs() -> Int64 { Int64(Date.now.timeIntervalSince1970 * 1000) }

    private func changed() {
        guard let delay = coalescer.changed(atMs: Self.nowMs()) else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(Int(delay))) { [weak self] in
            self?.flush()
        }
    }

    private func flush() {
        let now = Self.nowMs()
        coalescer.flushed(atMs: now)
        guard NowPlayingSendPolicy.needsSend(last: sent, next: track, nowMs: now) else { return }
        let position = track?.positionMs(atMs: now)
        sent = .init(track: track, positionMs: position, atMs: now)
        let players = track.map { [player($0, positionMs: position)] } ?? []
        Development.record("local media \(players.count)")
        onChange?(players)
    }

    private func player(_ track: NowPlayingTrack, positionMs: UInt64?) -> LocalPlayer {
        LocalPlayer(
            id: track.bundleIdentifier,
            app: appName(track.parentBundleIdentifier ?? track.bundleIdentifier),
            title: track.title,
            artist: track.artist,
            album: track.album,
            playing: track.playing,
            positionMs: positionMs,
            lengthMs: track.lengthMs,
            canPlay: true,
            canPause: true,
            canNext: true,
            canPrevious: true,
            canSeek: track.lengthMs != nil,
            cover: cover.map { CoverImage(mime: "image/jpeg", data: $0) }
        )
    }

    /// The name the Finder shows for the app, else its identifier.
    private func appName(_ bundleIdentifier: String) -> String {
        if let name = appNames[bundleIdentifier] { return name }
        var name = bundleIdentifier
        if let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleIdentifier) {
            let bundle = Bundle(url: url)
            let info = bundle?.localizedInfoDictionary ?? [:]
            name = info["CFBundleDisplayName"] as? String
                ?? bundle?.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String
                ?? bundle?.object(forInfoDictionaryKey: "CFBundleName") as? String
                ?? url.deletingPathExtension().lastPathComponent
        }
        appNames[bundleIdentifier] = name
        return name
    }
}
