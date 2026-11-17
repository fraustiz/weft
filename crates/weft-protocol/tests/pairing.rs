//! Groups and pairing: group change plaintexts, pairing code, pair request
//! MAC, control frames, frame dispatch and the client host check.

use weft_protocol::{
    Challenge, ControlError, ControlFrame, Error, FrameType, HEADER_LEN, Header, Identity, Kind,
    MAX_WELCOME_DEVICES, NONCE_LEN, PairRequest, PairingCode, PeerKeys, SessionKeys, TAG_LEN,
    VERSION, check_relay_name, classify, decode_device_added, decode_device_removed,
    decode_welcome, encode_device_added, encode_device_removed, encode_welcome, invite_request_mac,
    open, pair_request_mac, random_message_id, random_pairing_id, random_pairing_secret, seal,
    session_keys, verify_invite_request, verify_pair_request,
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

fn peer(n: u8) -> PeerKeys {
    PeerKeys {
        sign_pk: [n; 32],
        kx_pk: [n.wrapping_add(100); 32],
    }
}

fn peers(count: usize) -> Vec<PeerKeys> {
    (0..count as u8).map(peer).collect()
}

/// A frame with a valid header and `plaintext_len` bytes of garbage where the
/// ciphertext should be: only acceptable if it is rejected before decryption.
fn garbage_frame(kind: Kind, plaintext_len: usize) -> (Vec<u8>, [u8; 32]) {
    let (a, b, _, b_keys) = pair();
    let mut frame = header(&a, &b, kind).encode().to_vec();
    frame.extend_from_slice(&[0; NONCE_LEN]);
    frame.extend_from_slice(&vec![0; plaintext_len + TAG_LEN]);
    (frame, b_keys.rx)
}

// --- Group change plaintexts -------------------------------------------

#[test]
fn peer_keys_device_id_is_derived_from_sign_pk() {
    let id = Identity::generate();
    let keys = PeerKeys {
        sign_pk: id.sign_pk(),
        kx_pk: id.kx_pk(),
    };
    assert_eq!(keys.device_id(), id.device_id());
}

#[test]
fn welcome_round_trip_for_every_count() {
    for count in 0..=MAX_WELCOME_DEVICES {
        let members = peers(count);
        let plaintext = encode_welcome(&members).unwrap();
        assert_eq!(plaintext.len(), 1 + count * 64);
        assert_eq!(plaintext[0] as usize, count);
        assert_eq!(decode_welcome(&plaintext).unwrap(), members);
    }
    // sign_pk comes first, then kx_pk.
    let plaintext = encode_welcome(&[peer(5)]).unwrap();
    assert_eq!(&plaintext[1..33], &[5; 32]);
    assert_eq!(&plaintext[33..], &[105; 32]);
}

#[test]
fn welcome_rejects_more_than_eight_devices() {
    assert_eq!(MAX_WELCOME_DEVICES, 8);
    assert_eq!(encode_welcome(&peers(9)), Err(Error::TooManyDevices(9)));

    // Well-formed apart from the count.
    let mut nine = vec![9u8];
    nine.extend_from_slice(&[0; 9 * 64]);
    assert_eq!(decode_welcome(&nine), Err(Error::TooManyDevices(9)));
    assert_eq!(decode_welcome(&[0xff]), Err(Error::TooManyDevices(255)));
}

#[test]
fn welcome_decode_rejects_wrong_lengths() {
    assert_eq!(decode_welcome(&[]), Err(Error::InvalidPlaintextLength(0)));

    let plaintext = encode_welcome(&peers(2)).unwrap();
    for len in 1..plaintext.len() {
        assert_eq!(
            decode_welcome(&plaintext[..len]),
            Err(Error::InvalidPlaintextLength(len)),
            "truncated to {len}"
        );
    }
    let mut longer = plaintext.clone();
    longer.push(0);
    assert_eq!(
        decode_welcome(&longer),
        Err(Error::InvalidPlaintextLength(longer.len()))
    );

    // The count must match the number of entries, in both directions.
    let mut count_too_low = plaintext.clone();
    count_too_low[0] = 1;
    assert_eq!(
        decode_welcome(&count_too_low),
        Err(Error::InvalidPlaintextLength(plaintext.len()))
    );
    let mut count_too_high = plaintext.clone();
    count_too_high[0] = 3;
    assert_eq!(
        decode_welcome(&count_too_high),
        Err(Error::InvalidPlaintextLength(plaintext.len()))
    );
}

#[test]
fn device_added_round_trip_and_wrong_lengths() {
    let plaintext = encode_device_added(&peer(1));
    assert_eq!(plaintext.len(), 64);
    assert_eq!(decode_device_added(&plaintext).unwrap(), peer(1));

    for len in [0, 1, 32, 63] {
        assert_eq!(
            decode_device_added(&plaintext[..len]),
            Err(Error::InvalidPlaintextLength(len))
        );
    }
    assert_eq!(
        decode_device_added(&[0; 65]),
        Err(Error::InvalidPlaintextLength(65))
    );
}

#[test]
fn device_removed_round_trip_and_wrong_lengths() {
    let plaintext = encode_device_removed(&[6; 16]);
    assert_eq!(decode_device_removed(&plaintext).unwrap(), [6; 16]);

    for len in [0, 1, 15, 17, 32] {
        assert_eq!(
            decode_device_removed(&vec![0; len]),
            Err(Error::InvalidPlaintextLength(len))
        );
    }
}

#[test]
fn group_envelopes_round_trip() {
    let (a, b, a_keys, b_keys) = pair();

    for count in [0, 1, MAX_WELCOME_DEVICES] {
        let h = header(&a, &b, Kind::Welcome);
        let frame = seal(&h, &encode_welcome(&peers(count)).unwrap(), &a_keys.tx).unwrap();
        let (opened, plaintext) = open(&frame, &b_keys.rx).unwrap();
        assert_eq!(opened, h);
        assert_eq!(decode_welcome(&plaintext).unwrap(), peers(count));
    }

    let h = header(&a, &b, Kind::DeviceAdded);
    let frame = seal(&h, &encode_device_added(&peer(3)), &a_keys.tx).unwrap();
    let (opened, plaintext) = open(&frame, &b_keys.rx).unwrap();
    assert_eq!(opened, h);
    assert_eq!(decode_device_added(&plaintext).unwrap(), peer(3));

    let h = header(&b, &a, Kind::DeviceRemoved);
    let frame = seal(&h, &encode_device_removed(&[4; 16]), &b_keys.tx).unwrap();
    let (opened, plaintext) = open(&frame, &a_keys.rx).unwrap();
    assert_eq!(opened, h);
    assert_eq!(decode_device_removed(&plaintext).unwrap(), [4; 16]);
}

#[test]
fn group_kinds_decode_from_header_bytes() {
    let (a, b, _, _) = pair();
    for (byte, kind) in [
        (0x03, Kind::Welcome),
        (0x04, Kind::DeviceAdded),
        (0x05, Kind::DeviceRemoved),
    ] {
        let bytes = header(&a, &b, kind).encode();
        assert_eq!(bytes[1], byte);
        assert_eq!(Header::decode(&bytes).unwrap().kind, kind);
    }
}

#[test]
fn seal_rejects_malformed_group_plaintexts() {
    let (a, b, a_keys, _) = pair();
    let tx = &a_keys.tx;

    let welcome = header(&a, &b, Kind::Welcome);
    assert_eq!(
        seal(&welcome, &[], tx),
        Err(Error::InvalidPlaintextLength(0))
    );
    assert_eq!(
        seal(&welcome, &[1], tx),
        Err(Error::InvalidPlaintextLength(1))
    );
    assert_eq!(
        seal(&welcome, &[0; 64], tx),
        Err(Error::InvalidPlaintextLength(64))
    );
    // Right shape for one entry, but the count says two.
    let mut wrong_count = encode_welcome(&peers(1)).unwrap();
    wrong_count[0] = 2;
    assert_eq!(
        seal(&welcome, &wrong_count, tx),
        Err(Error::InvalidPlaintextLength(65))
    );
    // Nine entries: one more than allowed.
    let mut nine = vec![9u8];
    nine.extend_from_slice(&[0; 9 * 64]);
    assert_eq!(
        seal(&welcome, &nine, tx),
        Err(Error::PlaintextTooLarge(1 + 9 * 64))
    );

    let added = header(&a, &b, Kind::DeviceAdded);
    assert_eq!(
        seal(&added, &[0; 63], tx),
        Err(Error::InvalidPlaintextLength(63))
    );
    assert_eq!(
        seal(&added, &[0; 65], tx),
        Err(Error::PlaintextTooLarge(65))
    );
    assert_eq!(seal(&added, &[], tx), Err(Error::InvalidPlaintextLength(0)));

    let removed = header(&a, &b, Kind::DeviceRemoved);
    assert_eq!(
        seal(&removed, &[0; 15], tx),
        Err(Error::InvalidPlaintextLength(15))
    );
    assert_eq!(
        seal(&removed, &[0; 17], tx),
        Err(Error::PlaintextTooLarge(17))
    );
}

#[test]
fn open_rejects_wrong_group_lengths_before_decrypting() {
    // Every frame below is garbage: a length error (rather than
    // DecryptionFailed) proves the check ran before the AEAD.
    for (kind, len, expected) in [
        (Kind::Welcome, 0, Error::InvalidPlaintextLength(0)),
        (Kind::Welcome, 2, Error::InvalidPlaintextLength(2)),
        (Kind::Welcome, 64, Error::InvalidPlaintextLength(64)),
        (Kind::Welcome, 66, Error::InvalidPlaintextLength(66)),
        (Kind::Welcome, 512, Error::InvalidPlaintextLength(512)),
        (Kind::Welcome, 514, Error::PlaintextTooLarge(514)),
        (Kind::Welcome, 1 + 9 * 64, Error::PlaintextTooLarge(577)),
        (Kind::Welcome, 100_000, Error::PlaintextTooLarge(100_000)),
        (Kind::DeviceAdded, 0, Error::InvalidPlaintextLength(0)),
        (Kind::DeviceAdded, 63, Error::InvalidPlaintextLength(63)),
        (Kind::DeviceAdded, 65, Error::PlaintextTooLarge(65)),
        (Kind::DeviceRemoved, 0, Error::InvalidPlaintextLength(0)),
        (Kind::DeviceRemoved, 15, Error::InvalidPlaintextLength(15)),
        (Kind::DeviceRemoved, 17, Error::PlaintextTooLarge(17)),
    ] {
        let (frame, rx) = garbage_frame(kind, len);
        assert_eq!(
            open(&frame, &rx),
            Err(expected),
            "{kind:?} with {len} bytes"
        );
    }

    // The right lengths get as far as the AEAD.
    for (kind, len) in [
        (Kind::Welcome, 1),
        (Kind::Welcome, 65),
        (Kind::Welcome, 513),
        (Kind::DeviceAdded, 64),
        (Kind::DeviceRemoved, 16),
    ] {
        let (frame, rx) = garbage_frame(kind, len);
        assert_eq!(open(&frame, &rx), Err(Error::DecryptionFailed));
    }
    assert_eq!(
        garbage_frame(Kind::Welcome, 513).0.len(),
        HEADER_LEN + NONCE_LEN + 513 + TAG_LEN
    );
}

#[test]
fn group_envelope_kind_cannot_be_swapped() {
    // The kind is part of the associated data: a Device removed envelope
    // cannot be replayed as another kind, even one with a compatible length.
    let (a, b, a_keys, b_keys) = pair();
    let frame = seal(
        &header(&a, &b, Kind::DeviceRemoved),
        &encode_device_removed(&[4; 16]),
        &a_keys.tx,
    )
    .unwrap();
    let mut as_text = frame;
    as_text[1] = Kind::ClipboardText as u8;
    assert_eq!(open(&as_text, &b_keys.rx), Err(Error::DecryptionFailed));
}

// --- Randomness -----------------------------------------------------------

#[test]
fn random_helpers_return_fresh_values() {
    assert_ne!(random_message_id(), random_message_id());
    assert_ne!(random_pairing_id(), random_pairing_id());
    assert_ne!(random_pairing_secret(), random_pairing_secret());
    assert_ne!(random_message_id(), [0; 16]);
    assert_ne!(random_pairing_secret(), [0; 32]);
}

// --- Pairing code ---------------------------------------------------------

fn pairing_code(relay_url: &str) -> PairingCode {
    PairingCode {
        relay_url: relay_url.to_owned(),
        pairing_id: [0x11; 16],
        sign_pk: [0x22; 32],
        kx_pk: [0x33; 32],
        secret: [0x44; 32],
    }
}

const URL: &str = "wss://weft.frohub.eu/v1";

/// Standard base64url alphabet, to build inputs without going through the
/// code under test.
fn b64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .fold(0u32, |acc, &byte| (acc << 8) | u32::from(byte))
            << (8 * (3 - chunk.len()));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
        }
    }
    out
}

/// A code built from a raw payload.
fn code_from_payload(payload: &[u8]) -> String {
    format!("WEFT1:{}", b64url(payload))
}

fn payload(code: &PairingCode) -> Vec<u8> {
    let mut out = vec![code.relay_url.len() as u8];
    out.extend_from_slice(code.relay_url.as_bytes());
    out.extend_from_slice(&code.pairing_id);
    out.extend_from_slice(&code.sign_pk);
    out.extend_from_slice(&code.kx_pk);
    out.extend_from_slice(&code.secret);
    out
}

#[test]
fn pairing_code_round_trip() {
    // Url lengths covering the three base64 tail shapes, and both bounds.
    for url in [
        URL.to_owned(),
        "w".to_owned(),
        "ws".to_owned(),
        "wss".to_owned(),
        "wss://rélais.example/v1".to_owned(),
        "a".repeat(255),
    ] {
        let code = pairing_code(&url);
        let encoded = code.encode().unwrap();
        assert!(encoded.starts_with("WEFT1:"));
        assert!(
            encoded[6..]
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "not base64url without padding: {encoded}"
        );
        assert_eq!(encoded, code_from_payload(&payload(&code)));
        assert_eq!(PairingCode::decode(&encoded).unwrap(), code);
    }
}

#[test]
fn pairing_code_uses_the_url_safe_alphabet() {
    // 0xfb 0xff… produces '-' and '_' where standard base64 has '+' and '/'.
    let code = PairingCode {
        pairing_id: [0xfb; 16],
        sign_pk: [0xff; 32],
        ..pairing_code(URL)
    };
    let encoded = code.encode().unwrap();
    assert!(encoded.contains('-') && encoded.contains('_'));
    assert_eq!(PairingCode::decode(&encoded).unwrap(), code);

    // The same code in the standard alphabet is not accepted.
    let standard = encoded.replace('-', "+").replace('_', "/");
    assert_eq!(
        PairingCode::decode(&standard),
        Err(Error::MalformedPairingCode)
    );
}

#[test]
fn pairing_code_encode_rejects_bad_url_lengths() {
    assert_eq!(pairing_code("").encode(), Err(Error::RelayUrlLength(0)));
    assert_eq!(
        pairing_code(&"a".repeat(256)).encode(),
        Err(Error::RelayUrlLength(256))
    );
}

#[test]
fn pairing_code_decode_rejects_bad_prefix() {
    let encoded = pairing_code(URL).encode().unwrap();
    let body = &encoded[6..];
    for bad in [
        body.to_owned(),
        format!("WEFT2:{body}"),
        format!("weft1:{body}"),
        format!("WEFT1{body}"),
        format!(" WEFT1:{body}"),
        String::new(),
        "WEFT1:".to_owned(),
        "WEFT1".to_owned(),
    ] {
        assert_eq!(
            PairingCode::decode(&bad),
            Err(Error::MalformedPairingCode),
            "{bad:?}"
        );
    }
}

#[test]
fn pairing_code_decode_rejects_bad_base64() {
    // This url makes the payload length a multiple of 3: no base64 tail.
    let encoded = pairing_code("wss://relay.example/v1").encode().unwrap();
    assert_eq!((encoded.len() - 6) % 4, 0);
    assert!(PairingCode::decode(&encoded).is_ok());

    // Any character outside the alphabet, at any position.
    for bad_char in [
        '+', '/', '=', ' ', '\n', '\t', '.', '~', '%', '*', '\0', 'é',
    ] {
        for position in [6, encoded.len() / 2, encoded.len() - 1] {
            let mut bad = encoded.clone();
            bad.replace_range(position..=position, &bad_char.to_string());
            assert_eq!(
                PairingCode::decode(&bad),
                Err(Error::MalformedPairingCode),
                "{bad_char:?} at {position}"
            );
        }
    }

    // Trailing garbage and whitespace are not ignored.
    for suffix in ["=", "==", "\n", " ", "A", "AAAA"] {
        assert!(
            PairingCode::decode(&format!("{encoded}{suffix}")).is_err(),
            "suffix {suffix:?}"
        );
    }
}

#[test]
fn pairing_code_decode_rejects_padding() {
    // Urls of 3 and 2 bytes leave a 2- and 1-byte tail: padded base64 would
    // end with "=" or "==".
    for (url, padding) in [("wss", "="), ("ws", "==")] {
        let encoded = pairing_code(url).encode().unwrap();
        assert!(PairingCode::decode(&encoded).is_ok());
        assert_eq!((encoded.len() - 6 + padding.len()) % 4, 0);
        assert_eq!(
            PairingCode::decode(&format!("{encoded}{padding}")),
            Err(Error::MalformedPairingCode)
        );
    }
}

#[test]
fn pairing_code_decode_rejects_non_canonical_base64() {
    // With a 1-byte tail the last character only carries 2 bits; the 4 others
    // must be zero. Setting one of them still "decodes" to the same bytes in
    // a lax decoder, so two different strings would be the same code.
    let encoded = pairing_code("ws").encode().unwrap();
    let last = encoded.chars().last().unwrap();
    assert_eq!(last, 'A', "test expects a zero tail character");
    let mut non_canonical = encoded.clone();
    non_canonical.pop();
    non_canonical.push('B');
    assert_eq!(
        PairingCode::decode(&non_canonical),
        Err(Error::MalformedPairingCode)
    );

    // A single leftover character can never come out of an encoder.
    let whole = pairing_code("wss://relay.example/v1").encode().unwrap();
    assert_eq!(
        PairingCode::decode(&format!("{whole}A")),
        Err(Error::MalformedPairingCode)
    );
}

#[test]
fn pairing_code_decode_rejects_wrong_lengths() {
    let full = payload(&pairing_code(URL));
    assert!(PairingCode::decode(&code_from_payload(&full)).is_ok());

    for len in 1..full.len() {
        assert_eq!(
            PairingCode::decode(&code_from_payload(&full[..len])),
            Err(Error::MalformedPairingCode),
            "truncated to {len}"
        );
    }
    for extra in 1..=4 {
        let mut longer = full.clone();
        longer.extend_from_slice(&vec![0; extra]);
        assert_eq!(
            PairingCode::decode(&code_from_payload(&longer)),
            Err(Error::MalformedPairingCode),
            "{extra} extra bytes"
        );
    }

    // url_len that does not match the url actually present.
    for wrong_len in [URL.len() as u8 - 1, URL.len() as u8 + 1, 255] {
        let mut bad = full.clone();
        bad[0] = wrong_len;
        assert_eq!(
            PairingCode::decode(&code_from_payload(&bad)),
            Err(Error::MalformedPairingCode)
        );
    }

    // Far beyond the largest possible code.
    let huge = format!("WEFT1:{}", "A".repeat(1_000_000));
    assert_eq!(PairingCode::decode(&huge), Err(Error::MalformedPairingCode));
}

#[test]
fn pairing_code_decode_rejects_empty_url() {
    // Well-formed apart from url_len = 0.
    let mut empty_url = vec![0u8];
    empty_url.extend_from_slice(&[0x55; 112]);
    assert_eq!(
        PairingCode::decode(&code_from_payload(&empty_url)),
        Err(Error::RelayUrlLength(0))
    );
}

#[test]
fn pairing_code_decode_rejects_invalid_utf8_url() {
    let mut bad = payload(&pairing_code(URL));
    bad[3] = 0xff;
    assert_eq!(
        PairingCode::decode(&code_from_payload(&bad)),
        Err(Error::InvalidUtf8)
    );
}

#[test]
fn pairing_code_debug_hides_the_secret() {
    let code = PairingCode {
        secret: [0xab; 32],
        ..pairing_code(URL)
    };
    let debug = format!("{code:?}");
    assert!(debug.contains(URL));
    assert!(!debug.contains("171"), "{debug}"); // 0xab
}

// --- Pair request MAC -----------------------------------------------------

fn pair_request(secret: &[u8; 32]) -> PairRequest {
    PairRequest::new(secret, [0x11; 16], [0x22; 32], [0x33; 32])
}

#[test]
fn pair_request_mac_verifies() {
    let secret = random_pairing_secret();
    let request = pair_request(&secret);
    assert_eq!(
        request.mac,
        pair_request_mac(&secret, &[0x11; 16], &[0x22; 32], &[0x33; 32])
    );
    assert!(verify_pair_request(&secret, &request));

    // Still valid after a trip through the relay.
    let frame = ControlFrame::PairRequest(request).encode();
    let ControlFrame::PairRequest(received) = ControlFrame::decode(&frame).unwrap() else {
        panic!("not a pair request");
    };
    assert!(verify_pair_request(&secret, &received));
}

#[test]
fn pair_request_mac_rejects_any_flipped_bit() {
    let secret = [0x44; 32];
    let request = pair_request(&secret);
    for byte in 0..32 {
        for bit in 0..8 {
            let mut bad = request.clone();
            bad.mac[byte] ^= 1 << bit;
            assert!(
                !verify_pair_request(&secret, &bad),
                "mac accepted with bit {bit} of byte {byte} flipped"
            );
        }
    }
    let zero_mac = PairRequest {
        mac: [0; 32],
        ..request
    };
    assert!(!verify_pair_request(&secret, &zero_mac));
}

#[test]
fn pair_request_mac_rejects_wrong_secret() {
    let secret = [0x44; 32];
    let request = pair_request(&secret);
    let mut one_bit_off = secret;
    one_bit_off[31] ^= 0x80;
    assert!(!verify_pair_request(&one_bit_off, &request));
    assert!(!verify_pair_request(&[0; 32], &request));
    assert!(!verify_pair_request(&random_pairing_secret(), &request));
}

#[test]
fn pair_request_mac_covers_every_field() {
    let secret = [0x44; 32];
    let request = pair_request(&secret);

    // The relay swapping in its own keys, one at a time or both.
    let other_sign_pk = PairRequest {
        sign_pk: [0x99; 32],
        ..request.clone()
    };
    assert!(!verify_pair_request(&secret, &other_sign_pk));
    let other_kx_pk = PairRequest {
        kx_pk: [0x99; 32],
        ..request.clone()
    };
    assert!(!verify_pair_request(&secret, &other_kx_pk));

    // sign_pk and kx_pk exchanged.
    let swapped = PairRequest {
        sign_pk: request.kx_pk,
        kx_pk: request.sign_pk,
        ..request.clone()
    };
    assert!(!verify_pair_request(&secret, &swapped));
    assert_ne!(
        pair_request_mac(&secret, &[0x11; 16], &[0x22; 32], &[0x33; 32]),
        pair_request_mac(&secret, &[0x11; 16], &[0x33; 32], &[0x22; 32])
    );

    let other_pairing_id = PairRequest {
        pairing_id: [0x12; 16],
        ..request
    };
    assert!(!verify_pair_request(&secret, &other_pairing_id));
}

// --- Invitation MAC -------------------------------------------------------

fn invite_request(secret: &[u8; 32]) -> PairRequest {
    PairRequest::invite(secret, [0x11; 16], [0x22; 32], [0x33; 32])
}

#[test]
fn invite_request_mac_verifies() {
    let secret = random_pairing_secret();
    let request = invite_request(&secret);
    assert_eq!(
        request.mac,
        invite_request_mac(&secret, &[0x11; 16], &[0x22; 32], &[0x33; 32])
    );
    assert!(verify_invite_request(&secret, &request));

    // Still valid after a trip through the relay.
    let frame = ControlFrame::PairInvite(request).encode();
    let ControlFrame::PairInvite(received) = ControlFrame::decode(&frame).unwrap() else {
        panic!("not an invitation");
    };
    assert!(verify_invite_request(&secret, &received));
}

#[test]
fn join_and_invite_macs_never_verify_as_each_other() {
    let secret = [0x44; 32];
    let join = pair_request(&secret);
    let invite = invite_request(&secret);
    assert_ne!(join.mac, invite.mac);
    assert!(verify_pair_request(&secret, &join));
    assert!(verify_invite_request(&secret, &invite));
    assert!(!verify_invite_request(&secret, &join));
    assert!(!verify_pair_request(&secret, &invite));

    // The relay re-tagging a frame does not change its MAC: the slot owner
    // checks the MAC with the context of the tag it received.
    let mut retagged = ControlFrame::PairRequest(join.clone()).encode();
    retagged[0] = 0xB5;
    let ControlFrame::PairInvite(as_invite) = ControlFrame::decode(&retagged).unwrap() else {
        panic!("not an invitation");
    };
    assert!(!verify_invite_request(&secret, &as_invite));
    let mut retagged = ControlFrame::PairInvite(invite).encode();
    retagged[0] = 0xB1;
    let ControlFrame::PairRequest(as_join) = ControlFrame::decode(&retagged).unwrap() else {
        panic!("not a pair request");
    };
    assert!(!verify_pair_request(&secret, &as_join));
}

#[test]
fn invite_request_mac_rejects_any_flipped_bit() {
    let secret = [0x44; 32];
    let request = invite_request(&secret);
    for byte in 0..32 {
        for bit in 0..8 {
            let mut bad = request.clone();
            bad.mac[byte] ^= 1 << bit;
            assert!(
                !verify_invite_request(&secret, &bad),
                "mac accepted with bit {bit} of byte {byte} flipped"
            );
        }
    }
    // Any flipped bit in the covered fields too.
    for field in 0..3 {
        for byte in [0, 15] {
            let mut bad = request.clone();
            match field {
                0 => bad.pairing_id[byte] ^= 1,
                1 => bad.sign_pk[byte] ^= 1,
                _ => bad.kx_pk[byte] ^= 1,
            }
            assert!(!verify_invite_request(&secret, &bad));
        }
    }
}

#[test]
fn invite_request_mac_rejects_wrong_secret() {
    let secret = [0x44; 32];
    let request = invite_request(&secret);
    let mut one_bit_off = secret;
    one_bit_off[0] ^= 0x01;
    assert!(!verify_invite_request(&one_bit_off, &request));
    assert!(!verify_invite_request(&[0; 32], &request));
    assert!(!verify_invite_request(&random_pairing_secret(), &request));
}

// --- Control frames -------------------------------------------------------

fn control_frames() -> Vec<(ControlFrame, u8, usize)> {
    vec![
        (
            ControlFrame::PairOpen {
                pairing_id: [1; 16],
            },
            0xB0,
            17,
        ),
        (
            ControlFrame::PairRequest(pair_request(&[0x44; 32])),
            0xB1,
            113,
        ),
        (ControlFrame::GroupAdd { device_id: [2; 16] }, 0xB2, 17),
        (ControlFrame::GroupRemove { device_id: [3; 16] }, 0xB3, 17),
        (ControlFrame::GroupJoin { device_id: [4; 16] }, 0xB4, 17),
        (
            ControlFrame::PairInvite(invite_request(&[0x44; 32])),
            0xB5,
            113,
        ),
        (ControlFrame::Error(ControlError::UnknownPairing), 0xBE, 2),
        (ControlFrame::Error(ControlError::NotAllowed), 0xBE, 2),
        (ControlFrame::Error(ControlError::RateLimited), 0xBE, 2),
        (ControlFrame::Error(ControlError::GroupFull), 0xBE, 2),
    ]
}

#[test]
fn control_frames_round_trip() {
    for (frame, tag, len) in control_frames() {
        let bytes = frame.encode();
        assert_eq!(bytes[0], tag, "{frame:?}");
        assert_eq!(bytes.len(), len, "{frame:?}");
        assert_eq!(ControlFrame::decode(&bytes).unwrap(), frame);
        assert_eq!(classify(&bytes), FrameType::Control);
    }
}

#[test]
fn control_frame_layouts() {
    let mut expected = vec![0xB0];
    expected.extend_from_slice(&[1; 16]);
    assert_eq!(
        ControlFrame::PairOpen {
            pairing_id: [1; 16]
        }
        .encode(),
        expected
    );

    let request = PairRequest {
        pairing_id: [1; 16],
        sign_pk: [2; 32],
        kx_pk: [3; 32],
        mac: [4; 32],
    };
    let mut expected = vec![0xB1];
    expected.extend_from_slice(&[1; 16]);
    expected.extend_from_slice(&[2; 32]);
    expected.extend_from_slice(&[3; 32]);
    expected.extend_from_slice(&[4; 32]);
    assert_eq!(
        ControlFrame::PairRequest(request.clone()).encode(),
        expected
    );
    expected[0] = 0xB5;
    assert_eq!(ControlFrame::PairInvite(request).encode(), expected);

    let mut expected = vec![0xB4];
    expected.extend_from_slice(&[5; 16]);
    assert_eq!(
        ControlFrame::GroupJoin { device_id: [5; 16] }.encode(),
        expected
    );

    for (code, byte) in [
        (ControlError::UnknownPairing, 0x01),
        (ControlError::NotAllowed, 0x02),
        (ControlError::RateLimited, 0x03),
        (ControlError::GroupFull, 0x04),
    ] {
        assert_eq!(ControlFrame::Error(code).encode(), [0xBE, byte]);
    }
}

#[test]
fn control_frame_decode_rejects_wrong_lengths() {
    assert_eq!(ControlFrame::decode(&[]), Err(Error::MalformedControlFrame));
    for (frame, _, _) in control_frames() {
        let bytes = frame.encode();
        for len in 1..bytes.len() {
            assert_eq!(
                ControlFrame::decode(&bytes[..len]),
                Err(Error::MalformedControlFrame),
                "{frame:?} truncated to {len}"
            );
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(
            ControlFrame::decode(&longer),
            Err(Error::MalformedControlFrame),
            "{frame:?} with an extra byte"
        );
    }
}

#[test]
fn control_frame_decode_rejects_unknown_control_bytes() {
    for tag in 0xB6..=0xBF_u8 {
        if tag == 0xBE {
            continue;
        }
        // Whatever the body length.
        for body_len in [0, 1, 16, 112] {
            let mut frame = vec![tag];
            frame.extend_from_slice(&vec![0; body_len]);
            assert_eq!(
                ControlFrame::decode(&frame),
                Err(Error::UnknownControlFrame(tag))
            );
        }
    }
}

#[test]
fn control_frame_decode_rejects_other_frame_types() {
    for tag in [0x00, 0x01, 0xA0, 0xA1, 0xA2, 0xAF, 0xC0, 0xFF] {
        let mut frame = vec![tag];
        frame.extend_from_slice(&[0; 16]);
        assert_eq!(
            ControlFrame::decode(&frame),
            Err(Error::UnexpectedFrameType(tag))
        );
    }
}

#[test]
fn control_frame_decode_rejects_unknown_error_codes() {
    for code in (0x00..=0xFF_u8).filter(|code| !(0x01..=0x04).contains(code)) {
        assert_eq!(
            ControlFrame::decode(&[0xBE, code]),
            Err(Error::UnknownControlError(code))
        );
    }
}

// --- Frame dispatch -------------------------------------------------------

#[test]
fn classify_by_first_byte() {
    assert_eq!(classify(&[]), FrameType::Unknown);
    for byte in 0x00..=0xFF_u8 {
        let expected = match byte {
            0x01 => FrameType::Envelope,
            0xB0..=0xBF => FrameType::Control,
            _ => FrameType::Unknown,
        };
        assert_eq!(classify(&[byte]), expected, "{byte:#04x}");
        assert_eq!(classify(&[byte, 0, 0]), expected, "{byte:#04x}");
    }
}

#[test]
fn classify_real_frames() {
    let (a, b, a_keys, _) = pair();
    let envelope = seal(&header(&a, &b, Kind::Ping), b"", &a_keys.tx).unwrap();
    assert_eq!(classify(&envelope), FrameType::Envelope);

    // Authentication frames are not expected after the handshake.
    let challenge = Challenge::new("weft.frohub.eu", [9; 32]).unwrap();
    assert_eq!(classify(&challenge.encode()), FrameType::Unknown);
    assert_eq!(
        classify(&[weft_protocol::AUTH_ACCEPTED]),
        FrameType::Unknown
    );
}

// --- Client host check ----------------------------------------------------

#[test]
fn check_relay_name_requires_an_exact_match() {
    let challenge = Challenge::new("weft.frohub.eu", [9; 32]).unwrap();
    assert_eq!(check_relay_name(&challenge, "weft.frohub.eu"), Ok(()));

    for other in [
        "",
        "evil.example",
        "weft.frohub.eu:443",
        "weft.frohub.eu.",
        "weft.frohub.eu.evil.example",
        "evil.weft.frohub.eu",
        "WEFT.frohub.eu",
        " weft.frohub.eu",
        "weft.frohub.e",
        "wss://weft.frohub.eu",
    ] {
        assert_eq!(
            check_relay_name(&challenge, other),
            Err(Error::RelayNameMismatch),
            "{other:?}"
        );
    }

    let empty = Challenge::new("", [9; 32]).unwrap();
    assert_eq!(
        check_relay_name(&empty, "weft.frohub.eu"),
        Err(Error::RelayNameMismatch)
    );
}
