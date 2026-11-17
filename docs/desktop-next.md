# Desktop development status

This branch adds clipboard images to the shared Rust worker and the Windows
and Linux adapters. Images use offer/accept negotiation, bounded memory,
integrity checks, acknowledgements, and the existing encrypted envelope and
replay protection. Windows also accepts shell file drops and has a Paste files
button. A Ctrl+V file shortcut is not implemented.

The Linux client is a daemon and CLI using the desktop's installed utilities;
see `linux/README.md` for dependencies. Its adapter tests use fake utilities;
they do not establish that a real Wayland or X11 desktop works.

Authenticated LAN transport is under validation. Falling back to the relay
reuses the same sealed envelope so replay protection still applies.

Ephemeral key derivation and session epoch primitives are experimental and
are **not enabled automatically**. This branch does not claim forward secrecy:
restart recovery and control-channel negotiation still need implementation
and end-to-end tests before enabling key rotation.

Android and macOS image adapters remain to be connected. Older clients retain
text synchronization and do not advertise image support. The iOS client is on
the separate `wt/ios-app` branch and is undergoing Apple compiler validation.
No new public app release is implied by these development branches.
