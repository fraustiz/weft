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
}
