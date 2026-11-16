# Weft wire protocol — v1 (draft)

This document is the contract between the relay and the three clients (Kotlin, Swift, Rust). Every client must reproduce `crates/weft-protocol/vectors/v1.json` byte for byte.

All cryptography is **libsodium**. Byte strings are written in lowercase hex in this document and in the vectors file. Integers are big-endian.

## 1. Identity

Each install generates two key pairs:

| Key pair | libsodium | Use |
| --- | --- | --- |
| Signing | `crypto_sign` (Ed25519) | Authenticates the device to the relay and signs group changes |
| Key exchange | `crypto_kx` (X25519) | Derives session keys with each peer |

**Device id** — 16 bytes: `crypto_generichash(outlen = 16, input = sign_pk, key = none)` (BLAKE2b).

## 2. Session keys between two devices

For a pair of devices A and B, with kx public keys `kx_pk_A` and `kx_pk_B`:

- The device whose **device id is lower** (bytewise comparison) plays the libsodium *client* role. The other plays the *server* role.
- Client: `crypto_kx_client_session_keys(client_kx_pk, client_kx_sk, server_kx_pk)` → `(rx, tx)`.
- Server: `crypto_kx_server_session_keys(server_kx_pk, server_kx_sk, client_kx_pk)` → `(rx, tx)`.
- A device encrypts with its `tx` and decrypts with its `rx`. The client's `tx` equals the server's `rx`, and the other way round.

## 3. Message envelope

```
header (58 bytes) || nonce (24 bytes) || ciphertext (plaintext length + 16 bytes)
```

Header layout:

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 1 | `version` = `0x01` |
| 1 | 1 | `kind` (see below) |
| 2 | 16 | `sender` device id |
| 18 | 16 | `recipient` device id |
| 34 | 16 | `message_id`, random |
| 50 | 8 | `counter`, u64, per (sender, recipient) pair, starts at 1 (receivers treat 0 as invalid) |

- Encryption: `crypto_aead_xchacha20poly1305_ietf_encrypt(plaintext, ad = header, nonce, key = sender's tx)`.
- The **whole header is the associated data**: the relay can read it to route the message, but cannot change any field without the recipient rejecting the message.
- The nonce is 24 random bytes (`randombytes_buf`).

`kind` values:

| Value | Meaning | Plaintext |
| --- | --- | --- |
| `0x01` | Clipboard text | UTF-8 text, at most 256 KiB |
| `0x02` | Ping | empty (exactly 0 bytes) |

Other values are reserved. A recipient drops a message with an unknown `version` or `kind`.

**Replay protection (receiver side):**
- A receiver keeps, per sender, the highest `counter` accepted and the `message_id`s seen in a sliding window of the last 1024 counters, on disk.
- It drops a message whose `message_id` was already seen, or whose `counter` is older than the window.

## 4. Relay authentication

Transport: WebSocket over TLS. Each binary WebSocket frame carries one message.

1. On connect, the relay sends a challenge frame:
   `0xA0 || relay_name_len (1 byte) || relay_name (UTF-8) || nonce (32 bytes)`.
2. The device answers with:
   `0xA1 || sign_pk (32 bytes) || signature (64 bytes)`.
   The signature is `crypto_sign_detached(message = "weft-auth-v1" || relay_name_len || relay_name || nonce, sign_sk)`.
3. Both auth frames have an exact length; a frame with missing or extra bytes is rejected. The relay checks the signature against the name it actually serves, derives the device id from `sign_pk`, and answers `0xA2` (accepted) or closes the connection.

After `0xA2`, every frame from the device is an envelope (section 3). The relay:
- checks that the `sender` field equals the authenticated device id, and drops the frame otherwise;
- forwards the frame unchanged to the `recipient` if it is connected;
- never decrypts anything.

Groups, pairing and offline delivery come in a later revision of this document.

## 5. Test vectors

`crates/weft-protocol/vectors/v1.json` contains fixed inputs (seeds, nonces, message ids) and the expected outputs:
- keys and device ids;
- session keys for both roles;
- the encoded envelope;
- the authentication signature.

Keys are derived from fixed seeds:
- signing pair: `crypto_sign_seed_keypair` (32-byte seed) ;
- kx pair: `crypto_kx_seed_keypair` (32-byte seed).

Production code always uses random keys; seeds only exist to make the vectors reproducible.
