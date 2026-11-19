//! Generates `vectors/v1.json`, the test vectors every Weft client must
//! reproduce byte for byte (see `docs/protocol.md`, section 6).
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
//!
//! `pairing` (device A shows a pairing code, device B scans it and joins a
//! group that already holds a third device C; later A removes C):
//!   relay_url     relay url carried by the pairing code
//!   pairing_id    16-byte pairing id (random in production)
//!   secret        32-byte pairing secret (random in production)
//!   shown_by      "a": the code carries A's sign_pk and kx_pk
//!   joiner        "b": the device that scans the code and sends 0xB1
//!   code_payload  url_len || relay_url || pairing_id || sign_pk_A ||
//!                 kx_pk_A || secret, before base64
//!   code          the QR code string: "WEFT1:" followed by code_payload in
//!                 base64url without padding
//!   mac_input     "weft-pair-v1" || pairing_id || sign_pk_B || kx_pk_B
//!   mac           `crypto_generichash(outlen = 32, mac_input, key = secret)`
//!   control_frames  every control frame, encoded:
//!     pair_open               0xB0 || pairing_id                 (A → relay)
//!     pair_request            0xB1 || pairing_id || sign_pk_B || kx_pk_B || mac
//!     group_add               0xB2 || device id of B             (A → relay)
//!     group_remove            0xB3 || device id of C             (A → relay)
//!     error_unknown_pairing   0xBE 0x01
//!     error_not_allowed       0xBE 0x02
//!     error_rate_limited      0xBE 0x03
//!     error_group_full        0xBE 0x04
//!   device_c      a third device, same fields as `devices.a`; its seeds are
//!                 sign_seed = 80 81 … 9f and kx_seed = a0 a1 … bf
//!   session_a_c   session keys between A and C, same fields as `session`
//!                 (with "c" in place of "b")
//!   envelopes[]   same fields as the top-level `envelopes[]`, except that
//!                 there is no plaintext_utf8 (the plaintexts are binary):
//!     welcome_a_to_b         kind 3, plaintext = 01 || sign_pk_C || kx_pk_C
//!                            (one other member: C), key = `session.a.tx`
//!     device_added_a_to_c    kind 4, plaintext = sign_pk_B || kx_pk_B,
//!                            key = `session_a_c.a.tx`
//!     device_removed_a_to_b  kind 5, plaintext = device id of C,
//!                            key = `session.a.tx`
//!
//! `invitation` (device A shows the pairing code of the `pairing` section;
//! device B, already in a group, scans it and invites A into its group):
//!   code          the QR code string, the same as `pairing.code`
//!   pairing_id    the same as `pairing.pairing_id`
//!   secret        the same as `pairing.secret`
//!   shown_by      "a": the code carries A's sign_pk and kx_pk
//!   inviter       "b": the device that scans the code and sends 0xB5
//!   mac_input     "weft-invite-v1" || pairing_id || sign_pk_B || kx_pk_B
//!   mac           `crypto_generichash(outlen = 32, mac_input, key = secret)`
//!   control_frames:
//!     pair_invite  0xB5 || pairing_id || sign_pk_B || kx_pk_B || mac
//!                  (B → relay → A)
//!     group_join   0xB4 || device id of B                 (A → relay)
//!
//! `device_name` (device B announces its name to A, as it does right after
//! receiving A's Welcome):
//!   name          the name, as a string (UTF-8, 1 to 64 bytes, no control
//!                 characters; one non-ASCII character on purpose)
//!   max_len       largest name in bytes (64)
//!   envelope      same fields as the top-level `envelopes[]`:
//!     device_name_b_to_a  kind 6, plaintext = the name in UTF-8,
//!                         message_id = 91 92 … a0, counter = 2 (after the
//!                         ping, which took counter 1 from B to A),
//!                         nonce = 61 62 … 78, key = `session.b.tx`
//!
//! `features` (the two envelope kinds of `docs/features.md`, section 1):
//!   packet        a Packet from A to B:
//!     json          the JSON object, as a string (the plaintext in UTF-8)
//!     max_len       largest packet in bytes (262144)
//!     envelope      same fields as the top-level `envelopes[]`:
//!       packet_a_to_b  kind 7, message_id = 11 12 … 20, counter = 4 (after
//!                      the clipboard text, the welcome and the device
//!                      removed, counters 1 to 3 from A to B),
//!                      nonce = c1 c2 … d8, key = `session.a.tx`
//!   file_chunk    a File chunk from B to A:
//!     transfer_id   16-byte transfer id
//!     offset        u64 offset of the data in the file
//!     data          the chunk data
//!     max_data_len  largest chunk data in bytes (131072)
//!     envelope      same fields as the top-level `envelopes[]`, without
//!                   plaintext_utf8:
//!       file_chunk_b_to_a  kind 8, plaintext = transfer_id || offset (u64,
//!                          big-endian) || data, message_id = 31 32 … 40,
//!                          counter = 3 (after the ping and the device name),
//!                          nonce = e1 e2 … f8, key = `session.b.tx`
//!
//! `lan` (one weft-lan-v3 connection between A and B; who dialed does not
//! change any value):
//!   a_nonce, b_nonce        32-byte challenge nonces sent by A and by B
//!   a_challenge_frame       0xC2 || a_nonce
//!   b_challenge_frame       0xC2 || b_nonce
//!   a_signed_message        "weft-lan-v3" || b_nonce || a_nonce
//!   a_auth_frame            0xC3 || device id of A || Ed25519 signature of
//!                           a_signed_message by A
//!   b_signed_message        "weft-lan-v3" || a_nonce || b_nonce
//!   b_auth_frame            0xC3 || device id of B || signature by B
//!   receipts.a_to_b         receipts B sends for A's envelopes:
//!     key_input             "weft-lan-v3 receipt" || device id of A ||
//!                           device id of B || a_nonce || b_nonce
//!     key                   `crypto_generichash(outlen = 32, key_input,
//!                           key = session.a.tx)` (= session.b.rx)
//!     message_id            16-byte message id (the clipboard text's)
//!     mac                   `crypto_generichash(outlen = 16, message_id,
//!                           key = key)`
//!     record                00 00 00 00 || message_id || mac
//!   receipts.b_to_a         the same for B's envelopes, receipts sent by A:
//!                           device ids and nonces swapped, key =
//!                           session.b.tx (= session.a.rx), message id of
//!                           the ping

use serde_json::{Value, json};
use weft_protocol::{
    AUTH_ACCEPTED, Challenge, ControlError, ControlFrame, Header, Identity, Kind, LAN_CONTEXT,
    LAN_RECEIPT_CONTEXT, LanChallenge, LanReceiptKey, LanReceiptKeys, MAX_DEVICE_NAME_LEN,
    MAX_FILE_CHUNK_DATA_LEN, MAX_PACKET_LEN, PairRequest, PairingCode, PeerKeys, SessionKeys,
    VERSION, encode_device_added, encode_device_name, encode_device_removed, encode_file_chunk,
    encode_welcome, seal_with_nonce, session_keys, sign_auth, sign_lan_auth,
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
    plaintext: Plaintext<'a>,
}

enum Plaintext<'a> {
    /// Also written as a string, in `plaintext_utf8`.
    Text(&'a str),
    Bytes(&'a [u8]),
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
    let plaintext = match e.plaintext {
        Plaintext::Text(text) => text.as_bytes(),
        Plaintext::Bytes(bytes) => bytes,
    };
    let frame =
        seal_with_nonce(&header, plaintext, &e.nonce, &key).expect("vector envelope must be valid");
    let mut out = json!({
        "name": e.name,
        "sender": e.sender.0,
        "recipient": e.recipient.0,
        "version": header.version,
        "kind": header.kind as u8,
        "message_id": hex(&header.message_id),
        "counter": header.counter,
        "nonce": hex(&e.nonce),
        "key": hex(&key),
    });
    if let Plaintext::Text(text) = e.plaintext {
        out["plaintext_utf8"] = json!(text);
    }
    out["plaintext"] = json!(hex(plaintext));
    out["header"] = json!(hex(&header.encode()));
    out["frame"] = json!(hex(&frame));
    out
}

fn peer_keys(id: &Identity) -> PeerKeys {
    PeerKeys {
        sign_pk: id.sign_pk(),
        kx_pk: id.kx_pk(),
    }
}

/// The `pairing` section: A shows a code, B joins a group that already holds
/// C, then A removes C.
fn pairing_json(a: &Identity, b: &Identity, a_b_keys: &SessionKeys) -> Value {
    let (c_sign_seed, c_kx_seed) = (seq::<32>(0x80), seq::<32>(0xa0));
    let c = Identity::from_seeds(c_sign_seed, c_kx_seed);
    let a_c_keys = session_keys(a, &c.device_id(), &c.kx_pk()).expect("session keys A-C for A");
    let c_a_keys = session_keys(&c, &a.device_id(), &a.kx_pk()).expect("session keys A-C for C");
    let a_c_client = if a.device_id() < c.device_id() {
        "a"
    } else {
        "c"
    };

    let code = PairingCode {
        relay_url: "wss://weft.frohub.eu/v1".to_owned(),
        pairing_id: seq::<16>(0x50),
        sign_pk: a.sign_pk(),
        kx_pk: a.kx_pk(),
        secret: seq::<32>(0xc0),
    };
    let mut code_payload = vec![code.relay_url.len() as u8];
    code_payload.extend_from_slice(code.relay_url.as_bytes());
    code_payload.extend_from_slice(&code.pairing_id);
    code_payload.extend_from_slice(&code.sign_pk);
    code_payload.extend_from_slice(&code.kx_pk);
    code_payload.extend_from_slice(&code.secret);

    let request = PairRequest::new(&code.secret, code.pairing_id, b.sign_pk(), b.kx_pk());
    let mut mac_input = b"weft-pair-v1".to_vec();
    mac_input.extend_from_slice(&request.pairing_id);
    mac_input.extend_from_slice(&request.sign_pk);
    mac_input.extend_from_slice(&request.kx_pk);
    let mac = request.mac;

    let frame = |frame: ControlFrame| hex(&frame.encode());

    let welcome_plaintext = encode_welcome(&[peer_keys(&c)]).expect("valid welcome");
    let welcome = envelope_json(Envelope {
        name: "welcome_a_to_b",
        sender: ("a", a, a_b_keys),
        recipient: ("b", b),
        kind: Kind::Welcome,
        message_id: seq::<16>(0x61),
        counter: 2,
        nonce: seq::<24>(0x01),
        plaintext: Plaintext::Bytes(&welcome_plaintext),
    });
    let device_added = envelope_json(Envelope {
        name: "device_added_a_to_c",
        sender: ("a", a, &a_c_keys),
        recipient: ("c", &c),
        kind: Kind::DeviceAdded,
        message_id: seq::<16>(0x71),
        counter: 1,
        nonce: seq::<24>(0x21),
        plaintext: Plaintext::Bytes(&encode_device_added(&peer_keys(b))),
    });
    let device_removed = envelope_json(Envelope {
        name: "device_removed_a_to_b",
        sender: ("a", a, a_b_keys),
        recipient: ("b", b),
        kind: Kind::DeviceRemoved,
        message_id: seq::<16>(0x81),
        counter: 3,
        nonce: seq::<24>(0x41),
        plaintext: Plaintext::Bytes(&encode_device_removed(&c.device_id())),
    });

    json!({
        "relay_url": code.relay_url,
        "pairing_id": hex(&code.pairing_id),
        "secret": hex(&code.secret),
        "shown_by": "a",
        "joiner": "b",
        "code_payload": hex(&code_payload),
        "code": code.encode().expect("valid pairing code"),
        "mac_input": hex(&mac_input),
        "mac": hex(&mac),
        "control_frames": {
            "pair_open": frame(ControlFrame::PairOpen { pairing_id: code.pairing_id }),
            "pair_request": frame(ControlFrame::PairRequest(request)),
            "group_add": frame(ControlFrame::GroupAdd { device_id: b.device_id() }),
            "group_remove": frame(ControlFrame::GroupRemove { device_id: c.device_id() }),
            "error_unknown_pairing": frame(ControlFrame::Error(ControlError::UnknownPairing)),
            "error_not_allowed": frame(ControlFrame::Error(ControlError::NotAllowed)),
            "error_rate_limited": frame(ControlFrame::Error(ControlError::RateLimited)),
            "error_group_full": frame(ControlFrame::Error(ControlError::GroupFull)),
        },
        "device_c": device_json(c_sign_seed, c_kx_seed, &c),
        "session_a_c": {
            "client": a_c_client,
            "a": keys_json(&a_c_keys),
            "c": keys_json(&c_a_keys),
        },
        "envelopes": [welcome, device_added, device_removed],
    })
}

/// The `invitation` section: A shows the code of the `pairing` section, B
/// invites A into its group.
fn invitation_json(a: &Identity, b: &Identity) -> Value {
    let code = PairingCode {
        relay_url: "wss://weft.frohub.eu/v1".to_owned(),
        pairing_id: seq::<16>(0x50),
        sign_pk: a.sign_pk(),
        kx_pk: a.kx_pk(),
        secret: seq::<32>(0xc0),
    };
    let request = PairRequest::invite(&code.secret, code.pairing_id, b.sign_pk(), b.kx_pk());
    let mut mac_input = b"weft-invite-v1".to_vec();
    mac_input.extend_from_slice(&request.pairing_id);
    mac_input.extend_from_slice(&request.sign_pk);
    mac_input.extend_from_slice(&request.kx_pk);
    let mac = request.mac;
    let frame = |frame: ControlFrame| hex(&frame.encode());
    json!({
        "code": code.encode().expect("valid pairing code"),
        "pairing_id": hex(&code.pairing_id),
        "secret": hex(&code.secret),
        "shown_by": "a",
        "inviter": "b",
        "mac_input": hex(&mac_input),
        "mac": hex(&mac),
        "control_frames": {
            "pair_invite": frame(ControlFrame::PairInvite(request)),
            "group_join": frame(ControlFrame::GroupJoin { device_id: b.device_id() }),
        },
    })
}

/// The `device_name` section: B tells A its name.
fn device_name_json(a: &Identity, b: &Identity, b_a_keys: &SessionKeys) -> Value {
    let name = "Téléphone de test";
    encode_device_name(name).expect("valid device name");
    let envelope = envelope_json(Envelope {
        name: "device_name_b_to_a",
        sender: ("b", b, b_a_keys),
        recipient: ("a", a),
        kind: Kind::DeviceName,
        message_id: seq::<16>(0x91),
        counter: 2,
        nonce: seq::<24>(0x61),
        plaintext: Plaintext::Text(name),
    });
    json!({
        "name": name,
        "max_len": MAX_DEVICE_NAME_LEN,
        "envelope": envelope,
    })
}

/// The `features` section: a Packet from A to B and a File chunk from B to A.
fn features_json(
    a: &Identity,
    b: &Identity,
    a_b_keys: &SessionKeys,
    b_a_keys: &SessionKeys,
) -> Value {
    let json = r#"{"type":"battery","level":42,"charging":true,"low":false}"#;
    let packet = envelope_json(Envelope {
        name: "packet_a_to_b",
        sender: ("a", a, a_b_keys),
        recipient: ("b", b),
        kind: Kind::Packet,
        message_id: seq::<16>(0x11),
        counter: 4,
        nonce: seq::<24>(0xc1),
        plaintext: Plaintext::Text(json),
    });

    let transfer_id = seq::<16>(0xf0);
    let offset = 3 * MAX_FILE_CHUNK_DATA_LEN as u64;
    let data = b"weft file chunk \x00\x01\xfe\xff";
    let plaintext = encode_file_chunk(&transfer_id, offset, data).expect("valid file chunk");
    let file_chunk = envelope_json(Envelope {
        name: "file_chunk_b_to_a",
        sender: ("b", b, b_a_keys),
        recipient: ("a", a),
        kind: Kind::FileChunk,
        message_id: seq::<16>(0x31),
        counter: 3,
        nonce: seq::<24>(0xe1),
        plaintext: Plaintext::Bytes(&plaintext),
    });

    json!({
        "packet": {
            "json": json,
            "max_len": MAX_PACKET_LEN,
            "envelope": packet,
        },
        "file_chunk": {
            "transfer_id": hex(&transfer_id),
            "offset": offset,
            "data": hex(data),
            "max_data_len": MAX_FILE_CHUNK_DATA_LEN,
            "envelope": file_chunk,
        },
    })
}

/// The `lan` section: one weft-lan-v3 connection between A and B.
fn lan_json(a: &Identity, b: &Identity, a_keys: &SessionKeys, b_keys: &SessionKeys) -> Value {
    let (a_nonce, b_nonce) = (seq::<32>(0x30), seq::<32>(0x70));
    let a_auth = sign_lan_auth(a, &b_nonce, &a_nonce);
    let b_auth = sign_lan_auth(b, &a_nonce, &b_nonce);
    let at_a = LanReceiptKeys::derive(a, &b.device_id(), &b.kx_pk(), &a_nonce, &b_nonce)
        .expect("LAN receipt keys for A");
    let at_b = LanReceiptKeys::derive(b, &a.device_id(), &a.kx_pk(), &b_nonce, &a_nonce)
        .expect("LAN receipt keys for B");
    assert_eq!(at_a.outgoing.as_bytes(), at_b.incoming.as_bytes());
    assert_eq!(at_a.incoming.as_bytes(), at_b.outgoing.as_bytes());

    let receipt = |sender: &Identity,
                   recipient: &Identity,
                   sender_nonce: &[u8; 32],
                   recipient_nonce: &[u8; 32],
                   direction_key: &[u8; 32],
                   message_id: [u8; 16]| {
        let key = LanReceiptKey::derive(
            direction_key,
            &sender.device_id(),
            &recipient.device_id(),
            sender_nonce,
            recipient_nonce,
        );
        let key_input = [
            LAN_RECEIPT_CONTEXT,
            &sender.device_id(),
            &recipient.device_id(),
            sender_nonce,
            recipient_nonce,
        ]
        .concat();
        let mac = key.mac(&message_id);
        assert!(key.verify(&message_id, &mac));
        json!({
            "key_input": hex(&key_input),
            "key": hex(key.as_bytes()),
            "message_id": hex(&message_id),
            "mac": hex(&mac),
            "record": hex(&[[0u8; 4].as_slice(), &message_id, &mac].concat()),
        })
    };
    let a_to_b = receipt(a, b, &a_nonce, &b_nonce, &a_keys.tx, seq::<16>(0xa0));
    let b_to_a = receipt(b, a, &b_nonce, &a_nonce, &b_keys.tx, seq::<16>(0xd0));
    assert_eq!(a_to_b["key"], hex(at_a.outgoing.as_bytes()));
    assert_eq!(b_to_a["key"], hex(at_a.incoming.as_bytes()));

    json!({
        "a_nonce": hex(&a_nonce),
        "b_nonce": hex(&b_nonce),
        "a_challenge_frame": hex(&LanChallenge::from_nonce(a_nonce).encode()),
        "b_challenge_frame": hex(&LanChallenge::from_nonce(b_nonce).encode()),
        "a_signed_message": hex(&[LAN_CONTEXT, &b_nonce, &a_nonce].concat()),
        "a_auth_frame": hex(&a_auth.encode()),
        "b_signed_message": hex(&[LAN_CONTEXT, &a_nonce, &b_nonce].concat()),
        "b_auth_frame": hex(&b_auth.encode()),
        "receipts": {
            "a_to_b": a_to_b,
            "b_to_a": b_to_a,
        },
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
        plaintext: Plaintext::Text("héllo, weft ✂"),
    });
    let ping = envelope_json(Envelope {
        name: "ping_b_to_a",
        sender: ("b", &b, &b_keys),
        recipient: ("a", &a),
        kind: Kind::Ping,
        message_id: seq::<16>(0xd0),
        counter: 1,
        nonce: seq::<24>(0xb0),
        plaintext: Plaintext::Text(""),
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
        "pairing": pairing_json(&a, &b, &a_keys),
        "device_name": device_name_json(&a, &b, &b_keys),
        "features": features_json(&a, &b, &a_keys, &b_keys),
        "invitation": invitation_json(&a, &b),
        "lan": lan_json(&a, &b, &a_keys, &b_keys),
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
