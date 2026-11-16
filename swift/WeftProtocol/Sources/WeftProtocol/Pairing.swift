// Groups and pairing (docs/protocol.md, section 5): the pairing code shown as
// a QR code, the pair request MAC and the control frames exchanged with the
// relay. Mirrors crates/weft-protocol/src/pairing.rs.

import Foundation

extension Weft {
    /// Prefix of the pairing code string.
    public static let pairingCodePrefix = "WEFT1:"
    /// Largest relay url in a pairing code, in bytes.
    public static let maxRelayURLLength = Int(UInt8.max)
    /// Size of a pairing id.
    public static let pairingIDLength = 16
    /// Size of a pairing secret.
    public static let pairingSecretLength = 32
    /// Size of a pair request MAC.
    public static let pairRequestMACLength = 32

    /// First byte of the frame that opens a pairing slot.
    public static let controlPairOpen: UInt8 = 0xB0
    /// First byte of the frame a device sends to join.
    public static let controlPairRequest: UInt8 = 0xB1
    /// First byte of the frame that confirms a device into the group.
    public static let controlGroupAdd: UInt8 = 0xB2
    /// First byte of the frame that removes a device from the group.
    public static let controlGroupRemove: UInt8 = 0xB3
    /// First byte of the relay's error frame.
    public static let controlError: UInt8 = 0xBE

    static let pairContext = Array("weft-pair-v1".utf8)
    /// `pairing_id (16) || sign_pk (32) || kx_pk (32) || secret (32)`.
    static let pairingCodeFixedLength =
        pairingIDLength + Sodium.signPublicKeyBytes + Sodium.kxPublicKeyBytes + pairingSecretLength
    /// `pairing_id || sign_pk || kx_pk || mac`, the body of the `0xB1` frame.
    static let pairRequestBodyLength =
        pairingIDLength + Sodium.signPublicKeyBytes + Sodium.kxPublicKeyBytes + pairRequestMACLength
}

/// A random 16-byte pairing id for a new pairing code.
public func randomPairingID() -> Data {
    Data(Sodium.randomBytes(Weft.pairingIDLength))
}

/// A random 32-byte secret for a new pairing code.
public func randomPairingSecret() -> Data {
    Data(Sodium.randomBytes(Weft.pairingSecretLength))
}

// MARK: - Pairing code

/// The content of the QR code a device shows to let another one join.
///
/// The secret only ever travels through the QR code: never send a
/// `PairingCode` (or its encoded form) over the network. `description`,
/// `debugDescription` and the reflection mirror (`dump`) all hide it.
public struct PairingCode: Equatable, Sendable {
    /// Relay to connect to, e.g. `wss://weft.frohub.eu/v1`. 1 to 255 bytes of
    /// UTF-8, checked by `encode()`.
    public let relayURL: String
    /// 16-byte pairing id.
    public let pairingID: Data
    /// Signing public key of the device showing the code.
    public let signPublicKey: Data
    /// Key exchange public key of the device showing the code.
    public let kxPublicKey: Data
    /// 32-byte pairing secret.
    public let secret: Data

    public init(
        relayURL: String, pairingID: Data, signPublicKey: Data, kxPublicKey: Data, secret: Data
    ) throws(WeftError) {
        self.relayURL = relayURL
        self.pairingID = Data(try fixed(pairingID, Weft.pairingIDLength, "pairingID"))
        self.signPublicKey = Data(try fixed(signPublicKey, Sodium.signPublicKeyBytes, "signPublicKey"))
        self.kxPublicKey = Data(try fixed(kxPublicKey, Sodium.kxPublicKeyBytes, "kxPublicKey"))
        self.secret = Data(try fixed(secret, Weft.pairingSecretLength, "secret"))
    }

    /// `url_len (1) || relay_url || pairing_id || sign_pk || kx_pk || secret`.
    /// The caller zeroes it.
    func payload() throws(WeftError) -> [UInt8] {
        let url = Array(relayURL.utf8)
        guard (1...Weft.maxRelayURLLength).contains(url.count) else { throw .relayURLLength(url.count) }
        var out = [UInt8(url.count)]
        out.reserveCapacity(1 + url.count + Weft.pairingCodeFixedLength)
        out += url
        out += pairingID
        out += signPublicKey
        out += kxPublicKey
        out += secret
        return out
    }

    /// `"WEFT1:"` followed by the base64url (no padding) encoding of
    /// `url_len (1) || relay_url || pairing_id || sign_pk || kx_pk || secret`.
    ///
    /// Fails if the relay url is empty or longer than 255 bytes.
    public func encode() throws(WeftError) -> String {
        var payload = try payload()
        defer { Sodium.memzero(&payload) }
        return Weft.pairingCodePrefix + Sodium.base64URLEncode(payload)
    }

    /// Parses a scanned code. The prefix, the alphabet (no padding, no
    /// whitespace) and the length are all exact, and the base64 decoding is
    /// libsodium's strict `sodium_base642bin`. Works on the UTF-8 bytes, so
    /// no Unicode equivalence applies to the prefix.
    public init(decoding code: String) throws(WeftError) {
        let bytes = Array(code.utf8)
        let prefix = Array(Weft.pairingCodePrefix.utf8)
        guard bytes.starts(with: prefix) else { throw .malformedPairingCode }
        let b64 = Array(bytes[prefix.count...])
        // Bounds the work below: a QR scanner hands us arbitrary text.
        let maxPayloadLength = 1 + Weft.maxRelayURLLength + Weft.pairingCodeFixedLength
        guard b64.count <= (maxPayloadLength + 2) / 3 * 4 else { throw .malformedPairingCode }
        guard var payload = Sodium.base64URLDecode(b64) else { throw .malformedPairingCode }
        defer { Sodium.memzero(&payload) }
        try self.init(payload: payload)
    }

    private init(payload: [UInt8]) throws(WeftError) {
        guard let first = payload.first else { throw .malformedPairingCode }
        let urlLength = Int(first)
        guard urlLength > 0 else { throw .relayURLLength(0) }
        guard payload.count - 1 == urlLength + Weft.pairingCodeFixedLength else {
            throw .malformedPairingCode
        }
        let url = payload[1..<(1 + urlLength)]
        guard isValidUTF8(url) else { throw .invalidUTF8 }
        var offset = 1 + urlLength
        func take(_ count: Int) -> Data {
            defer { offset += count }
            return Data(payload[offset..<(offset + count)])
        }
        relayURL = String(decoding: url, as: UTF8.self)
        pairingID = take(Weft.pairingIDLength)
        signPublicKey = take(Sodium.signPublicKeyBytes)
        kxPublicKey = take(Sodium.kxPublicKeyBytes)
        secret = take(Weft.pairingSecretLength)
    }

    /// Byte-exact comparison (the relay url by its UTF-8 bytes, the secret
    /// in constant time).
    public static func == (lhs: PairingCode, rhs: PairingCode) -> Bool {
        lhs.relayURL.utf8.elementsEqual(rhs.relayURL.utf8)
            && lhs.pairingID == rhs.pairingID
            && lhs.signPublicKey == rhs.signPublicKey
            && lhs.kxPublicKey == rhs.kxPublicKey
            && Sodium.memcmp([UInt8](lhs.secret), [UInt8](rhs.secret))
    }
}

/// Hides the secret so it does not end up in logs.
extension PairingCode: CustomStringConvertible, CustomDebugStringConvertible, CustomReflectable {
    public var description: String {
        "PairingCode(relayURL: \(relayURL), pairingID: \(hexString(pairingID)), "
            + "signPublicKey: \(hexString(signPublicKey)), kxPublicKey: \(hexString(kxPublicKey)), "
            + "secret: <redacted>)"
    }

    public var debugDescription: String { description }

    public var customMirror: Mirror {
        Mirror(
            self,
            children: [
                "relayURL": relayURL,
                "pairingID": hexString(pairingID),
                "signPublicKey": hexString(signPublicKey),
                "kxPublicKey": hexString(kxPublicKey),
                "secret": "<redacted>",
            ],
            displayStyle: .struct)
    }
}

private func hexString(_ data: Data) -> String {
    data.map { String(format: "%02x", $0) }.joined()
}

// MARK: - Pair request

/// `"weft-pair-v1" || pairing_id || sign_pk || kx_pk`, the input of the
/// pair request MAC.
func pairRequestMACInput(
    pairingID: Data, signPublicKey: Data, kxPublicKey: Data
) throws(WeftError) -> [UInt8] {
    Weft.pairContext
        + (try fixed(pairingID, Weft.pairingIDLength, "pairingID"))
        + (try fixed(signPublicKey, Sodium.signPublicKeyBytes, "signPublicKey"))
        + (try fixed(kxPublicKey, Sodium.kxPublicKeyBytes, "kxPublicKey"))
}

/// MAC of a pair request: keyed BLAKE2b (`crypto_generichash`, 32-byte
/// output, key = `secret`) of `"weft-pair-v1" || pairing_id || sign_pk || kx_pk`,
/// where the keys are those of the device asking to join.
public func pairRequestMAC(
    secret: Data, pairingID: Data, signPublicKey: Data, kxPublicKey: Data
) throws(WeftError) -> Data {
    var key = try fixed(secret, Weft.pairingSecretLength, "secret")
    defer { Sodium.memzero(&key) }
    let input = try pairRequestMACInput(
        pairingID: pairingID, signPublicKey: signPublicKey, kxPublicKey: kxPublicKey)
    return Data(Sodium.genericHash32Keyed(input, key: key))
}

/// Body of the `0xB1` frame: a device asking to join a group.
public struct PairRequest: Equatable, Sendable {
    /// 16-byte pairing id.
    public let pairingID: Data
    /// Signing public key of the device asking to join.
    public let signPublicKey: Data
    /// Key exchange public key of the device asking to join.
    public let kxPublicKey: Data
    /// 32-byte MAC, see `pairRequestMAC(secret:pairingID:signPublicKey:kxPublicKey:)`.
    public let mac: Data

    public init(pairingID: Data, signPublicKey: Data, kxPublicKey: Data, mac: Data) throws(WeftError) {
        self.pairingID = Data(try fixed(pairingID, Weft.pairingIDLength, "pairingID"))
        self.signPublicKey = Data(try fixed(signPublicKey, Sodium.signPublicKeyBytes, "signPublicKey"))
        self.kxPublicKey = Data(try fixed(kxPublicKey, Sodium.kxPublicKeyBytes, "kxPublicKey"))
        self.mac = Data(try fixed(mac, Weft.pairRequestMACLength, "mac"))
    }

    /// Builds the request of the joining device, computing the MAC with the
    /// secret read from the pairing code.
    public init(secret: Data, pairingID: Data, signPublicKey: Data, kxPublicKey: Data) throws(WeftError) {
        let mac = try pairRequestMAC(
            secret: secret, pairingID: pairingID, signPublicKey: signPublicKey, kxPublicKey: kxPublicKey)
        try self.init(pairingID: pairingID, signPublicKey: signPublicKey, kxPublicKey: kxPublicKey, mac: mac)
    }

    /// `pairing_id || sign_pk || kx_pk || mac`.
    var body: [UInt8] {
        [UInt8](pairingID) + [UInt8](signPublicKey) + [UInt8](kxPublicKey) + [UInt8](mac)
    }

    /// `bytes` must be exactly `Weft.pairRequestBodyLength` long.
    init(body bytes: ArraySlice<UInt8>) {
        precondition(bytes.count == Weft.pairRequestBodyLength)
        let base = bytes.startIndex
        pairingID = Data(bytes[base..<(base + 16)])
        signPublicKey = Data(bytes[(base + 16)..<(base + 48)])
        kxPublicKey = Data(bytes[(base + 48)..<(base + 80)])
        mac = Data(bytes[(base + 80)..<(base + 112)])
    }
}

/// Checks the MAC of a received pair request in constant time
/// (`sodium_memcmp`).
///
/// The MAC covers the `pairing_id` carried by the request, not the one the
/// caller expects: the caller must also check that `request.pairingID` is
/// the id of the code it is showing.
public func verifyPairRequest(secret: Data, request: PairRequest) throws(WeftError) -> Bool {
    let expected = try pairRequestMAC(
        secret: secret, pairingID: request.pairingID,
        signPublicKey: request.signPublicKey, kxPublicKey: request.kxPublicKey)
    return Sodium.memcmp([UInt8](expected), [UInt8](request.mac))
}

// MARK: - Control frames

/// Error code carried by the relay's `0xBE` frame.
public enum ControlError: UInt8, Sendable, CaseIterable {
    /// Unknown or expired pairing.
    case unknownPairing = 0x01
    case notAllowed = 0x02
    case rateLimited = 0x03
    case groupFull = 0x04
}

/// A control frame (`0xB0`–`0xBF`), exchanged with the relay in clear.
public enum ControlFrame: Equatable, Sendable {
    /// `0xB0`, device → relay: opens a pairing slot (16-byte pairing id).
    case pairOpen(pairingID: Data)
    /// `0xB1`, joining device → relay → slot owner: asks to join.
    case pairRequest(PairRequest)
    /// `0xB2`, slot owner → relay: puts that device (16-byte id) in the
    /// sender's group.
    case groupAdd(deviceID: Data)
    /// `0xB3`, device → relay: removes that device (16-byte id) from the group.
    case groupRemove(deviceID: Data)
    /// `0xBE`, relay → device.
    case error(ControlError)

    /// The frame's first byte followed by its body. Throws
    /// `WeftError.invalidLength` if an id is not 16 bytes (the enum cannot
    /// check it at construction).
    public func encode() throws(WeftError) -> Data {
        switch self {
        case .pairOpen(let pairingID):
            return Data([Weft.controlPairOpen] + (try fixed(pairingID, Weft.pairingIDLength, "pairingID")))
        case .pairRequest(let request):
            return Data([Weft.controlPairRequest] + request.body)
        case .groupAdd(let deviceID):
            return Data([Weft.controlGroupAdd] + (try fixed(deviceID, Weft.deviceIDLength, "deviceID")))
        case .groupRemove(let deviceID):
            return Data([Weft.controlGroupRemove] + (try fixed(deviceID, Weft.deviceIDLength, "deviceID")))
        case .error(let code):
            return Data([Weft.controlError, code.rawValue])
        }
    }

    /// Decodes a whole frame. Every frame has an exact length; a frame with
    /// missing or extra bytes is rejected.
    public init(decoding frame: Data) throws(WeftError) {
        let bytes = [UInt8](frame)
        guard let tag = bytes.first else { throw .malformedControlFrame }
        let body = bytes[1...]
        func exact(_ length: Int) throws(WeftError) {
            guard body.count == length else { throw .malformedControlFrame }
        }
        switch tag {
        case Weft.controlPairOpen:
            try exact(Weft.pairingIDLength)
            self = .pairOpen(pairingID: Data(body))
        case Weft.controlPairRequest:
            try exact(Weft.pairRequestBodyLength)
            self = .pairRequest(PairRequest(body: body))
        case Weft.controlGroupAdd:
            try exact(Weft.deviceIDLength)
            self = .groupAdd(deviceID: Data(body))
        case Weft.controlGroupRemove:
            try exact(Weft.deviceIDLength)
            self = .groupRemove(deviceID: Data(body))
        case Weft.controlError:
            try exact(1)
            let code = bytes[1]
            guard let error = ControlError(rawValue: code) else { throw .unknownControlError(code) }
            self = .error(error)
        // The rest of the control range is reserved.
        case 0xB4...0xBD, 0xBF:
            throw .unknownControlFrame(tag)
        default:
            throw .unexpectedFrameType(tag)
        }
    }
}
