//! The client side of Weft that does not depend on the platform: what a
//! desktop app of a Weft group (`docs/protocol.md`) decides, sends and
//! keeps, apart from what it shows and how it reaches the system. The
//! Windows app uses it, and so can any other desktop app.
//!
//! - [`sync`] decides what happens to clipboard text;
//! - [`packets`], [`features`], [`transfer`] and [`media`] are the feature
//!   packets of `docs/features.md`, the switches, the file transfer rules
//!   and the clock of the players; [`files`] the disk side of transfers;
//! - [`sms`] keeps the phone's messages and contacts in memory; [`input`]
//!   decides what remote input means;
//! - [`worker`] holds the connection to the relay, on its own thread. It
//!   reaches the platform only through its [`worker::Hooks`], its
//!   [`worker::Command`]s and its [`worker::Snapshot`].
//!
//! The session logic itself is the one of the command-line client, used
//! as a library.

pub mod features;
pub mod files;
pub mod input;
pub mod media;
pub mod packets;
pub mod sms;
pub mod sync;
pub mod transfer;
pub mod worker;
