// Packet (kind 0x07) and file chunk (kind 0x08) envelopes, the kind-agnostic
// route header, invitation MACs and domain separation, content hashes and
// standard RFC 4648 base64. Mirrors crates/weft-protocol/tests/features.rs
// and the invitation tests in crates/weft-protocol/tests/pairing.rs.

import Foundation
import Testing
@testable import WeftProtocol

/// Seals `plaintext` as `kind` from A to B and opens it again when sealing
/// succeeds.
private func roundTrip(_ kind: Kind, _ plaintext: Data) throws -> Data {
    let p = try TestPair()
    let frame = try seal(header: p.header(from: p.a, to: p.b, kind: kind), plaintext: plaintext, tx: p.aKeys.tx)
    let (opened, openedPlaintext) = try open(frame: frame, rx: p.bKeys.rx)
    #expect(opened.kind == kind)
    return openedPlaintext
}

/// A Packet envelope whose plaintext is encrypted as is, bypassing the checks
/// in `seal`, so `open` has to catch invalid UTF-8 after decrypting.
private func rawPacketFrame(_ plaintext: [UInt8]) throws -> (frame: Data, rx: Data) {
    let p = try TestPair()
    let header = try p.header(from: p.a, to: p.b, kind: .packet).encodedBytes()
    let nonce = [UInt8](repeating: 9, count: Weft.nonceLength)
    let ciphertext = Sodium.aeadEncrypt(
        plaintext: plaintext, additionalData: header, nonce: nonce, key: [UInt8](p.aKeys.tx))
    return (Data(header + nonce + ciphertext), p.bKeys.rx)
}

// MARK: - Packet (0x07) and File chunk (0x08)

@Suite("feature envelopes")
struct FeatureEnvelopeTests {
    @Test(arguments: [(Kind.packet, UInt8(0x07)), (.fileChunk, 0x08)])
    func kindBytes(kind: Kind, byte: UInt8) throws {
        let p = try TestPair()
        let encoded = try p.header(from: p.a, to: p.b, kind: kind).encode()
        #expect(encoded[1] == byte)
        #expect(try Header(decoding: encoded).kind == kind)
    }

    @Test func packetIs1To256KiBOfUTF8() throws {
        #expect(Weft.maxPacketLength == 256 * 1024)
        let json = Data(#"{"type":"ping","message":"héllo"}"#.utf8)
        #expect(try roundTrip(.packet, json) == json)

        let max = bytes(UInt8(ascii: "a"), Weft.maxPacketLength)
        #expect(try roundTrip(.packet, max) == max)

        // The JSON itself is not checked at the wire layer.
        let nonJSON = Data("not json".utf8)
        #expect(try roundTrip(.packet, nonJSON) == nonJSON)

        #expect(throws: WeftError.invalidPlaintextLength(0)) {
            try roundTrip(.packet, Data())
        }
        #expect(throws: WeftError.plaintextTooLarge(Weft.maxPacketLength + 1)) {
            try roundTrip(.packet, bytes(UInt8(ascii: "a"), Weft.maxPacketLength + 1))
        }
        for invalid: [UInt8] in [
            Array(#"{"a":""#.utf8) + [0xFF] + Array(#""}"#.utf8),
            [0xED, 0xA0, 0x80],  // UTF-16 surrogate
            [0xC0, 0xAF],  // overlong encoding
            [0xC3],  // truncated sequence
        ] {
            #expect(throws: WeftError.invalidUTF8) {
                try roundTrip(.packet, Data(invalid))
            }
            let (rawFrame, rx) = try rawPacketFrame(invalid)
            #expect(throws: WeftError.invalidUTF8) {
                try open(frame: rawFrame, rx: rx)
            }
        }
    }

    @Test(arguments: [
        (0, WeftError.invalidPlaintextLength(0)),
        (Weft.maxPacketLength + 1, .plaintextTooLarge(Weft.maxPacketLength + 1)),
        (1, .decryptionFailed),
        (Weft.maxPacketLength, .decryptionFailed),
    ])
    func openRejectsPacketLengthsBeforeDecrypting(length: Int, expected: WeftError) throws {
        let (frame, rx) = try TestPair().garbageFrame(kind: .packet, plaintextLength: length)
        #expect(throws: expected, "\(length) bytes") {
            try open(frame: frame, rx: rx)
        }
    }

    @Test func fileChunkRoundTripAndLayout() throws {
        let transferID = bytes(0x42, 16)
        let data = Data("data".utf8)
        let plaintext = try encodeFileChunk(transferID: transferID, offset: 0x0102_0304_0506_0708, data: data)
        #expect(plaintext.prefix(16) == transferID)
        #expect(plaintext[16..<24] == Data([1, 2, 3, 4, 5, 6, 7, 8]))
        #expect(plaintext[24...] == data)

        let opened = try roundTrip(.fileChunk, plaintext)
        #expect(
            try decodeFileChunk(opened)
                == FileChunk(transferID: transferID, offset: 0x0102_0304_0506_0708, data: data))

        // Data slices keep their parent's indices.
        let padded = Data([0xAA, 0xBB]) + plaintext
        #expect(
            try decodeFileChunk(padded.dropFirst(2))
                == FileChunk(transferID: transferID, offset: 0x0102_0304_0506_0708, data: data))
    }

    @Test func fileChunkDataIs1To128KiB() throws {
        #expect(Weft.transferIDLength == 16)
        #expect(Weft.maxFileChunkDataLength == 131_072)
        let id = bytes(1, 16)
        let maxData = bytes(0xAB, Weft.maxFileChunkDataLength)
        let plaintext = try encodeFileChunk(transferID: id, offset: UInt64.max, data: maxData)
        #expect(plaintext.count == 24 + Weft.maxFileChunkDataLength)
        let decoded = try decodeFileChunk(plaintext)
        #expect(decoded.transferID == id)
        #expect(decoded.offset == UInt64.max)
        #expect(decoded.data == maxData)
        #expect(try roundTrip(.fileChunk, plaintext) == plaintext)

        let oneByte = try encodeFileChunk(transferID: id, offset: 0, data: Data([0]))
        #expect(try decodeFileChunk(oneByte).data == Data([0]))

        #expect(throws: WeftError.invalidPlaintextLength(24)) {
            try encodeFileChunk(transferID: id, offset: 0, data: Data())
        }
        let tooLarge = 24 + Weft.maxFileChunkDataLength + 1
        #expect(throws: WeftError.plaintextTooLarge(tooLarge)) {
            try encodeFileChunk(transferID: id, offset: 0, data: bytes(0, Weft.maxFileChunkDataLength + 1))
        }
        for length in [0, 1, 23, 24] {
            #expect(throws: WeftError.invalidPlaintextLength(length)) {
                try decodeFileChunk(bytes(0, length))
            }
            #expect(throws: WeftError.invalidPlaintextLength(length)) {
                try roundTrip(.fileChunk, bytes(0, length))
            }
        }
        #expect(throws: WeftError.plaintextTooLarge(tooLarge)) {
            try decodeFileChunk(bytes(0, tooLarge))
        }

        for (length, expected): (Int, WeftError) in [
            (0, .invalidPlaintextLength(0)),
            (24, .invalidPlaintextLength(24)),
            (tooLarge, .plaintextTooLarge(tooLarge)),
            (25, .decryptionFailed),
            (24 + Weft.maxFileChunkDataLength, .decryptionFailed),
        ] {
            let (frame, rx) = try TestPair().garbageFrame(kind: .fileChunk, plaintextLength: length)
            #expect(throws: expected, "\(length) bytes") {
                try open(frame: frame, rx: rx)
            }
        }
    }

    @Test func fileChunkChecksTransferIDLength() {
        #expect(throws: WeftError.invalidLength(field: "transferID", expected: 16, actual: 15)) {
            try FileChunk(transferID: bytes(0, 15), offset: 0, data: Data([1]))
        }
        #expect(throws: WeftError.invalidLength(field: "transferID", expected: 16, actual: 17)) {
            try encodeFileChunk(transferID: bytes(0, 17), offset: 0, data: Data([1]))
        }
    }
}

// MARK: - Route header

@Suite("route header")
struct RouteHeaderTests {
    @Test(arguments: [UInt8(0x00), 0x01, 0x07, 0x08, 0x7F, 0xFF])
    func ignoresKindIncludingUnknownFutureKinds(kind: UInt8) throws {
        let p = try TestPair()
        var frame = try seal(header: p.header(from: p.a, to: p.b, kind: .ping), plaintext: Data(), tx: p.aKeys.tx)
        frame[1] = kind
        let expected = try RouteHeader(kind: kind, sender: p.a.deviceID, recipient: p.b.deviceID)
        #expect(try RouteHeader(decoding: frame) == expected)

        // Data slices keep their parent's indices.
        let padded = Data([0xAA, 0xBB]) + frame
        #expect(try RouteHeader(decoding: padded.dropFirst(2)) == expected)
    }

    @Test func checksVersionAndLength() throws {
        let p = try TestPair()
        let frame = try seal(header: p.header(from: p.a, to: p.b, kind: .ping), plaintext: Data(), tx: p.aKeys.tx)
        #expect(frame.count == Weft.headerLength + Weft.nonceLength + Weft.tagLength)

        #expect(throws: WeftError.frameTooShort(frame.count - 1)) {
            try RouteHeader(decoding: frame.prefix(frame.count - 1))
        }
        #expect(throws: WeftError.frameTooShort(0)) {
            try RouteHeader(decoding: Data())
        }

        var badVersion = frame
        badVersion[0] = 0x02
        #expect(throws: WeftError.unsupportedVersion(0x02)) {
            try RouteHeader(decoding: badVersion)
        }

        #expect(throws: WeftError.invalidLength(field: "sender", expected: 16, actual: 15)) {
            try RouteHeader(kind: 0x01, sender: bytes(0, 15), recipient: bytes(0, 16))
        }
        #expect(throws: WeftError.invalidLength(field: "recipient", expected: 16, actual: 17)) {
            try RouteHeader(kind: 0x01, sender: bytes(0, 16), recipient: bytes(0, 17))
        }
    }
}

// MARK: - Invitation MAC & domain separation

@Suite("invitation MAC")
struct InviteRequestTests {
    private func joinRequest(_ secret: Data) throws -> PairRequest {
        try PairRequest(
            secret: secret, pairingID: bytes(0x11, 16), signPublicKey: bytes(0x22, 32), kxPublicKey: bytes(0x33, 32))
    }

    private func inviteRequest(_ secret: Data) throws -> PairRequest {
        try PairRequest.invite(
            secret: secret, pairingID: bytes(0x11, 16), signPublicKey: bytes(0x22, 32), kxPublicKey: bytes(0x33, 32))
    }

    @Test func macVerifiesAndRoundTrips() throws {
        let secret = randomPairingSecret()
        let request = try inviteRequest(secret)
        #expect(
            request.mac
                == (try inviteRequestMAC(
                    secret: secret, pairingID: bytes(0x11, 16), signPublicKey: bytes(0x22, 32),
                    kxPublicKey: bytes(0x33, 32))))
        #expect(try verifyInviteRequest(secret: secret, request: request))

        let frame = try ControlFrame.pairInvite(request).encode()
        guard case .pairInvite(let received) = try ControlFrame(decoding: frame) else {
            Issue.record("not an invitation")
            return
        }
        #expect(try verifyInviteRequest(secret: secret, request: received))
    }

    @Test func joinAndInviteMACsNeverVerifyAsEachOther() throws {
        let secret = bytes(0x44, 32)
        let join = try joinRequest(secret)
        let invite = try inviteRequest(secret)
        #expect(join.mac != invite.mac)
        #expect(try verifyPairRequest(secret: secret, request: join))
        #expect(try verifyInviteRequest(secret: secret, request: invite))
        #expect(!(try verifyInviteRequest(secret: secret, request: join)))
        #expect(!(try verifyPairRequest(secret: secret, request: invite)))

        // Re-tagging a frame on the wire does not make its MAC pass under the
        // other context.
        var joinAsInvite = try ControlFrame.pairRequest(join).encode()
        joinAsInvite[0] = Weft.controlPairInvite
        guard case .pairInvite(let asInvite) = try ControlFrame(decoding: joinAsInvite) else {
            Issue.record("not an invitation")
            return
        }
        #expect(!(try verifyInviteRequest(secret: secret, request: asInvite)))

        var inviteAsJoin = try ControlFrame.pairInvite(invite).encode()
        inviteAsJoin[0] = Weft.controlPairRequest
        guard case .pairRequest(let asJoin) = try ControlFrame(decoding: inviteAsJoin) else {
            Issue.record("not a pair request")
            return
        }
        #expect(!(try verifyPairRequest(secret: secret, request: asJoin)))
    }

    @Test func macRejectsFlippedBitsAndWrongSecret() throws {
        let secret = bytes(0x44, 32)
        let request = try inviteRequest(secret)
        for byte in 0..<32 {
            for bit in 0..<8 {
                var mac = request.mac
                mac[byte] ^= 1 << bit
                let bad = try PairRequest(
                    pairingID: request.pairingID, signPublicKey: request.signPublicKey,
                    kxPublicKey: request.kxPublicKey, mac: mac)
                #expect(
                    !(try verifyInviteRequest(secret: secret, request: bad)),
                    "mac accepted with bit \(bit) of byte \(byte) flipped")
            }
        }

        for byte in [0, 15] {
            var pid = request.pairingID
            pid[byte] ^= 1
            let badPID = try PairRequest(
                pairingID: pid, signPublicKey: request.signPublicKey, kxPublicKey: request.kxPublicKey,
                mac: request.mac)
            #expect(!(try verifyInviteRequest(secret: secret, request: badPID)))

            var signPK = request.signPublicKey
            signPK[byte] ^= 1
            let badSign = try PairRequest(
                pairingID: request.pairingID, signPublicKey: signPK, kxPublicKey: request.kxPublicKey,
                mac: request.mac)
            #expect(!(try verifyInviteRequest(secret: secret, request: badSign)))

            var kxPK = request.kxPublicKey
            kxPK[byte] ^= 1
            let badKx = try PairRequest(
                pairingID: request.pairingID, signPublicKey: request.signPublicKey, kxPublicKey: kxPK,
                mac: request.mac)
            #expect(!(try verifyInviteRequest(secret: secret, request: badKx)))
        }

        var oneBitOff = secret
        oneBitOff[0] ^= 0x01
        #expect(!(try verifyInviteRequest(secret: oneBitOff, request: request)))
        #expect(!(try verifyInviteRequest(secret: bytes(0, 32), request: request)))
        #expect(!(try verifyInviteRequest(secret: randomPairingSecret(), request: request)))
        #expect(throws: WeftError.invalidLength(field: "secret", expected: 32, actual: 31)) {
            try verifyInviteRequest(secret: bytes(0x44, 31), request: request)
        }
    }
}

// MARK: - Content hash & standard base64

@Suite("content hash and standard base64")
struct HelpersTests {
    @Test func contentHashesAreBLAKE2b128() throws {
        let cover = Data("cover".utf8)
        let hash = contentHash(cover)
        #expect(hash.count == 16)
        #expect(hash == contentHash(cover))
        #expect(hash != contentHash(Data("cover2".utf8)))
        #expect(hash != contentHash(Data()))
        #expect(contentHash(Data()).count == 16)

        // Works on Data slices too.
        let padded = Data([0xAA, 0xBB]) + cover
        #expect(contentHash(padded.dropFirst(2)) == hash)

        // Same unkeyed 16-byte BLAKE2b as deviceID(fromSignPublicKey:).
        let pk = bytes(7, 32)
        #expect(contentHash(pk) == (try deviceID(fromSignPublicKey: pk)))
    }

    @Test func standardBase64RoundTripsAndIsStrict() {
        // RFC 4648 section 10 test vectors.
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            let plainData = Data(plain.utf8)
            #expect(base64Encode(plainData) == encoded)
            #expect(base64Decode(encoded) == plainData)
        }

        // Standard alphabet ('+' and '/'), not URL-safe ('-' and '_').
        let highBytes = Data([0xFB, 0xFF])
        #expect(base64Encode(highBytes) == "+/8=")
        #expect(base64Decode("+/8=") == highBytes)

        // Data slices and arbitrary binary lengths.
        let padded = Data([0x00, 0xFB, 0xFF])
        #expect(base64Encode(padded.dropFirst()) == "+/8=")
        for length in 0...128 {
            let raw = Data(Sodium.randomBytes(length))
            #expect(base64Decode(base64Encode(raw)) == raw)
        }

        for refused in ["Zg", "Zg=", "-_8=", "Zm9v\n", "Zm 9v", " Zm9v", "Zh==", "A", "Zm9é"] {
            #expect(base64Decode(refused) == nil, "\(refused.debugDescription)")
        }
    }
}
