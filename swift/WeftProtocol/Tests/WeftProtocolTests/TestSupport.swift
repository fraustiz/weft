// Helpers shared by the group, pairing and device name tests: hex, the
// vectors document and fixed test identities. VectorTests.swift keeps its
// own private copies.

import Foundation
import Testing
@testable import WeftProtocol

func hexEncoded(_ data: Data) -> String {
    data.map { String(format: "%02x", $0) }.joined()
}

func hexDecoded(_ string: String) throws -> Data {
    let chars = Array(string.utf8)
    guard chars.count % 2 == 0 else { throw VectorError(description: "odd hex length: \(string)") }
    func nibble(_ c: UInt8) throws -> UInt8 {
        switch c {
        case UInt8(ascii: "0")...UInt8(ascii: "9"): return c - UInt8(ascii: "0")
        case UInt8(ascii: "a")...UInt8(ascii: "f"): return c - UInt8(ascii: "a") + 10
        default: throw VectorError(description: "bad hex digit in \(string)")
        }
    }
    var out = Data(capacity: chars.count / 2)
    for i in stride(from: 0, to: chars.count, by: 2) {
        out.append(try nibble(chars[i]) << 4 | nibble(chars[i + 1]))
    }
    return out
}

/// Typed access to the JSON vectors document.
struct VectorNode {
    let value: Any

    static func load() throws -> VectorNode {
        // .../Tests/WeftProtocolTests/TestSupport.swift -> repository root.
        var url = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 { url = url.deletingLastPathComponent() }
        let data = try Data(contentsOf: url.appendingPathComponent("crates/weft-protocol/vectors/v1.json"))
        return VectorNode(value: try JSONSerialization.jsonObject(with: data))
    }

    subscript(_ key: String) -> VectorNode {
        get throws {
            guard let dict = value as? [String: Any], let child = dict[key] else {
                throw VectorError(description: "missing key \(key)")
            }
            return VectorNode(value: child)
        }
    }

    var array: [VectorNode] {
        get throws {
            guard let list = value as? [Any] else { throw VectorError(description: "not an array") }
            return list.map(VectorNode.init)
        }
    }

    var string: String {
        get throws {
            guard let s = value as? String else { throw VectorError(description: "not a string") }
            return s
        }
    }

    var int: Int {
        get throws {
            guard let n = value as? NSNumber else { throw VectorError(description: "not a number") }
            return n.intValue
        }
    }

    var bytes: Data { get throws { try hexDecoded(string) } }
}

func bytes(_ value: UInt8, _ count: Int) -> Data {
    Data(repeating: value, count: count)
}

/// Two devices from fixed seeds (not the vector ones) and their session
/// keys, like `pair()` in the Rust tests.
struct TestPair {
    let a: Identity
    let b: Identity
    let aKeys: SessionKeys
    let bKeys: SessionKeys

    init() throws {
        a = try Identity(signSeed: bytes(1, 32), kxSeed: bytes(2, 32))
        b = try Identity(signSeed: bytes(3, 32), kxSeed: bytes(4, 32))
        aKeys = try sessionKeys(local: a, peerDeviceID: b.deviceID, peerKxPublicKey: b.kxPublicKey)
        bKeys = try sessionKeys(local: b, peerDeviceID: a.deviceID, peerKxPublicKey: a.kxPublicKey)
    }

    func header(from: Identity, to: Identity, kind: Kind) throws -> Header {
        try Header(
            kind: kind, sender: from.deviceID, recipient: to.deviceID,
            messageID: bytes(7, 16), counter: 1)
    }

    /// A frame from A to B with a valid header and `plaintextLength` bytes of
    /// garbage where the ciphertext should be: only acceptable if it is
    /// rejected before decryption. Returns the frame and B's `rx`.
    func garbageFrame(kind: Kind, plaintextLength: Int) throws -> (frame: Data, rx: Data) {
        var frame = try header(from: a, to: b, kind: kind).encode()
        frame.append(bytes(0, Weft.nonceLength))
        frame.append(bytes(0, plaintextLength + Weft.tagLength))
        return (frame, bKeys.rx)
    }
}
