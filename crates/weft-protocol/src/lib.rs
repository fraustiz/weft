//! Weft wire protocol, version 1.
//!
//! Implements `docs/protocol.md`: device identities, session keys between two
//! devices, the encrypted message envelope, the relay authentication
//! frames, groups and pairing (pairing code, control frames, group change
//! plaintexts), device names, and the packet and file chunk envelopes of
//! `docs/features.md`. All cryptography goes through libsodium so the output
//! is byte-identical with the Kotlin (lazysodium) and Swift (swift-sodium)
//! clients.

mod pairing;
mod sodium;

pub use pairing::{
    CONTROL_ERROR, CONTROL_GROUP_ADD, CONTROL_GROUP_JOIN, CONTROL_GROUP_REMOVE,
    CONTROL_PAIR_INVITE, CONTROL_PAIR_OPEN, CONTROL_PAIR_REQUEST, ControlError, ControlFrame,
    MAX_RELAY_URL_LEN, PAIRING_CODE_PREFIX, PairRequest, PairingCode, invite_request_mac,
    pair_request_mac, random_pairing_id, random_pairing_secret, verify_invite_request,
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
/// Largest packet plaintext (a JSON object), in bytes (256 KiB).
pub const MAX_PACKET_LEN: usize = 256 * 1024;
/// Size of a file transfer id.
pub const TRANSFER_ID_LEN: usize = 16;
/// Largest data in one file chunk, in bytes (128 KiB).
pub const MAX_FILE_CHUNK_DATA_LEN: usize = 128 * 1024;
/// Largest encoded envelope frame on the wire (`58 + 24 + 256 KiB + 16` bytes).
pub const MAX_ENVELOPE_FRAME_LEN: usize = HEADER_LEN + NONCE_LEN + MAX_CLIPBOARD_TEXT_LEN + TAG_LEN;
/// `transfer_id (16) || offset (8)`, before the data of a file chunk.
const FILE_CHUNK_PREFIX_LEN: usize = TRANSFER_ID_LEN + 8;

/// Domain-separation prefix for ephemeral forward-secrecy epoch keys.
pub const FS_CONTEXT: &[u8] = b"weft-fs-v1";

/// First byte of the relay challenge frame.
pub const AUTH_CHALLENGE: u8 = 0xA0;
/// First byte of the device response frame.
pub const AUTH_RESPONSE: u8 = 0xA1;
/// The one-byte frame the relay sends when authentication succeeds.
pub const AUTH_ACCEPTED: u8 = 0xA2;

/// First byte of a direct LAN challenge frame (`0xC0 || nonce_32`).
pub const LAN_CHALLENGE: u8 = 0xC0;
/// First byte of a direct LAN authentication frame (`0xC1 || device_id_16 || sig_64`).
pub const LAN_AUTH: u8 = 0xC1;
/// Domain-separation prefix for direct LAN mutual authentication with frame receipts.
pub const LAN_CONTEXT: &[u8] = b"weft-lan-v2";
/// Length of an encoded [`LanChallenge`] frame.
pub const LAN_CHALLENGE_LEN: usize = 1 + AUTH_NONCE_LEN;
/// Length of an encoded [`LanAuth`] frame.
pub const LAN_AUTH_LEN: usize = 1 + DEVICE_ID_LEN + sodium::SIGNATURE_BYTES;

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
    #[error("rekey epoch 0 is invalid")]
    ZeroEpoch,
    #[error("malformed LAN authentication frame")]
    MalformedLanFrame,
    #[error("device id does not match signing public key")]
    DeviceIdMismatch,
}

/// Derives a device id from a signing public key:
/// BLAKE2b (`crypto_generichash`), 16-byte output, no key.
pub fn device_id_from_sign_pk(sign_pk: &[u8; 32]) -> [u8; 16] {
    sodium::generichash_16(sign_pk)
}

/// `crypto_generichash` (BLAKE2b) of `data` without a key, 16-byte output:
/// a short name for content, such as a cover image. Not a MAC.
pub fn content_hash(data: &[u8]) -> [u8; 16] {
    sodium::generichash_16(data)
}

/// Standard base64 (RFC 4648 section 4, with padding), as packets carry
/// binary data.
pub fn base64_encode(data: &[u8]) -> String {
    sodium::base64_encode(data)
}

/// The inverse of [`base64_encode`]. `None` unless `text` is standard
/// base64 with its padding, without whitespace.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    sodium::base64_decode(text)
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
#[derive(Clone, PartialEq, Eq)]
pub struct SessionKeys {
    pub rx: [u8; 32],
    pub tx: [u8; 32],
}

impl Drop for SessionKeys {
    fn drop(&mut self) {
        self.zero();
    }
}

impl std::fmt::Debug for SessionKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionKeys { .. }")
    }
}

impl SessionKeys {
    /// Zeroes both directional keys in memory via `sodium_memzero`.
    pub fn zero(&mut self) {
        sodium::memzero(&mut self.rx);
        sodium::memzero(&mut self.tx);
    }
}

/// Zeroes a 32-byte secret key buffer via `sodium_memzero`.
pub fn zero_key(key: &mut [u8; 32]) {
    sodium::memzero(key);
}

/// An ephemeral X25519 key pair for a forward-secrecy session rekey.
///
/// The secret key is erased via `sodium_memzero` when dropped.
pub struct RekeyKeyPair {
    pk: [u8; sodium::KX_PK_BYTES],
    sk: [u8; sodium::KX_SK_BYTES],
}

impl RekeyKeyPair {
    /// Generates a fresh random ephemeral X25519 key pair (`crypto_kx_keypair`).
    pub fn generate() -> RekeyKeyPair {
        let (pk, sk) = sodium::kx_keypair();
        RekeyKeyPair { pk, sk }
    }

    /// Deterministically derives an ephemeral X25519 key pair from `seed`
    /// (`crypto_kx_seed_keypair`). Intended for tests and reproducible vectors.
    pub fn from_seed(mut seed: [u8; 32]) -> RekeyKeyPair {
        let (pk, sk) = sodium::kx_seed_keypair(&seed);
        sodium::memzero(&mut seed);
        RekeyKeyPair { pk, sk }
    }

    pub fn public_key(&self) -> &[u8; 32] {
        &self.pk
    }
}

impl Drop for RekeyKeyPair {
    fn drop(&mut self) {
        sodium::memzero(&mut self.sk);
    }
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

/// Derives ephemeral forward-secrecy session keys for `epoch`, mixing an
/// ephemeral X25519 exchange with the static directional [`SessionKeys`].
///
/// Requires `epoch > 0` and `own_id != peer_id`. As in [`session_keys`], the
/// device with the lower device id plays the `crypto_kx` client role. Each
/// directional key is computed with keyed 32-byte BLAKE2b (`key = static_rx`
/// or `static_tx`) over `b"weft-fs-v1" || epoch.to_be_bytes() || ephem_key`,
/// and the intermediate ephemeral secrets are zeroed before returning.
pub fn derive_epoch_keys(
    static_keys: &SessionKeys,
    epoch: u64,
    own_id: &[u8; 16],
    peer_id: &[u8; 16],
    own_ephem: &RekeyKeyPair,
    peer_ephem_pk: &[u8; 32],
) -> Result<SessionKeys, Error> {
    if epoch == 0 {
        return Err(Error::ZeroEpoch);
    }
    let role = match own_id.cmp(peer_id) {
        std::cmp::Ordering::Less => KxRole::Client,
        std::cmp::Ordering::Greater => KxRole::Server,
        std::cmp::Ordering::Equal => return Err(Error::SameDeviceId),
    };
    let (mut ephem_rx, mut ephem_tx) =
        sodium::kx_session_keys(role, &own_ephem.pk, &own_ephem.sk, peer_ephem_pk)
            .ok_or(Error::KeyExchangeFailed)?;

    let mut mix_buf = [0u8; FS_CONTEXT.len() + 8 + 32];
    mix_buf[..FS_CONTEXT.len()].copy_from_slice(FS_CONTEXT);
    mix_buf[FS_CONTEXT.len()..FS_CONTEXT.len() + 8].copy_from_slice(&epoch.to_be_bytes());

    mix_buf[FS_CONTEXT.len() + 8..].copy_from_slice(&ephem_rx);
    let rx = sodium::generichash_32_keyed(&mix_buf, &static_keys.rx);

    mix_buf[FS_CONTEXT.len() + 8..].copy_from_slice(&ephem_tx);
    let tx = sodium::generichash_32_keyed(&mix_buf, &static_keys.tx);

    sodium::memzero(&mut mix_buf);
    sodium::memzero(&mut ephem_rx);
    sodium::memzero(&mut ephem_tx);
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
    /// A UTF-8 JSON object, 1 to [`MAX_PACKET_LEN`] bytes. Only the length
    /// and the UTF-8 are checked here; the JSON is the client's business.
    Packet = 0x07,
    /// A piece of a file transfer; see [`encode_file_chunk`].
    FileChunk = 0x08,
}

impl Kind {
    /// Whether envelopes of this kind are eligible for relay offline queueing
    /// and therefore ALWAYS sealed and opened with the static [`SessionKeys`].
    pub fn is_static(self) -> bool {
        matches!(
            self,
            Kind::Welcome | Kind::DeviceAdded | Kind::DeviceRemoved | Kind::DeviceName
        )
    }

    fn from_byte(byte: u8) -> Result<Kind, Error> {
        match byte {
            0x01 => Ok(Kind::ClipboardText),
            0x02 => Ok(Kind::Ping),
            0x03 => Ok(Kind::Welcome),
            0x04 => Ok(Kind::DeviceAdded),
            0x05 => Ok(Kind::DeviceRemoved),
            0x06 => Ok(Kind::DeviceName),
            0x07 => Ok(Kind::Packet),
            0x08 => Ok(Kind::FileChunk),
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
            Kind::Packet => MAX_PACKET_LEN,
            Kind::FileChunk => FILE_CHUNK_PREFIX_LEN + MAX_FILE_CHUNK_DATA_LEN,
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
            Kind::Packet => len >= 1,
            Kind::FileChunk => len > FILE_CHUNK_PREFIX_LEN,
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
            Kind::ClipboardText | Kind::Packet => {
                std::str::from_utf8(plaintext).map_err(|_| Error::InvalidUtf8)?;
            }
            Kind::Welcome => {
                decode_welcome(plaintext)?;
            }
            Kind::DeviceName => {
                decode_device_name(plaintext)?;
            }
            // Fully checked by their length.
            Kind::Ping | Kind::DeviceAdded | Kind::DeviceRemoved | Kind::FileChunk => {}
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

/// A decoded file chunk plaintext (kind `0x08`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChunk {
    pub transfer_id: [u8; TRANSFER_ID_LEN],
    /// Position of `data` in the file, in bytes.
    pub offset: u64,
    /// 1 to [`MAX_FILE_CHUNK_DATA_LEN`] bytes.
    pub data: Vec<u8>,
}

/// File chunk plaintext (kind `0x08`):
/// `transfer_id (16) || offset (u64, big-endian) || data`, with 1 to
/// [`MAX_FILE_CHUNK_DATA_LEN`] bytes of data.
pub fn encode_file_chunk(
    transfer_id: &[u8; TRANSFER_ID_LEN],
    offset: u64,
    data: &[u8],
) -> Result<Vec<u8>, Error> {
    let len = FILE_CHUNK_PREFIX_LEN + data.len();
    Kind::FileChunk.check_plaintext_len(len)?;
    let mut out = Vec::with_capacity(len);
    out.extend_from_slice(transfer_id);
    out.extend_from_slice(&offset.to_be_bytes());
    out.extend_from_slice(data);
    Ok(out)
}

/// Parses a file chunk plaintext, with the same rules as
/// [`encode_file_chunk`].
pub fn decode_file_chunk(plaintext: &[u8]) -> Result<FileChunk, Error> {
    Kind::FileChunk.check_plaintext_len(plaintext.len())?;
    let (prefix, data) = plaintext.split_at(FILE_CHUNK_PREFIX_LEN);
    Ok(FileChunk {
        transfer_id: prefix[..TRANSFER_ID_LEN].try_into().unwrap(),
        offset: u64::from_be_bytes(prefix[TRANSFER_ID_LEN..].try_into().unwrap()),
        data: data.to_vec(),
    })
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

/// The header fields the relay routes an envelope on. Unlike
/// [`Header::decode`], [`RouteHeader::decode`] does not check `kind`, so
/// kinds added later need no relay update (`docs/features.md`, section 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteHeader {
    pub kind: u8,
    pub sender: [u8; 16],
    pub recipient: [u8; 16],
}

impl RouteHeader {
    /// Reads the header of a whole envelope frame. Checks the version and
    /// that the frame holds at least a header, a nonce and a tag.
    pub fn decode(frame: &[u8]) -> Result<RouteHeader, Error> {
        if frame.len() < HEADER_LEN + NONCE_LEN + TAG_LEN {
            return Err(Error::FrameTooShort(frame.len()));
        }
        if frame[0] != VERSION {
            return Err(Error::UnsupportedVersion(frame[0]));
        }
        Ok(RouteHeader {
            kind: frame[1],
            sender: frame[2..18].try_into().unwrap(),
            recipient: frame[18..34].try_into().unwrap(),
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

/// Direct LAN challenge frame (`0xC0 || nonce_32`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanChallenge {
    nonce: [u8; AUTH_NONCE_LEN],
}

impl LanChallenge {
    /// Generates a fresh random 32-byte LAN challenge.
    pub fn generate() -> LanChallenge {
        let mut nonce = [0u8; AUTH_NONCE_LEN];
        sodium::random_bytes(&mut nonce);
        LanChallenge { nonce }
    }

    pub fn from_nonce(nonce: [u8; AUTH_NONCE_LEN]) -> LanChallenge {
        LanChallenge { nonce }
    }

    pub fn nonce(&self) -> &[u8; AUTH_NONCE_LEN] {
        &self.nonce
    }

    /// `0xC0 || nonce_32`.
    pub fn encode(&self) -> [u8; LAN_CHALLENGE_LEN] {
        let mut out = [0u8; LAN_CHALLENGE_LEN];
        out[0] = LAN_CHALLENGE;
        out[1..].copy_from_slice(&self.nonce);
        out
    }

    pub fn decode(frame: &[u8]) -> Result<LanChallenge, Error> {
        let &tag = frame.first().ok_or(Error::MalformedLanFrame)?;
        if tag != LAN_CHALLENGE {
            return Err(Error::UnexpectedFrameType(tag));
        }
        if frame.len() != LAN_CHALLENGE_LEN {
            return Err(Error::MalformedLanFrame);
        }
        Ok(LanChallenge {
            nonce: frame[1..].try_into().unwrap(),
        })
    }
}

/// Direct LAN mutual authentication response (`0xC1 || device_id_16 || sig_64`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanAuth {
    pub device_id: [u8; DEVICE_ID_LEN],
    pub signature: [u8; sodium::SIGNATURE_BYTES],
}

impl LanAuth {
    /// `0xC1 || device_id_16 || signature_64`.
    pub fn encode(&self) -> [u8; LAN_AUTH_LEN] {
        let mut out = [0u8; LAN_AUTH_LEN];
        out[0] = LAN_AUTH;
        out[1..1 + DEVICE_ID_LEN].copy_from_slice(&self.device_id);
        out[1 + DEVICE_ID_LEN..].copy_from_slice(&self.signature);
        out
    }

    pub fn decode(frame: &[u8]) -> Result<LanAuth, Error> {
        let &tag = frame.first().ok_or(Error::MalformedLanFrame)?;
        if tag != LAN_AUTH {
            return Err(Error::UnexpectedFrameType(tag));
        }
        if frame.len() != LAN_AUTH_LEN {
            return Err(Error::MalformedLanFrame);
        }
        Ok(LanAuth {
            device_id: frame[1..1 + DEVICE_ID_LEN].try_into().unwrap(),
            signature: frame[1 + DEVICE_ID_LEN..].try_into().unwrap(),
        })
    }
}

fn lan_signed_message(peer_nonce: &[u8; 32], own_nonce: &[u8; 32]) -> [u8; LAN_CONTEXT.len() + 64] {
    let mut out = [0u8; LAN_CONTEXT.len() + 64];
    out[..LAN_CONTEXT.len()].copy_from_slice(LAN_CONTEXT);
    out[LAN_CONTEXT.len()..LAN_CONTEXT.len() + 32].copy_from_slice(peer_nonce);
    out[LAN_CONTEXT.len() + 32..].copy_from_slice(own_nonce);
    out
}

/// Signs `b"weft-lan-v2" || peer_nonce || own_nonce` with `identity`'s Ed25519
/// signing key.
pub fn sign_lan_auth(identity: &Identity, peer_nonce: &[u8; 32], own_nonce: &[u8; 32]) -> LanAuth {
    let message = lan_signed_message(peer_nonce, own_nonce);
    LanAuth {
        device_id: identity.device_id,
        signature: sodium::sign_detached(&message, &identity.sign_sk),
    }
}

/// Verifies a peer's [`LanAuth`] against its trusted `peer_sign_pk`, the
/// `own_nonce` we sent it, and the `peer_nonce` it sent us.
pub fn verify_lan_auth(
    peer_sign_pk: &[u8; 32],
    own_nonce: &[u8; 32],
    peer_nonce: &[u8; 32],
    auth: &LanAuth,
) -> Result<(), Error> {
    if device_id_from_sign_pk(peer_sign_pk) != auth.device_id {
        return Err(Error::DeviceIdMismatch);
    }
    let message = lan_signed_message(own_nonce, peer_nonce);
    if !sodium::verify_detached(&auth.signature, &message, peer_sign_pk) {
        return Err(Error::InvalidSignature);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_keys_are_redacted_and_can_be_explicitly_erased() {
        let mut keys = SessionKeys {
            rx: [0xab; 32],
            tx: [0xcd; 32],
        };
        assert_eq!(format!("{keys:?}"), "SessionKeys { .. }");
        keys.zero();
        assert_eq!(keys.rx, [0; 32]);
        assert_eq!(keys.tx, [0; 32]);
    }

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

    #[test]
    fn epoch_keys_are_symmetric_and_bound_to_epoch_and_static_keys() {
        let a = Identity::from_seeds([10; 32], [11; 32]);
        let b = Identity::from_seeds([20; 32], [21; 32]);
        let static_a = session_keys(&a, &b.device_id(), &b.kx_pk()).unwrap();
        let static_b = session_keys(&b, &a.device_id(), &a.kx_pk()).unwrap();
        assert_eq!(static_a.tx, static_b.rx);
        assert_eq!(static_a.rx, static_b.tx);

        let ephem_a = RekeyKeyPair::generate();
        let ephem_b = RekeyKeyPair::generate();

        let epoch1_a = derive_epoch_keys(
            &static_a,
            1,
            &a.device_id(),
            &b.device_id(),
            &ephem_a,
            ephem_b.public_key(),
        )
        .unwrap();
        let epoch1_b = derive_epoch_keys(
            &static_b,
            1,
            &b.device_id(),
            &a.device_id(),
            &ephem_b,
            ephem_a.public_key(),
        )
        .unwrap();

        assert_eq!(epoch1_a.tx, epoch1_b.rx);
        assert_eq!(epoch1_a.rx, epoch1_b.tx);
        assert_ne!(epoch1_a.tx, epoch1_a.rx);
        assert_ne!(epoch1_a.tx, static_a.tx);
        assert_ne!(epoch1_a.rx, static_a.rx);

        // Envelopes sealed under epoch 1 open with the peer's epoch 1 rx,
        // and never open with the static rx.
        let header = Header {
            version: VERSION,
            kind: Kind::ClipboardText,
            sender: a.device_id(),
            recipient: b.device_id(),
            message_id: [0x33; 16],
            counter: 1,
        };
        let frame = seal(&header, b"ephemeral", &epoch1_a.tx).unwrap();
        assert_eq!(open(&frame, &static_b.rx), Err(Error::DecryptionFailed));
        let (opened_header, opened_plain) = open(&frame, &epoch1_b.rx).unwrap();
        assert_eq!(opened_header, header);
        assert_eq!(opened_plain, b"ephemeral");

        // Epoch separation: epoch 2 produces completely distinct keys.
        let epoch2_a = derive_epoch_keys(
            &static_a,
            2,
            &a.device_id(),
            &b.device_id(),
            &ephem_a,
            ephem_b.public_key(),
        )
        .unwrap();
        assert_ne!(epoch2_a.tx, epoch1_a.tx);
        assert_ne!(epoch2_a.rx, epoch1_a.rx);

        // Static-key binding: different static keys yield distinct epoch keys
        // even for the exact same ephemeral key pairs and epoch.
        let c = Identity::from_seeds([30; 32], [31; 32]);
        let other_static = session_keys(&a, &c.device_id(), &c.kx_pk()).unwrap();
        let unbound_a = derive_epoch_keys(
            &other_static,
            1,
            &a.device_id(),
            &b.device_id(),
            &ephem_a,
            ephem_b.public_key(),
        )
        .unwrap();
        assert_ne!(unbound_a.tx, epoch1_a.tx);
        assert_ne!(unbound_a.rx, epoch1_a.rx);
    }

    #[test]
    fn epoch_keys_reject_invalid_inputs() {
        let a = Identity::from_seeds([1; 32], [2; 32]);
        let b = Identity::from_seeds([3; 32], [4; 32]);
        let static_a = session_keys(&a, &b.device_id(), &b.kx_pk()).unwrap();
        let ephem_a = RekeyKeyPair::from_seed([5; 32]);
        let ephem_b = RekeyKeyPair::from_seed([6; 32]);

        assert_eq!(
            derive_epoch_keys(
                &static_a,
                0,
                &a.device_id(),
                &b.device_id(),
                &ephem_a,
                ephem_b.public_key(),
            ),
            Err(Error::ZeroEpoch)
        );
        assert_eq!(
            derive_epoch_keys(
                &static_a,
                1,
                &a.device_id(),
                &a.device_id(),
                &ephem_a,
                ephem_b.public_key(),
            ),
            Err(Error::SameDeviceId)
        );
        assert_eq!(
            derive_epoch_keys(
                &static_a,
                1,
                &a.device_id(),
                &b.device_id(),
                &ephem_a,
                &[0u8; 32],
            ),
            Err(Error::KeyExchangeFailed)
        );
    }

    #[test]
    fn lan_mutual_auth_verifies_and_rejects_forgery() {
        let a = Identity::generate();
        let b = Identity::generate();
        let chal_a = LanChallenge::generate();
        let chal_b = LanChallenge::generate();

        let encoded_chal = chal_a.encode();
        assert_eq!(LanChallenge::decode(&encoded_chal).unwrap(), chal_a);
        assert_eq!(
            LanChallenge::decode(&encoded_chal[..10]),
            Err(Error::MalformedLanFrame)
        );

        let auth_a = sign_lan_auth(&a, chal_b.nonce(), chal_a.nonce());
        let auth_b = sign_lan_auth(&b, chal_a.nonce(), chal_b.nonce());
        assert_eq!(LanAuth::decode(&auth_a.encode()).unwrap(), auth_a);

        assert_eq!(
            verify_lan_auth(&a.sign_pk(), chal_b.nonce(), chal_a.nonce(), &auth_a),
            Ok(())
        );
        assert_eq!(
            verify_lan_auth(&b.sign_pk(), chal_a.nonce(), chal_b.nonce(), &auth_b),
            Ok(())
        );

        // An older peer must fail authentication before receipt framing starts.
        let legacy_message = [b"weft-lan-v1".as_slice(), chal_b.nonce(), chal_a.nonce()].concat();
        let legacy = LanAuth {
            device_id: a.device_id(),
            signature: sodium::sign_detached(&legacy_message, &a.sign_sk),
        };
        assert_eq!(
            verify_lan_auth(&a.sign_pk(), chal_b.nonce(), chal_a.nonce(), &legacy),
            Err(Error::InvalidSignature)
        );
        // Swapped nonces (reflection attempt) must fail.
        assert_eq!(
            verify_lan_auth(&a.sign_pk(), chal_a.nonce(), chal_b.nonce(), &auth_a),
            Err(Error::InvalidSignature)
        );
        // Untrusted / wrong signing key must fail.
        let stranger = Identity::generate();
        assert_eq!(
            verify_lan_auth(&stranger.sign_pk(), chal_b.nonce(), chal_a.nonce(), &auth_a),
            Err(Error::DeviceIdMismatch)
        );
        let mut forged = auth_a.clone();
        forged.device_id = stranger.device_id();
        assert_eq!(
            verify_lan_auth(&stranger.sign_pk(), chal_b.nonce(), chal_a.nonce(), &forged),
            Err(Error::InvalidSignature)
        );
    }

    #[test]
    fn static_vs_live_envelope_kinds() {
        for kind in [
            Kind::Welcome,
            Kind::DeviceAdded,
            Kind::DeviceRemoved,
            Kind::DeviceName,
        ] {
            assert!(kind.is_static(), "{kind:?}");
        }
        for kind in [
            Kind::ClipboardText,
            Kind::Ping,
            Kind::Packet,
            Kind::FileChunk,
        ] {
            assert!(!kind.is_static(), "{kind:?}");
        }
    }
}
