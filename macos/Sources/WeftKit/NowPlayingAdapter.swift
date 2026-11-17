// What plays on this Mac, as mediaremote-adapter reports it
// (https://github.com/ungive/mediaremote-adapter, BSD-3-Clause), and when
// to tell the other devices (docs/features.md section 6, `media`).
//
// The adapter's `stream --micros` prints one JSON object per line:
// `{"type": "data", "diff": Bool, "payload": {...}}`. A payload without
// `diff` is the whole state; with it, only the keys that changed, a key
// set to `null` being one that went away. An empty state: nothing plays.

import Foundation
import ImageIO
import UniformTypeIdentifiers

/// The player the system shows as now playing.
public struct NowPlayingTrack: Equatable, Sendable {
    /// The app that plays, which is also the player's id.
    public var bundleIdentifier: String
    /// The app around it, when the player is a part of another app.
    public var parentBundleIdentifier: String?
    public var title: String
    public var artist: String
    public var album: String
    public var playing: Bool
    /// The position at `timestampMs`, in ms.
    public var elapsedMs: UInt64?
    /// When `elapsedMs` was right, in ms since 1970.
    public var timestampMs: Int64?
    /// 1 at normal speed; the position moves on with it while playing.
    public var rate: Double
    public var lengthMs: UInt64?
    /// The cover, base64, as the adapter gives it.
    public var artwork: String?

    public init(
        bundleIdentifier: String, parentBundleIdentifier: String? = nil, title: String,
        artist: String = "", album: String = "", playing: Bool, elapsedMs: UInt64? = nil,
        timestampMs: Int64? = nil, rate: Double = 1, lengthMs: UInt64? = nil, artwork: String? = nil
    ) {
        self.bundleIdentifier = bundleIdentifier
        self.parentBundleIdentifier = parentBundleIdentifier
        self.title = title
        self.artist = artist
        self.album = album
        self.playing = playing
        self.elapsedMs = elapsedMs
        self.timestampMs = timestampMs
        self.rate = rate
        self.lengthMs = lengthMs
        self.artwork = artwork
    }

    /// Reads a full payload (`--micros` keys). `nil` when nothing plays:
    /// an empty payload, or one without its app or its title.
    public init?(payload: [String: Any]) {
        guard let bundle = payload["bundleIdentifier"] as? String, !bundle.isEmpty,
              let title = payload["title"] as? String else { return nil }
        func micros(_ key: String) -> Double? {
            guard let value = payload[key] as? NSNumber else { return nil }
            let number = value.doubleValue
            return number.isFinite ? number : nil
        }
        func milliseconds(_ key: String) -> UInt64? {
            micros(key).flatMap { $0 >= 0 ? UInt64($0 / 1000) : nil }
        }
        let length = milliseconds("durationMicros")
        self.init(
            bundleIdentifier: bundle,
            parentBundleIdentifier: (payload["parentApplicationBundleIdentifier"] as? String)
                .flatMap { $0.isEmpty ? nil : $0 },
            title: title,
            artist: payload["artist"] as? String ?? "",
            album: payload["album"] as? String ?? "",
            playing: (payload["playing"] as? NSNumber)?.boolValue ?? false,
            elapsedMs: milliseconds("elapsedTimeMicros"),
            timestampMs: micros("timestampEpochMicros").map { Int64($0 / 1000) },
            rate: micros("playbackRate") ?? 1,
            lengthMs: length == 0 ? nil : length,
            artwork: (payload["artworkData"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        )
    }

    /// The position at `nowMs`: the elapsed time moved on by the time since
    /// its timestamp at its rate while playing, up to the length.
    public func positionMs(atMs nowMs: Int64) -> UInt64? {
        guard let elapsedMs else { return nil }
        var position = Double(elapsedMs)
        if playing, let timestampMs, nowMs > timestampMs {
            let rate = rate > 0 ? rate : 1
            position += Double(nowMs - timestampMs) * rate
        }
        let moved = UInt64(max(0, position))
        if let lengthMs { return min(moved, lengthMs) }
        return moved
    }

    /// Whether `other` is the same thing to show, apart from the position.
    public func sameShown(as other: NowPlayingTrack) -> Bool {
        bundleIdentifier == other.bundleIdentifier && parentBundleIdentifier == other.parentBundleIdentifier
            && title == other.title && artist == other.artist && album == other.album
            && playing == other.playing && lengthMs == other.lengthMs && artwork == other.artwork
    }
}

/// The adapter's stream, line after line, folded into the current state.
public struct NowPlayingStream {
    private var state: [String: Any] = [:]

    public init() {}

    /// Folds one line in; returns false when it is not one of the
    /// adapter's data lines (it is then ignored).
    @discardableResult
    public mutating func apply(line: Data) -> Bool {
        guard let object = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
              object["type"] as? String == "data",
              let payload = object["payload"] as? [String: Any] else { return false }
        if (object["diff"] as? NSNumber)?.boolValue == true {
            for (key, value) in payload {
                if value is NSNull { state[key] = nil } else { state[key] = value }
            }
        } else {
            state = payload
        }
        return true
    }

    /// What plays now, if anything.
    public var track: NowPlayingTrack? { NowPlayingTrack(payload: state) }
}

/// When to send this Mac's players again. Receivers move a playing
/// player's position on by themselves, so the position alone is sent again
/// only when it jumps (a seek), not as it drifts.
public struct NowPlayingSendPolicy {
    /// A position further than this from where it should be is a seek.
    public static let seekJumpMs: Int64 = 2_000

    /// What was sent last, and when.
    public struct Sent: Equatable {
        public var track: NowPlayingTrack?
        public var positionMs: UInt64?
        public var atMs: Int64

        public init(track: NowPlayingTrack?, positionMs: UInt64?, atMs: Int64) {
            self.track = track
            self.positionMs = positionMs
            self.atMs = atMs
        }
    }

    public static func needsSend(last: Sent?, next: NowPlayingTrack?, nowMs: Int64) -> Bool {
        guard let last else { return true }
        switch (last.track, next) {
        case (nil, nil):
            return false
        case (nil, _), (_, nil):
            return true
        case (let before?, let after?):
            if !before.sameShown(as: after) { return true }
            let expected = MediaClock.position(
                positionMs: last.positionMs, lengthMs: before.lengthMs, playing: before.playing,
                receivedAtMs: last.atMs, nowMs: nowMs
            )
            let actual = after.positionMs(atMs: nowMs)
            switch (expected, actual) {
            case (nil, nil): return false
            case (let expected?, let actual?): return abs(Int64(actual) - Int64(expected)) > seekJumpMs
            default: return true
            }
        }
    }
}

/// Coalesces bursts of changes: the first change after a quiet moment goes
/// out at once, the next ones at most once per window.
public struct Coalescer {
    public let windowMs: Int64
    private var lastFlushMs: Int64?
    private var pending = false

    public init(windowMs: Int64) {
        self.windowMs = windowMs
    }

    /// A change at `nowMs`: the delay before flushing, in ms, or `nil` when
    /// a flush is already on its way (it will see this change too).
    public mutating func changed(atMs nowMs: Int64) -> Int64? {
        guard !pending else { return nil }
        pending = true
        guard let lastFlushMs else { return 0 }
        return max(0, lastFlushMs + windowMs - nowMs)
    }

    /// The flush ran at `nowMs`.
    public mutating func flushed(atMs nowMs: Int64) {
        pending = false
        lastFlushMs = nowMs
    }

    /// Forgets the pending flush, after it was dropped.
    public mutating func reset() {
        pending = false
    }
}

/// Covers as `media.art` wants them: the longer side at most `maxSide`
/// pixels, JPEG.
public enum CoverArt {
    public static let maxSide = 300
    public static let quality = 0.8

    /// The image `data` scaled down (never up) and encoded as JPEG; `nil`
    /// when it is not an image.
    public static func jpeg(from data: Data, maxSide: Int = maxSide, quality: Double = quality) -> Data? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              CGImageSourceGetCount(source) > 0 else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxSide,
            kCGImageSourceShouldCacheImmediately: true,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else {
            return nil
        }
        let output = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(
            output, UTType.jpeg.identifier as CFString, 1, nil
        ) else { return nil }
        CGImageDestinationAddImage(
            destination, image, [kCGImageDestinationLossyCompressionQuality: quality] as CFDictionary
        )
        guard CGImageDestinationFinalize(destination) else { return nil }
        return output as Data
    }

    /// The base64 cover of the adapter, made ready to send.
    public static func jpeg(fromBase64 text: String) -> Data? {
        Data(base64Encoded: text, options: .ignoreUnknownCharacters).flatMap { jpeg(from: $0) }
    }
}
