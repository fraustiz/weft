//! Weft wire protocol, version 1.
//!
//! Implements `docs/protocol.md`: device identities, session keys between two
//! devices, the encrypted message envelope and the relay authentication
//! frames. All cryptography goes through libsodium so the output is
//! byte-identical with the Kotlin (lazysodium) and Swift (swift-sodium)
//! clients.

mod sodium;

use sodium::KxRole;

/// Protocol version carried in every envelope header.
pub const VERSION: u8 = 0x01;
/// Size of an encoded envelope header.
pub const HEADER_LEN: usize = 58;
/// Size of the envelope nonce.
pub const NONCE_LEN: usize = sodium::AEAD_NONCE_BYTES;
/// Size of the AEAD authentication tag appended to the ciphertext.
pub const TAG_LEN: usize = sodium::AEAD_TAG_BYTES;
/// Largest clipboard text plaintext, in bytes (256 KiB).
pub const MAX_CLIPBOARD_TEXT_LEN: usize = 256 * 1024;

/// First byte of the relay challenge frame.
pub const AUTH_CHALLENGE: u8 = 0xA0;
/// First byte of the device response frame.
pub const AUTH_RESPONSE: u8 = 0xA1;
/// The one-byte frame the relay sends when authentication succeeds.
pub const AUTH_ACCEPTED: u8 = 0xA2;

const AUTH_CONTEXT: &[u8] = b"weft-auth-v1";
const AUTH_NONCE_LEN: usize = 32;
const AUTH_RESPONSE_LEN: usize = 1 + sodium::SIGN_PK_BYTES + sodium::SIGNATURE_BYTES;

/// Everything that can go wrong while handling protocol data.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("header must be {HEADER_LEN} bytes, got {0}")]
    HeaderLength(usize),
    #[error("frame too short: {0} bytes")]
    FrameTooShort(usize),
    #[error("unsupported protocol version {0:#04x}")]
    UnsupportedVersion(u8),
    #[error("unknown message kind {0:#04x}")]
    UnknownKind(u8),
    #[error("plaintext too large for its kind: {0} bytes")]
    PlaintextTooLarge(usize),
    #[error("ping plaintext must be empty")]
    PingNotEmpty,
    #[error("text is not valid UTF-8")]
    InvalidUtf8,
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("local and peer device ids are equal")]
    SameDeviceId,
    #[error("key exchange failed")]
    KeyExchangeFailed,
    #[error("unexpected frame type {0:#04x}")]
    UnexpectedFrameType(u8),
    #[error("malformed authentication frame")]
    MalformedAuthFrame,
    #[error("relay name longer than 255 bytes")]
    RelayNameTooLong,
    #[error("invalid signature")]
    InvalidSignature,
}

/// Derives a device id from a signing public key:
/// BLAKE2b (`crypto_generichash`), 16-byte output, no key.
pub fn device_id_from_sign_pk(sign_pk: &[u8; 32]) -> [u8; 16] {
    sodium::generichash_16(sign_pk)
}

/// A device's long-term key pairs (Ed25519 signing and X25519 key exchange).
pub struct Identity {
    sign_pk: [u8; sodium::SIGN_PK_BYTES],
    sign_sk: [u8; sodium::SIGN_SK_BYTES],
    kx_pk: [u8; sodium::KX_PK_BYTES],
    kx_sk: [u8; sodium::KX_SK_BYTES],
    device_id: [u8; 16],
}

impl Identity {
    /// Generates a fresh identity from random seeds.
    pub fn generate() -> Identity {
        let mut sign_seed = [0u8; 32];
        let mut kx_seed = [0u8; 32];
        sodium::random_bytes(&mut sign_seed);
        sodium::random_bytes(&mut kx_seed);
        let identity = Identity::from_seeds(sign_seed, kx_seed);
        sodium::memzero(&mut sign_seed);
        sodium::memzero(&mut kx_seed);
        identity
    }

    /// Derives an identity from its two seeds (`crypto_sign_seed_keypair` and
    /// `crypto_kx_seed_keypair`). Clients store the seeds and rebuild the
    /// identity at startup; test vectors use fixed seeds.
    pub fn from_seeds(sign_seed: [u8; 32], kx_seed: [u8; 32]) -> Identity {
        let (sign_pk, sign_sk) = sodium::sign_seed_keypair(&sign_seed);
        let (kx_pk, kx_sk) = sodium::kx_seed_keypair(&kx_seed);
        Identity {
            sign_pk,
            sign_sk,
            kx_pk,
            kx_sk,
            device_id: device_id_from_sign_pk(&sign_pk),
        }
    }

    pub fn device_id(&self) -> [u8; 16] {
        self.device_id
    }

    pub fn sign_pk(&self) -> [u8; 32] {
        self.sign_pk
    }

    pub fn kx_pk(&self) -> [u8; 32] {
        self.kx_pk
    }
}

impl Drop for Identity {
    fn drop(&mut self) {
        sodium::memzero(&mut self.sign_sk);
        sodium::memzero(&mut self.kx_sk);
    }
}

/// Session keys with one peer: encrypt with `tx`, decrypt with `rx`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionKeys {
    pub rx: [u8; 32],
    pub tx: [u8; 32],
}

/// Derives the session keys between `local` and a peer.
///
/// The device with the lower device id (bytewise) plays the libsodium
/// client role, the other one the server role.
pub fn session_keys(
    local: &Identity,
    peer_device_id: &[u8; 16],
    peer_kx_pk: &[u8; 32],
) -> Result<SessionKeys, Error> {
    let role = match local.device_id.cmp(peer_device_id) {
        std::cmp::Ordering::Less => KxRole::Client,
        std::cmp::Ordering::Greater => KxRole::Server,
        std::cmp::Ordering::Equal => return Err(Error::SameDeviceId),
    };
    let (rx, tx) = sodium::kx_session_keys(role, &local.kx_pk, &local.kx_sk, peer_kx_pk)
        .ok_or(Error::KeyExchangeFailed)?;
    Ok(SessionKeys { rx, tx })
}

/// Message kind carried in the envelope header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// UTF-8 text, at most [`MAX_CLIPBOARD_TEXT_LEN`] bytes.
    ClipboardText = 0x01,
    /// Empty plaintext.
    Ping = 0x02,
}

impl Kind {
    fn from_byte(byte: u8) -> Result<Kind, Error> {
        match byte {
            0x01 => Ok(Kind::ClipboardText),
            0x02 => Ok(Kind::Ping),
            other => Err(Error::UnknownKind(other)),
        }
    }

    /// Largest plaintext allowed for this kind.
    fn max_plaintext_len(self) -> usize {
        match self {
            Kind::ClipboardText => MAX_CLIPBOARD_TEXT_LEN,
            Kind::Ping => 0,
        }
    }

    fn check_plaintext(self, plaintext: &[u8]) -> Result<(), Error> {
        match self {
            Kind::ClipboardText => {
                if plaintext.len() > MAX_CLIPBOARD_TEXT_LEN {
                    return Err(Error::PlaintextTooLarge(plaintext.len()));
                }
                std::str::from_utf8(plaintext).map_err(|_| Error::InvalidUtf8)?;
            }
            Kind::Ping => {
                if !plaintext.is_empty() {
                    return Err(Error::PingNotEmpty);
                }
            }
        }
        Ok(())
    }
}

/// Envelope header. Sent in clear and authenticated as associated data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub version: u8,
    pub kind: Kind,
    pub sender: [u8; 16],
    pub recipient: [u8; 16],
    pub message_id: [u8; 16],
    pub counter: u64,
}

impl Header {
    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut out = [0u8; HEADER_LEN];
        out[0] = self.version;
        out[1] = self.kind as u8;
        out[2..18].copy_from_slice(&self.sender);
        out[18..34].copy_from_slice(&self.recipient);
        out[34..50].copy_from_slice(&self.message_id);
        out[50..58].copy_from_slice(&self.counter.to_be_bytes());
        out
    }

    /// Decodes exactly [`HEADER_LEN`] bytes. Rejects unknown versions and kinds.
    pub fn decode(bytes: &[u8]) -> Result<Header, Error> {
        let bytes: &[u8; HEADER_LEN] = bytes
            .try_into()
            .map_err(|_| Error::HeaderLength(bytes.len()))?;
        if bytes[0] != VERSION {
            return Err(Error::UnsupportedVersion(bytes[0]));
        }
        Ok(Header {
            version: bytes[0],
            kind: Kind::from_byte(bytes[1])?,
            sender: bytes[2..18].try_into().unwrap(),
            recipient: bytes[18..34].try_into().unwrap(),
            message_id: bytes[34..50].try_into().unwrap(),
            counter: u64::from_be_bytes(bytes[50..58].try_into().unwrap()),
        })
    }
}

/// Encrypts `plaintext` into an envelope with a random nonce.
pub fn seal(header: &Header, plaintext: &[u8], tx: &[u8; 32]) -> Result<Vec<u8>, Error> {
    let mut nonce = [0u8; NONCE_LEN];
    sodium::random_bytes(&mut nonce);
    seal_with_nonce(header, plaintext, &nonce, tx)
}

/// Encrypts `plaintext` into an envelope with the given nonce.
/// Only use a fixed nonce for test vectors; production code calls [`seal`].
pub fn seal_with_nonce(
    header: &Header,
    plaintext: &[u8],
    nonce: &[u8; NONCE_LEN],
    tx: &[u8; 32],
) -> Result<Vec<u8>, Error> {
    if header.version != VERSION {
        return Err(Error::UnsupportedVersion(header.version));
    }
    header.kind.check_plaintext(plaintext)?;
    let header_bytes = header.encode();
    let ciphertext = sodium::aead_encrypt(plaintext, &header_bytes, nonce, tx);
    let mut frame = Vec::with_capacity(HEADER_LEN + NONCE_LEN + ciphertext.len());
    frame.extend_from_slice(&header_bytes);
    frame.extend_from_slice(nonce);
    frame.extend_from_slice(&ciphertext);
    Ok(frame)
}

/// Decrypts an envelope with the receiver's `rx` key.
pub fn open(frame: &[u8], rx: &[u8; 32]) -> Result<(Header, Vec<u8>), Error> {
    if frame.len() < HEADER_LEN + NONCE_LEN + TAG_LEN {
        return Err(Error::FrameTooShort(frame.len()));
    }
    let (header_bytes, rest) = frame.split_at(HEADER_LEN);
    let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
    let header = Header::decode(header_bytes)?;
    let plaintext_len = ciphertext.len() - TAG_LEN;
    if plaintext_len > header.kind.max_plaintext_len() {
        return Err(match header.kind {
            Kind::Ping => Error::PingNotEmpty,
            Kind::ClipboardText => Error::PlaintextTooLarge(plaintext_len),
        });
    }
    let nonce: &[u8; NONCE_LEN] = nonce.try_into().unwrap();
    let plaintext =
        sodium::aead_decrypt(ciphertext, header_bytes, nonce, rx).ok_or(Error::DecryptionFailed)?;
    header.kind.check_plaintext(&plaintext)?;
    Ok((header, plaintext))
}

/// Relay challenge (`0xA0` frame).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    relay_name: String,
    nonce: [u8; AUTH_NONCE_LEN],
}

impl Challenge {
    /// Builds a challenge. The relay name must fit in 255 bytes of UTF-8.
    pub fn new(relay_name: impl Into<String>, nonce: [u8; 32]) -> Result<Challenge, Error> {
        let relay_name = relay_name.into();
        if relay_name.len() > u8::MAX as usize {
            return Err(Error::RelayNameTooLong);
        }
        Ok(Challenge { relay_name, nonce })
    }

    pub fn relay_name(&self) -> &str {
        &self.relay_name
    }

    pub fn nonce(&self) -> [u8; 32] {
        self.nonce
    }

    /// `relay_name_len || relay_name || nonce`, shared by the frame and the
    /// signed message.
    fn body(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(1 + self.relay_name.len() + AUTH_NONCE_LEN);
        out.push(self.relay_name.len() as u8);
        out.extend_from_slice(self.relay_name.as_bytes());
        out.extend_from_slice(&self.nonce);
        out
    }

    /// `0xA0 || relay_name_len || relay_name || nonce`.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![AUTH_CHALLENGE];
        out.extend_from_slice(&self.body());
        out
    }

    pub fn decode(frame: &[u8]) -> Result<Challenge, Error> {
        let (&tag, rest) = frame.split_first().ok_or(Error::MalformedAuthFrame)?;
        if tag != AUTH_CHALLENGE {
            return Err(Error::UnexpectedFrameType(tag));
        }
        let (&name_len, rest) = rest.split_first().ok_or(Error::MalformedAuthFrame)?;
        let name_len = name_len as usize;
        if rest.len() != name_len + AUTH_NONCE_LEN {
            return Err(Error::MalformedAuthFrame);
        }
        let (name, nonce) = rest.split_at(name_len);
        let relay_name = std::str::from_utf8(name).map_err(|_| Error::InvalidUtf8)?;
        Ok(Challenge {
            relay_name: relay_name.to_owned(),
            nonce: nonce.try_into().unwrap(),
        })
    }

    /// `"weft-auth-v1" || relay_name_len || relay_name || nonce`.
    fn signed_message(&self) -> Vec<u8> {
        let mut out = AUTH_CONTEXT.to_vec();
        out.extend_from_slice(&self.body());
        out
    }
}

/// Device answer to a challenge (`0xA1` frame).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthResponse {
    pub sign_pk: [u8; 32],
    pub signature: [u8; 64],
}

impl AuthResponse {
    /// `0xA1 || sign_pk || signature`.
    pub fn encode(&self) -> [u8; AUTH_RESPONSE_LEN] {
        let mut out = [0u8; AUTH_RESPONSE_LEN];
        out[0] = AUTH_RESPONSE;
        out[1..33].copy_from_slice(&self.sign_pk);
        out[33..].copy_from_slice(&self.signature);
        out
    }

    pub fn decode(frame: &[u8]) -> Result<AuthResponse, Error> {
        let &tag = frame.first().ok_or(Error::MalformedAuthFrame)?;
        if tag != AUTH_RESPONSE {
            return Err(Error::UnexpectedFrameType(tag));
        }
        if frame.len() != AUTH_RESPONSE_LEN {
            return Err(Error::MalformedAuthFrame);
        }
        Ok(AuthResponse {
            sign_pk: frame[1..33].try_into().unwrap(),
            signature: frame[33..].try_into().unwrap(),
        })
    }
}

/// Signs a relay challenge with the device's signing key.
pub fn sign_auth(identity: &Identity, challenge: &Challenge) -> AuthResponse {
    AuthResponse {
        sign_pk: identity.sign_pk,
        signature: sodium::sign_detached(&challenge.signed_message(), &identity.sign_sk),
    }
}

/// Relay side: checks a response against the challenge the relay sent (with
/// the name it actually serves) and returns the authenticated device id.
pub fn verify_auth(challenge: &Challenge, response: &AuthResponse) -> Result<[u8; 16], Error> {
    if !sodium::verify_detached(
        &response.signature,
        &challenge.signed_message(),
        &response.sign_pk,
    ) {
        return Err(Error::InvalidSignature);
    }
    Ok(device_id_from_sign_pk(&response.sign_pk))
}
