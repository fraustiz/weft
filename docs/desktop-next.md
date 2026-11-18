# Desktop development status

This branch adds clipboard images to the shared Rust worker and the Windows
and Linux adapters. Images use offer/accept negotiation, bounded memory,
integrity checks, acknowledgements, and the existing encrypted envelope and
replay protection. Windows also accepts shell file drops and has a Paste files
button. Ctrl+V file paste is limited to known noneditable controls in the
focused Weft window; changing focus cancels pending shortcut handling. Native
text paste is preserved. Manual validation on a Windows desktop is pending.

The Linux client is a daemon and CLI using the desktop's installed utilities;
see `linux/README.md` for dependencies. Its adapter tests use fake utilities;
they do not establish that a real Wayland or X11 desktop works.

Authenticated LAN transport is under validation. Falling back to the relay
reuses the same sealed envelope so replay protection still applies.

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
The iOS client is on the separate `wt/ios-app` branch; Swift tests and its
simulator app/extension build pass CI, while real-device validation is pending.
No new public app release is implied by these development branches.
