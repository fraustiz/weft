// Group change plaintexts (kinds 0x03–0x05), device names (kind 0x06) and
// file chunks (kind 0x08), docs/protocol.md sections 3 and 5 and
// docs/features.md section 1. Mirrors crates/weft-protocol/src/lib.rs.

import Foundation

/// A random 16-byte `message_id` for an envelope header.
public func randomMessageID() -> Data {
    Data(Sodium.randomBytes(Weft.messageIDLength))
}

// MARK: - Peer keys

/// Public keys of one trusted device, as carried by the group change
/// envelopes: `sign_pk (32) || kx_pk (32)`.
public struct PeerKeys: Equatable, Hashable, Sendable {
    /// 32-byte Ed25519 public key.
    public let signPublicKey: Data
    /// 32-byte X25519 public key.
    public let kxPublicKey: Data

    public init(signPublicKey: Data, kxPublicKey: Data) throws(WeftError) {
        self.signPublicKey = Data(try fixed(signPublicKey, Sodium.signPublicKeyBytes, "signPublicKey"))
        self.kxPublicKey = Data(try fixed(kxPublicKey, Sodium.kxPublicKeyBytes, "kxPublicKey"))
    }

    /// Both lengths already checked.
    private init(bytes: ArraySlice<UInt8>) {
        let split = bytes.startIndex + Sodium.signPublicKeyBytes
        signPublicKey = Data(bytes[bytes.startIndex..<split])
        kxPublicKey = Data(bytes[split..<bytes.endIndex])
    }

    /// The 16-byte device id derived from `signPublicKey`.
    public var deviceID: Data {
        Data(Sodium.genericHash16([UInt8](signPublicKey)))
    }

    var encodedBytes: [UInt8] {
        [UInt8](signPublicKey) + [UInt8](kxPublicKey)
    }

    /// `bytes` must be exactly `Weft.peerKeysLength` long.
    static func decode(_ bytes: ArraySlice<UInt8>) -> PeerKeys {
        precondition(bytes.count == Weft.peerKeysLength)
        return PeerKeys(bytes: bytes)
    }
}

// MARK: - Welcome (0x03)

/// Welcome plaintext (kind `0x03`): `count (1) || count × (sign_pk || kx_pk)`.
/// Lists the other members of the group: neither the sender nor the
/// recipient, so at most `Weft.maxWelcomeDevices` entries.
public func encodeWelcome(_ members: [PeerKeys]) throws(WeftError) -> Data {
    guard members.count <= Weft.maxWelcomeDevices else { throw .tooManyDevices(members.count) }
    var out = [UInt8(members.count)]
    out.reserveCapacity(1 + members.count * Weft.peerKeysLength)
    for member in members {
        out.append(contentsOf: member.encodedBytes)
    }
    return Data(out)
}

/// Parses a Welcome plaintext. The length must be exactly `1 + count × 64`
/// and `count` at most `Weft.maxWelcomeDevices`.
public func decodeWelcome(_ plaintext: Data) throws(WeftError) -> [PeerKeys] {
    try decodeWelcome(bytes: [UInt8](plaintext))
}

func decodeWelcome(bytes: [UInt8]) throws(WeftError) -> [PeerKeys] {
    guard let first = bytes.first else { throw .invalidPlaintextLength(0) }
    let count = Int(first)
    guard count <= Weft.maxWelcomeDevices else { throw .tooManyDevices(count) }
    guard bytes.count - 1 == count * Weft.peerKeysLength else {
        throw .invalidPlaintextLength(bytes.count)
    }
    return (0..<count).map { index in
        let start = 1 + index * Weft.peerKeysLength
        return PeerKeys.decode(bytes[start..<(start + Weft.peerKeysLength)])
    }
}

// MARK: - Device added (0x04)

/// Device added plaintext (kind `0x04`): `sign_pk || kx_pk` of the new device.
public func encodeDeviceAdded(_ device: PeerKeys) -> Data {
    Data(device.encodedBytes)
}

/// Parses a Device added plaintext: exactly 64 bytes.
public func decodeDeviceAdded(_ plaintext: Data) throws(WeftError) -> PeerKeys {
    guard plaintext.count == Weft.peerKeysLength else { throw .invalidPlaintextLength(plaintext.count) }
    return PeerKeys.decode([UInt8](plaintext)[...])
}

// MARK: - Device removed (0x05)

/// Device removed plaintext (kind `0x05`): the 16-byte id of the removed
/// device.
public func encodeDeviceRemoved(deviceID: Data) throws(WeftError) -> Data {
    Data(try fixed(deviceID, Weft.deviceIDLength, "deviceID"))
}

/// Parses a Device removed plaintext: exactly 16 bytes.
public func decodeDeviceRemoved(_ plaintext: Data) throws(WeftError) -> Data {
    guard plaintext.count == Weft.deviceIDLength else { throw .invalidPlaintextLength(plaintext.count) }
    return Data(plaintext)
}

// MARK: - Device name (0x06)

/// Device name plaintext (kind `0x06`): the sender's own name in UTF-8,
/// 1 to `Weft.maxDeviceNameLength` bytes, without control characters
/// (U+0000–U+001F, U+007F–U+009F).
///
/// The name is sent exactly as given: no Unicode normalisation.
public func encodeDeviceName(_ name: String) throws(WeftError) -> Data {
    try checkDeviceName(name)
    return Data(name.utf8)
}

/// Parses a Device name plaintext, with the same rules as
/// `encodeDeviceName(_:)`; the UTF-8 must be well-formed (no overlong forms,
/// no surrogates, no truncated sequences).
public func decodeDeviceName(_ plaintext: Data) throws(WeftError) -> String {
    try decodeDeviceName(bytes: [UInt8](plaintext))
}

func decodeDeviceName(bytes: [UInt8]) throws(WeftError) -> String {
    try checkDeviceNameLength(bytes.count)
    guard isValidUTF8(bytes) else { throw .invalidUTF8 }
    let name = String(decoding: bytes, as: UTF8.self)
    try checkDeviceName(name)
    return name
}

func checkDeviceNameLength(_ length: Int) throws(WeftError) {
    guard (1...Weft.maxDeviceNameLength).contains(length) else { throw .deviceNameLength(length) }
}

private func checkDeviceName(_ name: String) throws(WeftError) {
    try checkDeviceNameLength(name.utf8.count)
    // Unicode scalars, not Characters: "\r\n" is one Character.
    let isControl = { (scalar: Unicode.Scalar) in
        (0x00...0x1F).contains(scalar.value) || (0x7F...0x9F).contains(scalar.value)
    }
    if name.unicodeScalars.contains(where: isControl) {
        throw .deviceNameControlCharacter
    }
}

// MARK: - File chunk (0x08)

/// A decoded file chunk plaintext (kind `0x08`).
public struct FileChunk: Equatable, Sendable {
    /// 16-byte transfer id.
    public let transferID: Data
    /// Position of `data` in the file, in bytes.
    public let offset: UInt64
    /// 1 to `Weft.maxFileChunkDataLength` bytes.
    public let data: Data

    public init(transferID: Data, offset: UInt64, data: Data) throws(WeftError) {
        self.transferID = Data(try fixed(transferID, Weft.transferIDLength, "transferID"))
        self.offset = offset
        self.data = Data(data)
    }

    fileprivate init(uncheckedTransferID: Data, offset: UInt64, data: Data) {
        self.transferID = uncheckedTransferID
        self.offset = offset
        self.data = data
    }
}

/// File chunk plaintext (kind `0x08`):
/// `transfer_id (16) || offset (u64, big-endian) || data`, with 1 to
/// `Weft.maxFileChunkDataLength` bytes of data.
public func encodeFileChunk(transferID: Data, offset: UInt64, data: Data) throws(WeftError) -> Data {
    let id = try fixed(transferID, Weft.transferIDLength, "transferID")
    let length = Weft.fileChunkPrefixLength + data.count
    try Kind.fileChunk.checkPlaintextLength(length)
    var out = Data(capacity: length)
    out.append(contentsOf: id)
    withUnsafeBytes(of: offset.bigEndian) { out.append(contentsOf: $0) }
    out.append(data)
    return out
}

/// Parses a file chunk plaintext, with the same rules as
/// `encodeFileChunk(transferID:offset:data:)`.
public func decodeFileChunk(_ plaintext: Data) throws(WeftError) -> FileChunk {
    try Kind.fileChunk.checkPlaintextLength(plaintext.count)
    let prefix = [UInt8](plaintext.prefix(Weft.fileChunkPrefixLength))
    let transferID = Data(prefix[0..<Weft.transferIDLength])
    let offset = prefix[Weft.transferIDLength..<Weft.fileChunkPrefixLength].reduce(UInt64(0)) {
        ($0 << 8) | UInt64($1)
    }
    let data = Data(plaintext.dropFirst(Weft.fileChunkPrefixLength))
    return FileChunk(uncheckedTransferID: transferID, offset: offset, data: data)
}
