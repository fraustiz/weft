//! `weft`: command-line Weft device. Data goes to stdout, logs to stderr.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;
use weft_cli::session::{JOIN_TIMEOUT, NAME_TIMEOUT, PAIRING_TIMEOUT};
use weft_cli::{Error, Event, Result, Session, State, StateDir, default_state_dir, hex, short_id};
use weft_protocol::PairingCode;

#[derive(Parser)]
#[command(
    name = "weft",
    version,
    about = "Share clipboard text between your devices"
)]
struct Cli {
    /// State directory (default: the platform data directory + /weft).
    #[arg(long, env = "WEFT_STATE", global = true, value_name = "DIR")]
    state: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create this device's identity.
    Init {
        /// Relay url, e.g. wss://weft.example.org/v1.
        #[arg(long)]
        relay: String,
        /// Replace an existing identity (its trusted devices are lost).
        #[arg(long)]
        force: bool,
        /// This device's name, shown on the other devices (default: the
        /// host name). 1 to 64 bytes, no control characters.
        #[arg(long)]
        name: Option<String>,
    },
    /// Change this device's name and send it to every trusted device.
    Rename {
        /// The new name: 1 to 64 bytes, no control characters.
        name: String,
    },
    /// Show a pairing code and add the device that uses it.
    Pair,
    /// Join the group of the device showing a pairing code.
    Join {
        /// The WEFT1: code.
        code: String,
    },
    /// Print received clipboard text as JSON lines.
    Listen {
        /// Also put received text on the system clipboard (macOS only).
        #[arg(long)]
        copy: bool,
    },
    /// Send text to every trusted device.
    Send {
        /// The text to send.
        #[arg(required_unless_present = "stdin", conflicts_with = "stdin")]
        text: Option<String>,
        /// Read the text from standard input.
        #[arg(long)]
        stdin: bool,
    },
    /// List the trusted devices.
    Devices,
    /// Remove a device from the group.
    Remove {
        /// Device id, or a unique prefix of it, in hex.
        device: String,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_target(false)
        .without_time()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("WEFT_LOG")
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("weft: error: {err}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let dir = match cli.state.or_else(default_state_dir) {
        Some(dir) => StateDir::new(dir),
        None => return Err(Error::NoStateDir),
    };
    match cli.command {
        Command::Init { relay, force, name } => {
            let mut state = State::generate(&relay)?;
            if let Some(name) = name {
                state.set_name(&name)?;
            }
            dir.create(&state, force)?;
            tracing::info!(state = %dir.file().display(), name = %state.name(), "identity created");
            println!("{}", hex(&state.identity().device_id()));
        }
        Command::Rename { name } => {
            // Saved first: the name is kept even if the relay is unreachable.
            let (result, state) = dir.update(|state| state.set_name(&name))?;
            result?;
            if state.devices.is_empty() {
                tracing::info!(name = %state.name(), "device renamed");
            } else {
                let mut session = Session::connect(dir).await?;
                let sent = session.rename(&name).await?;
                session.close().await;
                tracing::info!(name = %state.name(), "device renamed; sent to {sent} device(s)");
            }
        }
        Command::Pair => pair(dir).await?,
        Command::Join { code } => join(dir, &code).await?,
        Command::Listen { copy } => listen(dir, copy).await?,
        Command::Send { text, stdin } => {
            let text = match text {
                Some(text) if !stdin => text,
                _ => read_stdin()?,
            };
            let mut session = Session::connect(dir).await?;
            let sent = session.send_text(&text).await?;
            session.close().await;
            if sent == 0 {
                tracing::warn!("no trusted device: run `weft pair` or `weft join` first");
            } else {
                tracing::info!("sent to {sent} device(s)");
            }
        }
        Command::Devices => {
            let state = dir.load()?;
            for device in &state.devices {
                let name = device.name.as_deref().unwrap_or("-");
                println!("{}  {name}", hex(&device.device_id()));
            }
        }
        Command::Remove { device } => {
            let mut session = Session::connect(dir).await?;
            let removed = session.remove(&device).await?;
            session.close().await;
            tracing::info!(device = %short_id(&removed.device_id()), "device removed");
        }
    }
    Ok(())
}

async fn pair(dir: StateDir) -> Result<()> {
    let mut session = Session::connect(dir).await?;
    let code = session.open_pairing().await?;
    let encoded = code.encode()?;
    let qr = QrCode::new(encoded.as_bytes())
        .expect("a pairing code always fits in a QR code")
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .build();
    // The code is the one secret ever printed: on stdout only, never in logs.
    {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{qr}\n{encoded}");
        let _ = out.flush();
    }
    tracing::info!(
        "scan the QR code, or run `weft join <code>` on the other device; waiting up to 5 minutes"
    );
    let device = session.wait_pair_request(&code, PAIRING_TIMEOUT).await?;
    tracing::info!(device = %short_id(&device), "device added");
    // The new device announces its name as soon as it has the Welcome.
    if !session
        .wait_names(&[device], NAME_TIMEOUT, log_event)
        .await?
    {
        tracing::warn!(device = %short_id(&device), "the new device did not send its name");
    }
    session.close().await;
    println!("{}", short_id(&device));
    Ok(())
}

async fn join(dir: StateDir, code: &str) -> Result<()> {
    let code = PairingCode::decode(code.trim())?;
    dir.load_or_create(&code.relay_url)?;
    let mut session = Session::connect(dir).await?;
    let devices = session.join(&code, JOIN_TIMEOUT).await?;
    // The device that showed the code sends its name right after the
    // Welcome. Other members only can if they are online: not waited for.
    let owner = weft_protocol::device_id_from_sign_pk(&code.sign_pk);
    if !session
        .wait_names(&[owner], NAME_TIMEOUT, log_event)
        .await?
    {
        tracing::warn!(device = %short_id(&owner), "the device did not send its name");
    }
    session.close().await;
    tracing::info!("joined the group: {} trusted device(s)", devices.len());
    for device in devices {
        println!("{}", short_id(&device));
    }
    Ok(())
}

async fn listen(dir: StateDir, copy: bool) -> Result<()> {
    let copy = copy && clipboard_available();
    let on_event = |event: &Event| match event {
        Event::Clipboard { from, text } => {
            let line = serde_json::json!({ "from": hex(from), "text": text });
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
            if copy {
                copy_to_clipboard(text);
            }
        }
        event => log_event(event),
    };
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("stopping");
    };
    weft_cli::session::listen(&dir, on_event, shutdown).await
}

/// Logs an event to stderr. Clipboard text is never logged.
fn log_event(event: &Event) {
    match event {
        Event::Clipboard { from, .. } => {
            tracing::debug!(from = %short_id(from), "clipboard text received (not shown)")
        }
        Event::DeviceName { from, name } => {
            tracing::info!(device = %short_id(from), %name, "device name received")
        }
        Event::Ping { from } => tracing::debug!(from = %short_id(from), "ping"),
        Event::Welcome { from, added } => {
            tracing::info!(from = %short_id(from), added = added.len(), "welcome received")
        }
        Event::DeviceAdded { by, device } => {
            tracing::info!(by = %short_id(by), device = %short_id(device), "device added")
        }
        Event::DeviceRemoved { by, device } => {
            tracing::info!(by = %short_id(by), device = %short_id(device), "device removed")
        }
        Event::RelayError(code) => tracing::warn!(error = %Error::Relay(*code), "from the relay"),
        Event::Dropped { from, reason } => {
            let from = from.as_ref().map(short_id).unwrap_or_else(|| "-".into());
            tracing::warn!(%from, "dropped a frame: {reason}")
        }
    }
}

fn read_stdin() -> Result<String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|source| Error::Io {
            context: "reading standard input".into(),
            source,
        })?;
    Ok(text)
}

fn clipboard_available() -> bool {
    if cfg!(target_os = "macos") {
        true
    } else {
        tracing::warn!("--copy is only supported on macOS for now; ignored");
        false
    }
}

/// Puts `text` on the macOS clipboard with `pbcopy`.
fn copy_to_clipboard(text: &str) {
    use std::process::{Command, Stdio};
    let result = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()
        });
    match result {
        Ok(status) if status.success() => {}
        Ok(status) => tracing::warn!("pbcopy failed: {status}"),
        Err(err) => tracing::warn!("pbcopy failed: {err}"),
    }
}
