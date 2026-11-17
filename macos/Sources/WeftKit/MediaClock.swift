// Where another device's player is now: a `media` packet gives the position
// at the moment it was sent, and a playing player moves on with time
// (docs/features.md section 6).

import Foundation

public enum MediaClock {
    /// The position `nowMs - receivedAtMs` after it was read, up to the
    /// length when there is one; the same position while paused. `nil`
    /// when the player gave none.
    public static func position(
        positionMs: UInt64?,
        lengthMs: UInt64?,
        playing: Bool,
        receivedAtMs: Int64,
        nowMs: Int64
    ) -> UInt64? {
        guard let positionMs else { return nil }
        guard playing else { return positionMs }
        let elapsed = UInt64(max(0, nowMs - receivedAtMs))
        let (moved, overflow) = positionMs.addingReportingOverflow(elapsed)
        let position = overflow ? UInt64.max : moved
        if let lengthMs, lengthMs > 0 { return min(position, lengthMs) }
        return position
    }

    /// A track time: `3:07`, or `1:02:45` past an hour.
    public static func time(_ ms: UInt64) -> String {
        let seconds = ms / 1000
        let (hours, minutes, rest) = (seconds / 3600, seconds / 60 % 60, seconds % 60)
        if hours > 0 {
            return String(format: "%llu:%02llu:%02llu", hours, minutes, rest)
        }
        return String(format: "%llu:%02llu", minutes, rest)
    }

    /// Whether a position the user dragged to is a seek worth sending,
    /// rather than the bar following the clock: more than 1.5 s away.
    public static func isSeek(from shownMs: UInt64?, to wantedMs: UInt64) -> Bool {
        guard let shownMs else { return true }
        let distance = shownMs > wantedMs ? shownMs - wantedMs : wantedMs - shownMs
        return distance > 1500
    }
}
