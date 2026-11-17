// Thin wrappers around the few libsodium functions the protocol uses.
//
// Every wrapper makes sure libsodium is initialised first. Buffer lengths are
// checked by the callers in WeftProtocol.swift (fixed sizes below), which is
// what makes the pointer accesses here sound.

import Clibsodium

enum Sodium {
    static let signSeedBytes = 32
    static let signPublicKeyBytes = 32
    static let signSecretKeyBytes = 64
    static let signatureBytes = 64
    static let kxSeedBytes = 32
    static let kxPublicKeyBytes = 32
    static let kxSecretKeyBytes = 32
    static let sessionKeyBytes = 32
    static let aeadKeyBytes = 32
    static let aeadNonceBytes = 24
    static let aeadTagBytes = 16

    /// Initialised once per process (Swift globals and static lets are lazy
    /// and thread-safe). `sodium_init` returns 0 on success, 1 if already
    /// initialised and -1 on failure.
    private static let initialised: Bool = sodium_init() >= 0

    static func ensureInitialised() {
        precondition(initialised, "sodium_init failed")
    }

    static func randomBytes(_ count: Int) -> [UInt8] {
        ensureInitialised()
        var out = [UInt8](repeating: 0, count: count)
        out.withUnsafeMutableBytes { randombytes_buf($0.baseAddress!, $0.count) }
        return out
    }

    static func memzero(_ buffer: inout [UInt8]) {
        buffer.withUnsafeMutableBytes { raw in
            guard let base = raw.baseAddress else { return }
            sodium_memzero(base, raw.count)
        }
    }

    /// `crypto_sign_seed_keypair` -> (pk, sk).
    static func signSeedKeypair(seed: [UInt8]) -> (pk: [UInt8], sk: [UInt8]) {
        ensureInitialised()
        precondition(seed.count == signSeedBytes)
        var pk = [UInt8](repeating: 0, count: signPublicKeyBytes)
        var sk = [UInt8](repeating: 0, count: signSecretKeyBytes)
        let rc = crypto_sign_seed_keypair(&pk, &sk, seed)
        precondition(rc == 0, "crypto_sign_seed_keypair failed")
        return (pk, sk)
    }

    /// `crypto_kx_seed_keypair` -> (pk, sk).
    static func kxSeedKeypair(seed: [UInt8]) -> (pk: [UInt8], sk: [UInt8]) {
        ensureInitialised()
        precondition(seed.count == kxSeedBytes)
        var pk = [UInt8](repeating: 0, count: kxPublicKeyBytes)
        var sk = [UInt8](repeating: 0, count: kxSecretKeyBytes)
        let rc = crypto_kx_seed_keypair(&pk, &sk, seed)
        precondition(rc == 0, "crypto_kx_seed_keypair failed")
        return (pk, sk)
    }

    /// `crypto_generichash` (BLAKE2b) without a key, 16-byte output.
    static func genericHash16(_ input: [UInt8]) -> [UInt8] {
        ensureInitialised()
        var out = [UInt8](repeating: 0, count: 16)
        let rc = crypto_generichash(&out, 16, input, UInt64(input.count), nil, 0)
        precondition(rc == 0, "crypto_generichash failed")
        return out
    }

    enum KxRole { case client, server }

    /// `crypto_kx_client_session_keys` / `crypto_kx_server_session_keys`
    /// -> (rx, tx), or nil if the peer public key is unacceptable.
    static func kxSessionKeys(
        role: KxRole, localPublicKey: [UInt8], localSecretKey: [UInt8], peerPublicKey: [UInt8]
    ) -> (rx: [UInt8], tx: [UInt8])? {
        ensureInitialised()
        precondition(localPublicKey.count == kxPublicKeyBytes)
        precondition(localSecretKey.count == kxSecretKeyBytes)
        precondition(peerPublicKey.count == kxPublicKeyBytes)
        var rx = [UInt8](repeating: 0, count: sessionKeyBytes)
        var tx = [UInt8](repeating: 0, count: sessionKeyBytes)
        let rc: Int32
        switch role {
        case .client:
            rc = crypto_kx_client_session_keys(&rx, &tx, localPublicKey, localSecretKey, peerPublicKey)
        case .server:
            rc = crypto_kx_server_session_keys(&rx, &tx, localPublicKey, localSecretKey, peerPublicKey)
        }
        guard rc == 0 else { return nil }
        return (rx, tx)
    }

    /// `crypto_sign_detached`.
    static func signDetached(message: [UInt8], secretKey: [UInt8]) -> [UInt8] {
        ensureInitialised()
        precondition(secretKey.count == signSecretKeyBytes)
        var signature = [UInt8](repeating: 0, count: signatureBytes)
        let rc = crypto_sign_detached(&signature, nil, message, UInt64(message.count), secretKey)
        precondition(rc == 0, "crypto_sign_detached failed")
        return signature
    }

    /// `crypto_sign_verify_detached`.
    static func verifyDetached(signature: [UInt8], message: [UInt8], publicKey: [UInt8]) -> Bool {
        ensureInitialised()
        precondition(signature.count == signatureBytes)
        precondition(publicKey.count == signPublicKeyBytes)
        return crypto_sign_verify_detached(signature, message, UInt64(message.count), publicKey) == 0
    }

    /// `crypto_aead_xchacha20poly1305_ietf_encrypt` -> ciphertext || tag.
    static func aeadEncrypt(
        plaintext: [UInt8], additionalData: [UInt8], nonce: [UInt8], key: [UInt8]
    ) -> [UInt8] {
        ensureInitialised()
        precondition(nonce.count == aeadNonceBytes)
        precondition(key.count == aeadKeyBytes)
        var ciphertext = [UInt8](repeating: 0, count: plaintext.count + aeadTagBytes)
        var ciphertextLength: UInt64 = 0
        let rc = crypto_aead_xchacha20poly1305_ietf_encrypt(
            &ciphertext, &ciphertextLength,
            plaintext, UInt64(plaintext.count),
            additionalData, UInt64(additionalData.count),
            nil, nonce, key)
        precondition(rc == 0 && ciphertextLength == UInt64(ciphertext.count), "AEAD encryption failed")
        return ciphertext
    }

    /// `crypto_aead_xchacha20poly1305_ietf_decrypt` -> plaintext, or nil if
    /// authentication fails.
    static func aeadDecrypt(
        ciphertext: [UInt8], additionalData: [UInt8], nonce: [UInt8], key: [UInt8]
    ) -> [UInt8]? {
        ensureInitialised()
        precondition(nonce.count == aeadNonceBytes)
        precondition(key.count == aeadKeyBytes)
        guard ciphertext.count >= aeadTagBytes else { return nil }
        // Allocate at least one byte so the output pointer is never dangling.
        var plaintext = [UInt8](repeating: 0, count: max(1, ciphertext.count - aeadTagBytes))
        var plaintextLength: UInt64 = 0
        let rc = crypto_aead_xchacha20poly1305_ietf_decrypt(
            &plaintext, &plaintextLength, nil,
            ciphertext, UInt64(ciphertext.count),
            additionalData, UInt64(additionalData.count),
            nonce, key)
        guard rc == 0 else { return nil }
        return Array(plaintext.prefix(Int(plaintextLength)))
    }

    /// `crypto_generichash` (BLAKE2b) with a 32-byte key, 32-byte output.
    static func genericHash32Keyed(_ input: [UInt8], key: [UInt8]) -> [UInt8] {
        ensureInitialised()
        precondition(key.count == 32)
        var out = [UInt8](repeating: 0, count: 32)
        let rc = crypto_generichash(&out, 32, input, UInt64(input.count), key, 32)
        precondition(rc == 0, "crypto_generichash failed")
        return out
    }

    /// `sodium_memcmp`: constant-time equality of two buffers of the same
    /// size. Buffers of different sizes are unequal (the size is not secret).
    static func memcmp(_ a: [UInt8], _ b: [UInt8]) -> Bool {
        ensureInitialised()
        guard a.count == b.count else { return false }
        guard !a.isEmpty else { return true }
        return sodium_memcmp(a, b, a.count) == 0
    }

    private static let base64URLVariant = Int32(sodium_base64_VARIANT_URLSAFE_NO_PADDING)
    private static let base64Variant = Int32(sodium_base64_VARIANT_ORIGINAL)

    /// `sodium_bin2base64`, URL-safe alphabet, no padding.
    static func base64URLEncode(_ bin: [UInt8]) -> String {
        bin2base64(bin, variant: base64URLVariant)
    }

    /// `sodium_base642bin`, URL-safe alphabet, no padding. `nil` if the input
    /// has any other byte (padding and whitespace included), a length that
    /// cannot be produced by the encoder, or non-zero trailing bits.
    static func base64URLDecode(_ b64: [UInt8]) -> [UInt8]? {
        base642bin(b64, variant: base64URLVariant)
    }

    /// `sodium_bin2base64`, standard alphabet, with padding.
    static func base64Encode(_ bin: [UInt8]) -> String {
        bin2base64(bin, variant: base64Variant)
    }

    /// `sodium_base642bin`, standard alphabet, with padding. `nil` if the
    /// input has any other byte (whitespace included), missing padding, or
    /// non-zero trailing bits.
    static func base64Decode(_ b64: [UInt8]) -> [UInt8]? {
        base642bin(b64, variant: base64Variant)
    }

    private static func bin2base64(_ bin: [UInt8], variant: Int32) -> String {
        ensureInitialised()
        // libsodium encodes nothing as nothing; this also keeps the input
        // pointer below non-null.
        guard !bin.isEmpty else { return "" }
        // The returned length includes the trailing NUL byte.
        let maxLength = sodium_base64_encoded_len(bin.count, variant)
        var out = [CChar](repeating: 0, count: maxLength)
        let result = sodium_bin2base64(&out, maxLength, bin, bin.count, variant)
        precondition(result != nil, "sodium_bin2base64 failed")
        let ascii = out.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
        return String(decoding: ascii, as: UTF8.self)
    }

    private static func base642bin(_ b64: [UInt8], variant: Int32) -> [UInt8]? {
        ensureInitialised()
        // libsodium decodes nothing as nothing; this also keeps the input
        // pointer below non-null.
        guard !b64.isEmpty else { return [] }
        // Larger than any decoding of b64.count characters, and never empty.
        var out = [UInt8](repeating: 0, count: b64.count / 4 * 3 + 3)
        var outLength = 0
        let outCapacity = out.count
        // With no `ignore` set and no `b64_end` pointer, libsodium fails
        // unless the whole input is valid.
        let rc = b64.withUnsafeBytes { raw -> Int32 in
            let input = raw.bindMemory(to: CChar.self)
            return sodium_base642bin(
                &out, outCapacity, input.baseAddress, input.count,
                nil, &outLength, nil, variant)
        }
        guard rc == 0 else {
            memzero(&out)
            return nil
        }
        let decoded = Array(out.prefix(outLength))
        memzero(&out)
        return decoded
    }
}
