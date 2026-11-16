//! The committed vectors must match what the generator produces today.

#[path = "../examples/gen_vectors.rs"]
#[allow(dead_code)]
mod gen_vectors;

#[test]
fn committed_vectors_match_generator() {
    let committed = include_str!("../vectors/v1.json");
    let generated = gen_vectors::vectors_json();
    assert!(
        committed == generated,
        "vectors/v1.json is out of date: run `cargo run -p weft-protocol --example gen_vectors`"
    );
}

fn unhex<const N: usize>(value: &serde_json::Value) -> [u8; N] {
    unhex_vec(value).try_into().expect("unexpected length")
}

fn unhex_vec(value: &serde_json::Value) -> Vec<u8> {
    let hex = value.as_str().expect("hex string");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex digit"))
        .collect()
}

/// Reads the committed pairing vectors back the way a receiving client
/// would, so the decoding side is checked against the file too.
#[test]
fn committed_pairing_vectors_decode() {
    use weft_protocol::{
        ControlError, ControlFrame, Kind, PairingCode, PeerKeys, decode_device_added,
        decode_device_removed, decode_welcome, open, verify_pair_request,
    };

    let doc: serde_json::Value = serde_json::from_str(include_str!("../vectors/v1.json")).unwrap();
    let (a, b) = (&doc["devices"]["a"], &doc["devices"]["b"]);
    let pairing = &doc["pairing"];
    let c = &pairing["device_c"];
    let keys = |device: &serde_json::Value| PeerKeys {
        sign_pk: unhex(&device["sign_pk"]),
        kx_pk: unhex(&device["kx_pk"]),
    };
    for device in [a, b, c] {
        assert_eq!(keys(device).device_id(), unhex::<16>(&device["device_id"]));
    }

    // B scans the code and learns A's keys and the secret.
    let code = PairingCode::decode(pairing["code"].as_str().unwrap()).unwrap();
    assert_eq!(code.relay_url, pairing["relay_url"].as_str().unwrap());
    assert_eq!(code.pairing_id, unhex::<16>(&pairing["pairing_id"]));
    assert_eq!(code.sign_pk, keys(a).sign_pk);
    assert_eq!(code.kx_pk, keys(a).kx_pk);
    assert_eq!(code.secret, unhex::<32>(&pairing["secret"]));

    // A receives B's request and checks it.
    let frames = &pairing["control_frames"];
    let decode = |name: &str| ControlFrame::decode(&unhex_vec(&frames[name])).unwrap();
    let ControlFrame::PairRequest(request) = decode("pair_request") else {
        panic!("pair_request is not a pair request");
    };
    assert_eq!(request.pairing_id, code.pairing_id);
    assert_eq!(request.sign_pk, keys(b).sign_pk);
    assert_eq!(request.kx_pk, keys(b).kx_pk);
    assert_eq!(request.mac, unhex::<32>(&pairing["mac"]));
    assert!(verify_pair_request(&code.secret, &request));

    assert_eq!(
        decode("pair_open"),
        ControlFrame::PairOpen {
            pairing_id: code.pairing_id
        }
    );
    assert_eq!(
        decode("group_add"),
        ControlFrame::GroupAdd {
            device_id: keys(b).device_id()
        }
    );
    assert_eq!(
        decode("group_remove"),
        ControlFrame::GroupRemove {
            device_id: keys(c).device_id()
        }
    );
    for (name, code) in [
        ("error_unknown_pairing", ControlError::UnknownPairing),
        ("error_not_allowed", ControlError::NotAllowed),
        ("error_rate_limited", ControlError::RateLimited),
        ("error_group_full", ControlError::GroupFull),
    ] {
        assert_eq!(decode(name), ControlFrame::Error(code));
    }

    // Recipients open the three group envelopes with their rx key.
    let b_rx: [u8; 32] = unhex(&doc["session"]["b"]["rx"]);
    let c_rx: [u8; 32] = unhex(&pairing["session_a_c"]["c"]["rx"]);
    let open_vector = |index: usize, name: &str, rx: &[u8; 32]| {
        let envelope = &pairing["envelopes"][index];
        assert_eq!(envelope["name"], name);
        let (header, plaintext) = open(&unhex_vec(&envelope["frame"]), rx).unwrap();
        assert_eq!(header.sender, keys(a).device_id());
        assert_eq!(plaintext, unhex_vec(&envelope["plaintext"]));
        (header, plaintext)
    };

    let (header, plaintext) = open_vector(0, "welcome_a_to_b", &b_rx);
    assert_eq!(header.kind, Kind::Welcome);
    assert_eq!(header.recipient, keys(b).device_id());
    assert_eq!(decode_welcome(&plaintext).unwrap(), [keys(c)]);

    let (header, plaintext) = open_vector(1, "device_added_a_to_c", &c_rx);
    assert_eq!(header.kind, Kind::DeviceAdded);
    assert_eq!(header.recipient, keys(c).device_id());
    assert_eq!(decode_device_added(&plaintext).unwrap(), keys(b));

    let (header, plaintext) = open_vector(2, "device_removed_a_to_b", &b_rx);
    assert_eq!(header.kind, Kind::DeviceRemoved);
    assert_eq!(header.recipient, keys(b).device_id());
    assert_eq!(
        decode_device_removed(&plaintext).unwrap(),
        keys(c).device_id()
    );
}
