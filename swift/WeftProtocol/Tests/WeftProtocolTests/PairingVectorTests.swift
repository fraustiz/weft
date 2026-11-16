// Interoperability checks against the `pairing` and `device_name` sections
// of the Rust reference test vectors (crates/weft-protocol/vectors/v1.json).

import Foundation
import Testing
@testable import WeftProtocol

/// Devices A, B and C rebuilt from the seeds in the vectors.
private struct GroupDevices {
    let vectors: VectorNode
    let a: Identity
    let b: Identity
    let c: Identity

    init() throws {
        vectors = try VectorNode.load()
        let devices = try vectors["devices"]
        let deviceC = try vectors["pairing"]["device_c"]
        a = try Identity(signSeed: devices["a"]["sign_seed"].bytes, kxSeed: devices["a"]["kx_seed"].bytes)
        b = try Identity(signSeed: devices["b"]["sign_seed"].bytes, kxSeed: devices["b"]["kx_seed"].bytes)
        c = try Identity(signSeed: deviceC["sign_seed"].bytes, kxSeed: deviceC["kx_seed"].bytes)
    }

    func identity(_ name: String) throws -> Identity {
        switch name {
        case "a": return a
        case "b": return b
        case "c": return c
        default: throw VectorError(description: "unknown device \(name)")
        }
    }

    func peerKeys(_ name: String) throws -> PeerKeys {
        let identity = try identity(name)
        return try PeerKeys(signPublicKey: identity.signPublicKey, kxPublicKey: identity.kxPublicKey)
    }

    /// Session keys of `local` with `peer`.
    func keys(_ local: String, with peer: String) throws -> SessionKeys {
        let localIdentity = try identity(local)
        let peerIdentity = try identity(peer)
        return try sessionKeys(
            local: localIdentity, peerDeviceID: peerIdentity.deviceID, peerKxPublicKey: peerIdentity.kxPublicKey)
    }

    func pairingCode() throws -> PairingCode {
        let v = try vectors["pairing"]
        return try PairingCode(
            relayURL: v["relay_url"].string, pairingID: v["pairing_id"].bytes,
            signPublicKey: a.signPublicKey, kxPublicKey: a.kxPublicKey, secret: v["secret"].bytes)
    }

    func pairRequest() throws -> PairRequest {
        let v = try vectors["pairing"]
        return try PairRequest(
            secret: v["secret"].bytes, pairingID: v["pairing_id"].bytes,
            signPublicKey: b.signPublicKey, kxPublicKey: b.kxPublicKey)
    }
}

/// Checks one envelope of the vectors: header, seal with the fixed nonce,
/// open of the committed frame. Returns the opened plaintext.
private func checkEnvelope(_ v: VectorNode, devices d: GroupDevices) throws -> Data {
    let senderName = try v["sender"].string
    let recipientName = try v["recipient"].string
    let sender = try d.identity(senderName)
    let recipient = try d.identity(recipientName)
    let senderKeys = try d.keys(senderName, with: recipientName)
    let recipientKeys = try d.keys(recipientName, with: senderName)

    let kind = try #require(Kind(rawValue: UInt8(v["kind"].int)))
    let header = try Header(
        version: UInt8(v["version"].int), kind: kind,
        sender: sender.deviceID, recipient: recipient.deviceID,
        messageID: v["message_id"].bytes, counter: UInt64(v["counter"].int))
    let plaintext = try v["plaintext"].bytes
    #expect(senderKeys.tx == (try v["key"].bytes))
    #expect(recipientKeys.rx == (try v["key"].bytes))

    let headerBytes = header.encode()
    #expect(hexEncoded(headerBytes) == (try v["header"].string))
    #expect(try Header(decoding: headerBytes) == header)

    let frame = try seal(header: header, plaintext: plaintext, nonce: v["nonce"].bytes, tx: senderKeys.tx)
    #expect(hexEncoded(frame) == (try v["frame"].string))
    #expect(classify(frame) == .envelope)

    let (opened, openedPlaintext) = try open(frame: v["frame"].bytes, rx: recipientKeys.rx)
    #expect(opened == header)
    #expect(openedPlaintext == plaintext)
    return openedPlaintext
}

@Suite("v1 vectors: pairing")
struct PairingVectorTests {
    @Test func roles() throws {
        let v = try VectorNode.load()["pairing"]
        #expect(try v["shown_by"].string == "a")
        #expect(try v["joiner"].string == "b")
    }

    @Test func codePayload() throws {
        let d = try GroupDevices()
        let v = try d.vectors["pairing"]
        let code = try d.pairingCode()
        #expect(code.relayURL == "wss://weft.frohub.eu/v1")
        #expect(hexEncoded(Data(try code.payload())) == (try v["code_payload"].string))
    }

    @Test func codeEncodes() throws {
        let d = try GroupDevices()
        #expect(try d.pairingCode().encode() == (try d.vectors["pairing"]["code"].string))
    }

    @Test func codeDecodes() throws {
        let d = try GroupDevices()
        let v = try d.vectors["pairing"]
        let decoded = try PairingCode(decoding: v["code"].string)
        #expect(decoded == (try d.pairingCode()))
        #expect(decoded.relayURL == (try v["relay_url"].string))
        #expect(decoded.pairingID == (try v["pairing_id"].bytes))
        #expect(decoded.signPublicKey == d.a.signPublicKey)
        #expect(decoded.kxPublicKey == d.a.kxPublicKey)
        #expect(decoded.secret == (try v["secret"].bytes))
    }

    @Test func macInputAndMAC() throws {
        let d = try GroupDevices()
        let v = try d.vectors["pairing"]
        let input = try pairRequestMACInput(
            pairingID: v["pairing_id"].bytes, signPublicKey: d.b.signPublicKey, kxPublicKey: d.b.kxPublicKey)
        #expect(hexEncoded(Data(input)) == (try v["mac_input"].string))

        let mac = try pairRequestMAC(
            secret: v["secret"].bytes, pairingID: v["pairing_id"].bytes,
            signPublicKey: d.b.signPublicKey, kxPublicKey: d.b.kxPublicKey)
        #expect(hexEncoded(mac) == (try v["mac"].string))

        let request = try d.pairRequest()
        #expect(request.mac == mac)
        #expect(try verifyPairRequest(secret: v["secret"].bytes, request: request))
    }

    @Test(arguments: [
        "pair_open", "pair_request", "group_add", "group_remove",
        "error_unknown_pairing", "error_not_allowed", "error_rate_limited", "error_group_full",
    ])
    func controlFrame(name: String) throws {
        let d = try GroupDevices()
        let v = try d.vectors["pairing"]
        let frame: ControlFrame =
            switch name {
            case "pair_open": .pairOpen(pairingID: try v["pairing_id"].bytes)
            case "pair_request": .pairRequest(try d.pairRequest())
            case "group_add": .groupAdd(deviceID: d.b.deviceID)
            case "group_remove": .groupRemove(deviceID: d.c.deviceID)
            case "error_unknown_pairing": .error(.unknownPairing)
            case "error_not_allowed": .error(.notAllowed)
            case "error_rate_limited": .error(.rateLimited)
            case "error_group_full": .error(.groupFull)
            default: throw VectorError(description: "unknown frame \(name)")
            }
        let expected = try v["control_frames"][name]
        #expect(hexEncoded(try frame.encode()) == (try expected.string))
        #expect(try ControlFrame(decoding: expected.bytes) == frame)
        #expect(classify(try expected.bytes) == .control)
    }

    @Test func receivedPairRequestVerifies() throws {
        let d = try GroupDevices()
        let v = try d.vectors["pairing"]
        guard case .pairRequest(let request) = try ControlFrame(decoding: v["control_frames"]["pair_request"].bytes)
        else {
            Issue.record("not a pair request")
            return
        }
        #expect(request.pairingID == (try v["pairing_id"].bytes))
        #expect(request.signPublicKey == d.b.signPublicKey)
        #expect(request.kxPublicKey == d.b.kxPublicKey)
        #expect(hexEncoded(request.mac) == (try v["mac"].string))
        #expect(try verifyPairRequest(secret: v["secret"].bytes, request: request))
    }

    @Test func deviceC() throws {
        let d = try GroupDevices()
        let v = try d.vectors["pairing"]["device_c"]
        #expect(hexEncoded(d.c.signPublicKey) == (try v["sign_pk"].string))
        #expect(hexEncoded(d.c.kxPublicKey) == (try v["kx_pk"].string))
        #expect(hexEncoded(d.c.deviceID) == (try v["device_id"].string))
        #expect(hexEncoded(try deviceID(fromSignPublicKey: v["sign_pk"].bytes)) == (try v["device_id"].string))
        #expect(hexEncoded(try d.peerKeys("c").deviceID) == (try v["device_id"].string))
    }

    @Test func sessionAC() throws {
        let d = try GroupDevices()
        let v = try d.vectors["pairing"]["session_a_c"]
        let client = d.a.deviceID.lexicographicallyPrecedes(d.c.deviceID) ? "a" : "c"
        #expect(client == (try v["client"].string))

        let a = try d.keys("a", with: "c")
        let c = try d.keys("c", with: "a")
        #expect(hexEncoded(a.rx) == (try v["a"]["rx"].string))
        #expect(hexEncoded(a.tx) == (try v["a"]["tx"].string))
        #expect(hexEncoded(c.rx) == (try v["c"]["rx"].string))
        #expect(hexEncoded(c.tx) == (try v["c"]["tx"].string))
        #expect(a.tx == c.rx && a.rx == c.tx)
    }

    @Test func envelopeNames() throws {
        let names = try VectorNode.load()["pairing"]["envelopes"].array.map { try $0["name"].string }
        #expect(names == ["welcome_a_to_b", "device_added_a_to_c", "device_removed_a_to_b"])
    }

    @Test func welcomeEnvelope() throws {
        let d = try GroupDevices()
        let v = try d.vectors["pairing"]["envelopes"].array[0]
        #expect(try v["kind"].int == Int(Kind.welcome.rawValue))
        let members = [try d.peerKeys("c")]
        #expect(try encodeWelcome(members) == (try v["plaintext"].bytes))
        let plaintext = try checkEnvelope(v, devices: d)
        #expect(try decodeWelcome(plaintext) == members)
    }

    @Test func deviceAddedEnvelope() throws {
        let d = try GroupDevices()
        let v = try d.vectors["pairing"]["envelopes"].array[1]
        #expect(try v["kind"].int == Int(Kind.deviceAdded.rawValue))
        let added = try d.peerKeys("b")
        #expect(encodeDeviceAdded(added) == (try v["plaintext"].bytes))
        let plaintext = try checkEnvelope(v, devices: d)
        #expect(try decodeDeviceAdded(plaintext) == added)
    }

    @Test func deviceRemovedEnvelope() throws {
        let d = try GroupDevices()
        let v = try d.vectors["pairing"]["envelopes"].array[2]
        #expect(try v["kind"].int == Int(Kind.deviceRemoved.rawValue))
        #expect(try encodeDeviceRemoved(deviceID: d.c.deviceID) == (try v["plaintext"].bytes))
        let plaintext = try checkEnvelope(v, devices: d)
        #expect(try decodeDeviceRemoved(plaintext) == d.c.deviceID)
    }
}

@Suite("v1 vectors: device name")
struct DeviceNameVectorTests {
    @Test func maxLength() throws {
        #expect(try VectorNode.load()["device_name"]["max_len"].int == Weft.maxDeviceNameLength)
    }

    @Test func envelope() throws {
        let d = try GroupDevices()
        let section = try d.vectors["device_name"]
        let v = try section["envelope"]
        let name = try section["name"].string
        #expect(try v["name"].string == "device_name_b_to_a")
        #expect(try v["kind"].int == Int(Kind.deviceName.rawValue))
        #expect(try v["plaintext_utf8"].string == name)
        #expect(try encodeDeviceName(name) == (try v["plaintext"].bytes))
        #expect(Data(name.utf8) == (try v["plaintext"].bytes))

        let plaintext = try checkEnvelope(v, devices: d)
        #expect(try decodeDeviceName(plaintext) == name)
        // Byte-exact, not just canonically equivalent.
        #expect(Data(try decodeDeviceName(plaintext).utf8) == plaintext)
    }
}
