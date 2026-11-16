// Device name envelopes (kind 0x06): length, UTF-8 and control character
// rules, in encodeDeviceName / decodeDeviceName and in seal / open. Mirrors
// crates/weft-protocol/tests/device_name.rs and the unit test in
// crates/weft-protocol/src/lib.rs.

import Foundation
import Testing
@testable import WeftProtocol

/// Checks a name through all four entry points: encode, decode, seal and
/// (when sealing works) open. `nil` means valid.
private func check(
    _ name: [UInt8], _ expected: WeftError?, sourceLocation: SourceLocation = #_sourceLocation
) throws {
    let p = try TestPair()
    let header = try p.header(from: p.a, to: p.b, kind: .deviceName)
    let plaintext = Data(name)
    let text: String? = isValidUTF8(name) ? String(decoding: name, as: UTF8.self) : nil
    let label = Comment(rawValue: "\(name)")
    if let expected {
        if let text {
            #expect(throws: expected, label, sourceLocation: sourceLocation) { try encodeDeviceName(text) }
        }
        #expect(throws: expected, label, sourceLocation: sourceLocation) { try decodeDeviceName(plaintext) }
        #expect(throws: expected, label, sourceLocation: sourceLocation) {
            try seal(header: header, plaintext: plaintext, tx: p.aKeys.tx)
        }
    } else {
        let text = try #require(text, sourceLocation: sourceLocation)
        #expect(try encodeDeviceName(text) == plaintext, sourceLocation: sourceLocation)
        #expect(try decodeDeviceName(plaintext) == text, sourceLocation: sourceLocation)
        let frame = try seal(header: header, plaintext: plaintext, tx: p.aKeys.tx)
        let (opened, openedPlaintext) = try open(frame: frame, rx: p.bKeys.rx)
        #expect(opened.kind == .deviceName, sourceLocation: sourceLocation)
        #expect(openedPlaintext == plaintext, sourceLocation: sourceLocation)
    }
}

private func utf8(_ string: String) -> [UInt8] { Array(string.utf8) }

/// A Device name envelope whose plaintext is encrypted as is, bypassing the
/// checks in `seal`, so `open` has to catch it after decrypting.
private func rawNameFrame(_ plaintext: [UInt8]) throws -> (frame: Data, rx: Data) {
    let p = try TestPair()
    let header = try p.header(from: p.a, to: p.b, kind: .deviceName).encodedBytes()
    let nonce = [UInt8](repeating: 9, count: Weft.nonceLength)
    let ciphertext = Sodium.aeadEncrypt(
        plaintext: plaintext, additionalData: header, nonce: nonce, key: [UInt8](p.aKeys.tx))
    return (Data(header + nonce + ciphertext), p.bKeys.rx)
}

@Suite("device name")
struct DeviceNameTests {
    @Test func kindByteIs0x06() throws {
        let p = try TestPair()
        let encoded = try p.header(from: p.a, to: p.b, kind: .deviceName).encode()
        #expect(encoded[1] == 0x06)
        #expect(try Header(decoding: encoded).kind == .deviceName)
    }

    @Test func validNames() throws {
        #expect(Weft.maxDeviceNameLength == 64)
        try check(utf8("x"), nil)
        try check(utf8("Téléphone de test"), nil)
        try check(utf8("MacBook Air ✂"), nil)
        try check([UInt8](repeating: UInt8(ascii: "a"), count: 64), nil)
        // 32 two-byte characters: exactly 64 bytes.
        try check(utf8(String(repeating: "é", count: 32)), nil)
        try check(utf8("  spaces are fine  "), nil)
        // Decomposed form is kept as is, byte for byte.
        try check(utf8("Te\u{301}le\u{301}phone"), nil)
    }

    @Test func lengthIs1To64Bytes() throws {
        try check([], .deviceNameLength(0))
        try check([UInt8](repeating: UInt8(ascii: "a"), count: 65), .deviceNameLength(65))
        // 64 bytes of 'a' and one 2-byte character: 66 bytes, 65 characters.
        try check(utf8(String(repeating: "a", count: 64) + "é"), .deviceNameLength(66))
        try check([UInt8](repeating: UInt8(ascii: "a"), count: 1000), .deviceNameLength(1000))
    }

    @Test(arguments: ["\u{0}", "\u{1}", "\n", "\u{1f}", "\u{7f}", "\u{80}", "\u{85}", "\u{9f}"])
    func controlCharactersAreRejectedAtEachBoundary(c: String) throws {
        try check(utf8("ab\(c)cd"), .deviceNameControlCharacter)
        // Alone, first or last.
        try check(utf8(c), .deviceNameControlCharacter)
        try check(utf8("\(c)x"), .deviceNameControlCharacter)
        try check(utf8("x\(c)"), .deviceNameControlCharacter)
    }

    @Test func charactersJustOutsideTheRangesAreAccepted() throws {
        for c in [" ", "~", "\u{a0}", "\u{ff}"] {
            try check(utf8("ab\(c)cd"), nil)
        }
        // "\r\n" is a single Character in Swift but two control scalars.
        try check(utf8("a\r\nb"), .deviceNameControlCharacter)
    }

    @Test func invalidUTF8IsRejected() throws {
        for name: [UInt8] in [
            [0xFF],
            [0x61, 0x62, 0x63, 0xC3],
            [0xC3, 0x28],
            [0xED, 0xA0, 0x80],  // UTF-16 surrogate
            [0xC0, 0xAF],  // overlong encoding
        ] {
            try check(name, .invalidUTF8)
        }
    }

    @Test func badLengthsAreRejectedBeforeDecrypting() throws {
        for length in [0, 65, 100, 256 * 1024] {
            let (frame, rx) = try TestPair().garbageFrame(kind: .deviceName, plaintextLength: length)
            #expect(throws: WeftError.deviceNameLength(length), "\(length) bytes") { try open(frame: frame, rx: rx) }
        }
        // Lengths 1 and 64 get as far as the AEAD.
        for length in [1, 64] {
            let (frame, rx) = try TestPair().garbageFrame(kind: .deviceName, plaintextLength: length)
            #expect(throws: WeftError.decryptionFailed) { try open(frame: frame, rx: rx) }
        }
        let (frame, _) = try TestPair().garbageFrame(kind: .deviceName, plaintextLength: 64)
        #expect(frame.count == Weft.headerLength + Weft.nonceLength + 64 + Weft.tagLength)
    }

    @Test func kindCannotBeSwappedWithClipboardText() throws {
        // A name and a clipboard text share the same plaintext rules up to
        // 64 bytes, but the kind is authenticated.
        let p = try TestPair()
        var frame = try seal(
            header: p.header(from: p.a, to: p.b, kind: .deviceName), plaintext: Data("Laptop".utf8), tx: p.aKeys.tx)
        frame[1] = Kind.clipboardText.rawValue
        #expect(throws: WeftError.decryptionFailed) { try open(frame: frame, rx: p.bKeys.rx) }
    }

    @Test func openChecksTheDecryptedDeviceName() throws {
        let (frame, rx) = try rawNameFrame(utf8("Téléphone"))
        #expect(try open(frame: frame, rx: rx).1 == Data(utf8("Téléphone")))

        for (plaintext, expected): ([UInt8], WeftError) in [
            (utf8("tab\there"), .deviceNameControlCharacter),
            (utf8("c1 \u{85}"), .deviceNameControlCharacter),
            ([0xFF, 0xFE], .invalidUTF8),
            (utf8("cut ") + [0xC3], .invalidUTF8),
        ] {
            let (frame, rx) = try rawNameFrame(plaintext)
            #expect(throws: expected, "\(plaintext)") { try open(frame: frame, rx: rx) }
        }
    }

    @Test func openChecksTheDecryptedWelcome() throws {
        // Same idea for a Welcome whose count byte is over the limit but whose
        // length is allowed: only the decrypted content can reveal it.
        let p = try TestPair()
        let header = try p.header(from: p.a, to: p.b, kind: .welcome).encodedBytes()
        let nonce = [UInt8](repeating: 9, count: Weft.nonceLength)
        let plaintext = [UInt8(9)] + [UInt8](repeating: 0, count: 8 * 64)
        let ciphertext = Sodium.aeadEncrypt(
            plaintext: plaintext, additionalData: header, nonce: nonce, key: [UInt8](p.aKeys.tx))
        #expect(throws: WeftError.tooManyDevices(9)) {
            try open(frame: Data(header + nonce + ciphertext), rx: p.bKeys.rx)
        }
    }
}
