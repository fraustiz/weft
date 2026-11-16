//! Packet (kind `0x07`) and file chunk (kind `0x08`) envelopes, and the
//! kind-agnostic header the relay routes on (`docs/features.md`, section 1).

use weft_protocol::{
    Error, FileChunk, HEADER_LEN, Header, Identity, Kind, MAX_FILE_CHUNK_DATA_LEN, MAX_PACKET_LEN,
    NONCE_LEN, RouteHeader, SessionKeys, TAG_LEN, VERSION, decode_file_chunk, encode_file_chunk,
    open, seal, session_keys,
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

/// Seals `plaintext` as `kind` from A to B, and opens it again when sealing
/// works.
fn round_trip(kind: Kind, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
    let (a, b, a_keys, b_keys) = pair();
    let frame = seal(&header(&a, &b, kind), plaintext, &a_keys.tx)?;
    let (opened, plaintext) = open(&frame, &b_keys.rx)?;
    assert_eq!(opened.kind, kind);
    Ok(plaintext)
}

/// A frame of `kind` with `len` bytes of garbage as the ciphertext body, to
/// check what `open` rejects before decrypting.
fn garbage_frame(kind: Kind, len: usize) -> Vec<u8> {
    let (a, b, _, _) = pair();
    let mut frame = header(&a, &b, kind).encode().to_vec();
    frame.resize(HEADER_LEN + NONCE_LEN + len + TAG_LEN, 0);
    frame
}

#[test]
fn kind_bytes() {
    let (a, b, _, _) = pair();
    for (kind, byte) in [(Kind::Packet, 0x07), (Kind::FileChunk, 0x08)] {
        let bytes = header(&a, &b, kind).encode();
        assert_eq!(bytes[1], byte);
        assert_eq!(Header::decode(&bytes).unwrap().kind, kind);
    }
}

#[test]
fn packet_is_1_to_256_kib_of_utf8() {
    let json = r#"{"type":"ping","message":"héllo"}"#;
    assert_eq!(
        round_trip(Kind::Packet, json.as_bytes()).unwrap(),
        json.as_bytes()
    );
    let max = vec![b'a'; MAX_PACKET_LEN];
    assert_eq!(round_trip(Kind::Packet, &max).unwrap(), max);

    assert_eq!(
        round_trip(Kind::Packet, b""),
        Err(Error::InvalidPlaintextLength(0))
    );
    assert_eq!(
        round_trip(Kind::Packet, &vec![b'a'; MAX_PACKET_LEN + 1]),
        Err(Error::PlaintextTooLarge(MAX_PACKET_LEN + 1))
    );
    assert_eq!(
        round_trip(Kind::Packet, b"{\"a\":\"\xff\"}"),
        Err(Error::InvalidUtf8)
    );
    // The JSON itself is not checked at this layer.
    assert!(round_trip(Kind::Packet, b"not json").is_ok());
}

#[test]
fn open_rejects_packet_lengths_before_decrypting() {
    let (_, _, _, b_keys) = pair();
    for (len, expected) in [
        (0, Error::InvalidPlaintextLength(0)),
        (
            MAX_PACKET_LEN + 1,
            Error::PlaintextTooLarge(MAX_PACKET_LEN + 1),
        ),
        (1, Error::DecryptionFailed),
        (MAX_PACKET_LEN, Error::DecryptionFailed),
    ] {
        assert_eq!(
            open(&garbage_frame(Kind::Packet, len), &b_keys.rx),
            Err(expected),
            "{len} bytes"
        );
    }
}

#[test]
fn file_chunk_round_trip() {
    let transfer_id = [0x42; 16];
    let plaintext = encode_file_chunk(&transfer_id, 0x0102_0304_0506_0708, b"data").unwrap();
    assert_eq!(&plaintext[..16], &transfer_id);
    assert_eq!(&plaintext[16..24], &[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(&plaintext[24..], b"data");
    let opened = round_trip(Kind::FileChunk, &plaintext).unwrap();
    assert_eq!(
        decode_file_chunk(&opened).unwrap(),
        FileChunk {
            transfer_id,
            offset: 0x0102_0304_0506_0708,
            data: b"data".to_vec(),
        }
    );
}

#[test]
fn file_chunk_data_is_1_to_128_kib() {
    assert_eq!(MAX_FILE_CHUNK_DATA_LEN, 131072);
    let id = [1; 16];
    let max = vec![0xab; MAX_FILE_CHUNK_DATA_LEN];
    let plaintext = encode_file_chunk(&id, u64::MAX, &max).unwrap();
    assert_eq!(plaintext.len(), 24 + MAX_FILE_CHUNK_DATA_LEN);
    assert_eq!(decode_file_chunk(&plaintext).unwrap().data, max);
    assert!(round_trip(Kind::FileChunk, &plaintext).is_ok());
    assert!(encode_file_chunk(&id, 0, &[0]).is_ok());

    assert_eq!(
        encode_file_chunk(&id, 0, b""),
        Err(Error::InvalidPlaintextLength(24))
    );
    let too_large = 24 + MAX_FILE_CHUNK_DATA_LEN + 1;
    assert_eq!(
        encode_file_chunk(&id, 0, &vec![0; MAX_FILE_CHUNK_DATA_LEN + 1]),
        Err(Error::PlaintextTooLarge(too_large))
    );
    for len in [0, 1, 23, 24] {
        assert_eq!(
            decode_file_chunk(&vec![0; len]),
            Err(Error::InvalidPlaintextLength(len))
        );
        assert_eq!(
            round_trip(Kind::FileChunk, &vec![0; len]),
            Err(Error::InvalidPlaintextLength(len))
        );
    }
    assert_eq!(
        decode_file_chunk(&vec![0; too_large]),
        Err(Error::PlaintextTooLarge(too_large))
    );
    let (_, _, _, b_keys) = pair();
    assert_eq!(
        open(&garbage_frame(Kind::FileChunk, too_large), &b_keys.rx),
        Err(Error::PlaintextTooLarge(too_large))
    );
}

#[test]
fn route_header_ignores_kind() {
    let (a, b, a_keys, _) = pair();
    let mut frame = seal(&header(&a, &b, Kind::Ping), b"", &a_keys.tx).unwrap();
    for kind in [0x01, 0x07, 0x08, 0x7f, 0x00, 0xff] {
        frame[1] = kind;
        assert_eq!(
            RouteHeader::decode(&frame).unwrap(),
            RouteHeader {
                kind,
                sender: a.device_id(),
                recipient: b.device_id(),
            }
        );
    }
}

#[test]
fn route_header_checks_version_and_length() {
    let (a, b, a_keys, _) = pair();
    let frame = seal(&header(&a, &b, Kind::Ping), b"", &a_keys.tx).unwrap();
    assert_eq!(frame.len(), HEADER_LEN + NONCE_LEN + TAG_LEN);
    assert_eq!(
        RouteHeader::decode(&frame[..frame.len() - 1]),
        Err(Error::FrameTooShort(frame.len() - 1))
    );
    assert_eq!(RouteHeader::decode(&[]), Err(Error::FrameTooShort(0)));
    let mut bad_version = frame;
    bad_version[0] = 0x02;
    assert_eq!(
        RouteHeader::decode(&bad_version),
        Err(Error::UnsupportedVersion(2))
    );
}
