import AppKit
import ImageIO
import Testing
@testable import WeftKit

private func line(_ object: [String: Any]) -> Data {
    try! JSONSerialization.data(withJSONObject: object)
}

@Suite struct NowPlayingStreamTests {
    /// A payload as `stream --micros` prints it.
    nonisolated(unsafe) static let full: [String: Any] = [
        "bundleIdentifier": "com.example.player",
        "title": "Song",
        "artist": "Band",
        "album": "Record",
        "playing": true,
        "durationMicros": 180_000_000,
        "elapsedTimeMicros": 30_000_000,
        "timestampEpochMicros": 1_000_000_000,
        "playbackRate": 1,
        "artworkData": "aGVsbG8=",
        "artworkMimeType": "image/jpeg",
        "trackNumber": 1,
    ]

    @Test func readsAFullPayload() throws {
        var applied: Bool
        var stream = NowPlayingStream()
        applied = stream.apply(line: line(["type": "data", "diff": false, "payload": Self.full]))
        #expect(applied)
        let track = try #require(stream.track)
        #expect(track.bundleIdentifier == "com.example.player")
        #expect(track.title == "Song")
        #expect(track.artist == "Band")
        #expect(track.album == "Record")
        #expect(track.playing)
        #expect(track.lengthMs == 180_000)
        #expect(track.elapsedMs == 30_000)
        #expect(track.timestampMs == 1_000_000)
        #expect(track.artwork == "aGVsbG8=")
    }

    @Test func foldsDiffsIntoTheState() throws {
        var stream = NowPlayingStream()
        stream.apply(line: line(["type": "data", "diff": false, "payload": Self.full]))
        stream.apply(line: line(["type": "data", "diff": true, "payload": [
            "playing": false, "artist": NSNull(), "elapsedTimeMicros": 31_500_000,
        ]]))
        let track = try #require(stream.track)
        #expect(!track.playing)
        #expect(track.artist == "")
        #expect(track.elapsedMs == 31_500)
        #expect(track.title == "Song")
        #expect(track.artwork == "aGVsbG8=")
    }

    @Test func aFullPayloadReplacesEverything() throws {
        var stream = NowPlayingStream()
        stream.apply(line: line(["type": "data", "diff": false, "payload": Self.full]))
        stream.apply(line: line(["type": "data", "diff": false, "payload": [
            "bundleIdentifier": "com.example.other", "title": "Other", "playing": false,
        ]]))
        let track = try #require(stream.track)
        #expect(track.bundleIdentifier == "com.example.other")
        #expect(track.album == "")
        #expect(track.lengthMs == nil)
        #expect(track.artwork == nil)
    }

    @Test func anEmptyPayloadMeansNothingPlays() {
        var applied: Bool
        var stream = NowPlayingStream()
        stream.apply(line: line(["type": "data", "diff": false, "payload": Self.full]))
        applied = stream.apply(line: line(["type": "data", "diff": false, "payload": [String: Any]()]))
        #expect(applied)
        #expect(stream.track == nil)
    }

    @Test func ignoresWhatIsNotAnAdapterLine() {
        var applied: Bool
        var stream = NowPlayingStream()
        applied = stream.apply(line: Data("not json".utf8))
        #expect(!applied)
        applied = stream.apply(line: line(["type": "other", "payload": Self.full]))
        #expect(!applied)
        #expect(stream.track == nil)
    }

    @Test func needsAnAppAndATitle() {
        #expect(NowPlayingTrack(payload: ["title": "Song", "playing": true]) == nil)
        #expect(NowPlayingTrack(payload: ["bundleIdentifier": "com.example.player", "playing": true]) == nil)
        // A zero duration is no duration (a live stream).
        let live = NowPlayingTrack(payload: ["bundleIdentifier": "b", "title": "t", "durationMicros": 0])
        #expect(live?.lengthMs == nil)
        #expect(live?.playing == false)
    }

    @Test func thePositionMovesOnWithTheTimestampAndRate() throws {
        let track = try #require(NowPlayingTrack(payload: Self.full))
        #expect(track.positionMs(atMs: 1_000_000) == 30_000)
        #expect(track.positionMs(atMs: 1_004_000) == 34_000)
        var fast = track
        fast.rate = 2
        #expect(fast.positionMs(atMs: 1_004_000) == 38_000)
        // Up to the length.
        #expect(track.positionMs(atMs: 2_000_000) == 180_000)
        var paused = track
        paused.playing = false
        #expect(paused.positionMs(atMs: 1_004_000) == 30_000)
    }
}

@Suite struct NowPlayingSendPolicyTests {
    let track = NowPlayingTrack(
        bundleIdentifier: "com.example.player", title: "Song", artist: "Band", playing: true,
        elapsedMs: 10_000, timestampMs: 0, lengthMs: 200_000
    )

    private func sent(_ track: NowPlayingTrack?, at: Int64) -> NowPlayingSendPolicy.Sent {
        .init(track: track, positionMs: track?.positionMs(atMs: at), atMs: at)
    }

    @Test func sendsTheFirstStateEvenEmpty() {
        #expect(NowPlayingSendPolicy.needsSend(last: nil, next: nil, nowMs: 0))
        #expect(NowPlayingSendPolicy.needsSend(last: nil, next: track, nowMs: 0))
    }

    @Test func nothingToNothingIsNotSentAgain() {
        #expect(!NowPlayingSendPolicy.needsSend(last: sent(nil, at: 0), next: nil, nowMs: 10))
    }

    @Test func startingAndStoppingAreSent() {
        #expect(NowPlayingSendPolicy.needsSend(last: sent(nil, at: 0), next: track, nowMs: 10))
        #expect(NowPlayingSendPolicy.needsSend(last: sent(track, at: 0), next: nil, nowMs: 10))
    }

    @Test func driftIsNotSent() {
        // Played on for 30 s, just as receivers expect.
        var later = track
        later.elapsedMs = 40_000
        later.timestampMs = 30_000
        #expect(!NowPlayingSendPolicy.needsSend(last: sent(track, at: 0), next: later, nowMs: 30_000))
        // A second off: still drift.
        later.elapsedMs = 41_000
        #expect(!NowPlayingSendPolicy.needsSend(last: sent(track, at: 0), next: later, nowMs: 30_000))
    }

    @Test func aSeekIsSent() {
        var seeked = track
        seeked.elapsedMs = 90_000
        seeked.timestampMs = 5_000
        #expect(NowPlayingSendPolicy.needsSend(last: sent(track, at: 0), next: seeked, nowMs: 5_000))
        // Back to the start.
        seeked.elapsedMs = 0
        #expect(NowPlayingSendPolicy.needsSend(last: sent(track, at: 0), next: seeked, nowMs: 5_000))
    }

    @Test func playPauseTrackAndCoverChangesAreSent() {
        let last = sent(track, at: 0)
        var paused = track
        paused.playing = false
        #expect(NowPlayingSendPolicy.needsSend(last: last, next: paused, nowMs: 100))
        var next = track
        next.title = "Next song"
        #expect(NowPlayingSendPolicy.needsSend(last: last, next: next, nowMs: 100))
        var covered = track
        covered.artwork = "aGVsbG8="
        #expect(NowPlayingSendPolicy.needsSend(last: last, next: covered, nowMs: 100))
        var other = track
        other.bundleIdentifier = "com.example.other"
        #expect(NowPlayingSendPolicy.needsSend(last: last, next: other, nowMs: 100))
    }

    @Test func aPausedPlayerStaysWhereItWas() {
        var paused = track
        paused.playing = false
        let last = sent(paused, at: 0)
        #expect(!NowPlayingSendPolicy.needsSend(last: last, next: paused, nowMs: 60_000))
    }
}

@Suite struct CoalescerTests {
    @Test func theFirstChangeGoesAtOnce() {
        var delay: Int64?
        var coalescer = Coalescer(windowMs: 250)
        delay = coalescer.changed(atMs: 1_000)
        #expect(delay == 0)
    }

    @Test func changesWhileAFlushWaitsJoinIt() {
        var delay: Int64?
        var coalescer = Coalescer(windowMs: 250)
        delay = coalescer.changed(atMs: 1_000)
        #expect(delay == 0)
        delay = coalescer.changed(atMs: 1_001)
        #expect(delay == nil)
        delay = coalescer.changed(atMs: 1_002)
        #expect(delay == nil)
        coalescer.flushed(atMs: 1_003)
        // Right after a flush: wait for the rest of the window.
        delay = coalescer.changed(atMs: 1_100)
        #expect(delay == 153)
        delay = coalescer.changed(atMs: 1_150)
        #expect(delay == nil)
        coalescer.flushed(atMs: 1_253)
        // After a quiet moment: at once again.
        delay = coalescer.changed(atMs: 5_000)
        #expect(delay == 0)
    }

    @Test func aResetForgetsThePendingFlush() {
        var delay: Int64?
        var coalescer = Coalescer(windowMs: 250)
        _ = coalescer.changed(atMs: 0)
        coalescer.reset()
        delay = coalescer.changed(atMs: 10)
        #expect(delay == 0)
    }
}

@Suite struct CoverArtTests {
    private func png(width: Int, height: Int) throws -> Data {
        let rep = try #require(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8,
            samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
            bytesPerRow: 0, bitsPerPixel: 0
        ))
        return try #require(rep.representation(using: .png, properties: [:]))
    }

    private func size(_ data: Data) throws -> (Int, Int, String) {
        let source = try #require(CGImageSourceCreateWithData(data as CFData, nil))
        let image = try #require(CGImageSourceCreateImageAtIndex(source, 0, nil))
        return (image.width, image.height, CGImageSourceGetType(source) as String? ?? "")
    }

    @Test func scalesTheLongerSideDownToTheLimit() throws {
        let jpeg = try #require(CoverArt.jpeg(from: try png(width: 1200, height: 600)))
        let (width, height, type) = try size(jpeg)
        #expect(width == 300)
        #expect(height == 150)
        #expect(type == "public.jpeg")
    }

    @Test func neverScalesUp() throws {
        let jpeg = try #require(CoverArt.jpeg(from: try png(width: 100, height: 120)))
        let (width, height, _) = try size(jpeg)
        #expect(width == 100)
        #expect(height == 120)
    }

    @Test func readsTheAdaptersBase64() throws {
        let text = try png(width: 640, height: 640).base64EncodedString()
        let jpeg = try #require(CoverArt.jpeg(fromBase64: text))
        #expect(try size(jpeg).0 == 300)
        #expect(CoverArt.jpeg(fromBase64: "aGVsbG8=") == nil)
    }
}
