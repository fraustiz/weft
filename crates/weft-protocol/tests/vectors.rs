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

/// A receives B's Device name envelope from the committed file.
#[test]
fn committed_device_name_vector_decodes() {
    use weft_protocol::{
        Kind, MAX_DEVICE_NAME_LEN, decode_device_name, device_id_from_sign_pk, encode_device_name,
        open,
    };

    let doc: serde_json::Value = serde_json::from_str(include_str!("../vectors/v1.json")).unwrap();
    let section = &doc["device_name"];
    let envelope = &section["envelope"];
    let name = section["name"].as_str().unwrap();
    assert_eq!(name, "Téléphone de test");
    assert_eq!(section["max_len"], MAX_DEVICE_NAME_LEN);
    assert_eq!(envelope["name"], "device_name_b_to_a");
    assert_eq!(envelope["kind"], Kind::DeviceName as u8);
    assert_eq!(envelope["plaintext_utf8"], name);
    assert_eq!(unhex_vec(&envelope["plaintext"]), name.as_bytes());
    assert_eq!(encode_device_name(name).unwrap(), name.as_bytes());

    let device_id =
        |device: &str| device_id_from_sign_pk(&unhex(&doc["devices"][device]["sign_pk"]));
    let a_rx: [u8; 32] = unhex(&doc["session"]["a"]["rx"]);
    assert_eq!(
        unhex::<32>(&envelope["key"]),
        unhex::<32>(&doc["session"]["b"]["tx"])
    );
    let frame = unhex_vec(&envelope["frame"]);
    assert_eq!(frame[..58], unhex_vec(&envelope["header"])[..]);
    let (header, plaintext) = open(&frame, &a_rx).unwrap();
    assert_eq!(header.kind, Kind::DeviceName);
    assert_eq!(header.sender, device_id("b"));
    assert_eq!(header.recipient, device_id("a"));
    assert_eq!(header.counter, envelope["counter"].as_u64().unwrap());
    assert_eq!(header.message_id, unhex::<16>(&envelope["message_id"]));
    assert_eq!(decode_device_name(&plaintext).unwrap(), name);
}

/// B receives A's Packet and A receives B's File chunk from the committed
/// file.
#[test]
fn committed_feature_vectors_decode() {
    use weft_protocol::{
        FileChunk, Kind, MAX_FILE_CHUNK_DATA_LEN, MAX_PACKET_LEN, RouteHeader, decode_file_chunk,
        device_id_from_sign_pk, encode_file_chunk, open,
    };

    let doc: serde_json::Value = serde_json::from_str(include_str!("../vectors/v1.json")).unwrap();
    let section = &doc["features"];
    let device_id =
        |device: &str| device_id_from_sign_pk(&unhex(&doc["devices"][device]["sign_pk"]));
    let rx = |device: &str| -> [u8; 32] { unhex(&doc["session"][device]["rx"]) };
    let open_vector = |envelope: &serde_json::Value, kind: Kind, from: &str, to: &str| {
        let frame = unhex_vec(&envelope["frame"]);
        assert_eq!(frame[..58], unhex_vec(&envelope["header"])[..]);
        let route = RouteHeader::decode(&frame).unwrap();
        assert_eq!(
            route,
            RouteHeader {
                kind: kind as u8,
                sender: device_id(from),
                recipient: device_id(to),
            }
        );
        let (header, plaintext) = open(&frame, &rx(to)).unwrap();
        assert_eq!(header.kind, kind);
        assert_eq!(envelope["kind"], kind as u8);
        assert_eq!(header.counter, envelope["counter"].as_u64().unwrap());
        assert_eq!(header.message_id, unhex::<16>(&envelope["message_id"]));
        assert_eq!(plaintext, unhex_vec(&envelope["plaintext"]));
        plaintext
    };

    let packet = &section["packet"];
    assert_eq!(packet["max_len"], MAX_PACKET_LEN);
    let envelope = &packet["envelope"];
    assert_eq!(envelope["name"], "packet_a_to_b");
    let json = packet["json"].as_str().unwrap();
    assert_eq!(envelope["plaintext_utf8"], json);
    let plaintext = open_vector(envelope, Kind::Packet, "a", "b");
    assert_eq!(plaintext, json.as_bytes());

    let chunk = &section["file_chunk"];
    assert_eq!(chunk["max_data_len"], MAX_FILE_CHUNK_DATA_LEN);
    let envelope = &chunk["envelope"];
    assert_eq!(envelope["name"], "file_chunk_b_to_a");
    let plaintext = open_vector(envelope, Kind::FileChunk, "b", "a");
    let expected = FileChunk {
        transfer_id: unhex(&chunk["transfer_id"]),
        offset: chunk["offset"].as_u64().unwrap(),
        data: unhex_vec(&chunk["data"]),
    };
    assert_eq!(decode_file_chunk(&plaintext).unwrap(), expected);
    assert_eq!(
        encode_file_chunk(&expected.transfer_id, expected.offset, &expected.data).unwrap(),
        plaintext
    );
}

/// A receives B's invitation from the committed file and answers it.
#[test]
fn committed_invitation_vectors_decode() {
    use weft_protocol::{
        ControlFrame, PairingCode, device_id_from_sign_pk, invite_request_mac,
        verify_invite_request, verify_pair_request,
    };

    let doc: serde_json::Value = serde_json::from_str(include_str!("../vectors/v1.json")).unwrap();
    let section = &doc["invitation"];
    let pairing = &doc["pairing"];
    let (a, b) = (&doc["devices"]["a"], &doc["devices"]["b"]);
    assert_eq!(section["code"], pairing["code"]);
    assert_eq!(section["shown_by"], "a");
    assert_eq!(section["inviter"], "b");

    let code = PairingCode::decode(section["code"].as_str().unwrap()).unwrap();
    assert_eq!(code.pairing_id, unhex::<16>(&section["pairing_id"]));
    assert_eq!(code.secret, unhex::<32>(&section["secret"]));
    assert_eq!(code.sign_pk, unhex::<32>(&a["sign_pk"]));

    let mut mac_input = b"weft-invite-v1".to_vec();
    mac_input.extend_from_slice(&code.pairing_id);
    mac_input.extend_from_slice(&unhex::<32>(&b["sign_pk"]));
    mac_input.extend_from_slice(&unhex::<32>(&b["kx_pk"]));
    assert_eq!(unhex_vec(&section["mac_input"]), mac_input);

    let frames = &section["control_frames"];
    let frame = unhex_vec(&frames["pair_invite"]);
    assert_eq!(frame[0], 0xB5);
    let ControlFrame::PairInvite(request) = ControlFrame::decode(&frame).unwrap() else {
        panic!("pair_invite is not an invitation");
    };
    assert_eq!(request.pairing_id, code.pairing_id);
    assert_eq!(request.sign_pk, unhex::<32>(&b["sign_pk"]));
    assert_eq!(request.kx_pk, unhex::<32>(&b["kx_pk"]));
    assert_eq!(request.mac, unhex::<32>(&section["mac"]));
    assert_eq!(
        request.mac,
        invite_request_mac(
            &code.secret,
            &request.pairing_id,
            &request.sign_pk,
            &request.kx_pk
        )
    );
    assert!(verify_invite_request(&code.secret, &request));
    assert!(!verify_pair_request(&code.secret, &request));
    assert_ne!(section["mac"], pairing["mac"]);

    let b_id = device_id_from_sign_pk(&request.sign_pk);
    assert_eq!(b_id, unhex::<16>(&b["device_id"]));
    let frame = unhex_vec(&frames["group_join"]);
    assert_eq!(
        ControlFrame::decode(&frame).unwrap(),
        ControlFrame::GroupJoin { device_id: b_id }
    );
    assert_eq!(ControlFrame::GroupJoin { device_id: b_id }.encode(), frame);
}
