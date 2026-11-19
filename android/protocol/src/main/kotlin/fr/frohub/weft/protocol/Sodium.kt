package fr.frohub.weft.protocol

import com.goterl.lazysodium.LazySodiumAndroid
import com.goterl.lazysodium.Sodium as NativeSodium
import com.goterl.lazysodium.SodiumAndroid
import com.sun.jna.Memory
import com.sun.jna.Native

/**
 * Thin wrapper over the libsodium functions the protocol uses, called through
 * lazysodium's native interface. Callers pass buffers of the right size; the
 * public API checks every external input before reaching this object.
 */
internal object Sodium {
    const val SIGN_SEED_BYTES = 32
    const val SIGN_PK_BYTES = 32
    const val SIGN_SK_BYTES = 64
    const val SIGNATURE_BYTES = 64
    const val KX_SEED_BYTES = 32
    const val KX_PK_BYTES = 32
    const val KX_SK_BYTES = 32
    const val KX_SESSION_KEY_BYTES = 32
    const val AEAD_KEY_BYTES = 32
    const val AEAD_NONCE_BYTES = 24
    const val AEAD_TAG_BYTES = 16
    const val DEVICE_ID_BYTES = 16
    const val GENERIC_HASH_32_BYTES = 32

    /** libsodium's `sodium_base64_VARIANT_URLSAFE_NO_PADDING`. */
    private const val BASE64_URLSAFE_NO_PADDING = 7

    private val native: NativeSodium by lazy {
        val sodium = LazySodiumAndroid(SodiumAndroid()).sodium
        check(sodium.sodium_init() >= 0) { "sodium_init failed" }
        sodium
    }

    enum class KxRole { CLIENT, SERVER }

    fun signSeedKeypair(seed: ByteArray): Pair<ByteArray, ByteArray> {
        val pk = ByteArray(SIGN_PK_BYTES)
        val sk = ByteArray(SIGN_SK_BYTES)
        check(native.crypto_sign_seed_keypair(pk, sk, seed) == 0) { "crypto_sign_seed_keypair failed" }
        return pk to sk
    }

    fun kxSeedKeypair(seed: ByteArray): Pair<ByteArray, ByteArray> {
        val pk = ByteArray(KX_PK_BYTES)
        val sk = ByteArray(KX_SK_BYTES)
        check(native.crypto_kx_seed_keypair(pk, sk, seed) == 0) { "crypto_kx_seed_keypair failed" }
        return pk to sk
    }

    /** BLAKE2b with a 16-byte output and no key. */
    fun genericHash16(input: ByteArray): ByteArray {
        val out = ByteArray(DEVICE_ID_BYTES)
        check(native.crypto_generichash(out, out.size, input, input.size.toLong(), null, 0) == 0) {
            "crypto_generichash failed"
        }
        return out
    }

    /** Returns `(rx, tx)`, or null when the peer public key is unacceptable. */
    fun kxSessionKeys(
        role: KxRole,
        localPk: ByteArray,
        localSk: ByteArray,
        peerPk: ByteArray,
    ): Pair<ByteArray, ByteArray>? {
        val rx = ByteArray(KX_SESSION_KEY_BYTES)
        val tx = ByteArray(KX_SESSION_KEY_BYTES)
        val result = when (role) {
            KxRole.CLIENT -> native.crypto_kx_client_session_keys(rx, tx, localPk, localSk, peerPk)
            KxRole.SERVER -> native.crypto_kx_server_session_keys(rx, tx, localPk, localSk, peerPk)
        }
        if (result != 0) {
            memzero(rx)
            memzero(tx)
            return null
        }
        return rx to tx
    }

    fun signDetached(message: ByteArray, sk: ByteArray): ByteArray {
        val signature = ByteArray(SIGNATURE_BYTES)
        check(native.crypto_sign_detached(signature, null, message, message.size.toLong(), sk) == 0) {
            "crypto_sign_detached failed"
        }
        return signature
    }

    fun verifyDetached(signature: ByteArray, message: ByteArray, pk: ByteArray): Boolean =
        native.crypto_sign_verify_detached(signature, message, message.size.toLong(), pk) == 0

    fun aeadEncrypt(plaintext: ByteArray, additionalData: ByteArray, nonce: ByteArray, key: ByteArray): ByteArray {
        val ciphertext = ByteArray(plaintext.size + AEAD_TAG_BYTES)
        val ciphertextLength = LongArray(1)
        check(
            native.crypto_aead_xchacha20poly1305_ietf_encrypt(
                ciphertext, ciphertextLength,
                plaintext, plaintext.size.toLong(),
                additionalData, additionalData.size.toLong(),
                null, nonce, key,
            ) == 0 && ciphertextLength[0] == ciphertext.size.toLong(),
        ) { "crypto_aead_xchacha20poly1305_ietf_encrypt failed" }
        return ciphertext
    }

    /** Returns the plaintext, or null when authentication fails. */
    fun aeadDecrypt(ciphertext: ByteArray, additionalData: ByteArray, nonce: ByteArray, key: ByteArray): ByteArray? {
        if (ciphertext.size < AEAD_TAG_BYTES) return null
        val plaintext = ByteArray(ciphertext.size - AEAD_TAG_BYTES)
        val plaintextLength = LongArray(1)
        val result = native.crypto_aead_xchacha20poly1305_ietf_decrypt(
            plaintext, plaintextLength,
            null,
            ciphertext, ciphertext.size.toLong(),
            additionalData, additionalData.size.toLong(),
            nonce, key,
        )
        if (result != 0 || plaintextLength[0] != plaintext.size.toLong()) {
            memzero(plaintext)
            return null
        }
        return plaintext
    }

    /** BLAKE2b with a 32-byte output and a 32-byte key. */
    fun genericHash32Keyed(input: ByteArray, key: ByteArray): ByteArray {
        val out = ByteArray(GENERIC_HASH_32_BYTES)
        check(native.crypto_generichash(out, out.size, input, input.size.toLong(), key, key.size) == 0) {
            "crypto_generichash failed"
        }
        return out
    }

    /** BLAKE2b with a 16-byte output and a 32-byte key. */
    fun genericHash16Keyed(input: ByteArray, key: ByteArray): ByteArray {
        val out = ByteArray(DEVICE_ID_BYTES)
        check(native.crypto_generichash(out, out.size, input, input.size.toLong(), key, key.size) == 0) {
            "crypto_generichash failed"
        }
        return out
    }

    /** `sodium_memcmp`: constant-time equality of two arrays of the same size. */
    fun memcmp(a: ByteArray, b: ByteArray): Boolean {
        require(a.size == b.size) { "memcmp needs arrays of the same size" }
        return native.sodium_memcmp(a, b, a.size) == 0
    }

    /** `sodium_bin2base64`, URL-safe alphabet, no padding. */
    fun base64UrlEncode(bin: ByteArray): String {
        // The encoded length includes the trailing NUL byte.
        val out = ByteArray(native.sodium_base64_encoded_len(bin.size, BASE64_URLSAFE_NO_PADDING))
        native.sodium_bin2base64(out, out.size, bin, bin.size, BASE64_URLSAFE_NO_PADDING)
        // Read the output buffer rather than the returned pointer: JNA copies
        // the array back after the call, the returned pointer may not outlive it.
        check(out.isNotEmpty() && out[out.size - 1] == 0.toByte()) { "sodium_bin2base64 failed" }
        return String(out, 0, out.size - 1, Charsets.US_ASCII)
    }

    /**
     * `sodium_base642bin`, URL-safe alphabet, no padding. Returns null if the
     * input has any other byte (padding and whitespace included), a length
     * the encoder cannot produce, or non-zero trailing bits.
     */
    fun base64UrlDecode(b64: ByteArray): ByteArray? {
        val out = ByteArray(b64.size / 4 * 3 + 3)
        Memory(Native.SIZE_T_SIZE.toLong()).use { binLength ->
            binLength.clear()
            // No `ignore` characters and no `b64_end` pointer: libsodium fails
            // unless the whole input is valid.
            val result = native.sodium_base642bin(
                out, out.size, b64, b64.size, null, binLength, null, BASE64_URLSAFE_NO_PADDING,
            )
            if (result != 0) {
                memzero(out)
                return null
            }
            val length = if (Native.SIZE_T_SIZE == 8) binLength.getLong(0) else binLength.getInt(0).toLong()
            check(length in 0..out.size) { "sodium_base642bin returned a bad length" }
            val decoded = out.copyOf(length.toInt())
            memzero(out)
            return decoded
        }
    }

    fun randomBytes(count: Int): ByteArray {
        val out = ByteArray(count)
        if (count > 0) native.randombytes_buf(out, count)
        return out
    }

    fun memzero(buffer: ByteArray) {
        if (buffer.isNotEmpty()) native.sodium_memzero(buffer, buffer.size)
    }
}
