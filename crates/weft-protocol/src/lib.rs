//! Weft wire protocol, version 1.
//!
//! Implements `docs/protocol.md`: device identities, session keys between two
//! devices, the encrypted message envelope, the relay authentication
//! frames, groups and pairing (pairing code, control frames, group change
//! plaintexts) and device names. All cryptography goes through libsodium so the output
//! is byte-identical with the Kotlin (lazysodium) and Swift (swift-sodium)
//! clients.

mod pairing;
mod sodium;

pub use pairing::{
    CONTROL_ERROR, CONTROL_GROUP_ADD, CONTROL_GROUP_REMOVE, CONTROL_PAIR_OPEN,
    CONTROL_PAIR_REQUEST, ControlError, ControlFrame, MAX_RELAY_URL_LEN, PAIRING_CODE_PREFIX,
    PairRequest, PairingCode, pair_request_mac, random_pairing_id, random_pairing_secret,
    verify_pair_request,
};
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
/// Largest number of devices in one group.
pub const MAX_GROUP_SIZE: usize = 10;
/// Largest number of devices a Welcome plaintext can list: the group without
/// its sender and its recipient.
pub const MAX_WELCOME_DEVICES: usize = MAX_GROUP_SIZE - 2;
/// Size of one `sign_pk || kx_pk` entry in a group change plaintext.
pub const PEER_KEYS_LEN: usize = sodium::SIGN_PK_BYTES + sodium::KX_PK_BYTES;
/// Size of a device id.
pub const DEVICE_ID_LEN: usize = 16;
/// Largest device name, in bytes of UTF-8.
pub const MAX_DEVICE_NAME_LEN: usize = 64;

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
    #[error("plaintext length {0} is not valid for its kind")]
    InvalidPlaintextLength(usize),
    #[error("welcome lists {0} devices, at most {MAX_WELCOME_DEVICES} allowed")]
    TooManyDevices(usize),
    #[error("malformed pairing code")]
    MalformedPairingCode,
    #[error("relay url must be 1 to 255 bytes, got {0}")]
    RelayUrlLength(usize),
    #[error("malformed control frame")]
    MalformedControlFrame,
    #[error("unknown control frame {0:#04x}")]
    UnknownControlFrame(u8),
    #[error("unknown control error code {0:#04x}")]
    UnknownControlError(u8),
    #[error("relay name in the challenge is not the host we connected to")]
    RelayNameMismatch,
    #[error("device name must be 1 to {MAX_DEVICE_NAME_LEN} bytes, got {0}")]
    DeviceNameLength(usize),
    #[error("device name contains a control character")]
    DeviceNameControlCharacter,
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
    /// The other members of the group a device just joined; see
    /// [`encode_welcome`].
    Welcome = 0x03,
    /// Keys of a device that joined the group; see [`encode_device_added`].
    DeviceAdded = 0x04,
    /// Id of a device that left the group; see [`encode_device_removed`].
    DeviceRemoved = 0x05,
    /// The sender's own name; see [`encode_device_name`].
    DeviceName = 0x06,
}

impl Kind {
    fn from_byte(byte: u8) -> Result<Kind, Error> {
        match byte {
            0x01 => Ok(Kind::ClipboardText),
            0x02 => Ok(Kind::Ping),
            0x03 => Ok(Kind::Welcome),
            0x04 => Ok(Kind::DeviceAdded),
            0x05 => Ok(Kind::DeviceRemoved),
            0x06 => Ok(Kind::DeviceName),
            other => Err(Error::UnknownKind(other)),
        }
    }

    /// Largest plaintext allowed for this kind.
    fn max_plaintext_len(self) -> usize {
        match self {
            Kind::ClipboardText => MAX_CLIPBOARD_TEXT_LEN,
            Kind::Ping => 0,
            Kind::Welcome => 1 + MAX_WELCOME_DEVICES * PEER_KEYS_LEN,
            Kind::DeviceAdded => PEER_KEYS_LEN,
            Kind::DeviceRemoved => DEVICE_ID_LEN,
            Kind::DeviceName => MAX_DEVICE_NAME_LEN,
        }
    }

    /// Checks everything that can be known from the plaintext length alone,
    /// so `open` can reject a frame before decrypting it.
    fn check_plaintext_len(self, len: usize) -> Result<(), Error> {
        if self == Kind::DeviceName {
            return check_device_name_len(len);
        }
        if len > self.max_plaintext_len() {
            return Err(match self {
                Kind::Ping => Error::PingNotEmpty,
                _ => Error::PlaintextTooLarge(len),
            });
        }
        let valid = match self {
            Kind::ClipboardText | Kind::Ping => true,
            Kind::Welcome => len % PEER_KEYS_LEN == 1,
            Kind::DeviceAdded => len == PEER_KEYS_LEN,
            Kind::DeviceRemoved => len == DEVICE_ID_LEN,
            Kind::DeviceName => unreachable!("checked above"),
        };
        if !valid {
            return Err(Error::InvalidPlaintextLength(len));
        }
        Ok(())
    }

    fn check_plaintext(self, plaintext: &[u8]) -> Result<(), Error> {
        self.check_plaintext_len(plaintext.len())?;
        match self {
            Kind::ClipboardText => {
                std::str::from_utf8(plaintext).map_err(|_| Error::InvalidUtf8)?;
            }
            Kind::Welcome => {
                decode_welcome(plaintext)?;
            }
            Kind::DeviceName => {
                decode_device_name(plaintext)?;
            }
            // Fully checked by their length.
            Kind::Ping | Kind::DeviceAdded | Kind::DeviceRemoved => {}
        }
        Ok(())
    }
}

/// Public keys of one trusted device, as carried by the group change
/// envelopes: `sign_pk (32) || kx_pk (32)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerKeys {
    pub sign_pk: [u8; 32],
    pub kx_pk: [u8; 32],
}

impl PeerKeys {
    /// The device id derived from `sign_pk`.
    pub fn device_id(&self) -> [u8; 16] {
        device_id_from_sign_pk(&self.sign_pk)
    }

    fn encode(&self) -> [u8; PEER_KEYS_LEN] {
        let mut out = [0u8; PEER_KEYS_LEN];
        out[..32].copy_from_slice(&self.sign_pk);
        out[32..].copy_from_slice(&self.kx_pk);
        out
    }

    fn decode(bytes: &[u8; PEER_KEYS_LEN]) -> PeerKeys {
        PeerKeys {
            sign_pk: bytes[..32].try_into().unwrap(),
            kx_pk: bytes[32..].try_into().unwrap(),
        }
    }
}

/// Welcome plaintext (kind `0x03`): `count (1) || count × (sign_pk || kx_pk)`.
/// Lists the other members of the group: neither the sender nor the
/// recipient, so at most [`MAX_WELCOME_DEVICES`] entries.
pub fn encode_welcome(members: &[PeerKeys]) -> Result<Vec<u8>, Error> {
    if members.len() > MAX_WELCOME_DEVICES {
        return Err(Error::TooManyDevices(members.len()));
    }
    let mut out = Vec::with_capacity(1 + members.len() * PEER_KEYS_LEN);
    out.push(members.len() as u8);
    for member in members {
        out.extend_from_slice(&member.encode());
    }
    Ok(out)
}

/// Parses a Welcome plaintext. The length must be exactly
/// `1 + count × 64` and `count` at most [`MAX_WELCOME_DEVICES`].
pub fn decode_welcome(plaintext: &[u8]) -> Result<Vec<PeerKeys>, Error> {
    let (&count, entries) = plaintext
        .split_first()
        .ok_or(Error::InvalidPlaintextLength(0))?;
    let count = count as usize;
    if count > MAX_WELCOME_DEVICES {
        return Err(Error::TooManyDevices(count));
    }
    if entries.len() != count * PEER_KEYS_LEN {
        return Err(Error::InvalidPlaintextLength(plaintext.len()));
    }
    let (entries, _) = entries.as_chunks::<PEER_KEYS_LEN>();
    Ok(entries.iter().map(PeerKeys::decode).collect())
}

/// Device added plaintext (kind `0x04`): `sign_pk || kx_pk` of the new device.
pub fn encode_device_added(device: &PeerKeys) -> [u8; PEER_KEYS_LEN] {
    device.encode()
}

/// Parses a Device added plaintext: exactly 64 bytes.
pub fn decode_device_added(plaintext: &[u8]) -> Result<PeerKeys, Error> {
    let bytes: &[u8; PEER_KEYS_LEN] = plaintext
        .try_into()
        .map_err(|_| Error::InvalidPlaintextLength(plaintext.len()))?;
    Ok(PeerKeys::decode(bytes))
}

/// Device removed plaintext (kind `0x05`): the id of the removed device.
pub fn encode_device_removed(device_id: &[u8; 16]) -> [u8; DEVICE_ID_LEN] {
    *device_id
}

/// Parses a Device removed plaintext: exactly 16 bytes.
pub fn decode_device_removed(plaintext: &[u8]) -> Result<[u8; 16], Error> {
    plaintext
        .try_into()
        .map_err(|_| Error::InvalidPlaintextLength(plaintext.len()))
}

/// Device name plaintext (kind `0x06`): the sender's own name in UTF-8,
/// 1 to [`MAX_DEVICE_NAME_LEN`] bytes, without control characters
/// (U+0000–U+001F, U+007F–U+009F).
pub fn encode_device_name(name: &str) -> Result<Vec<u8>, Error> {
    check_device_name(name)?;
    Ok(name.as_bytes().to_vec())
}

/// Parses a Device name plaintext, with the same rules as
/// [`encode_device_name`].
pub fn decode_device_name(plaintext: &[u8]) -> Result<String, Error> {
    check_device_name_len(plaintext.len())?;
    let name = std::str::from_utf8(plaintext).map_err(|_| Error::InvalidUtf8)?;
    check_device_name(name)?;
    Ok(name.to_owned())
}

fn check_device_name_len(len: usize) -> Result<(), Error> {
    if len == 0 || len > MAX_DEVICE_NAME_LEN {
        return Err(Error::DeviceNameLength(len));
    }
    Ok(())
}

fn check_device_name(name: &str) -> Result<(), Error> {
    check_device_name_len(name.len())?;
    let is_control = |c: char| matches!(c, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}');
    if name.chars().any(is_control) {
        return Err(Error::DeviceNameControlCharacter);
    }
    Ok(())
}

/// A random `message_id` for an envelope header.
pub fn random_message_id() -> [u8; 16] {
    let mut id = [0u8; 16];
    sodium::random_bytes(&mut id);
    id
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
    header
        .kind
        .check_plaintext_len(ciphertext.len() - TAG_LEN)?;
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

/// Client side: checks that the relay name in the challenge is exactly the
/// host the client connected to, before signing anything. `expected_host` is
/// the host without its port; stripping the port is the caller's job.
pub fn check_relay_name(challenge: &Challenge, expected_host: &str) -> Result<(), Error> {
    if challenge.relay_name != expected_host {
        return Err(Error::RelayNameMismatch);
    }
    Ok(())
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

/// What a frame received after authentication is, from its first byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// `0x01`: a message envelope, see [`open`] and [`Header::decode`].
    Envelope,
    /// `0xB0`–`0xBF`: a control frame, see [`ControlFrame::decode`].
    Control,
    /// Anything else, including an empty frame. To be dropped.
    Unknown,
}

/// Tells an envelope from a control frame by the first byte. This only
/// dispatches: the frame still has to be decoded and checked.
pub fn classify(frame: &[u8]) -> FrameType {
    match frame.first() {
        Some(&VERSION) => FrameType::Envelope,
        Some(0xB0..=0xBF) => FrameType::Control,
        _ => FrameType::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Device name envelope whose plaintext is encrypted as is, bypassing
    /// the checks in `seal`, so `open` has to catch it after decrypting.
    fn raw_name_frame(plaintext: &[u8]) -> (Vec<u8>, [u8; 32]) {
        let a = Identity::from_seeds([1; 32], [2; 32]);
        let b = Identity::from_seeds([3; 32], [4; 32]);
        let a_keys = session_keys(&a, &b.device_id(), &b.kx_pk()).unwrap();
        let b_keys = session_keys(&b, &a.device_id(), &a.kx_pk()).unwrap();
        let header = Header {
            version: VERSION,
            kind: Kind::DeviceName,
            sender: a.device_id(),
            recipient: b.device_id(),
            message_id: [7; 16],
            counter: 1,
        }
        .encode();
        let nonce = [9; NONCE_LEN];
        let mut frame = header.to_vec();
        frame.extend_from_slice(&nonce);
        frame.extend_from_slice(&sodium::aead_encrypt(
            plaintext, &header, &nonce, &a_keys.tx,
        ));
        (frame, b_keys.rx)
    }

    #[test]
    fn open_checks_the_decrypted_device_name() {
        let (frame, rx) = raw_name_frame("Téléphone".as_bytes());
        assert_eq!(open(&frame, &rx).unwrap().1, "Téléphone".as_bytes());

        for (plaintext, expected) in [
            (&b"tab\there"[..], Error::DeviceNameControlCharacter),
            ("c1 \u{85}".as_bytes(), Error::DeviceNameControlCharacter),
            (&b"\xff\xfe"[..], Error::InvalidUtf8),
            (&b"cut \xc3"[..], Error::InvalidUtf8),
        ] {
            let (frame, rx) = raw_name_frame(plaintext);
            assert_eq!(open(&frame, &rx), Err(expected), "{plaintext:?}");
        }
    }
}
