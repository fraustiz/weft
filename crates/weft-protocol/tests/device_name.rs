//! Device name envelopes (kind `0x06`): length, UTF-8 and control
//! character rules, in `encode_device_name` / `decode_device_name` and in
//! `seal` / `open`.

use weft_protocol::{
    Error, HEADER_LEN, Header, Identity, Kind, MAX_DEVICE_NAME_LEN, NONCE_LEN, SessionKeys,
    TAG_LEN, VERSION, decode_device_name, encode_device_name, open, seal, session_keys,
};

fn pair() -> (Identity, Identity, SessionKeys, SessionKeys) {
    let a = Identity::from_seeds([1; 32], [2; 32]);
    let b = Identity::from_seeds([3; 32], [4; 32]);
    let a_keys = session_keys(&a, &b.device_id(), &b.kx_pk()).unwrap();
    let b_keys = session_keys(&b, &a.device_id(), &a.kx_pk()).unwrap();
    (a, b, a_keys, b_keys)
}

fn header(from: &Identity, to: &Identity) -> Header {
    Header {
        version: VERSION,
        kind: Kind::DeviceName,
        sender: from.device_id(),
        recipient: to.device_id(),
        message_id: [7; 16],
        counter: 1,
    }
}

/// Checks a name through all four entry points: encode, decode, seal and
/// (when sealing works) open.
fn check(name: &[u8], expected: Result<(), Error>) {
    let (a, b, a_keys, b_keys) = pair();
    let sealed = seal(&header(&a, &b), name, &a_keys.tx);
    let decoded = decode_device_name(name);
    match &expected {
        Ok(()) => {
            let text = std::str::from_utf8(name).unwrap();
            assert_eq!(encode_device_name(text).unwrap(), name);
            assert_eq!(decoded.unwrap(), text);
            let (header, plaintext) = open(&sealed.unwrap(), &b_keys.rx).unwrap();
            assert_eq!(header.kind, Kind::DeviceName);
            assert_eq!(plaintext, name);
        }
        Err(err) => {
            if let Ok(text) = std::str::from_utf8(name) {
                assert_eq!(encode_device_name(text).as_ref(), Err(err), "{name:?}");
            }
            assert_eq!(decoded.as_ref(), Err(err), "{name:?}");
            assert_eq!(sealed.as_ref(), Err(err), "{name:?}");
        }
    }
}

#[test]
fn kind_byte_is_0x06() {
    let (a, b, _, _) = pair();
    let bytes = header(&a, &b).encode();
    assert_eq!(bytes[1], 0x06);
    assert_eq!(Header::decode(&bytes).unwrap().kind, Kind::DeviceName);
}

#[test]
fn valid_names() {
    assert_eq!(MAX_DEVICE_NAME_LEN, 64);
    check(b"x", Ok(()));
    check("Téléphone de test".as_bytes(), Ok(()));
    check("MacBook Air ✂".as_bytes(), Ok(()));
    check(&[b'a'; 64], Ok(()));
    // 32 two-byte characters: exactly 64 bytes.
    check("é".repeat(32).as_bytes(), Ok(()));
    check(b"  spaces are fine  ", Ok(()));
}

#[test]
fn length_is_1_to_64_bytes() {
    check(b"", Err(Error::DeviceNameLength(0)));
    check(&[b'a'; 65], Err(Error::DeviceNameLength(65)));
    // 64 bytes of 'a' and one 2-byte character: 66 bytes, 65 characters.
    let long = format!("{}é", "a".repeat(64));
    check(long.as_bytes(), Err(Error::DeviceNameLength(66)));
    check(&[b'a'; 1000], Err(Error::DeviceNameLength(1000)));
}

#[test]
fn control_characters_are_rejected_at_each_boundary() {
    for c in [
        '\u{0}', '\u{1}', '\n', '\u{1f}', '\u{7f}', '\u{80}', '\u{85}', '\u{9f}',
    ] {
        let name = format!("ab{c}cd");
        check(name.as_bytes(), Err(Error::DeviceNameControlCharacter));
        // Alone, first or last.
        check(
            c.to_string().as_bytes(),
            Err(Error::DeviceNameControlCharacter),
        );
        check(
            format!("{c}x").as_bytes(),
            Err(Error::DeviceNameControlCharacter),
        );
        check(
            format!("x{c}").as_bytes(),
            Err(Error::DeviceNameControlCharacter),
        );
    }
    // Just outside the two ranges.
    for c in [' ', '~', '\u{a0}', '\u{ff}'] {
        check(format!("ab{c}cd").as_bytes(), Ok(()));
    }
}

#[test]
fn invalid_utf8_is_rejected() {
    for name in [
        &b"\xff"[..],
        &b"abc\xc3"[..],
        &b"\xc3\x28"[..],
        &b"\xed\xa0\x80"[..], // UTF-16 surrogate
        &b"\xc0\xaf"[..],     // overlong encoding
    ] {
        check(name, Err(Error::InvalidUtf8));
    }
}

/// A frame with a valid Device name header and `plaintext_len` bytes of
/// garbage as the ciphertext.
fn garbage_frame(plaintext_len: usize) -> (Vec<u8>, [u8; 32]) {
    let (a, b, _, b_keys) = pair();
    let mut frame = header(&a, &b).encode().to_vec();
    frame.extend_from_slice(&[0; NONCE_LEN]);
    frame.extend_from_slice(&vec![0; plaintext_len + TAG_LEN]);
    (frame, b_keys.rx)
}

#[test]
fn bad_lengths_are_rejected_before_decrypting() {
    for len in [0, 65, 100, 256 * 1024] {
        let (frame, rx) = garbage_frame(len);
        assert_eq!(
            open(&frame, &rx),
            Err(Error::DeviceNameLength(len)),
            "{len} bytes"
        );
    }
    // Lengths 1 and 64 get as far as the AEAD.
    for len in [1, 64] {
        let (frame, rx) = garbage_frame(len);
        assert_eq!(open(&frame, &rx), Err(Error::DecryptionFailed));
    }
    assert_eq!(
        garbage_frame(64).0.len(),
        HEADER_LEN + NONCE_LEN + 64 + TAG_LEN
    );
}

#[test]
fn kind_cannot_be_swapped_with_clipboard_text() {
    // A name and a clipboard text share the same plaintext rules up to
    // 64 bytes, but the kind is authenticated.
    let (a, b, a_keys, b_keys) = pair();
    let mut frame = seal(&header(&a, &b), b"Laptop", &a_keys.tx).unwrap();
    frame[1] = Kind::ClipboardText as u8;
    assert_eq!(open(&frame, &b_keys.rx), Err(Error::DecryptionFailed));
}
