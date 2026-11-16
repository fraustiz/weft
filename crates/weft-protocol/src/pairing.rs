//! Groups and pairing (`docs/protocol.md`, section 5): the pairing code shown
//! as a QR code, the pair request MAC and the control frames exchanged with
//! the relay.

use crate::{Error, sodium};

/// Prefix of the pairing code string.
pub const PAIRING_CODE_PREFIX: &str = "WEFT1:";
/// Largest relay url in a pairing code, in bytes.
pub const MAX_RELAY_URL_LEN: usize = u8::MAX as usize;

/// First byte of the frame that opens a pairing slot.
pub const CONTROL_PAIR_OPEN: u8 = 0xB0;
/// First byte of the frame a device sends to join.
pub const CONTROL_PAIR_REQUEST: u8 = 0xB1;
/// First byte of the frame that confirms a device into the group.
pub const CONTROL_GROUP_ADD: u8 = 0xB2;
/// First byte of the frame that removes a device from the group.
pub const CONTROL_GROUP_REMOVE: u8 = 0xB3;
/// First byte of the relay's error frame.
pub const CONTROL_ERROR: u8 = 0xBE;

const PAIR_CONTEXT: &[u8] = b"weft-pair-v1";
/// `pairing_id (16) || sign_pk (32) || kx_pk (32) || secret (32)`.
const PAIRING_CODE_FIXED_LEN: usize = 16 + 32 + 32 + 32;
const PAIR_REQUEST_LEN: usize = 1 + 16 + 32 + 32 + 32;

/// A random pairing id for a new pairing code.
pub fn random_pairing_id() -> [u8; 16] {
    let mut id = [0u8; 16];
    sodium::random_bytes(&mut id);
    id
}

/// A random secret for a new pairing code.
pub fn random_pairing_secret() -> [u8; 32] {
    let mut secret = [0u8; 32];
    sodium::random_bytes(&mut secret);
    secret
}

/// The content of the QR code a device shows to let another one join.
///
/// The secret only ever travels through the QR code: never send a
/// `PairingCode` (or its encoded form) over the network.
#[derive(Clone, PartialEq, Eq)]
pub struct PairingCode {
    /// Relay to connect to, e.g. `wss://weft.frohub.eu/v1`. 1 to 255 bytes.
    pub relay_url: String,
    pub pairing_id: [u8; 16],
    /// Signing public key of the device showing the code.
    pub sign_pk: [u8; 32],
    /// Key exchange public key of the device showing the code.
    pub kx_pk: [u8; 32],
    pub secret: [u8; 32],
}

/// Hides the secret so it does not end up in logs.
impl std::fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingCode")
            .field("relay_url", &self.relay_url)
            .field("pairing_id", &self.pairing_id)
            .field("sign_pk", &self.sign_pk)
            .field("kx_pk", &self.kx_pk)
            .field("secret", &"<redacted>")
            .finish()
    }
}

impl PairingCode {
    /// `"WEFT1:"` followed by the base64url (no padding) encoding of
    /// `url_len (1) || relay_url || pairing_id || sign_pk || kx_pk || secret`.
    ///
    /// Fails if the relay url is empty or longer than 255 bytes.
    pub fn encode(&self) -> Result<String, Error> {
        let url = self.relay_url.as_bytes();
        if url.is_empty() || url.len() > MAX_RELAY_URL_LEN {
            return Err(Error::RelayUrlLength(url.len()));
        }
        let mut payload = Vec::with_capacity(1 + url.len() + PAIRING_CODE_FIXED_LEN);
        payload.push(url.len() as u8);
        payload.extend_from_slice(url);
        payload.extend_from_slice(&self.pairing_id);
        payload.extend_from_slice(&self.sign_pk);
        payload.extend_from_slice(&self.kx_pk);
        payload.extend_from_slice(&self.secret);
        let mut code = PAIRING_CODE_PREFIX.to_owned();
        code.push_str(&sodium::base64url_encode(&payload));
        sodium::memzero(&mut payload);
        Ok(code)
    }

    /// Parses a scanned code. The prefix, the alphabet (no padding, no
    /// whitespace) and the length are all exact.
    pub fn decode(code: &str) -> Result<PairingCode, Error> {
        let b64 = code
            .strip_prefix(PAIRING_CODE_PREFIX)
            .ok_or(Error::MalformedPairingCode)?;
        // Bounds the allocation below: a QR scanner hands us arbitrary text.
        let max_payload_len = 1 + MAX_RELAY_URL_LEN + PAIRING_CODE_FIXED_LEN;
        if b64.len() > max_payload_len.div_ceil(3) * 4 {
            return Err(Error::MalformedPairingCode);
        }
        let mut payload = sodium::base64url_decode(b64).ok_or(Error::MalformedPairingCode)?;
        let result = PairingCode::from_payload(&payload);
        sodium::memzero(&mut payload);
        result
    }

    fn from_payload(payload: &[u8]) -> Result<PairingCode, Error> {
        let (&url_len, rest) = payload.split_first().ok_or(Error::MalformedPairingCode)?;
        let url_len = url_len as usize;
        if url_len == 0 {
            return Err(Error::RelayUrlLength(0));
        }
        if rest.len() != url_len + PAIRING_CODE_FIXED_LEN {
            return Err(Error::MalformedPairingCode);
        }
        let (url, rest) = rest.split_at(url_len);
        let relay_url = std::str::from_utf8(url).map_err(|_| Error::InvalidUtf8)?;
        Ok(PairingCode {
            relay_url: relay_url.to_owned(),
            pairing_id: rest[..16].try_into().unwrap(),
            sign_pk: rest[16..48].try_into().unwrap(),
            kx_pk: rest[48..80].try_into().unwrap(),
            secret: rest[80..].try_into().unwrap(),
        })
    }
}

/// MAC of a pair request: keyed BLAKE2b (`crypto_generichash`, 32-byte
/// output, key = `secret`) of
/// `"weft-pair-v1" || pairing_id || sign_pk || kx_pk`, where the keys are
/// those of the device asking to join.
pub fn pair_request_mac(
    secret: &[u8; 32],
    pairing_id: &[u8; 16],
    sign_pk: &[u8; 32],
    kx_pk: &[u8; 32],
) -> [u8; 32] {
    let mut input = Vec::with_capacity(PAIR_CONTEXT.len() + 16 + 32 + 32);
    input.extend_from_slice(PAIR_CONTEXT);
    input.extend_from_slice(pairing_id);
    input.extend_from_slice(sign_pk);
    input.extend_from_slice(kx_pk);
    sodium::generichash_32_keyed(&input, secret)
}

/// Body of the `0xB1` frame: a device asking to join a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairRequest {
    pub pairing_id: [u8; 16],
    /// Signing public key of the device asking to join.
    pub sign_pk: [u8; 32],
    /// Key exchange public key of the device asking to join.
    pub kx_pk: [u8; 32],
    pub mac: [u8; 32],
}

impl PairRequest {
    /// Builds the request of the joining device, computing the MAC with the
    /// secret read from the pairing code.
    pub fn new(
        secret: &[u8; 32],
        pairing_id: [u8; 16],
        sign_pk: [u8; 32],
        kx_pk: [u8; 32],
    ) -> PairRequest {
        PairRequest {
            pairing_id,
            sign_pk,
            kx_pk,
            mac: pair_request_mac(secret, &pairing_id, &sign_pk, &kx_pk),
        }
    }
}

/// Checks the MAC of a received pair request in constant time.
///
/// The MAC covers the `pairing_id` carried by the request, not the one the
/// caller expects: the caller must also check that `request.pairing_id` is
/// the id of the code it is showing.
pub fn verify_pair_request(secret: &[u8; 32], request: &PairRequest) -> bool {
    let expected = pair_request_mac(
        secret,
        &request.pairing_id,
        &request.sign_pk,
        &request.kx_pk,
    );
    sodium::memcmp_32(&expected, &request.mac)
}

/// Error code carried by the relay's `0xBE` frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlError {
    /// Unknown or expired pairing.
    UnknownPairing = 0x01,
    NotAllowed = 0x02,
    RateLimited = 0x03,
    GroupFull = 0x04,
}

impl ControlError {
    fn from_byte(byte: u8) -> Result<ControlError, Error> {
        match byte {
            0x01 => Ok(ControlError::UnknownPairing),
            0x02 => Ok(ControlError::NotAllowed),
            0x03 => Ok(ControlError::RateLimited),
            0x04 => Ok(ControlError::GroupFull),
            other => Err(Error::UnknownControlError(other)),
        }
    }
}

/// A control frame (`0xB0`–`0xBF`), exchanged with the relay in clear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlFrame {
    /// `0xB0`, device → relay: opens a pairing slot.
    PairOpen { pairing_id: [u8; 16] },
    /// `0xB1`, joining device → relay → slot owner: asks to join.
    PairRequest(PairRequest),
    /// `0xB2`, slot owner → relay: puts that device in the sender's group.
    GroupAdd { device_id: [u8; 16] },
    /// `0xB3`, device → relay: removes that device from the group.
    GroupRemove { device_id: [u8; 16] },
    /// `0xBE`, relay → device.
    Error(ControlError),
}

impl ControlFrame {
    /// The frame's first byte followed by its body.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            ControlFrame::PairOpen { pairing_id } => tagged(CONTROL_PAIR_OPEN, pairing_id),
            ControlFrame::PairRequest(request) => {
                let mut out = Vec::with_capacity(PAIR_REQUEST_LEN);
                out.push(CONTROL_PAIR_REQUEST);
                out.extend_from_slice(&request.pairing_id);
                out.extend_from_slice(&request.sign_pk);
                out.extend_from_slice(&request.kx_pk);
                out.extend_from_slice(&request.mac);
                out
            }
            ControlFrame::GroupAdd { device_id } => tagged(CONTROL_GROUP_ADD, device_id),
            ControlFrame::GroupRemove { device_id } => tagged(CONTROL_GROUP_REMOVE, device_id),
            ControlFrame::Error(code) => vec![CONTROL_ERROR, *code as u8],
        }
    }

    /// Decodes a whole frame. Every frame has an exact length; a frame with
    /// missing or extra bytes is rejected.
    pub fn decode(frame: &[u8]) -> Result<ControlFrame, Error> {
        let (&tag, body) = frame.split_first().ok_or(Error::MalformedControlFrame)?;
        match tag {
            CONTROL_PAIR_OPEN => Ok(ControlFrame::PairOpen {
                pairing_id: exact(body)?,
            }),
            CONTROL_PAIR_REQUEST => {
                let body: [u8; PAIR_REQUEST_LEN - 1] = exact(body)?;
                Ok(ControlFrame::PairRequest(PairRequest {
                    pairing_id: body[..16].try_into().unwrap(),
                    sign_pk: body[16..48].try_into().unwrap(),
                    kx_pk: body[48..80].try_into().unwrap(),
                    mac: body[80..].try_into().unwrap(),
                }))
            }
            CONTROL_GROUP_ADD => Ok(ControlFrame::GroupAdd {
                device_id: exact(body)?,
            }),
            CONTROL_GROUP_REMOVE => Ok(ControlFrame::GroupRemove {
                device_id: exact(body)?,
            }),
            CONTROL_ERROR => {
                let [code] = exact(body)?;
                Ok(ControlFrame::Error(ControlError::from_byte(code)?))
            }
            // The rest of the control range is reserved.
            0xB4..=0xBD | 0xBF => Err(Error::UnknownControlFrame(tag)),
            other => Err(Error::UnexpectedFrameType(other)),
        }
    }
}

fn tagged(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + body.len());
    out.push(tag);
    out.extend_from_slice(body);
    out
}

/// The body as a fixed-size array, or an error if its length is not exact.
fn exact<const N: usize>(body: &[u8]) -> Result<[u8; N], Error> {
    body.try_into().map_err(|_| Error::MalformedControlFrame)
}
