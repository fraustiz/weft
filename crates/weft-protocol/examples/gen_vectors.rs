//! Generates `vectors/v1.json`, the test vectors every Weft client must
//! reproduce byte for byte (see `docs/protocol.md`, section 5).
//!
//! Run: `cargo run -p weft-protocol --example gen_vectors`
//! The file is written in place and also printed on stdout.
//!
//! All byte strings are lowercase hex, integers are JSON numbers.
//!
//! `devices.a` / `devices.b`:
//!   sign_seed   32-byte seed for `crypto_sign_seed_keypair`
//!   kx_seed     32-byte seed for `crypto_kx_seed_keypair`
//!   sign_pk     Ed25519 public key
//!   kx_pk       X25519 public key
//!   device_id   `crypto_generichash(outlen = 16, sign_pk, no key)`
//!
//! `session`:
//!   client      "a" or "b": the device with the lower device id, which
//!               plays the `crypto_kx` client role
//!   a.rx, a.tx  session keys computed by device A
//!   b.rx, b.tx  session keys computed by device B (b.rx == a.tx, b.tx == a.rx)
//!
//! `envelopes[]` (one ClipboardText from A to B, one Ping from B to A):
//!   name            human-readable label
//!   sender          "a" or "b"
//!   recipient       "a" or "b"
//!   version         header version byte
//!   kind            header kind byte (1 = ClipboardText, 2 = Ping)
//!   message_id      16-byte message id
//!   counter         u64 counter
//!   nonce           24-byte XChaCha20-Poly1305 nonce
//!   key             encryption key used (the sender's tx)
//!   plaintext_utf8  plaintext as a UTF-8 string
//!   plaintext       plaintext bytes
//!   header          the 58-byte encoded header (also the AEAD associated data)
//!   frame           full envelope: header || nonce || ciphertext
//!
//! `auth` (device A answering a relay challenge):
//!   relay_name       relay name in the challenge
//!   nonce            32-byte challenge nonce
//!   challenge_frame  encoded 0xA0 frame
//!   device           "a"
//!   signed_message   "weft-auth-v1" || relay_name_len || relay_name || nonce
//!   signature        Ed25519 detached signature of signed_message
//!   response_frame   encoded 0xA1 frame
//!   accepted_frame   the 0xA2 frame

use serde_json::{Value, json};
use weft_protocol::{
    AUTH_ACCEPTED, Challenge, Header, Identity, Kind, SessionKeys, VERSION, seal_with_nonce,
    session_keys, sign_auth,
};

/// `N` consecutive bytes starting at `start`, e.g. `seq::<4>(0x10)` = 10 11 12 13.
fn seq<const N: usize>(start: u8) -> [u8; N] {
    std::array::from_fn(|i| start.wrapping_add(i as u8))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn device_json(sign_seed: [u8; 32], kx_seed: [u8; 32], id: &Identity) -> Value {
    json!({
        "sign_seed": hex(&sign_seed),
        "kx_seed": hex(&kx_seed),
        "sign_pk": hex(&id.sign_pk()),
        "kx_pk": hex(&id.kx_pk()),
        "device_id": hex(&id.device_id()),
    })
}

fn keys_json(keys: &SessionKeys) -> Value {
    json!({ "rx": hex(&keys.rx), "tx": hex(&keys.tx) })
}

struct Envelope<'a> {
    name: &'a str,
    sender: (&'a str, &'a Identity, &'a SessionKeys),
    recipient: (&'a str, &'a Identity),
    kind: Kind,
    message_id: [u8; 16],
    counter: u64,
    nonce: [u8; 24],
    plaintext: &'a str,
}

fn envelope_json(e: Envelope) -> Value {
    let header = Header {
        version: VERSION,
        kind: e.kind,
        sender: e.sender.1.device_id(),
        recipient: e.recipient.1.device_id(),
        message_id: e.message_id,
        counter: e.counter,
    };
    let key = e.sender.2.tx;
    let frame = seal_with_nonce(&header, e.plaintext.as_bytes(), &e.nonce, &key)
        .expect("vector envelope must be valid");
    json!({
        "name": e.name,
        "sender": e.sender.0,
        "recipient": e.recipient.0,
        "version": header.version,
        "kind": header.kind as u8,
        "message_id": hex(&header.message_id),
        "counter": header.counter,
        "nonce": hex(&e.nonce),
        "key": hex(&key),
        "plaintext_utf8": e.plaintext,
        "plaintext": hex(e.plaintext.as_bytes()),
        "header": hex(&header.encode()),
        "frame": hex(&frame),
    })
}

/// Builds the full vectors document, ending with a newline.
pub fn vectors_json() -> String {
    let (a_sign_seed, a_kx_seed) = (seq::<32>(0x00), seq::<32>(0x20));
    let (b_sign_seed, b_kx_seed) = (seq::<32>(0x40), seq::<32>(0x60));
    let a = Identity::from_seeds(a_sign_seed, a_kx_seed);
    let b = Identity::from_seeds(b_sign_seed, b_kx_seed);

    let a_keys = session_keys(&a, &b.device_id(), &b.kx_pk()).expect("session keys for A");
    let b_keys = session_keys(&b, &a.device_id(), &a.kx_pk()).expect("session keys for B");
    let client = if a.device_id() < b.device_id() {
        "a"
    } else {
        "b"
    };

    let clipboard = envelope_json(Envelope {
        name: "clipboard_text_a_to_b",
        sender: ("a", &a, &a_keys),
        recipient: ("b", &b),
        kind: Kind::ClipboardText,
        message_id: seq::<16>(0xa0),
        counter: 1,
        nonce: seq::<24>(0x80),
        plaintext: "héllo, weft ✂",
    });
    let ping = envelope_json(Envelope {
        name: "ping_b_to_a",
        sender: ("b", &b, &b_keys),
        recipient: ("a", &a),
        kind: Kind::Ping,
        message_id: seq::<16>(0xd0),
        counter: 1,
        nonce: seq::<24>(0xb0),
        plaintext: "",
    });

    let challenge = Challenge::new("weft.frohub.eu", seq::<32>(0xe0)).expect("valid challenge");
    let challenge_frame = challenge.encode();
    let response = sign_auth(&a, &challenge);
    let mut signed_message = b"weft-auth-v1".to_vec();
    signed_message.extend_from_slice(&challenge_frame[1..]);

    let doc = json!({
        "protocol_version": VERSION,
        "devices": {
            "a": device_json(a_sign_seed, a_kx_seed, &a),
            "b": device_json(b_sign_seed, b_kx_seed, &b),
        },
        "session": {
            "client": client,
            "a": keys_json(&a_keys),
            "b": keys_json(&b_keys),
        },
        "envelopes": [clipboard, ping],
        "auth": {
            "relay_name": challenge.relay_name(),
            "nonce": hex(&challenge.nonce()),
            "challenge_frame": hex(&challenge_frame),
            "device": "a",
            "signed_message": hex(&signed_message),
            "signature": hex(&response.signature),
            "response_frame": hex(&response.encode()),
            "accepted_frame": hex(&[AUTH_ACCEPTED]),
        },
    });
    let mut out = serde_json::to_string_pretty(&doc).expect("serialize vectors");
    out.push('\n');
    out
}

fn main() {
    let json = vectors_json();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/vectors/v1.json");
    std::fs::write(path, &json).expect("write vectors/v1.json");
    print!("{json}");
}
