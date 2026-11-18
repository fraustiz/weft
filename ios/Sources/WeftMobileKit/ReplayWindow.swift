// Sliding 1024-counter replay protection window (docs/protocol.md, section 3).

import Foundation
import WeftProtocol

public enum ReplayError: Error, Sendable, Equatable {
    /// Counter 0 is never valid on the wire.
    case zeroCounter
    /// Counter is older than the 1024-message sliding window.
    case tooOld
    /// Message ID or counter was already accepted inside the window.
    case duplicate
    /// Message ID is not 16 bytes.
    case invalidMessageIDLength(Int)
}

public struct SeenMessage: Codable, Sendable, Equatable {
    public let counter: UInt64
    public let idHex: String

    public init(counter: UInt64, idHex: String) {
        self.counter = counter
        self.idHex = idHex.lowercased()
    }

    private enum CodingKeys: String, CodingKey {
        case counter
        case idHex = "id"
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        if let str = try? container.decode(String.self, forKey: .counter),
           let parsed = UInt64(str) {
            self.counter = parsed
        } else {
            self.counter = try container.decode(UInt64.self, forKey: .counter)
        }
        let id = try container.decode(String.self, forKey: .idHex).lowercased()
        guard Data(hexString: id, expectedCount: Weft.messageIDLength) != nil else {
            throw DecodingError.dataCorruptedError(
                forKey: .idHex,
                in: container,
                debugDescription: "Invalid 16-byte hex message ID"
            )
        }
        self.idHex = id
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        // Encode u64 as decimal string so JSON numbers never lose precision above 2^53.
        try container.encode(String(counter), forKey: .counter)
        try container.encode(idHex, forKey: .idHex)
    }
}

public struct ReplayWindow: Codable, Sendable, Equatable {
    public static let windowSize: UInt64 = 1024

    public private(set) var highestCounter: UInt64
    public private(set) var seen: [SeenMessage]

    public init(highestCounter: UInt64 = 0, seen: [SeenMessage] = []) {
        self.highestCounter = highestCounter
        let floor = highestCounter > Self.windowSize ? highestCounter - Self.windowSize : 0
        self.seen = seen.filter { $0.counter > floor }
    }

    private enum CodingKeys: String, CodingKey {
        case highestCounter = "recv_highest"
        case seen = "recv_seen"
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let highest: UInt64
        if let str = try? container.decode(String.self, forKey: .highestCounter),
           let parsed = UInt64(str) {
            highest = parsed
        } else {
            highest = (try? container.decode(UInt64.self, forKey: .highestCounter)) ?? 0
        }
        let entries = (try? container.decode([SeenMessage].self, forKey: .seen)) ?? []
        self.init(highestCounter: highest, seen: entries)
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(String(highestCounter), forKey: .highestCounter)
        try container.encode(seen, forKey: .seen)
    }

    /// Checks whether `(counter, messageID)` can be accepted without mutating state.
    public func check(counter: UInt64, messageID: Data) -> ReplayError? {
        guard messageID.count == Weft.messageIDLength else {
            return .invalidMessageIDLength(messageID.count)
        }
        if counter == 0 {
            return .zeroCounter
        }
        if highestCounter >= Self.windowSize && counter <= highestCounter - Self.windowSize {
            return .tooOld
        }
        let hex = messageID.hexString
        if seen.contains(where: { $0.idHex == hex || $0.counter == counter }) {
            return .duplicate
        }
        return nil
    }

    /// Records a verified `(counter, messageID)` after AEAD authentication succeeds.
    public mutating func record(counter: UInt64, messageID: Data) {
        let nextHighest = max(highestCounter, counter)
        let floor = nextHighest > Self.windowSize ? nextHighest - Self.windowSize : 0
        var updated = seen.filter { $0.counter > floor }
        if counter > floor {
            updated.append(SeenMessage(counter: counter, idHex: messageID.hexString))
        }
        self.highestCounter = nextHighest
        self.seen = updated
    }

    /// Validates and records `(counter, messageID)` in one step.
    public mutating func accept(counter: UInt64, messageID: Data) throws(ReplayError) {
        if let error = check(counter: counter, messageID: messageID) {
            throw error
        }
        record(counter: counter, messageID: messageID)
    }
}
