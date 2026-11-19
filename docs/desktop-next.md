# Desktop development status

This branch adds clipboard images to the shared Rust worker and the Windows
and Linux adapters. Images use offer/accept negotiation, bounded memory,
integrity checks, acknowledgements, and the existing encrypted envelope and
replay protection. Windows also accepts shell file drops and has a Paste files
button. Ctrl+V file paste is limited to known noneditable controls in the
focused Weft window; changing focus cancels pending shortcut handling. Native
text paste is preserved. On a Windows desktop, file paste from a button and
the navigation bar, the Paste files button, and an Explorer drop onto a
device page were verified end to end against a local relay. A text field
still accepts text paste and does not send files.

The Linux client is a daemon and CLI using the desktop's installed utilities;
see `linux/README.md` for dependencies. Its adapter tests use fake utilities;
they do not establish that a real Wayland or X11 desktop works.

Authenticated LAN transport (`weft-lan-v3`, see `docs/protocol.md`, section 7)
is under validation. Receipts carry a MAC under a per-connection, per-direction
key derived from the pairwise session keys, so an on-path attacker cannot
confirm a frame it dropped. Falling back to the relay reuses the same sealed
envelope so replay protection still applies. A `weft-lan-v2` peer fails the
handshake and uses the relay.
Android now shares this authenticated transport, using numeric private IPv4
endpoints announced through the relay. Routes are opportunistic: reconnects
and later endpoint announcements retry them; there is no periodic redial.
Android/Rust interoperability, sleep and network changes need device tests.

Ephemeral key derivation and session epoch primitives are experimental and
are **not enabled automatically**. This branch does not claim forward secrecy:
restart recovery and control-channel negotiation still need implementation
and end-to-end tests before enabling key rotation.

Android image support adds an explicit clipboard action and a separate image
clipboard share target; ordinary file sharing keeps its existing meaning.
See `clipboard-images.md` for the wire contract. Android interoperability on a
real device remains to be validated. The macOS adapter now supports PNG/JPEG
and converts local TIFF images, with bounded decoding and pixel fingerprints
to suppress echoes. Apple compilation and clipboard interoperability require
CI and real-device validation; image decoding currently runs on the main thread.
Older clients retain text synchronization and do not advertise image support.
The iOS client lives in `ios/`; Swift tests and its simulator app/extension
build pass CI, and it was tested in the iOS Simulator against the macOS app.
Real-device validation is pending, and it is not part of the releases yet.
