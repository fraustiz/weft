# Clipboard image wire contract

Image support is negotiated with `clipboard.image.offer` and
`clipboard.image.accept` capabilities. Text-only clients keep their existing
behavior. This extension uses the existing encrypted packet (`0x07`) and file
chunk (`0x08`) envelopes and the same per-peer replay protection.

The sender offers a fresh transfer ID (16 bytes, 32 lowercase hex characters):

```json
{"type":"clipboard.image.offer","id":"0123456789abcdef0123456789abcdef","mime":"image/png","size":12345,"hash":"0123456789abcdef0123456789abcdef"}
```

`hash` is the 16-byte BLAKE2b digest of the encoded image bytes, in lowercase
hex. It is BLAKE2b configured for a 16-byte output, not a truncated 64-byte
digest. Supported MIME types are `image/png` and `image/jpeg`; payload size is
positive and at most 16 MiB. Receivers validate both the declared MIME and
payload signature before applying it. Native decoders must bound decoded
dimensions independently of compressed size.

The receiver accepts with `{"type":"clipboard.image.accept","id":"…"}`.
The sender then uses existing file chunks: transfer ID, byte offset and bytes.
Offsets must be contiguous; chunk payloads are at most the existing protocol
chunk limit (128 KiB). Receivers acknowledge progress with `file.ack` and send
`file.done` only after the complete encoded payload matches the advertised
size, hash and type. Any corruption or invalid offset cancels the transfer
with `file.cancel`. Senders must handle acknowledgements: some implementations
limit the number of chunks in flight.

Each peer may have at most one incoming clipboard image. The Rust receiver
reserves at most 32 MiB across the group, accounting for the full sizes of
offers rather than bytes received so far. Unfinished image buffers expire;
they are cleared on relay reconnection and never resumed as stale clipboard
content. Image data and offers are not persisted as clipboard history.

Global/directional clipboard pause and native sensitive-content exclusions
apply to images as to text. Native writes must suppress echoes, including when
the platform converts JPEG to PNG. Android sends only through an explicit
foreground clipboard action or share action; this feature does not enable
background clipboard reads.
