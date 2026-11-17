// Interoperability checks against the Rust reference test vectors
// (crates/weft-protocol/vectors/v1.json), plus a few negative checks.

import Foundation
import Testing
@testable import WeftProtocol

// MARK: - Vector loading

/// `swift/WeftProtocol/Tests/WeftProtocolTests/VectorTests.swift` ->
/// `crates/weft-protocol/vectors/v1.json`.
private let vectorsURL = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()  // .../Tests/WeftProtocolTests
    .deletingLastPathComponent()  // .../Tests
    .deletingLastPathComponent()  // .../swift/WeftProtocol
    .deletingLastPathComponent()  // .../swift
    .deletingLastPathComponent()  // repository root
    .appendingPathComponent("crates/weft-protocol/vectors/v1.json")

struct VectorError: Error, CustomStringConvertible {
    let description: String
}

private func hex(_ data: Data) -> String {
    data.map { String(format: "%02x", $0) }.joined()
}

private func unhex(_ string: String) throws -> Data {
    let chars = Array(string.utf8)
    guard chars.count % 2 == 0 else { throw VectorError(description: "odd hex length: \(string)") }
    var out = Data(capacity: chars.count / 2)
    func nibble(_ c: UInt8) throws -> UInt8 {
        switch c {
        case UInt8(ascii: "0")...UInt8(ascii: "9"): return c - UInt8(ascii: "0")
        case UInt8(ascii: "a")...UInt8(ascii: "f"): return c - UInt8(ascii: "a") + 10
        default: throw VectorError(description: "bad hex digit in \(string)")
        }
    }
    for i in stride(from: 0, to: chars.count, by: 2) {
        out.append(try nibble(chars[i]) << 4 | nibble(chars[i + 1]))
    }
    return out
}

/// Minimal typed access to the JSON document.
private struct Node {
    let value: Any

    subscript(_ key: String) -> Node {
        get throws {
            guard let dict = value as? [String: Any], let child = dict[key] else {
                throw VectorError(description: "missing key \(key)")
            }
            return Node(value: child)
        }
    }

    var array: [Node] {
        get throws {
            guard let list = value as? [Any] else { throw VectorError(description: "not an array") }
            return list.map(Node.init)
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

    var uint64: UInt64 {
        get throws {
            guard let n = value as? NSNumber else { throw VectorError(description: "not a number") }
            return n.uint64Value
        }
    }

    var bytes: Data { get throws { try unhex(string) } }
}

private func loadVectors() throws -> Node {
    let data = try Data(contentsOf: vectorsURL)
    return Node(value: try JSONSerialization.jsonObject(with: data))
}

/// Both devices, rebuilt from the seeds in the vectors.
private struct Devices {
    let vectors: Node
    let a: Identity
    let b: Identity

    init() throws {
        vectors = try loadVectors()
        let devices = try vectors["devices"]
        a = try Identity(signSeed: devices["a"]["sign_seed"].bytes, kxSeed: devices["a"]["kx_seed"].bytes)
        b = try Identity(signSeed: devices["b"]["sign_seed"].bytes, kxSeed: devices["b"]["kx_seed"].bytes)
    }

    func identity(_ name: String) throws -> Identity {
        switch name {
        case "a": return a
        case "b": return b
        default: throw VectorError(description: "unknown device \(name)")
        }
    }

    func keys(_ name: String) throws -> SessionKeys {
        let local = try identity(name)
        let peer = name == "a" ? b : a
        return try sessionKeys(local: local, peerDeviceID: peer.deviceID, peerKxPublicKey: peer.kxPublicKey)
    }
}

// MARK: - Positive checks (every vector field)

@Suite("v1 vectors")
struct VectorTests {
    @Test func protocolVersion() throws {
        let vectors = try loadVectors()
        #expect(try vectors["protocol_version"].int == Int(Weft.version))
    }

    @Test(arguments: ["a", "b"])
    func deviceKeys(name: String) throws {
        let d = try Devices()
        let expected = try d.vectors["devices"][name]
        let identity = try d.identity(name)
        #expect(hex(identity.signPublicKey) == (try expected["sign_pk"].string))
        #expect(hex(identity.kxPublicKey) == (try expected["kx_pk"].string))
        #expect(hex(identity.deviceID) == (try expected["device_id"].string))
        #expect(hex(try deviceID(fromSignPublicKey: expected["sign_pk"].bytes)) == (try expected["device_id"].string))
    }

    @Test func clientRole() throws {
        let d = try Devices()
        let client = d.a.deviceID.lexicographicallyPrecedes(d.b.deviceID) ? "a" : "b"
        #expect(client == (try d.vectors["session"]["client"].string))
    }

    @Test(arguments: ["a", "b"])
    func sessionKeysMatch(name: String) throws {
        let d = try Devices()
        let expected = try d.vectors["session"][name]
        let keys = try d.keys(name)
        #expect(hex(keys.rx) == (try expected["rx"].string))
        #expect(hex(keys.tx) == (try expected["tx"].string))
    }

    @Test func sessionKeysAreCrossed() throws {
        let d = try Devices()
        let (a, b) = (try d.keys("a"), try d.keys("b"))
        #expect(a.tx == b.rx)
        #expect(a.rx == b.tx)
    }

    @Test(arguments: [0, 1])
    func envelope(index: Int) throws {
        let d = try Devices()
        let v = try d.vectors["envelopes"].array[index]
        let senderName = try v["sender"].string
        let recipientName = try v["recipient"].string
        let sender = try d.identity(senderName)
        let recipient = try d.identity(recipientName)
        let senderKeys = try d.keys(senderName)
        let recipientKeys = try d.keys(recipientName)

        let kind = try #require(Kind(rawValue: UInt8(v["kind"].int)))
        let header = try Header(
            version: UInt8(v["version"].int), kind: kind,
            sender: sender.deviceID, recipient: recipient.deviceID,
            messageID: v["message_id"].bytes, counter: UInt64(v["counter"].int))
        let plaintext = try v["plaintext"].bytes

        // Inputs are consistent with the rest of the document.
        #expect(Data(try v["plaintext_utf8"].string.utf8) == plaintext)
        #expect(senderKeys.tx == (try v["key"].bytes))

        // Header bytes and round trip.
        let headerBytes = header.encode()
        #expect(headerBytes.count == Weft.headerLength)
        #expect(hex(headerBytes) == (try v["header"].string))
        #expect(try Header(decoding: headerBytes) == header)

        // Full frame from seal with the fixed nonce.
        let frame = try seal(header: header, plaintext: plaintext, nonce: v["nonce"].bytes, tx: senderKeys.tx)
        #expect(hex(frame) == (try v["frame"].string))
        #expect(
            try RouteHeader(decoding: frame)
                == RouteHeader(kind: kind.rawValue, sender: sender.deviceID, recipient: recipient.deviceID))

        // Open the committed frame.
        let (opened, openedPlaintext) = try open(frame: v["frame"].bytes, rx: recipientKeys.rx)
        #expect(opened == header)
        #expect(openedPlaintext == plaintext)
    }

    @Test func randomNonceSealRoundTrip() throws {
        let d = try Devices()
        let header = try Header(
            kind: .clipboardText, sender: d.a.deviceID, recipient: d.b.deviceID,
            messageID: Data(repeating: 7, count: 16), counter: 42)
        let text = Data("round trip".utf8)
        let first = try seal(header: header, plaintext: text, tx: d.keys("a").tx)
        let second = try seal(header: header, plaintext: text, tx: d.keys("a").tx)
        #expect(first != second)
        let (_, plaintext) = try open(frame: first, rx: d.keys("b").rx)
        #expect(plaintext == text)
    }

    @Test func auth() throws {
        let d = try Devices()
        let v = try d.vectors["auth"]
        #expect(try v["device"].string == "a")

        let challenge = try Challenge(relayName: v["relay_name"].string, nonce: v["nonce"].bytes)
        #expect(hex(challenge.encode()) == (try v["challenge_frame"].string))
        #expect(try Challenge(decoding: v["challenge_frame"].bytes) == challenge)
        #expect(hex(Data(challenge.signedMessage)) == (try v["signed_message"].string))

        let response = signAuth(identity: d.a, challenge: challenge)
        #expect(hex(response.signature) == (try v["signature"].string))
        #expect(hex(response.encode()) == (try v["response_frame"].string))
        #expect(try AuthResponse(decoding: v["response_frame"].bytes) == response)

        #expect(try verifyAuth(challenge: challenge, response: response) == d.a.deviceID)
        #expect(hex(Data([Weft.authAccepted])) == (try v["accepted_frame"].string))
    }
}

@Suite("v1 vectors: features")
struct FeatureVectorTests {
    private func checkFeatureEnvelope(_ v: Node, devices d: Devices, expectedKind: Kind) throws -> Data {
        let senderName = try v["sender"].string
        let recipientName = try v["recipient"].string
        let sender = try d.identity(senderName)
        let recipient = try d.identity(recipientName)
        let senderKeys = try d.keys(senderName)
        let recipientKeys = try d.keys(recipientName)

        let kind = try #require(Kind(rawValue: UInt8(v["kind"].int)))
        #expect(kind == expectedKind)
        let header = try Header(
            version: UInt8(v["version"].int), kind: kind,
            sender: sender.deviceID, recipient: recipient.deviceID,
            messageID: v["message_id"].bytes, counter: v["counter"].uint64)
        let plaintext = try v["plaintext"].bytes
        #expect(senderKeys.tx == (try v["key"].bytes))
        #expect(recipientKeys.rx == (try v["key"].bytes))

        let headerBytes = header.encode()
        #expect(hex(headerBytes) == (try v["header"].string))
        #expect(try Header(decoding: headerBytes) == header)

        let frame = try seal(header: header, plaintext: plaintext, nonce: v["nonce"].bytes, tx: senderKeys.tx)
        #expect(hex(frame) == (try v["frame"].string))
        #expect(classify(frame) == .envelope)
        #expect(
            try RouteHeader(decoding: frame)
                == RouteHeader(kind: kind.rawValue, sender: sender.deviceID, recipient: recipient.deviceID))

        let (opened, openedPlaintext) = try open(frame: v["frame"].bytes, rx: recipientKeys.rx)
        #expect(opened == header)
        #expect(openedPlaintext == plaintext)
        return openedPlaintext
    }

    @Test func packetEnvelope() throws {
        let d = try Devices()
        let packet = try d.vectors["features"]["packet"]
        #expect(try packet["max_len"].int == Weft.maxPacketLength)

        let json = try packet["json"].string
        let envelope = try packet["envelope"]
        #expect(try envelope["name"].string == "packet_a_to_b")
        #expect(try envelope["plaintext_utf8"].string == json)
        #expect(Data(json.utf8) == (try envelope["plaintext"].bytes))

        let plaintext = try checkFeatureEnvelope(envelope, devices: d, expectedKind: .packet)
        #expect(plaintext == Data(json.utf8))
    }

    @Test func fileChunkEnvelope() throws {
        let d = try Devices()
        let chunk = try d.vectors["features"]["file_chunk"]
        #expect(try chunk["max_data_len"].int == Weft.maxFileChunkDataLength)

        let expected = try FileChunk(
            transferID: chunk["transfer_id"].bytes,
            offset: chunk["offset"].uint64,
            data: chunk["data"].bytes)
        let envelope = try chunk["envelope"]
        #expect(try envelope["name"].string == "file_chunk_b_to_a")
        #expect(
            try encodeFileChunk(transferID: expected.transferID, offset: expected.offset, data: expected.data)
                == (try envelope["plaintext"].bytes))

        let plaintext = try checkFeatureEnvelope(envelope, devices: d, expectedKind: .fileChunk)
        #expect(try decodeFileChunk(plaintext) == expected)
    }
}

// MARK: - Negative checks

@Suite("negative")
struct NegativeTests {
    @Test(arguments: [0, 1, 2, 20, 40, 57])
    func tamperedHeaderByteFailsToOpen(offset: Int) throws {
        let d = try Devices()
        let v = try d.vectors["envelopes"].array[0]
        var frame = try v["frame"].bytes
        frame[offset] ^= 0x01
        #expect(throws: WeftError.self) { try open(frame: frame, rx: d.keys("b").rx) }
    }

    @Test func tamperedSenderIsDecryptionFailure() throws {
        let d = try Devices()
        var frame = try d.vectors["envelopes"].array[0]["frame"].bytes
        frame[5] ^= 0x80
        #expect(throws: WeftError.decryptionFailed) { try open(frame: frame, rx: d.keys("b").rx) }
    }

    @Test func tamperedVersionAndKindAreRejected() throws {
        let d = try Devices()
        let original = try d.vectors["envelopes"].array[0]["frame"].bytes
        var frame = original
        frame[0] = 0x02
        #expect(throws: WeftError.unsupportedVersion(0x02)) { try open(frame: frame, rx: d.keys("b").rx) }
        frame = original
        frame[1] = 0x7f
        #expect(throws: WeftError.unknownKind(0x7f)) { try open(frame: frame, rx: d.keys("b").rx) }
        // A clipboard frame relabelled as a ping is rejected before decrypting.
        frame = original
        frame[1] = Kind.ping.rawValue
        #expect(throws: WeftError.pingNotEmpty) { try open(frame: frame, rx: d.keys("b").rx) }
    }

    @Test func wrongKeyFailsToOpen() throws {
        let d = try Devices()
        let frame = try d.vectors["envelopes"].array[0]["frame"].bytes
        #expect(throws: WeftError.decryptionFailed) { try open(frame: frame, rx: d.keys("a").rx) }
    }

    @Test func shortAndEmptyFramesAreRejected() throws {
        let d = try Devices()
        let frame = try d.vectors["envelopes"].array[1]["frame"].bytes
        #expect(throws: WeftError.frameTooShort(97)) { try open(frame: frame.prefix(97), rx: d.keys("a").rx) }
        #expect(throws: WeftError.frameTooShort(0)) { try open(frame: Data(), rx: d.keys("a").rx) }
        #expect(throws: WeftError.headerLength(57)) { try Header(decoding: Data(count: 57)) }
    }

    @Test func wrongRelayNameFailsVerifyAuth() throws {
        let d = try Devices()
        let v = try d.vectors["auth"]
        let challenge = try Challenge(relayName: v["relay_name"].string, nonce: v["nonce"].bytes)
        let response = signAuth(identity: d.a, challenge: challenge)
        let other = try Challenge(relayName: "evil.example.org", nonce: v["nonce"].bytes)
        #expect(throws: WeftError.invalidSignature) { try verifyAuth(challenge: other, response: response) }
    }

    @Test func wrongNonceFailsVerifyAuth() throws {
        let d = try Devices()
        let v = try d.vectors["auth"]
        let challenge = try Challenge(relayName: v["relay_name"].string, nonce: v["nonce"].bytes)
        let response = signAuth(identity: d.a, challenge: challenge)
        let other = try Challenge(relayName: v["relay_name"].string, nonce: Data(repeating: 0, count: 32))
        #expect(throws: WeftError.invalidSignature) { try verifyAuth(challenge: other, response: response) }
    }

    @Test func authFramesNeedExactLength() throws {
        let v = try loadVectors()["auth"]
        let challenge = try v["challenge_frame"].bytes
        let response = try v["response_frame"].bytes
        #expect(throws: WeftError.malformedAuthFrame) { try Challenge(decoding: challenge + [0]) }
        #expect(throws: WeftError.malformedAuthFrame) { try Challenge(decoding: challenge.dropLast()) }
        #expect(throws: WeftError.malformedAuthFrame) { try Challenge(decoding: Data()) }
        #expect(throws: WeftError.malformedAuthFrame) { try Challenge(decoding: Data([0xA0])) }
        #expect(throws: WeftError.unexpectedFrameType(0xA1)) { try Challenge(decoding: response) }
        #expect(throws: WeftError.malformedAuthFrame) { try AuthResponse(decoding: response + [0]) }
        #expect(throws: WeftError.malformedAuthFrame) { try AuthResponse(decoding: response.dropLast()) }
        #expect(throws: WeftError.unexpectedFrameType(0xA0)) { try AuthResponse(decoding: challenge) }
    }

    @Test func invalidUTF8RelayNameIsRejected() {
        var frame = Data([0xA0, 2, 0xC3, 0x28])
        frame.append(Data(repeating: 0, count: 32))
        #expect(throws: WeftError.invalidUTF8) { try Challenge(decoding: frame) }
    }

    @Test func relayNameTooLong() {
        #expect(throws: WeftError.relayNameTooLong) {
            try Challenge(relayName: String(repeating: "x", count: 256), nonce: Data(count: 32))
        }
    }

    @Test func sealValidatesPlaintext() throws {
        let d = try Devices()
        let tx = try d.keys("a").tx
        let nonce = Data(count: 24)
        let text = try Header(
            kind: .clipboardText, sender: d.a.deviceID, recipient: d.b.deviceID,
            messageID: Data(count: 16), counter: 1)
        let ping = try Header(
            kind: .ping, sender: d.a.deviceID, recipient: d.b.deviceID,
            messageID: Data(count: 16), counter: 1)
        #expect(throws: WeftError.invalidUTF8) { try seal(header: text, plaintext: Data([0xFF]), nonce: nonce, tx: tx) }
        #expect(throws: WeftError.plaintextTooLarge(256 * 1024 + 1)) {
            try seal(header: text, plaintext: Data(repeating: 0x61, count: 256 * 1024 + 1), nonce: nonce, tx: tx)
        }
        _ = try seal(header: text, plaintext: Data(repeating: 0x61, count: 256 * 1024), nonce: nonce, tx: tx)
        #expect(throws: WeftError.pingNotEmpty) { try seal(header: ping, plaintext: Data([0]), nonce: nonce, tx: tx) }
        var badVersion = text
        badVersion.version = 2
        #expect(throws: WeftError.unsupportedVersion(2)) {
            try seal(header: badVersion, plaintext: Data(), nonce: nonce, tx: tx)
        }
    }

    @Test func sameDeviceIDIsRejected() throws {
        let d = try Devices()
        #expect(throws: WeftError.sameDeviceID) {
            try sessionKeys(local: d.a, peerDeviceID: d.a.deviceID, peerKxPublicKey: d.a.kxPublicKey)
        }
    }

    @Test func wrongInputLengthsThrow() throws {
        #expect(throws: WeftError.self) { try Identity(signSeed: Data(count: 31), kxSeed: Data(count: 32)) }
        #expect(throws: WeftError.self) { try deviceID(fromSignPublicKey: Data(count: 33)) }
        #expect(throws: WeftError.self) { try open(frame: Data(count: 200), rx: Data(count: 31)) }
    }

    @Test func generatedIdentitiesDiffer() throws {
        let x = Identity.generate()
        let y = Identity.generate()
        #expect(x.deviceID != y.deviceID)
        #expect(try deviceID(fromSignPublicKey: x.signPublicKey) == x.deviceID)
        let xy = try sessionKeys(local: x, peerDeviceID: y.deviceID, peerKxPublicKey: y.kxPublicKey)
        let yx = try sessionKeys(local: y, peerDeviceID: x.deviceID, peerKxPublicKey: x.kxPublicKey)
        #expect(xy.tx == yx.rx && xy.rx == yx.tx)
    }
}
