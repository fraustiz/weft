// Buffers untrusted peer envelopes arriving during `pairRequest` / `groupJoin`
// until `Welcome` (0x03) from the expected owner settles, then replays them in
// FIFO arrival order (matching Android's `DeferredJoinFramesTest` semantics).

import Foundation
import WeftProtocol

public enum JoinDisposition: Sendable, Equatable {
    /// Envelope is `Welcome` (0x03) from the expected slot/group owner; handle as Welcome now.
    case welcome
    /// Envelope arrived from an untrusted peer (or non-Welcome from untrusted owner) while unsettled; buffered.
    case deferred
    /// Envelope is from an already-trusted peer or the buffer has already settled; process immediately.
    case processNow
}

public final class JoinFrameBuffer: @unchecked Sendable {
    public static let defaultMaxDeferred = 64

    public let ownerHex: String
    public let maxDeferred: Int

    private let lock = NSLock()
    private var settled = false
    private var frames: [Data] = []

    public init(ownerHex: String, maxDeferred: Int = JoinFrameBuffer.defaultMaxDeferred) {
        self.ownerHex = ownerHex.lowercased()
        self.maxDeferred = max(0, maxDeferred)
    }

    /// Classifies an incoming envelope frame and buffers a defensive copy if deferred.
    public func onEnvelope(
        senderHex: String,
        kind: Kind,
        isTrusted: Bool,
        frame: Data
    ) -> JoinDisposition {
        lock.lock()
        defer { lock.unlock() }

        if settled {
            return .processNow
        }
        let normalizedSender = senderHex.lowercased()
        if normalizedSender == ownerHex && kind == .welcome {
            return .welcome
        }
        if isTrusted {
            return .processNow
        }
        if frames.count < maxDeferred {
            // Defensive byte copy so caller mutations never alter buffered frames.
            frames.append(Data([UInt8](frame)))
        }
        return .deferred
    }

    /// Marks the join settled, drains buffered frames in FIFO arrival order,
    /// and causes all future envelopes to return `.processNow`.
    public func settleAndDrain() -> [Data] {
        lock.lock()
        defer { lock.unlock() }

        settled = true
        let drained = frames
        frames.removeAll(keepingCapacity: false)
        return drained
    }

    public var bufferedCount: Int {
        lock.lock()
        defer { lock.unlock() }
        return frames.count
    }

    public var isSettled: Bool {
        lock.lock()
        defer { lock.unlock() }
        return settled
    }
}
