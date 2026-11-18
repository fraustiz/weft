//! Safe wrappers around the few libsodium functions the protocol uses.
//!
//! Every wrapper calls [`init`] first, so callers never touch `sodium_init`
//! themselves. All buffers are fixed-size arrays or slices whose lengths are
//! checked here, which is what makes the `unsafe` blocks sound.

use std::sync::Once;

use libsodium_sys as ffi;

pub const SIGN_SEED_BYTES: usize = 32;
pub const SIGN_PK_BYTES: usize = 32;
pub const SIGN_SK_BYTES: usize = 64;
pub const SIGNATURE_BYTES: usize = 64;
pub const KX_SEED_BYTES: usize = 32;
pub const KX_PK_BYTES: usize = 32;
pub const KX_SK_BYTES: usize = 32;
pub const SESSION_KEY_BYTES: usize = 32;
pub const AEAD_NONCE_BYTES: usize = 24;
pub const AEAD_TAG_BYTES: usize = 16;

static INIT: Once = Once::new();

/// Initialises libsodium exactly once per process.
pub fn init() {
    INIT.call_once(|| {
        // SAFETY: sodium_init is thread-safe and may be called repeatedly.
        // It returns 0 on success, 1 if already initialised, -1 on failure.
        let rc = unsafe { ffi::sodium_init() };
        assert!(rc >= 0, "sodium_init failed");
    });
}

pub fn random_bytes(buf: &mut [u8]) {
    init();
    // SAFETY: writes exactly buf.len() bytes into buf.
    unsafe { ffi::randombytes_buf(buf.as_mut_ptr().cast(), buf.len()) };
}

pub fn memzero(buf: &mut [u8]) {
    // SAFETY: zeroes exactly buf.len() bytes of buf.
    unsafe { ffi::sodium_memzero(buf.as_mut_ptr().cast(), buf.len()) };
}

/// `crypto_sign_seed_keypair` → (pk, sk).
pub fn sign_seed_keypair(
    seed: &[u8; SIGN_SEED_BYTES],
) -> ([u8; SIGN_PK_BYTES], [u8; SIGN_SK_BYTES]) {
    init();
    let mut pk = [0u8; SIGN_PK_BYTES];
    let mut sk = [0u8; SIGN_SK_BYTES];
    // SAFETY: all buffers have the sizes libsodium expects.
    let rc =
        unsafe { ffi::crypto_sign_seed_keypair(pk.as_mut_ptr(), sk.as_mut_ptr(), seed.as_ptr()) };
    assert_eq!(rc, 0, "crypto_sign_seed_keypair failed");
    (pk, sk)
}

/// `crypto_kx_keypair` → fresh random (pk, sk).
pub fn kx_keypair() -> ([u8; KX_PK_BYTES], [u8; KX_SK_BYTES]) {
    init();
    let mut pk = [0u8; KX_PK_BYTES];
    let mut sk = [0u8; KX_SK_BYTES];
    // SAFETY: both buffers have the sizes libsodium expects.
    let rc = unsafe { ffi::crypto_kx_keypair(pk.as_mut_ptr(), sk.as_mut_ptr()) };
    assert_eq!(rc, 0, "crypto_kx_keypair failed");
    (pk, sk)
}

/// `crypto_kx_seed_keypair` → (pk, sk).
pub fn kx_seed_keypair(seed: &[u8; KX_SEED_BYTES]) -> ([u8; KX_PK_BYTES], [u8; KX_SK_BYTES]) {
    init();
    let mut pk = [0u8; KX_PK_BYTES];
    let mut sk = [0u8; KX_SK_BYTES];
    // SAFETY: all buffers have the sizes libsodium expects.
    let rc =
        unsafe { ffi::crypto_kx_seed_keypair(pk.as_mut_ptr(), sk.as_mut_ptr(), seed.as_ptr()) };
    assert_eq!(rc, 0, "crypto_kx_seed_keypair failed");
    (pk, sk)
}

/// `crypto_generichash` (BLAKE2b) without a key, 16-byte output.
pub fn generichash_16(input: &[u8]) -> [u8; 16] {
    init();
    let mut out = [0u8; 16];
    // SAFETY: out is 16 bytes (within BLAKE2b's 16..=64 range); no key.
    let rc = unsafe {
        ffi::crypto_generichash(
            out.as_mut_ptr(),
            out.len(),
            input.as_ptr(),
            input.len() as u64,
            std::ptr::null(),
            0,
        )
    };
    assert_eq!(rc, 0, "crypto_generichash failed");
    out
}

/// Which side of `crypto_kx` the local device plays.
#[derive(Clone, Copy)]
pub enum KxRole {
    Client,
    Server,
}

/// `crypto_kx_{client,server}_session_keys` → (rx, tx), or `None` if the
/// peer public key is not acceptable to libsodium.
pub fn kx_session_keys(
    role: KxRole,
    local_pk: &[u8; KX_PK_BYTES],
    local_sk: &[u8; KX_SK_BYTES],
    peer_pk: &[u8; KX_PK_BYTES],
) -> Option<([u8; SESSION_KEY_BYTES], [u8; SESSION_KEY_BYTES])> {
    init();
    let mut rx = [0u8; SESSION_KEY_BYTES];
    let mut tx = [0u8; SESSION_KEY_BYTES];
    // SAFETY: all buffers have the sizes libsodium expects.
    let rc = unsafe {
        match role {
            KxRole::Client => ffi::crypto_kx_client_session_keys(
                rx.as_mut_ptr(),
                tx.as_mut_ptr(),
                local_pk.as_ptr(),
                local_sk.as_ptr(),
                peer_pk.as_ptr(),
            ),
            KxRole::Server => ffi::crypto_kx_server_session_keys(
                rx.as_mut_ptr(),
                tx.as_mut_ptr(),
                local_pk.as_ptr(),
                local_sk.as_ptr(),
                peer_pk.as_ptr(),
            ),
        }
    };
    (rc == 0).then_some((rx, tx))
}

/// `crypto_sign_detached`.
pub fn sign_detached(message: &[u8], sk: &[u8; SIGN_SK_BYTES]) -> [u8; SIGNATURE_BYTES] {
    init();
    let mut sig = [0u8; SIGNATURE_BYTES];
    // SAFETY: sig is 64 bytes, sk is 64 bytes, message length is passed.
    let rc = unsafe {
        ffi::crypto_sign_detached(
            sig.as_mut_ptr(),
            std::ptr::null_mut(),
            message.as_ptr(),
            message.len() as u64,
            sk.as_ptr(),
        )
    };
    assert_eq!(rc, 0, "crypto_sign_detached failed");
    sig
}

/// `crypto_sign_verify_detached`; `true` if the signature is valid.
pub fn verify_detached(
    sig: &[u8; SIGNATURE_BYTES],
    message: &[u8],
    pk: &[u8; SIGN_PK_BYTES],
) -> bool {
    init();
    // SAFETY: sig is 64 bytes, pk is 32 bytes, message length is passed.
    let rc = unsafe {
        ffi::crypto_sign_verify_detached(
            sig.as_ptr(),
            message.as_ptr(),
            message.len() as u64,
            pk.as_ptr(),
        )
    };
    rc == 0
}

/// `crypto_aead_xchacha20poly1305_ietf_encrypt` → ciphertext || tag.
pub fn aead_encrypt(
    plaintext: &[u8],
    ad: &[u8],
    nonce: &[u8; AEAD_NONCE_BYTES],
    key: &[u8; SESSION_KEY_BYTES],
) -> Vec<u8> {
    init();
    let mut out = vec![0u8; plaintext.len() + AEAD_TAG_BYTES];
    let mut out_len: u64 = 0;
    // SAFETY: out has room for plaintext + tag; nonce and key have the
    // expected sizes; lengths are passed explicitly.
    let rc = unsafe {
        ffi::crypto_aead_xchacha20poly1305_ietf_encrypt(
            out.as_mut_ptr(),
            &mut out_len,
            plaintext.as_ptr(),
            plaintext.len() as u64,
            ad.as_ptr(),
            ad.len() as u64,
            std::ptr::null(),
            nonce.as_ptr(),
            key.as_ptr(),
        )
    };
    assert_eq!(rc, 0, "crypto_aead_xchacha20poly1305_ietf_encrypt failed");
    out.truncate(out_len as usize);
    out
}

/// `crypto_aead_xchacha20poly1305_ietf_decrypt`; `None` if authentication fails.
pub fn aead_decrypt(
    ciphertext: &[u8],
    ad: &[u8],
    nonce: &[u8; AEAD_NONCE_BYTES],
    key: &[u8; SESSION_KEY_BYTES],
) -> Option<Vec<u8>> {
    init();
    if ciphertext.len() < AEAD_TAG_BYTES {
        return None;
    }
    let mut out = vec![0u8; ciphertext.len() - AEAD_TAG_BYTES];
    let mut out_len: u64 = 0;
    // SAFETY: out has room for ciphertext - tag bytes; nonce and key have the
    // expected sizes; lengths are passed explicitly.
    let rc = unsafe {
        ffi::crypto_aead_xchacha20poly1305_ietf_decrypt(
            out.as_mut_ptr(),
            &mut out_len,
            std::ptr::null_mut(),
            ciphertext.as_ptr(),
            ciphertext.len() as u64,
            ad.as_ptr(),
            ad.len() as u64,
            nonce.as_ptr(),
            key.as_ptr(),
        )
    };
    if rc != 0 {
        return None;
    }
    out.truncate(out_len as usize);
    Some(out)
}

/// `crypto_generichash` (BLAKE2b) with a 32-byte key, 32-byte output.
pub fn generichash_32_keyed(input: &[u8], key: &[u8; 32]) -> [u8; 32] {
    init();
    let mut out = [0u8; 32];
    // SAFETY: out and key are 32 bytes (both within BLAKE2b's ranges); the
    // input length is passed explicitly.
    let rc = unsafe {
        ffi::crypto_generichash(
            out.as_mut_ptr(),
            out.len(),
            input.as_ptr(),
            input.len() as u64,
            key.as_ptr(),
            key.len(),
        )
    };
    assert_eq!(rc, 0, "crypto_generichash failed");
    out
}

/// `sodium_memcmp`: constant-time equality of two 32-byte values.
pub fn memcmp_32(a: &[u8; 32], b: &[u8; 32]) -> bool {
    init();
    // SAFETY: both buffers are exactly 32 bytes.
    let rc = unsafe { ffi::sodium_memcmp(a.as_ptr().cast(), b.as_ptr().cast(), a.len()) };
    rc == 0
}

const BASE64URL_VARIANT: std::ffi::c_int =
    ffi::sodium_base64_VARIANT_URLSAFE_NO_PADDING as std::ffi::c_int;
const BASE64_VARIANT: std::ffi::c_int = ffi::sodium_base64_VARIANT_ORIGINAL as std::ffi::c_int;

/// `sodium_bin2base64`, URL-safe alphabet, no padding.
pub fn base64url_encode(bin: &[u8]) -> String {
    bin2base64(bin, BASE64URL_VARIANT)
}

/// `sodium_base642bin`, URL-safe alphabet, no padding. `None` if the input
/// has any other character (padding and whitespace included), a length that
/// cannot be produced by the encoder, or non-zero trailing bits.
pub fn base64url_decode(b64: &str) -> Option<Vec<u8>> {
    base642bin(b64, BASE64URL_VARIANT)
}

/// `sodium_bin2base64`, standard alphabet, with padding.
pub fn base64_encode(bin: &[u8]) -> String {
    bin2base64(bin, BASE64_VARIANT)
}

/// `sodium_base642bin`, standard alphabet, with padding. `None` if the
/// input has any other character (whitespace included), missing padding,
/// or non-zero trailing bits.
pub fn base64_decode(b64: &str) -> Option<Vec<u8>> {
    base642bin(b64, BASE64_VARIANT)
}

fn bin2base64(bin: &[u8], variant: std::ffi::c_int) -> String {
    init();
    // SAFETY: pure function of its arguments. The returned length includes
    // the trailing NUL byte.
    let max_len = unsafe { ffi::sodium_base64_encoded_len(bin.len(), variant) };
    let mut out = vec![0u8; max_len];
    // SAFETY: out has the size libsodium asked for; bin length is passed.
    unsafe {
        ffi::sodium_bin2base64(
            out.as_mut_ptr().cast(),
            out.len(),
            bin.as_ptr(),
            bin.len(),
            variant,
        )
    };
    out.pop(); // trailing NUL
    String::from_utf8(out).expect("base64 output is ASCII")
}

fn base642bin(b64: &str, variant: std::ffi::c_int) -> Option<Vec<u8>> {
    init();
    let mut out = vec![0u8; b64.len() / 4 * 3 + 3];
    let mut out_len: usize = 0;
    // SAFETY: out is larger than any decoding of b64.len() characters and
    // its size is passed; b64 is read for exactly b64.len() bytes. With no
    // `ignore` set and no `b64_end` pointer, libsodium fails unless the
    // whole input is valid.
    let rc = unsafe {
        ffi::sodium_base642bin(
            out.as_mut_ptr(),
            out.len(),
            b64.as_ptr().cast(),
            b64.len(),
            std::ptr::null(),
            &mut out_len,
            std::ptr::null_mut(),
            variant,
        )
    };
    if rc != 0 {
        return None;
    }
    out.truncate(out_len);
    Some(out)
}
