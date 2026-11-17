// Groups and pairing: group change plaintexts, pairing code, pair request
// MAC, control frames, frame dispatch and the client host check. Mirrors
// crates/weft-protocol/tests/pairing.rs.

import Foundation
import Testing
@testable import WeftProtocol

private func peer(_ n: UInt8) -> PeerKeys {
    try! PeerKeys(signPublicKey: bytes(n, 32), kxPublicKey: bytes(n &+ 100, 32))
}

private func peers(_ count: Int) -> [PeerKeys] {
    (0..<count).map { peer(UInt8($0)) }
}

// MARK: - Group change plaintexts

@Suite("group change plaintexts")
struct GroupPlaintextTests {
    @Test func peerKeysDeviceIDIsDerivedFromSignPublicKey() throws {
        let id = Identity.generate()
        let keys = try PeerKeys(signPublicKey: id.signPublicKey, kxPublicKey: id.kxPublicKey)
        #expect(keys.deviceID == id.deviceID)
    }

    @Test func peerKeysCheckLengths() {
        #expect(throws: WeftError.invalidLength(field: "signPublicKey", expected: 32, actual: 31)) {
            try PeerKeys(signPublicKey: bytes(0, 31), kxPublicKey: bytes(0, 32))
        }
        #expect(throws: WeftError.invalidLength(field: "kxPublicKey", expected: 32, actual: 33)) {
            try PeerKeys(signPublicKey: bytes(0, 32), kxPublicKey: bytes(0, 33))
        }
    }

    @Test func welcomeRoundTripForEveryCount() throws {
        for count in 0...Weft.maxWelcomeDevices {
            let members = peers(count)
            let plaintext = try encodeWelcome(members)
            #expect(plaintext.count == 1 + count * 64)
            #expect(Int(plaintext[0]) == count)
            #expect(try decodeWelcome(plaintext) == members)
        }
        // sign_pk comes first, then kx_pk.
        let plaintext = try encodeWelcome([peer(5)])
        #expect(plaintext[1..<33] == bytes(5, 32))
        #expect(plaintext[33...] == bytes(105, 32))
    }

    @Test func welcomeRejectsMoreThanEightDevices() {
        #expect(Weft.maxWelcomeDevices == 8)
        #expect(throws: WeftError.tooManyDevices(9)) { try encodeWelcome(peers(9)) }

        // Well-formed apart from the count.
        let nine = Data([9]) + bytes(0, 9 * 64)
        #expect(throws: WeftError.tooManyDevices(9)) { try decodeWelcome(nine) }
        #expect(throws: WeftError.tooManyDevices(255)) { try decodeWelcome(Data([0xFF])) }
    }

    @Test func welcomeDecodeRejectsWrongLengths() throws {
        #expect(throws: WeftError.invalidPlaintextLength(0)) { try decodeWelcome(Data()) }

        let plaintext = try encodeWelcome(peers(2))
        for length in 1..<plaintext.count {
            #expect(throws: WeftError.invalidPlaintextLength(length), "truncated to \(length)") {
                try decodeWelcome(plaintext.prefix(length))
            }
        }
        let longer = plaintext + [0]
        #expect(throws: WeftError.invalidPlaintextLength(longer.count)) { try decodeWelcome(longer) }

        // The count must match the number of entries, in both directions.
        var countTooLow = plaintext
        countTooLow[0] = 1
        #expect(throws: WeftError.invalidPlaintextLength(plaintext.count)) { try decodeWelcome(countTooLow) }
        var countTooHigh = plaintext
        countTooHigh[0] = 3
        #expect(throws: WeftError.invalidPlaintextLength(plaintext.count)) { try decodeWelcome(countTooHigh) }
    }

    @Test func welcomeDecodesFromASlice() throws {
        // Data slices keep their parent's indices.
        let padded = Data([0xAA, 0xBB]) + (try encodeWelcome(peers(2)))
        #expect(try decodeWelcome(padded.dropFirst(2)) == peers(2))
        #expect(try decodeDeviceAdded(padded.dropFirst(3).prefix(64)) == peer(0))
    }

    @Test func deviceAddedRoundTripAndWrongLengths() throws {
        let plaintext = encodeDeviceAdded(peer(1))
        #expect(plaintext.count == 64)
        #expect(try decodeDeviceAdded(plaintext) == peer(1))

        for length in [0, 1, 32, 63] {
            #expect(throws: WeftError.invalidPlaintextLength(length)) {
                try decodeDeviceAdded(plaintext.prefix(length))
            }
        }
        #expect(throws: WeftError.invalidPlaintextLength(65)) { try decodeDeviceAdded(bytes(0, 65)) }
    }

    @Test func deviceRemovedRoundTripAndWrongLengths() throws {
        let plaintext = try encodeDeviceRemoved(deviceID: bytes(6, 16))
        #expect(try decodeDeviceRemoved(plaintext) == bytes(6, 16))

        for length in [0, 1, 15, 17, 32] {
            #expect(throws: WeftError.invalidPlaintextLength(length)) { try decodeDeviceRemoved(bytes(0, length)) }
        }
        #expect(throws: WeftError.invalidLength(field: "deviceID", expected: 16, actual: 15)) {
            try encodeDeviceRemoved(deviceID: bytes(0, 15))
        }
    }

    @Test func groupEnvelopesRoundTrip() throws {
        let p = try TestPair()

        for count in [0, 1, Weft.maxWelcomeDevices] {
            let h = try p.header(from: p.a, to: p.b, kind: .welcome)
            let frame = try seal(header: h, plaintext: encodeWelcome(peers(count)), tx: p.aKeys.tx)
            let (opened, plaintext) = try open(frame: frame, rx: p.bKeys.rx)
            #expect(opened == h)
            #expect(try decodeWelcome(plaintext) == peers(count))
        }

        let added = try p.header(from: p.a, to: p.b, kind: .deviceAdded)
        let addedFrame = try seal(header: added, plaintext: encodeDeviceAdded(peer(3)), tx: p.aKeys.tx)
        let (addedOpened, addedPlaintext) = try open(frame: addedFrame, rx: p.bKeys.rx)
        #expect(addedOpened == added)
        #expect(try decodeDeviceAdded(addedPlaintext) == peer(3))

        let removed = try p.header(from: p.b, to: p.a, kind: .deviceRemoved)
        let removedFrame = try seal(
            header: removed, plaintext: encodeDeviceRemoved(deviceID: bytes(4, 16)), tx: p.bKeys.tx)
        let (removedOpened, removedPlaintext) = try open(frame: removedFrame, rx: p.aKeys.rx)
        #expect(removedOpened == removed)
        #expect(try decodeDeviceRemoved(removedPlaintext) == bytes(4, 16))
    }

    @Test(arguments: [(UInt8(0x03), Kind.welcome), (0x04, .deviceAdded), (0x05, .deviceRemoved), (0x06, .deviceName)])
    func groupKindsDecodeFromHeaderBytes(byte: UInt8, kind: Kind) throws {
        let p = try TestPair()
        let encoded = try p.header(from: p.a, to: p.b, kind: kind).encode()
        #expect(encoded[1] == byte)
        #expect(try Header(decoding: encoded).kind == kind)
    }

    @Test func sealRejectsMalformedGroupPlaintexts() throws {
        let p = try TestPair()
        let tx = p.aKeys.tx
        func sealing(_ kind: Kind, _ plaintext: Data) throws {
            _ = try seal(header: p.header(from: p.a, to: p.b, kind: kind), plaintext: plaintext, tx: tx)
        }

        #expect(throws: WeftError.invalidPlaintextLength(0)) { try sealing(.welcome, Data()) }
        #expect(throws: WeftError.invalidPlaintextLength(1)) { try sealing(.welcome, Data([1])) }
        #expect(throws: WeftError.invalidPlaintextLength(64)) { try sealing(.welcome, bytes(0, 64)) }
        // Right shape for one entry, but the count says two.
        var wrongCount = try encodeWelcome(peers(1))
        wrongCount[0] = 2
        #expect(throws: WeftError.invalidPlaintextLength(65)) { try sealing(.welcome, wrongCount) }
        // Nine entries: one more than allowed.
        let nine = Data([9]) + bytes(0, 9 * 64)
        #expect(throws: WeftError.plaintextTooLarge(1 + 9 * 64)) { try sealing(.welcome, nine) }

        #expect(throws: WeftError.invalidPlaintextLength(63)) { try sealing(.deviceAdded, bytes(0, 63)) }
        #expect(throws: WeftError.plaintextTooLarge(65)) { try sealing(.deviceAdded, bytes(0, 65)) }
        #expect(throws: WeftError.invalidPlaintextLength(0)) { try sealing(.deviceAdded, Data()) }

        #expect(throws: WeftError.invalidPlaintextLength(15)) { try sealing(.deviceRemoved, bytes(0, 15)) }
        #expect(throws: WeftError.plaintextTooLarge(17)) { try sealing(.deviceRemoved, bytes(0, 17)) }
    }

    @Test(arguments: [
        (Kind.welcome, 0, WeftError.invalidPlaintextLength(0)),
        (.welcome, 2, .invalidPlaintextLength(2)),
        (.welcome, 64, .invalidPlaintextLength(64)),
        (.welcome, 66, .invalidPlaintextLength(66)),
        (.welcome, 512, .invalidPlaintextLength(512)),
        (.welcome, 514, .plaintextTooLarge(514)),
        (.welcome, 1 + 9 * 64, .plaintextTooLarge(577)),
        (.welcome, 100_000, .plaintextTooLarge(100_000)),
        (.deviceAdded, 0, .invalidPlaintextLength(0)),
        (.deviceAdded, 63, .invalidPlaintextLength(63)),
        (.deviceAdded, 65, .plaintextTooLarge(65)),
        (.deviceRemoved, 0, .invalidPlaintextLength(0)),
        (.deviceRemoved, 15, .invalidPlaintextLength(15)),
        (.deviceRemoved, 17, .plaintextTooLarge(17)),
    ])
    func openRejectsWrongGroupLengthsBeforeDecrypting(kind: Kind, length: Int, expected: WeftError) throws {
        // The frame is garbage: a length error (rather than decryptionFailed)
        // proves the check ran before the AEAD.
        let (frame, rx) = try TestPair().garbageFrame(kind: kind, plaintextLength: length)
        #expect(throws: expected) { try open(frame: frame, rx: rx) }
    }

    @Test(arguments: [(Kind.welcome, 1), (.welcome, 65), (.welcome, 513), (.deviceAdded, 64), (.deviceRemoved, 16)])
    func rightGroupLengthsReachTheAEAD(kind: Kind, length: Int) throws {
        let (frame, rx) = try TestPair().garbageFrame(kind: kind, plaintextLength: length)
        #expect(throws: WeftError.decryptionFailed) { try open(frame: frame, rx: rx) }
        #expect(frame.count == Weft.headerLength + Weft.nonceLength + length + Weft.tagLength)
    }

    @Test func groupEnvelopeKindCannotBeSwapped() throws {
        // The kind is part of the associated data: a Device removed envelope
        // cannot be replayed as another kind, even one with a compatible length.
        let p = try TestPair()
        var frame = try seal(
            header: p.header(from: p.a, to: p.b, kind: .deviceRemoved),
            plaintext: encodeDeviceRemoved(deviceID: bytes(4, 16)), tx: p.aKeys.tx)
        frame[1] = Kind.clipboardText.rawValue
        #expect(throws: WeftError.decryptionFailed) { try open(frame: frame, rx: p.bKeys.rx) }
    }
}

// MARK: - Randomness

@Suite("random helpers")
struct RandomTests {
    @Test func randomHelpersReturnFreshValues() {
        #expect(randomMessageID() != randomMessageID())
        #expect(randomPairingID() != randomPairingID())
        #expect(randomPairingSecret() != randomPairingSecret())
        #expect(randomMessageID() != bytes(0, 16))
        #expect(randomPairingSecret() != bytes(0, 32))
        #expect(randomMessageID().count == 16)
        #expect(randomPairingID().count == 16)
        #expect(randomPairingSecret().count == 32)
    }
}

// MARK: - Pairing code

private let url = "wss://weft.frohub.eu/v1"

private func pairingCode(_ relayURL: String, pairingID: UInt8 = 0x11, signPK: UInt8 = 0x22, secret: UInt8 = 0x44)
    -> PairingCode
{
    try! PairingCode(
        relayURL: relayURL, pairingID: bytes(pairingID, 16), signPublicKey: bytes(signPK, 32),
        kxPublicKey: bytes(0x33, 32), secret: bytes(secret, 32))
}

/// Standard base64url alphabet, to build inputs without going through the
/// code under test.
private func b64url(_ input: [UInt8]) -> String {
    let alphabet = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_".utf8)
    var out = [UInt8]()
    for start in stride(from: 0, to: input.count, by: 3) {
        let chunk = input[start..<min(start + 3, input.count)]
        let n = chunk.reduce(UInt32(0)) { ($0 << 8) | UInt32($1) } << (8 * (3 - chunk.count))
        for i in 0...chunk.count {
            out.append(alphabet[Int((n >> (18 - 6 * i)) & 63)])
        }
    }
    return String(decoding: out, as: UTF8.self)
}

/// A code built from a raw payload.
private func code(fromPayload payload: [UInt8]) -> String {
    "WEFT1:" + b64url(payload)
}

/// `"WEFT1:"` + the given bytes, which may not be valid UTF-8.
private func codeString(_ utf8: [UInt8]) -> String {
    String(decoding: utf8, as: UTF8.self)
}

@Suite("pairing code")
struct PairingCodeTests {
    @Test(arguments: [url, "w", "ws", "wss", "wss://rélais.example/v1", String(repeating: "a", count: 255)])
    func roundTrip(relayURL: String) throws {
        // Url lengths covering the three base64 tail shapes, and both bounds.
        let pairing = pairingCode(relayURL)
        let encoded = try pairing.encode()
        #expect(encoded.hasPrefix("WEFT1:"))
        #expect(encoded.utf8.dropFirst(6).allSatisfy { c in
            (UInt8(ascii: "A")...UInt8(ascii: "Z")).contains(c) || (UInt8(ascii: "a")...UInt8(ascii: "z")).contains(c)
                || (UInt8(ascii: "0")...UInt8(ascii: "9")).contains(c) || c == UInt8(ascii: "-")
                || c == UInt8(ascii: "_")
        })
        #expect(encoded == code(fromPayload: try pairing.payload()))
        #expect(try PairingCode(decoding: encoded) == pairing)
    }

    @Test func libsodiumBase64MatchesTheReferenceEncoder() throws {
        // Every tail shape, many lengths, random content.
        for length in 0...300 {
            let input = Sodium.randomBytes(length)
            let encoded = Sodium.base64URLEncode(input)
            #expect(encoded == b64url(input), "length \(length)")
            #expect(Sodium.base64URLDecode(Array(encoded.utf8)) == input, "length \(length)")
        }
    }

    @Test func usesTheURLSafeAlphabet() throws {
        // 0xfb 0xff… produces '-' and '_' where standard base64 has '+' and '/'.
        let code = pairingCode(url, pairingID: 0xFB, signPK: 0xFF)
        let encoded = try code.encode()
        #expect(encoded.contains("-") && encoded.contains("_"))
        #expect(try PairingCode(decoding: encoded) == code)

        // The same code in the standard alphabet is not accepted.
        let standard = encoded.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        #expect(throws: WeftError.malformedPairingCode) { try PairingCode(decoding: standard) }
    }

    @Test func encodeRejectsBadURLLengths() {
        #expect(throws: WeftError.relayURLLength(0)) { try pairingCode("").encode() }
        #expect(throws: WeftError.relayURLLength(256)) {
            try pairingCode(String(repeating: "a", count: 256)).encode()
        }
        // Counted in UTF-8 bytes: 128 two-byte characters.
        #expect(throws: WeftError.relayURLLength(256)) {
            try pairingCode(String(repeating: "é", count: 128)).encode()
        }
    }

    @Test func decodeRejectsBadPrefix() throws {
        let encoded = try pairingCode(url).encode()
        let body = String(encoded.dropFirst(6))
        for bad in [
            body, "WEFT2:\(body)", "weft1:\(body)", "WEFT1\(body)", " WEFT1:\(body)", "", "WEFT1:", "WEFT1",
            // A combining mark right after the prefix: still the exact prefix
            // bytes, then a byte outside the alphabet.
            "WEFT1:\u{301}\(body)",
            // A fullwidth colon is not ':'.
            "WEFT1\u{FF1A}\(body)",
        ] {
            #expect(throws: WeftError.malformedPairingCode, "\(bad.debugDescription)") {
                try PairingCode(decoding: bad)
            }
        }
    }

    @Test func decodeRejectsBadBase64() throws {
        // This url makes the payload length a multiple of 3: no base64 tail.
        let encoded = Array(try pairingCode("wss://relay.example/v1").encode().utf8)
        #expect((encoded.count - 6) % 4 == 0)
        _ = try PairingCode(decoding: codeString(encoded))

        // Any character outside the alphabet, at any position.
        for badChar in ["+", "/", "=", " ", "\n", "\t", ".", "~", "%", "*", "\0", "é"] {
            for position in [6, encoded.count / 2, encoded.count - 1] {
                var bad = encoded
                bad.replaceSubrange(position...position, with: Array(badChar.utf8))
                #expect(throws: WeftError.malformedPairingCode, "\(badChar.debugDescription) at \(position)") {
                    try PairingCode(decoding: codeString(bad))
                }
            }
        }

        // Trailing garbage and whitespace are not ignored.
        for suffix in ["=", "==", "\n", " ", "A", "AAAA", "\r\n"] {
            #expect(throws: WeftError.self, "suffix \(suffix.debugDescription)") {
                try PairingCode(decoding: codeString(encoded) + suffix)
            }
        }
        // Nor is leading whitespace after the prefix.
        #expect(throws: WeftError.malformedPairingCode) {
            try PairingCode(decoding: "WEFT1: " + codeString(Array(encoded[6...])))
        }
    }

    @Test(arguments: [("wss", "="), ("ws", "==")])
    func decodeRejectsPadding(relayURL: String, padding: String) throws {
        // Urls of 3 and 2 bytes leave a 2- and 1-byte tail: padded base64
        // would end with "=" or "==".
        let encoded = try pairingCode(relayURL).encode()
        _ = try PairingCode(decoding: encoded)
        #expect((encoded.utf8.count - 6 + padding.utf8.count) % 4 == 0)
        #expect(throws: WeftError.malformedPairingCode) { try PairingCode(decoding: encoded + padding) }
    }

    @Test func decodeRejectsNonCanonicalBase64() throws {
        // With a 1-byte tail the last character only carries 2 bits; the 4
        // others must be zero. Setting one of them still "decodes" to the same
        // bytes in a lax decoder, so two different strings would be the same
        // code.
        let oneByteTail = try pairingCode("ws").encode()
        #expect(oneByteTail.last == "A", "test expects a zero tail character")
        #expect(throws: WeftError.malformedPairingCode) {
            try PairingCode(decoding: String(oneByteTail.dropLast()) + "B")
        }

        // With a 2-byte tail the last character carries 4 bits; the 2 others
        // must be zero (0x44 0x44 ends in 'Q' = 0b010000, 'R' sets a low bit).
        let twoByteTail = try pairingCode("wss").encode()
        #expect(twoByteTail.last == "Q")
        #expect(throws: WeftError.malformedPairingCode) {
            try PairingCode(decoding: String(twoByteTail.dropLast()) + "R")
        }

        // A single leftover character can never come out of an encoder.
        let whole = try pairingCode("wss://relay.example/v1").encode()
        #expect(throws: WeftError.malformedPairingCode) { try PairingCode(decoding: whole + "A") }
    }

    @Test func decodeRejectsWrongLengths() throws {
        let full = try pairingCode(url).payload()
        _ = try PairingCode(decoding: code(fromPayload: full))

        for length in 1..<full.count {
            #expect(throws: WeftError.malformedPairingCode, "truncated to \(length)") {
                try PairingCode(decoding: code(fromPayload: Array(full.prefix(length))))
            }
        }
        for extra in 1...4 {
            #expect(throws: WeftError.malformedPairingCode, "\(extra) extra bytes") {
                try PairingCode(decoding: code(fromPayload: full + [UInt8](repeating: 0, count: extra)))
            }
        }

        // url_len that does not match the url actually present.
        for wrongLength in [UInt8(url.utf8.count - 1), UInt8(url.utf8.count + 1), 255] {
            var bad = full
            bad[0] = wrongLength
            #expect(throws: WeftError.malformedPairingCode) { try PairingCode(decoding: code(fromPayload: bad)) }
        }

        // Far beyond the largest possible code.
        let huge = "WEFT1:" + String(repeating: "A", count: 1_000_000)
        #expect(throws: WeftError.malformedPairingCode) { try PairingCode(decoding: huge) }
    }

    @Test func decodeAcceptsTheLongestURLAndNothingLonger() throws {
        // 255-byte url: 368-byte payload, 491 base64 characters (bound: 492).
        let longest = try pairingCode(String(repeating: "a", count: 255)).encode()
        #expect(longest.utf8.count - 6 == 491)
        _ = try PairingCode(decoding: longest)
        #expect(throws: WeftError.malformedPairingCode) {
            try PairingCode(decoding: "WEFT1:" + String(repeating: "A", count: 496))
        }
    }

    @Test func decodeRejectsEmptyURL() {
        // Well-formed apart from url_len = 0.
        let emptyURL = [UInt8(0)] + [UInt8](repeating: 0x55, count: 112)
        #expect(throws: WeftError.relayURLLength(0)) { try PairingCode(decoding: code(fromPayload: emptyURL)) }
    }

    @Test func decodeRejectsInvalidUTF8URL() throws {
        var bad = try pairingCode(url).payload()
        bad[3] = 0xFF
        #expect(throws: WeftError.invalidUTF8) { try PairingCode(decoding: code(fromPayload: bad)) }
    }

    @Test func constructorChecksLengths() {
        #expect(throws: WeftError.invalidLength(field: "secret", expected: 32, actual: 31)) {
            try PairingCode(
                relayURL: url, pairingID: bytes(0, 16), signPublicKey: bytes(0, 32), kxPublicKey: bytes(0, 32),
                secret: bytes(0, 31))
        }
        #expect(throws: WeftError.invalidLength(field: "pairingID", expected: 16, actual: 17)) {
            try PairingCode(
                relayURL: url, pairingID: bytes(0, 17), signPublicKey: bytes(0, 32), kxPublicKey: bytes(0, 32),
                secret: bytes(0, 32))
        }
    }

    @Test func equalityComparesEveryField() {
        #expect(pairingCode(url) == pairingCode(url))
        #expect(pairingCode(url) != pairingCode(url, secret: 0x45))
        #expect(pairingCode(url) != pairingCode(url, pairingID: 0x12))
        #expect(pairingCode(url) != pairingCode("wss://other.example/v1"))
        // Byte-exact: precomposed and decomposed "é" are different urls.
        #expect(pairingCode("wss://r\u{E9}lais") != pairingCode("wss://re\u{301}lais"))
    }

    @Test func descriptionHidesTheSecret() {
        let code = pairingCode(url, secret: 0xAB)
        var dumped = ""
        dump(code, to: &dumped)
        for text in [code.description, code.debugDescription, "\(code)", String(reflecting: code), dumped] {
            #expect(text.contains(url), "\(text)")
            #expect(text.contains("<redacted>"), "\(text)")
            #expect(!text.lowercased().contains("abab"), "\(text)")
            #expect(!text.contains("171"), "\(text)")  // 0xab
        }
    }
}

// MARK: - Pair request MAC

private func pairRequest(secret: Data) -> PairRequest {
    try! PairRequest(secret: secret, pairingID: bytes(0x11, 16), signPublicKey: bytes(0x22, 32), kxPublicKey: bytes(0x33, 32))
}

private func with(
    _ request: PairRequest, pairingID: Data? = nil, signPublicKey: Data? = nil, kxPublicKey: Data? = nil,
    mac: Data? = nil
) -> PairRequest {
    try! PairRequest(
        pairingID: pairingID ?? request.pairingID, signPublicKey: signPublicKey ?? request.signPublicKey,
        kxPublicKey: kxPublicKey ?? request.kxPublicKey, mac: mac ?? request.mac)
}

@Suite("pair request MAC")
struct PairRequestTests {
    @Test func macVerifies() throws {
        let secret = randomPairingSecret()
        let request = pairRequest(secret: secret)
        #expect(
            request.mac
                == (try pairRequestMAC(
                    secret: secret, pairingID: bytes(0x11, 16), signPublicKey: bytes(0x22, 32),
                    kxPublicKey: bytes(0x33, 32))))
        #expect(try verifyPairRequest(secret: secret, request: request))

        // Still valid after a trip through the relay.
        let frame = try ControlFrame.pairRequest(request).encode()
        guard case .pairRequest(let received) = try ControlFrame(decoding: frame) else {
            Issue.record("not a pair request")
            return
        }
        #expect(try verifyPairRequest(secret: secret, request: received))
    }

    @Test func macRejectsAnyFlippedBit() throws {
        let secret = bytes(0x44, 32)
        let request = pairRequest(secret: secret)
        for byte in 0..<32 {
            for bit in 0..<8 {
                var mac = request.mac
                mac[byte] ^= 1 << bit
                #expect(
                    !(try verifyPairRequest(secret: secret, request: with(request, mac: mac))),
                    "mac accepted with bit \(bit) of byte \(byte) flipped")
            }
        }
        #expect(!(try verifyPairRequest(secret: secret, request: with(request, mac: bytes(0, 32)))))
    }

    @Test func macRejectsWrongSecret() throws {
        let secret = bytes(0x44, 32)
        let request = pairRequest(secret: secret)
        var oneBitOff = secret
        oneBitOff[31] ^= 0x80
        #expect(!(try verifyPairRequest(secret: oneBitOff, request: request)))
        #expect(!(try verifyPairRequest(secret: bytes(0, 32), request: request)))
        #expect(!(try verifyPairRequest(secret: randomPairingSecret(), request: request)))
        #expect(throws: WeftError.invalidLength(field: "secret", expected: 32, actual: 31)) {
            try verifyPairRequest(secret: bytes(0x44, 31), request: request)
        }
    }

    @Test func macCoversEveryField() throws {
        let secret = bytes(0x44, 32)
        let request = pairRequest(secret: secret)

        // The relay swapping in its own keys, one at a time.
        #expect(!(try verifyPairRequest(secret: secret, request: with(request, signPublicKey: bytes(0x99, 32)))))
        #expect(!(try verifyPairRequest(secret: secret, request: with(request, kxPublicKey: bytes(0x99, 32)))))

        // sign_pk and kx_pk exchanged.
        let swapped = with(request, signPublicKey: request.kxPublicKey, kxPublicKey: request.signPublicKey)
        #expect(!(try verifyPairRequest(secret: secret, request: swapped)))
        #expect(
            try pairRequestMAC(
                secret: secret, pairingID: bytes(0x11, 16), signPublicKey: bytes(0x22, 32),
                kxPublicKey: bytes(0x33, 32))
                != pairRequestMAC(
                    secret: secret, pairingID: bytes(0x11, 16), signPublicKey: bytes(0x33, 32),
                    kxPublicKey: bytes(0x22, 32)))

        #expect(!(try verifyPairRequest(secret: secret, request: with(request, pairingID: bytes(0x12, 16)))))
    }
}

// MARK: - Control frames

private func inviteRequest(secret: Data) -> PairRequest {
    try! PairRequest.invite(
        secret: secret, pairingID: bytes(0x11, 16), signPublicKey: bytes(0x22, 32), kxPublicKey: bytes(0x33, 32))
}

private let controlFrames: [(ControlFrame, UInt8, Int)] = [
    (.pairOpen(pairingID: bytes(1, 16)), 0xB0, 17),
    (.pairRequest(pairRequest(secret: bytes(0x44, 32))), 0xB1, 113),
    (.groupAdd(deviceID: bytes(2, 16)), 0xB2, 17),
    (.groupRemove(deviceID: bytes(3, 16)), 0xB3, 17),
    (.groupJoin(deviceID: bytes(4, 16)), 0xB4, 17),
    (.pairInvite(inviteRequest(secret: bytes(0x44, 32))), 0xB5, 113),
    (.error(.unknownPairing), 0xBE, 2),
    (.error(.notAllowed), 0xBE, 2),
    (.error(.rateLimited), 0xBE, 2),
    (.error(.groupFull), 0xBE, 2),
]

@Suite("control frames")
struct ControlFrameTests {
    @Test func roundTrip() throws {
        for (frame, tag, length) in controlFrames {
            let encoded = try frame.encode()
            #expect(encoded[0] == tag, "\(frame)")
            #expect(encoded.count == length, "\(frame)")
            #expect(try ControlFrame(decoding: encoded) == frame)
            #expect(classify(encoded) == .control)
        }
    }

    @Test func layouts() throws {
        #expect(try ControlFrame.pairOpen(pairingID: bytes(1, 16)).encode() == Data([0xB0]) + bytes(1, 16))

        let request = try PairRequest(
            pairingID: bytes(1, 16), signPublicKey: bytes(2, 32), kxPublicKey: bytes(3, 32), mac: bytes(4, 32))
        #expect(
            try ControlFrame.pairRequest(request).encode()
                == Data([0xB1]) + bytes(1, 16) + bytes(2, 32) + bytes(3, 32) + bytes(4, 32))
        #expect(
            try ControlFrame.pairInvite(request).encode()
                == Data([0xB5]) + bytes(1, 16) + bytes(2, 32) + bytes(3, 32) + bytes(4, 32))

        #expect(try ControlFrame.groupAdd(deviceID: bytes(5, 16)).encode() == Data([0xB2]) + bytes(5, 16))
        #expect(try ControlFrame.groupRemove(deviceID: bytes(6, 16)).encode() == Data([0xB3]) + bytes(6, 16))
        #expect(try ControlFrame.groupJoin(deviceID: bytes(7, 16)).encode() == Data([0xB4]) + bytes(7, 16))

        for (code, byte) in [
            (ControlError.unknownPairing, UInt8(0x01)), (.notAllowed, 0x02), (.rateLimited, 0x03), (.groupFull, 0x04),
        ] {
            #expect(try ControlFrame.error(code).encode() == Data([0xBE, byte]))
        }
    }

    @Test func encodeRejectsWrongIDLengths() {
        #expect(throws: WeftError.invalidLength(field: "pairingID", expected: 16, actual: 15)) {
            try ControlFrame.pairOpen(pairingID: bytes(0, 15)).encode()
        }
        #expect(throws: WeftError.invalidLength(field: "deviceID", expected: 16, actual: 17)) {
            try ControlFrame.groupAdd(deviceID: bytes(0, 17)).encode()
        }
        #expect(throws: WeftError.invalidLength(field: "deviceID", expected: 16, actual: 0)) {
            try ControlFrame.groupRemove(deviceID: Data()).encode()
        }
        #expect(throws: WeftError.invalidLength(field: "deviceID", expected: 16, actual: 15)) {
            try ControlFrame.groupJoin(deviceID: bytes(0, 15)).encode()
        }
    }

    @Test func decodeRejectsWrongLengths() throws {
        #expect(throws: WeftError.malformedControlFrame) { try ControlFrame(decoding: Data()) }
        for (frame, _, _) in controlFrames {
            let encoded = try frame.encode()
            for length in 1..<encoded.count {
                #expect(throws: WeftError.malformedControlFrame, "\(frame) truncated to \(length)") {
                    try ControlFrame(decoding: encoded.prefix(length))
                }
            }
            #expect(throws: WeftError.malformedControlFrame, "\(frame) with an extra byte") {
                try ControlFrame(decoding: encoded + [0])
            }
        }
    }

    @Test func decodesFromASlice() throws {
        // Data slices keep their parent's indices.
        let padded = Data([0xFF, 0xFF]) + (try ControlFrame.error(.groupFull).encode())
        #expect(try ControlFrame(decoding: padded.dropFirst(2)) == .error(.groupFull))
    }

    @Test(arguments: [UInt8](0xB6...0xBD) + [0xBF])
    func decodeRejectsUnknownControlBytes(tag: UInt8) {
        // Whatever the body length.
        for bodyLength in [0, 1, 16, 112] {
            #expect(throws: WeftError.unknownControlFrame(tag)) {
                try ControlFrame(decoding: Data([tag]) + bytes(0, bodyLength))
            }
        }
    }

    @Test(arguments: [UInt8(0x00), 0x01, 0xA0, 0xA1, 0xA2, 0xAF, 0xC0, 0xFF])
    func decodeRejectsOtherFrameTypes(tag: UInt8) {
        #expect(throws: WeftError.unexpectedFrameType(tag)) { try ControlFrame(decoding: Data([tag]) + bytes(0, 16)) }
    }

    @Test func decodeRejectsUnknownErrorCodes() {
        for code in UInt8(0x00)...0xFF where !(0x01...0x04).contains(code) {
            #expect(throws: WeftError.unknownControlError(code)) { try ControlFrame(decoding: Data([0xBE, code])) }
        }
    }
}

// MARK: - Frame dispatch

@Suite("frame dispatch")
struct ClassifyTests {
    @Test func classifyByFirstByte() {
        #expect(classify(Data()) == .unknown)
        for byte in UInt8(0x00)...0xFF {
            let expected: FrameType =
                switch byte {
                case 0x01: .envelope
                case 0xB0...0xBF: .control
                default: .unknown
                }
            #expect(classify(Data([byte])) == expected, "\(byte)")
            #expect(classify(Data([byte, 0, 0])) == expected, "\(byte)")
            // Data slices keep their parent's indices.
            #expect(classify(Data([0x00, byte]).dropFirst()) == expected, "\(byte)")
        }
    }

    @Test func classifyRealFrames() throws {
        let p = try TestPair()
        let envelope = try seal(header: p.header(from: p.a, to: p.b, kind: .ping), plaintext: Data(), tx: p.aKeys.tx)
        #expect(classify(envelope) == .envelope)

        // Authentication frames are not expected after the handshake.
        let challenge = try Challenge(relayName: "weft.frohub.eu", nonce: bytes(9, 32))
        #expect(classify(challenge.encode()) == .unknown)
        #expect(classify(Data([Weft.authAccepted])) == .unknown)
    }
}

// MARK: - Client host check

@Suite("relay name check")
struct RelayNameTests {
    @Test func checkRelayNameRequiresAnExactMatch() throws {
        let challenge = try Challenge(relayName: "weft.frohub.eu", nonce: bytes(9, 32))
        try checkRelayName(challenge: challenge, expectedHost: "weft.frohub.eu")

        for other in [
            "", "evil.example", "weft.frohub.eu:443", "weft.frohub.eu.", "weft.frohub.eu.evil.example",
            "evil.weft.frohub.eu", "WEFT.frohub.eu", " weft.frohub.eu", "weft.frohub.e", "wss://weft.frohub.eu",
        ] {
            #expect(throws: WeftError.relayNameMismatch, "\(other.debugDescription)") {
                try checkRelayName(challenge: challenge, expectedHost: other)
            }
        }

        let empty = try Challenge(relayName: "", nonce: bytes(9, 32))
        #expect(throws: WeftError.relayNameMismatch) {
            try checkRelayName(challenge: empty, expectedHost: "weft.frohub.eu")
        }
    }

    @Test func checkRelayNameIsByteExact() throws {
        // Swift's String == treats these as equal (canonical equivalence);
        // the protocol compares bytes.
        let precomposed = try Challenge(relayName: "r\u{E9}lais.example", nonce: bytes(9, 32))
        #expect(throws: WeftError.relayNameMismatch) {
            try checkRelayName(challenge: precomposed, expectedHost: "re\u{301}lais.example")
        }
        try checkRelayName(challenge: precomposed, expectedHost: "r\u{E9}lais.example")
    }
}
