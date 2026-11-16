// Weft wire protocol, version 1.
//
// Implements docs/protocol.md: device identities, session keys between two
// devices, the encrypted message envelope, the relay authentication
// frames, the group change and device name plaintexts (Groups.swift) and
// pairing (Pairing.swift). Mirrors the Rust reference (crates/weft-protocol) and must produce
// byte-identical output (see crates/weft-protocol/vectors/v1.json).

import Foundation

/// Protocol constants.
public enum Weft {
    /// Protocol version carried in every envelope header.
    public static let version: UInt8 = 0x01
    /// Size of an encoded envelope header.
    public static let headerLength = 58
    /// Size of the envelope nonce.
    public static let nonceLength = Sodium.aeadNonceBytes
    /// Size of the AEAD authentication tag appended to the ciphertext.
    public static let tagLength = Sodium.aeadTagBytes
    /// Largest clipboard text plaintext, in bytes (256 KiB).
    public static let maxClipboardTextLength = 256 * 1024
    /// Size of a device id.
    public static let deviceIDLength = 16
    /// Size of a message id.
    public static let messageIDLength = 16
    /// Largest number of devices in one group.
    public static let maxGroupSize = 10
    /// Largest number of devices a Welcome plaintext can list: the group
    /// without its sender and its recipient.
    public static let maxWelcomeDevices = maxGroupSize - 2
    /// Size of one `sign_pk || kx_pk` entry in a group change plaintext.
    public static let peerKeysLength = Sodium.signPublicKeyBytes + Sodium.kxPublicKeyBytes
    /// Largest device name, in bytes of UTF-8.
    public static let maxDeviceNameLength = 64

    /// First byte of the relay challenge frame.
    public static let authChallenge: UInt8 = 0xA0
    /// First byte of the device response frame.
    public static let authResponse: UInt8 = 0xA1
    /// The one-byte frame the relay sends when authentication succeeds.
    public static let authAccepted: UInt8 = 0xA2

    static let authContext = Array("weft-auth-v1".utf8)
    static let authNonceLength = 32
    static let authResponseLength = 1 + Sodium.signPublicKeyBytes + Sodium.signatureBytes
}

/// Everything that can go wrong while handling protocol data.
public enum WeftError: Error, Equatable, Sendable {
    /// A fixed-size input (seed, key, id, nonce, signature) has the wrong size.
    case invalidLength(field: String, expected: Int, actual: Int)
    case headerLength(Int)
    case frameTooShort(Int)
    case unsupportedVersion(UInt8)
    case unknownKind(UInt8)
    case plaintextTooLarge(Int)
    case pingNotEmpty
    case invalidUTF8
    case decryptionFailed
    case sameDeviceID
    case keyExchangeFailed
    case unexpectedFrameType(UInt8)
    case malformedAuthFrame
    case relayNameTooLong
    case invalidSignature
    /// The plaintext length is not valid for its kind (group change
    /// plaintexts have exact lengths).
    case invalidPlaintextLength(Int)
    /// A Welcome plaintext lists more than `Weft.maxWelcomeDevices` devices.
    case tooManyDevices(Int)
    case malformedPairingCode
    /// The relay url of a pairing code must be 1 to 255 bytes of UTF-8.
    case relayURLLength(Int)
    case malformedControlFrame
    /// A reserved byte of the control range (`0xB4`–`0xBD`, `0xBF`).
    case unknownControlFrame(UInt8)
    case unknownControlError(UInt8)
    /// The relay name in the challenge is not the host the client connected to.
    case relayNameMismatch
    /// A device name must be 1 to `Weft.maxDeviceNameLength` bytes of UTF-8;
    /// the value is its size in bytes.
    case deviceNameLength(Int)
    /// A device name contains a control character (U+0000–U+001F,
    /// U+007F–U+009F).
    case deviceNameControlCharacter
}

// MARK: - Helpers

/// Copies `data` into a zero-based byte array, checking its size.
func fixed(_ data: Data, _ count: Int, _ field: String) throws(WeftError) -> [UInt8] {
    guard data.count == count else {
        throw .invalidLength(field: field, expected: count, actual: data.count)
    }
    return [UInt8](data)
}

/// Strict UTF-8 validation: decoding with repair and re-encoding gives the
/// same bytes back only when the input was well-formed UTF-8.
func isValidUTF8<C: Collection>(_ bytes: C) -> Bool where C.Element == UInt8 {
    let decoded = String(decoding: bytes, as: UTF8.self)
    return decoded.utf8.elementsEqual(bytes)
}

// MARK: - Identity

/// Derives a device id from a signing public key:
/// BLAKE2b (`crypto_generichash`), 16-byte output, no key.
public func deviceID(fromSignPublicKey signPublicKey: Data) throws(WeftError) -> Data {
    let pk = try fixed(signPublicKey, Sodium.signPublicKeyBytes, "signPublicKey")
    return Data(Sodium.genericHash16(pk))
}

/// A device's long-term key pairs (Ed25519 signing and X25519 key exchange).
///
/// The secret keys never leave this object and are zeroed when it is
/// deallocated.
public final class Identity: @unchecked Sendable {
    // Immutable after init; only `deinit` writes to the secret buffers.
    private let signPK: [UInt8]
    private var signSK: [UInt8]
    private let kxPK: [UInt8]
    private var kxSK: [UInt8]
    private let id: [UInt8]

    /// Derives an identity from its two 32-byte seeds
    /// (`crypto_sign_seed_keypair` and `crypto_kx_seed_keypair`). Clients
    /// store the seeds and rebuild the identity at startup; test vectors use
    /// fixed seeds.
    public convenience init(signSeed: Data, kxSeed: Data) throws(WeftError) {
        var sign = try fixed(signSeed, Sodium.signSeedBytes, "signSeed")
        var kx = try fixed(kxSeed, Sodium.kxSeedBytes, "kxSeed")
        defer {
            Sodium.memzero(&sign)
            Sodium.memzero(&kx)
        }
        self.init(signSeedBytes: sign, kxSeedBytes: kx)
    }

    private init(signSeedBytes: [UInt8], kxSeedBytes: [UInt8]) {
        let (signPK, signSK) = Sodium.signSeedKeypair(seed: signSeedBytes)
        let (kxPK, kxSK) = Sodium.kxSeedKeypair(seed: kxSeedBytes)
        self.signPK = signPK
        self.signSK = signSK
        self.kxPK = kxPK
        self.kxSK = kxSK
        self.id = Sodium.genericHash16(signPK)
    }

    /// Generates a fresh identity from random seeds.
    public static func generate() -> Identity {
        var sign = Sodium.randomBytes(Sodium.signSeedBytes)
        var kx = Sodium.randomBytes(Sodium.kxSeedBytes)
        defer {
            Sodium.memzero(&sign)
            Sodium.memzero(&kx)
        }
        return Identity(signSeedBytes: sign, kxSeedBytes: kx)
    }

    deinit {
        Sodium.memzero(&signSK)
        Sodium.memzero(&kxSK)
    }

    /// 16-byte device id.
    public var deviceID: Data { Data(id) }
    /// 32-byte Ed25519 public key.
    public var signPublicKey: Data { Data(signPK) }
    /// 32-byte X25519 public key.
    public var kxPublicKey: Data { Data(kxPK) }

    var deviceIDBytes: [UInt8] { id }
    var signPublicKeyBytes: [UInt8] { signPK }

    fileprivate func kxSessionKeys(role: Sodium.KxRole, peer: [UInt8]) -> (rx: [UInt8], tx: [UInt8])? {
        Sodium.kxSessionKeys(role: role, localPublicKey: kxPK, localSecretKey: kxSK, peerPublicKey: peer)
    }

    fileprivate func sign(_ message: [UInt8]) -> [UInt8] {
        Sodium.signDetached(message: message, secretKey: signSK)
    }
}

// MARK: - Session keys

/// Session keys with one peer: encrypt with `tx`, decrypt with `rx`.
public struct SessionKeys: Equatable, Sendable {
    public let rx: Data
    public let tx: Data
}

/// Derives the session keys between `local` and a peer.
///
/// The device with the lower device id (bytewise) plays the libsodium client
/// role, the other one the server role.
public func sessionKeys(
    local: Identity, peerDeviceID: Data, peerKxPublicKey: Data
) throws(WeftError) -> SessionKeys {
    let peerID = try fixed(peerDeviceID, Weft.deviceIDLength, "peerDeviceID")
    let peerPK = try fixed(peerKxPublicKey, Sodium.kxPublicKeyBytes, "peerKxPublicKey")
    let localID = local.deviceIDBytes
    let role: Sodium.KxRole
    if localID == peerID {
        throw .sameDeviceID
    } else if localID.lexicographicallyPrecedes(peerID) {
        role = .client
    } else {
        role = .server
    }
    guard let keys = local.kxSessionKeys(role: role, peer: peerPK) else {
        throw .keyExchangeFailed
    }
    return SessionKeys(rx: Data(keys.rx), tx: Data(keys.tx))
}

// MARK: - Envelope

/// Message kind carried in the envelope header.
public enum Kind: UInt8, Sendable {
    /// UTF-8 text, at most `Weft.maxClipboardTextLength` bytes.
    case clipboardText = 0x01
    /// Empty plaintext.
    case ping = 0x02
    /// The other members of the group a device just joined; see
    /// `encodeWelcome(_:)`.
    case welcome = 0x03
    /// Keys of a device that joined the group; see `encodeDeviceAdded(_:)`.
    case deviceAdded = 0x04
    /// Id of a device that left the group; see `encodeDeviceRemoved(_:)`.
    case deviceRemoved = 0x05
    /// The sender's own name; see `encodeDeviceName(_:)`.
    case deviceName = 0x06

    /// Largest plaintext allowed for this kind.
    var maxPlaintextLength: Int {
        switch self {
        case .clipboardText: Weft.maxClipboardTextLength
        case .ping: 0
        case .welcome: 1 + Weft.maxWelcomeDevices * Weft.peerKeysLength
        case .deviceAdded: Weft.peerKeysLength
        case .deviceRemoved: Weft.deviceIDLength
        case .deviceName: Weft.maxDeviceNameLength
        }
    }

    /// Checks everything that can be known from the plaintext length alone,
    /// so `open` can reject a frame before decrypting it.
    func checkPlaintextLength(_ length: Int) throws(WeftError) {
        if self == .deviceName {
            try checkDeviceNameLength(length)
            return
        }
        if length > maxPlaintextLength {
            throw self == .ping ? .pingNotEmpty : .plaintextTooLarge(length)
        }
        let valid: Bool
        switch self {
        case .clipboardText, .ping: valid = true
        case .welcome: valid = length % Weft.peerKeysLength == 1
        case .deviceAdded: valid = length == Weft.peerKeysLength
        case .deviceRemoved: valid = length == Weft.deviceIDLength
        case .deviceName: valid = true  // Checked above.
        }
        guard valid else { throw .invalidPlaintextLength(length) }
    }

    func checkPlaintext(_ plaintext: [UInt8]) throws(WeftError) {
        try checkPlaintextLength(plaintext.count)
        switch self {
        case .clipboardText:
            guard isValidUTF8(plaintext) else { throw .invalidUTF8 }
        case .welcome:
            _ = try decodeWelcome(bytes: plaintext)
        case .deviceName:
            _ = try decodeDeviceName(bytes: plaintext)
        // Fully checked by their length.
        case .ping, .deviceAdded, .deviceRemoved:
            break
        }
    }
}

/// Envelope header. Sent in clear and authenticated as associated data.
public struct Header: Equatable, Sendable {
    public var version: UInt8
    public var kind: Kind
    /// 16-byte sender device id.
    public private(set) var sender: Data
    /// 16-byte recipient device id.
    public private(set) var recipient: Data
    /// 16-byte random message id.
    public private(set) var messageID: Data
    public var counter: UInt64

    public init(
        version: UInt8 = Weft.version, kind: Kind, sender: Data, recipient: Data,
        messageID: Data, counter: UInt64
    ) throws(WeftError) {
        self.version = version
        self.kind = kind
        self.sender = Data(try fixed(sender, Weft.deviceIDLength, "sender"))
        self.recipient = Data(try fixed(recipient, Weft.deviceIDLength, "recipient"))
        self.messageID = Data(try fixed(messageID, Weft.messageIDLength, "messageID"))
        self.counter = counter
    }

    /// The 58-byte wire form.
    public func encode() -> Data {
        Data(encodedBytes())
    }

    func encodedBytes() -> [UInt8] {
        var out = [UInt8]()
        out.reserveCapacity(Weft.headerLength)
        out.append(version)
        out.append(kind.rawValue)
        out.append(contentsOf: sender)
        out.append(contentsOf: recipient)
        out.append(contentsOf: messageID)
        withUnsafeBytes(of: counter.bigEndian) { out.append(contentsOf: $0) }
        return out
    }

    /// Decodes exactly `Weft.headerLength` bytes. Rejects unknown versions and
    /// kinds.
    public init(decoding data: Data) throws(WeftError) {
        try self.init(bytes: [UInt8](data))
    }

    init(bytes: [UInt8]) throws(WeftError) {
        guard bytes.count == Weft.headerLength else { throw .headerLength(bytes.count) }
        guard bytes[0] == Weft.version else { throw .unsupportedVersion(bytes[0]) }
        guard let kind = Kind(rawValue: bytes[1]) else { throw .unknownKind(bytes[1]) }
        self.version = bytes[0]
        self.kind = kind
        self.sender = Data(bytes[2..<18])
        self.recipient = Data(bytes[18..<34])
        self.messageID = Data(bytes[34..<50])
        self.counter = bytes[50..<58].reduce(0) { ($0 << 8) | UInt64($1) }
    }
}

/// Encrypts `plaintext` into an envelope with a random nonce.
public func seal(header: Header, plaintext: Data, tx: Data) throws(WeftError) -> Data {
    let nonce = Data(Sodium.randomBytes(Weft.nonceLength))
    return try seal(header: header, plaintext: plaintext, nonce: nonce, tx: tx)
}

/// Encrypts `plaintext` into an envelope with the given 24-byte nonce.
/// Only use a fixed nonce for test vectors; production code calls
/// `seal(header:plaintext:tx:)`.
public func seal(header: Header, plaintext: Data, nonce: Data, tx: Data) throws(WeftError) -> Data {
    let nonceBytes = try fixed(nonce, Weft.nonceLength, "nonce")
    let key = try fixed(tx, Sodium.aeadKeyBytes, "tx")
    guard header.version == Weft.version else { throw .unsupportedVersion(header.version) }
    let plaintextBytes = [UInt8](plaintext)
    try header.kind.checkPlaintext(plaintextBytes)
    let headerBytes = header.encodedBytes()
    let ciphertext = Sodium.aeadEncrypt(
        plaintext: plaintextBytes, additionalData: headerBytes, nonce: nonceBytes, key: key)
    var frame = Data(capacity: headerBytes.count + nonceBytes.count + ciphertext.count)
    frame.append(contentsOf: headerBytes)
    frame.append(contentsOf: nonceBytes)
    frame.append(contentsOf: ciphertext)
    return frame
}

/// Decrypts an envelope with the receiver's `rx` key.
public func open(frame: Data, rx: Data) throws(WeftError) -> (Header, Data) {
    let key = try fixed(rx, Sodium.aeadKeyBytes, "rx")
    let bytes = [UInt8](frame)
    guard bytes.count >= Weft.headerLength + Weft.nonceLength + Weft.tagLength else {
        throw .frameTooShort(bytes.count)
    }
    let headerBytes = Array(bytes[0..<Weft.headerLength])
    let nonce = Array(bytes[Weft.headerLength..<(Weft.headerLength + Weft.nonceLength)])
    let ciphertext = Array(bytes[(Weft.headerLength + Weft.nonceLength)...])
    let header = try Header(bytes: headerBytes)
    try header.kind.checkPlaintextLength(ciphertext.count - Weft.tagLength)
    guard let plaintext = Sodium.aeadDecrypt(
        ciphertext: ciphertext, additionalData: headerBytes, nonce: nonce, key: key)
    else {
        throw .decryptionFailed
    }
    try header.kind.checkPlaintext(plaintext)
    return (header, Data(plaintext))
}

// MARK: - Relay authentication

/// Relay challenge (`0xA0` frame).
public struct Challenge: Equatable, Sendable {
    public let relayName: String
    /// 32-byte challenge nonce.
    public let nonce: Data

    /// Builds a challenge. The relay name must fit in 255 bytes of UTF-8.
    public init(relayName: String, nonce: Data) throws(WeftError) {
        guard relayName.utf8.count <= Int(UInt8.max) else { throw .relayNameTooLong }
        self.relayName = relayName
        self.nonce = Data(try fixed(nonce, Weft.authNonceLength, "nonce"))
    }

    /// `relay_name_len || relay_name || nonce`, shared by the frame and the
    /// signed message.
    private var body: [UInt8] {
        let name = Array(relayName.utf8)
        return [UInt8(name.count)] + name + [UInt8](nonce)
    }

    /// `0xA0 || relay_name_len || relay_name || nonce`.
    public func encode() -> Data {
        Data([Weft.authChallenge] + body)
    }

    public init(decoding frame: Data) throws(WeftError) {
        let bytes = [UInt8](frame)
        guard let tag = bytes.first else { throw .malformedAuthFrame }
        guard tag == Weft.authChallenge else { throw .unexpectedFrameType(tag) }
        guard bytes.count >= 2 else { throw .malformedAuthFrame }
        let nameLength = Int(bytes[1])
        guard bytes.count - 2 == nameLength + Weft.authNonceLength else { throw .malformedAuthFrame }
        let name = bytes[2..<(2 + nameLength)]
        guard isValidUTF8(name) else { throw .invalidUTF8 }
        self.relayName = String(decoding: name, as: UTF8.self)
        self.nonce = Data(bytes[(2 + nameLength)...])
    }

    /// `"weft-auth-v1" || relay_name_len || relay_name || nonce`.
    var signedMessage: [UInt8] {
        Weft.authContext + body
    }
}

/// Device answer to a challenge (`0xA1` frame).
public struct AuthResponse: Equatable, Sendable {
    /// 32-byte Ed25519 public key.
    public let signPublicKey: Data
    /// 64-byte Ed25519 detached signature.
    public let signature: Data

    public init(signPublicKey: Data, signature: Data) throws(WeftError) {
        self.signPublicKey = Data(try fixed(signPublicKey, Sodium.signPublicKeyBytes, "signPublicKey"))
        self.signature = Data(try fixed(signature, Sodium.signatureBytes, "signature"))
    }

    /// `0xA1 || sign_pk || signature`.
    public func encode() -> Data {
        var out = Data([Weft.authResponse])
        out.append(signPublicKey)
        out.append(signature)
        return out
    }

    public init(decoding frame: Data) throws(WeftError) {
        let bytes = [UInt8](frame)
        guard let tag = bytes.first else { throw .malformedAuthFrame }
        guard tag == Weft.authResponse else { throw .unexpectedFrameType(tag) }
        guard bytes.count == Weft.authResponseLength else { throw .malformedAuthFrame }
        self.signPublicKey = Data(bytes[1..<33])
        self.signature = Data(bytes[33...])
    }
}

/// Client side: checks that the relay name in the challenge is exactly the
/// host the client connected to, before signing anything. `expectedHost` is
/// the lowercased host without its port; stripping the port is the caller's
/// job. The comparison is on the UTF-8 bytes, not on Swift's canonical
/// string equivalence.
public func checkRelayName(challenge: Challenge, expectedHost: String) throws(WeftError) {
    guard challenge.relayName.utf8.elementsEqual(expectedHost.utf8) else {
        throw .relayNameMismatch
    }
}

/// Signs a relay challenge with the device's signing key.
public func signAuth(identity: Identity, challenge: Challenge) -> AuthResponse {
    let signature = identity.sign(challenge.signedMessage)
    // Both sizes are fixed by libsodium, so this cannot fail.
    return try! AuthResponse(signPublicKey: identity.signPublicKey, signature: Data(signature))
}

/// Relay side: checks a response against the challenge the relay sent (with
/// the name it actually serves) and returns the authenticated device id.
public func verifyAuth(challenge: Challenge, response: AuthResponse) throws(WeftError) -> Data {
    let publicKey = [UInt8](response.signPublicKey)
    guard Sodium.verifyDetached(
        signature: [UInt8](response.signature), message: challenge.signedMessage, publicKey: publicKey)
    else {
        throw .invalidSignature
    }
    return Data(Sodium.genericHash16(publicKey))
}

// MARK: - Frame dispatch

/// What a frame received after authentication is, from its first byte.
public enum FrameType: Sendable {
    /// `0x01`: a message envelope, see `open(frame:rx:)`.
    case envelope
    /// `0xB0`–`0xBF`: a control frame, see `ControlFrame(decoding:)`.
    case control
    /// Anything else, including an empty frame. To be dropped.
    case unknown
}

/// Tells an envelope from a control frame by the first byte. This only
/// dispatches: the frame still has to be decoded and checked.
public func classify(_ frame: Data) -> FrameType {
    switch frame.first {
    case Weft.version?: .envelope
    case (0xB0...0xBF)?: .control
    default: .unknown
    }
}
