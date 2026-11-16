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
| `0x03` | Welcome (section 5) | `count (1 byte, 0..8) || count × (sign_pk 32 || kx_pk 32)` |
| `0x04` | Device added (section 5) | `sign_pk (32) || kx_pk (32)` |
| `0x05` | Device removed (section 5) | `device_id (16)` |

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

After `0xA2`, the first byte of every frame tells what it is: `0x01` is an envelope (section 3), `0xB0`–`0xBF` are control frames (section 5). The relay drops anything else.

For envelopes, the relay:
- checks that the `sender` field equals the authenticated device id, and drops the frame otherwise;
- forwards the frame unchanged only if `sender` and `recipient` are in the **same group** (section 5) and the recipient is connected;
- never decrypts anything.

The client checks that the `relay_name` in the challenge equals the host it connected to (without port) before signing.

## 5. Groups and pairing

### Principles
- A **group** is the set of devices of one person, at most 10. A device belongs to at most one group.
- **Each device keeps its own list of trusted devices** (sign_pk, kx_pk) and only ever encrypts to that list. The relay's copy of the group is used for routing and abuse control only: it can refuse to deliver, never add a device anyone will trust.
- Trust changes travel as end-to-end encrypted envelopes between members (kinds `0x03`–`0x05`), so they are authenticated by the AEAD session keys.

### Pairing code (QR)
Device A (already in a group, or alone) shows a QR code with the string `WEFT1:` followed by the base64url (no padding) encoding of:

```
url_len (1) || relay_url (UTF-8, e.g. "wss://weft.frohub.eu/v1") || pairing_id (16) || sign_pk_A (32) || kx_pk_A (32) || secret (32)
```

`pairing_id` and `secret` are random. The **secret never goes through the relay**: it only travels through the QR code, from screen to camera. The code is single-use and expires after 5 minutes.

### Control frames

| Byte | Direction | Body | Meaning |
| --- | --- | --- | --- |
| `0xB0` | device → relay | `pairing_id (16)` | A opens a pairing slot (replaces any open slot of A; expires after 5 min) |
| `0xB1` | B → relay → A | `pairing_id (16) || sign_pk_B (32) || kx_pk_B (32) || mac (32)` | B asks to join; the relay checks that `sign_pk_B` is the key B authenticated with, forwards the frame unchanged to the slot owner and closes the slot |
| `0xB2` | A → relay | `device_id (16)` | A confirms B: the relay puts B in A's group (creating it if A had none; B leaves its previous group) |
| `0xB3` | device → relay | `device_id (16)` | Removes that device (itself or another member) from the relay group |
| `0xBE` | relay → device | `code (1)` | Error: `0x01` unknown or expired pairing, `0x02` not allowed, `0x03` rate limited, `0x04` group full |

The relay accepts `0xB2` only for a device that sent a matching `0xB1` to this slot owner in the last 5 minutes; otherwise it answers `0xBE 0x02`.

`mac = crypto_generichash(outlen = 32, input = "weft-pair-v1" || pairing_id || sign_pk_B || kx_pk_B, key = secret)` (keyed BLAKE2b). Without the secret, the relay cannot produce a valid request, so it cannot slip its own key in.

### Flow
1. A opens a slot (`0xB0`) and shows the QR code.
2. B scans it, now trusts A's keys from the code, connects, authenticates and sends `0xB1`.
3. A checks the MAC (constant-time). If it is wrong, A ignores the request. Otherwise A adds B to its trusted list and sends `0xB2`.
4. A sends B a **Welcome** envelope (`0x03`) listing the other members (not A, not B), and sends each other member a **Device added** envelope (`0x04`) with B's keys. Each recipient adds the keys to its trusted list.
5. Removing a device: any member sends `0xB3` to the relay and a **Device removed** envelope (`0x05`) to the remaining members, which drop that device from their list.

Offline delivery comes in a later revision of this document.

## 6. Test vectors

`crates/weft-protocol/vectors/v1.json` contains fixed inputs (seeds, nonces, message ids) and the expected outputs:
- keys and device ids;
- session keys for both roles;
- the encoded envelope;
- the authentication signature;
- a pairing code, a pairing request MAC and the control frames.

Keys are derived from fixed seeds:
- signing pair: `crypto_sign_seed_keypair` (32-byte seed) ;
- kx pair: `crypto_kx_seed_keypair` (32-byte seed).

Production code always uses random keys; seeds only exist to make the vectors reproducible.
