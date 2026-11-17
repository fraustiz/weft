//! Weft for Linux (`weft-desktop`): background desktop sync daemon and control
//! CLI built on [`weft_client`], [`weft_cli`], and [`weft_protocol`].

pub mod clipboard;
pub mod daemon;
pub mod desktop;
pub mod input;
pub mod ipc;
pub mod media;
pub mod notify;
pub mod presets;
pub mod qr;
pub mod runner;
pub mod sound;
pub mod update;
pub mod volume;
