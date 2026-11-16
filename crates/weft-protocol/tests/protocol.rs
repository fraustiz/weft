use weft_protocol::{
    AuthResponse, Challenge, Error, HEADER_LEN, Header, Identity, Kind, MAX_CLIPBOARD_TEXT_LEN,
    NONCE_LEN, SessionKeys, TAG_LEN, VERSION, device_id_from_sign_pk, open, seal, session_keys,
    sign_auth, verify_auth,
};

fn pair() -> (Identity, Identity, SessionKeys, SessionKeys) {
    let a = Identity::from_seeds([1; 32], [2; 32]);
    let b = Identity::from_seeds([3; 32], [4; 32]);
    let a_keys = session_keys(&a, &b.device_id(), &b.kx_pk()).unwrap();
    let b_keys = session_keys(&b, &a.device_id(), &a.kx_pk()).unwrap();
    (a, b, a_keys, b_keys)
}

fn header(from: &Identity, to: &Identity, kind: Kind) -> Header {
    Header {
        version: VERSION,
        kind,
        sender: from.device_id(),
        recipient: to.device_id(),
        message_id: [7; 16],
        counter: 1,
    }
}

fn clipboard_frame() -> (Vec<u8>, [u8; 32]) {
    let (a, b, a_keys, b_keys) = pair();
    let frame = seal(&header(&a, &b, Kind::ClipboardText), b"secret", &a_keys.tx).unwrap();
    (frame, b_keys.rx)
}

#[test]
fn device_id_is_blake2b_of_sign_pk() {
    let id = Identity::generate();
    assert_eq!(id.device_id(), device_id_from_sign_pk(&id.sign_pk()));
}

#[test]
fn session_keys_mirror_each_other() {
    let (_, _, a_keys, b_keys) = pair();
    assert_eq!(a_keys.tx, b_keys.rx);
    assert_eq!(a_keys.rx, b_keys.tx);
    assert_ne!(a_keys.rx, a_keys.tx);
}

#[test]
fn session_keys_reject_same_device() {
    let a = Identity::generate();
    assert_eq!(
        session_keys(&a, &a.device_id(), &a.kx_pk()),
        Err(Error::SameDeviceId)
    );
}

#[test]
fn header_round_trip() {
    let (a, b, _, _) = pair();
    let h = Header {
        counter: 0x0102030405060708,
        ..header(&a, &b, Kind::Ping)
    };
    let bytes = h.encode();
    assert_eq!(bytes.len(), HEADER_LEN);
    assert_eq!(&bytes[50..], &[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(Header::decode(&bytes).unwrap(), h);
}

#[test]
fn header_decode_rejects_bad_input() {
    let (a, b, _, _) = pair();
    let bytes = header(&a, &b, Kind::Ping).encode();
    assert_eq!(Header::decode(&bytes[..57]), Err(Error::HeaderLength(57)));
    assert_eq!(Header::decode(&[]), Err(Error::HeaderLength(0)));

    let mut bad_version = bytes;
    bad_version[0] = 0x02;
    assert_eq!(
        Header::decode(&bad_version),
        Err(Error::UnsupportedVersion(2))
    );

    let mut bad_kind = bytes;
    bad_kind[1] = 0x06;
    assert_eq!(Header::decode(&bad_kind), Err(Error::UnknownKind(6)));
}

#[test]
fn seal_open_round_trip_both_directions() {
    let (a, b, a_keys, b_keys) = pair();

    let h = header(&a, &b, Kind::ClipboardText);
    let frame = seal(&h, "héllo".as_bytes(), &a_keys.tx).unwrap();
    assert_eq!(
        frame.len(),
        HEADER_LEN + NONCE_LEN + "héllo".len() + TAG_LEN
    );
    assert_eq!(open(&frame, &b_keys.rx).unwrap(), (h, "héllo".into()));

    let h = header(&b, &a, Kind::Ping);
    let frame = seal(&h, b"", &b_keys.tx).unwrap();
    assert_eq!(open(&frame, &a_keys.rx).unwrap(), (h, Vec::new()));
}

#[test]
fn seal_uses_fresh_nonces() {
    let (a, b, a_keys, _) = pair();
    let h = header(&a, &b, Kind::ClipboardText);
    assert_ne!(
        seal(&h, b"x", &a_keys.tx).unwrap(),
        seal(&h, b"x", &a_keys.tx).unwrap()
    );
}

#[test]
fn tampering_any_byte_fails() {
    let (frame, rx) = clipboard_frame();
    for i in 0..frame.len() {
        let mut tampered = frame.clone();
        tampered[i] ^= 0x01;
        assert!(open(&tampered, &rx).is_err(), "tampered byte {i} accepted");
    }
}

#[test]
fn wrong_key_fails() {
    let (a, b, a_keys, _) = pair();
    let frame = seal(&header(&a, &b, Kind::ClipboardText), b"hi", &a_keys.tx).unwrap();
    // The sender's own rx is not the key it encrypted with.
    assert_eq!(open(&frame, &a_keys.rx), Err(Error::DecryptionFailed));
    assert_eq!(open(&frame, &[0; 32]), Err(Error::DecryptionFailed));
}

#[test]
fn truncated_frames_are_errors() {
    let (frame, rx) = clipboard_frame();
    for len in 0..frame.len() {
        assert!(open(&frame[..len], &rx).is_err(), "truncated to {len}");
    }
    let mut longer = frame.clone();
    longer.push(0);
    assert!(open(&longer, &rx).is_err());
}

#[test]
fn clipboard_size_limit() {
    let (a, b, a_keys, b_keys) = pair();
    let h = header(&a, &b, Kind::ClipboardText);

    let max = vec![b'a'; MAX_CLIPBOARD_TEXT_LEN];
    let frame = seal(&h, &max, &a_keys.tx).unwrap();
    assert_eq!(open(&frame, &b_keys.rx).unwrap().1, max);

    let too_big = vec![b'a'; MAX_CLIPBOARD_TEXT_LEN + 1];
    assert_eq!(
        seal(&h, &too_big, &a_keys.tx),
        Err(Error::PlaintextTooLarge(MAX_CLIPBOARD_TEXT_LEN + 1))
    );

    // An oversized frame is rejected before decryption.
    let mut oversized = h.encode().to_vec();
    oversized.extend_from_slice(&[0; NONCE_LEN]);
    oversized.extend_from_slice(&vec![0; MAX_CLIPBOARD_TEXT_LEN + 1 + TAG_LEN]);
    assert_eq!(
        open(&oversized, &b_keys.rx),
        Err(Error::PlaintextTooLarge(MAX_CLIPBOARD_TEXT_LEN + 1))
    );
}

#[test]
fn plaintext_must_match_kind() {
    let (a, b, a_keys, _) = pair();
    assert_eq!(
        seal(&header(&a, &b, Kind::Ping), b"x", &a_keys.tx),
        Err(Error::PingNotEmpty)
    );
    assert_eq!(
        seal(&header(&a, &b, Kind::ClipboardText), &[0xff], &a_keys.tx),
        Err(Error::InvalidUtf8)
    );
}

#[test]
fn open_rejects_unknown_version_and_kind() {
    let (frame, rx) = clipboard_frame();
    let mut bad_version = frame.clone();
    bad_version[0] = 0x00;
    assert_eq!(open(&bad_version, &rx), Err(Error::UnsupportedVersion(0)));
    let mut bad_kind = frame;
    bad_kind[1] = 0xff;
    assert_eq!(open(&bad_kind, &rx), Err(Error::UnknownKind(0xff)));
}

#[test]
fn seal_rejects_unknown_version() {
    let (a, b, a_keys, _) = pair();
    let h = Header {
        version: 2,
        ..header(&a, &b, Kind::Ping)
    };
    assert_eq!(seal(&h, b"", &a_keys.tx), Err(Error::UnsupportedVersion(2)));
}

fn challenge() -> Challenge {
    Challenge::new("weft.frohub.eu", [9; 32]).unwrap()
}

#[test]
fn challenge_round_trip() {
    let c = challenge();
    let frame = c.encode();
    assert_eq!(frame[0], 0xA0);
    assert_eq!(frame[1] as usize, "weft.frohub.eu".len());
    assert_eq!(Challenge::decode(&frame).unwrap(), c);
}

#[test]
fn challenge_decode_rejects_bad_input() {
    let frame = challenge().encode();
    for len in 0..frame.len() {
        assert!(Challenge::decode(&frame[..len]).is_err());
    }
    let mut longer = frame.clone();
    longer.push(0);
    assert_eq!(Challenge::decode(&longer), Err(Error::MalformedAuthFrame));
    let mut wrong_tag = frame.clone();
    wrong_tag[0] = 0xA1;
    assert_eq!(
        Challenge::decode(&wrong_tag),
        Err(Error::UnexpectedFrameType(0xA1))
    );
    let mut bad_utf8 = frame;
    bad_utf8[2] = 0xff;
    assert_eq!(Challenge::decode(&bad_utf8), Err(Error::InvalidUtf8));
}

#[test]
fn challenge_rejects_long_relay_name() {
    assert!(Challenge::new("a".repeat(255), [0; 32]).is_ok());
    assert_eq!(
        Challenge::new("a".repeat(256), [0; 32]),
        Err(Error::RelayNameTooLong)
    );
}

#[test]
fn auth_response_round_trip_and_bad_input() {
    let response = sign_auth(&Identity::generate(), &challenge());
    let frame = response.encode();
    assert_eq!(frame[0], 0xA1);
    assert_eq!(AuthResponse::decode(&frame).unwrap(), response);
    for len in 0..frame.len() {
        assert!(AuthResponse::decode(&frame[..len]).is_err());
    }
    let mut longer = frame.to_vec();
    longer.push(0);
    assert_eq!(
        AuthResponse::decode(&longer),
        Err(Error::MalformedAuthFrame)
    );
    let mut wrong_tag = frame;
    wrong_tag[0] = 0xA0;
    assert_eq!(
        AuthResponse::decode(&wrong_tag),
        Err(Error::UnexpectedFrameType(0xA0))
    );
}

#[test]
fn verify_auth_returns_device_id() {
    let id = Identity::generate();
    let c = challenge();
    let response = AuthResponse::decode(&sign_auth(&id, &c).encode()).unwrap();
    assert_eq!(verify_auth(&c, &response), Ok(id.device_id()));
}

#[test]
fn verify_auth_rejects_wrong_relay_name_nonce_or_key() {
    let id = Identity::generate();
    let c = challenge();
    let response = sign_auth(&id, &c);

    let other_name = Challenge::new("evil.example", c.nonce()).unwrap();
    assert_eq!(
        verify_auth(&other_name, &response),
        Err(Error::InvalidSignature)
    );

    let other_nonce = Challenge::new(c.relay_name(), [8; 32]).unwrap();
    assert_eq!(
        verify_auth(&other_nonce, &response),
        Err(Error::InvalidSignature)
    );

    let other_key = AuthResponse {
        sign_pk: Identity::generate().sign_pk(),
        ..response.clone()
    };
    assert_eq!(verify_auth(&c, &other_key), Err(Error::InvalidSignature));

    let mut bad_signature = response;
    bad_signature.signature[0] ^= 1;
    assert_eq!(
        verify_auth(&c, &bad_signature),
        Err(Error::InvalidSignature)
    );
}
