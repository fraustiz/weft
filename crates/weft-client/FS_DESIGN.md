# Ephemeral-session negotiation: decision and remaining work

Status: design prerequisite, **not implemented or enabled**. The current
client must not advertise forward secrecy or automatically enable the
experimental `session.rekey` packets. Static-key v1 operation remains available
to existing clients, with its existing security limitations.

## Why enabling the current packets is unsafe

`SessionRekeyAck` identifies only an integer epoch. It does not bind the
initiator's proposal, the responder's contribution, or either connection
generation. Replacing staged keys while an old acknowledgement is in flight
can activate different keys on the two peers.

All packets currently share one live envelope kind. Once an epoch is active,
that kind stops accepting static-key ciphertext after the grace period. A
restarted peer consequently cannot deliver capabilities or negotiate recovery.
Accepting arbitrary static packets again would also let commands bypass the
session's encryption policy.

Session epoch state is intentionally memory-only. On restart the current
sender falls back to static keys. Without a persistent per-peer encryption
policy, automatic activation would therefore silently downgrade on every
restart. Keeping an old receive key forever is not a solution: it weakens
erasure and conceals failure to negotiate.

## Required protocol revision

The implementation needs a reviewed wire revision before activation, with:

- A distinct static-key authenticated control channel for capability hello,
  proposal, response and key confirmation only. No clipboard content, user
  commands, transfers, or arbitrary feature packets may use that channel.
  A control packet disguised as an ordinary feature packet is rejected.
- A generation identifier for each process/session, a fresh proposal identifier,
  both ephemeral public keys, both device identities, and the negotiated
  protocol/cipher-suite version bound into the handshake transcript and key
  confirmation. An epoch number alone cannot identify a handshake.
- Explicit epoch identification authenticated with each live envelope. The
  receiver must not guess an unbounded history of keys. Legacy wire framing
  and new framing must be independently negotiated and validated.
- A persistent per-trusted-peer policy: legacy-compatible before an upgrade,
  encryption-required after both sides confirm support. Persist the policy
  before sending the first upgraded live message. A capability omission or
  restart cannot undo it. Removal/re-pairing is an explicit trust reset.
- A standard, reviewed authenticated key-exchange construction and compatible
  vectors for Rust, Kotlin and Swift. The present KDF unit tests prove matching
  directional keys, not security of a completed handshake or ratchet.

These are changes to the protocol contract, not just a worker timer. An
unused envelope-kind number must not be assigned independently in each app.
The relay's version and length checks, client framing, and all language
implementations need to agree before releasing the revision.

## Session-state requirements

Use one deterministic initiator per device pair. A new proposal never deletes
the current active transmit key. Keep at most one pending exchange and a
bounded previous receive epoch, erasing discarded secrets. A repeated proposal
returns the same pending response, rather than generating different keys.

Acknowledgements and confirmations must identify the exact transcript. Ignore
stale, mismatched or duplicate transitions without replacing active keys.
Retransmit control messages with bounded backoff; on expiry retain the current
secure epoch or report that encryption is unavailable. Never resume static
live traffic for a peer whose policy requires upgraded encryption.

After a restart, live sends wait for authenticated recovery or fail explicitly;
they must not pass through `seal_device_envelope`'s static fallback. Design this
at the `Session` layer as well as the worker: the CLI can open the same state
directory and call send methods without a worker. An implementation solely in
the worker leaves this downgrade path open.

Keep v1 offline group-change and device-name delivery on the existing
authenticated static path. Do not re-encrypt or discard already queued
envelopes. File transfer should resume through acknowledgements after recovery,
without persisting ephemeral secrets. Rotation must work identically over LAN
and relay; fallback reuses one sealed frame and one counter.

Replay protection remains persistent and shared between channels. Reserve a
counter before sending any control or live frame. New-generation controls need
a persisted freshness rule in addition to the sliding replay window: a delayed,
previously unseen hello inside that window must not roll back the generation.

## Validation before enabling

Pure state-machine tests must cover mutual support and legacy compatibility,
simultaneous proposals, duplicate/reordered messages, lost proposal/response/
acknowledgement/confirmation, rotation timeout, peer removal, and stale-generation
controls. Assert that every terminal path erases abandoned key material.

CI transport tests must cover restarting either peer and both peers, a crash
between policy persistence and key activation, live messages during each
transition, offline queued group updates, file resumption, and replayed frames
crossing LAN/relay. Verify that an encryption-required peer never emits a
static live frame during any recovery or failure path. Run cross-language
vectors and interoperability tests before claiming forward secrecy.

No local network tests should run on the current Windows host: they repeatedly
triggered firewall prompts. Pure crypto checks and compilation are safe here;
transport and restart tests belong in CI.
