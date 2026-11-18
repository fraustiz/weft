//! `weft-desktop`: Linux desktop daemon and control CLI for Weft.
//!
//! Running `weft-desktop` with no subcommand (or `weft-desktop daemon`) starts
//! the single-instance background daemon. All other subcommands communicate
//! with the running daemon through `<state_dir>/daemon.json`.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use weft_linux::daemon::{self, DaemonConfig, DaemonError, default_state_dir};
use weft_linux::desktop::{DesktopPaths, install_desktop};
use weft_linux::ipc::{IpcClient, IpcRequest, IpcResponse, PairPollOutcome};
use weft_linux::presets::{COMMAND_PRESETS, find_preset};
use weft_linux::qr::render_terminal_qr;
use weft_linux::runner::SystemRunner;
use weft_linux::update::{INSTALLED_VERSION_NAME, UpdateState, Updater};

#[derive(Parser)]
#[command(
    name = "weft-desktop",
    version = INSTALLED_VERSION_NAME,
    about = "Weft Linux desktop daemon and control CLI"
)]
struct Cli {
    /// State directory (defaults to $WEFT_STATE, $XDG_STATE_HOME/weft, or ~/.local/state/weft).
    #[arg(long = "state-dir", global = true, value_name = "DIR")]
    state_dir: Option<PathBuf>,

    /// Emit machine-readable JSON output (suitable for Waybar, Polybar, or scripts).
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Clone, Copy, ValueEnum)]
enum ToggleArg {
    On,
    Off,
}

impl ToggleArg {
    fn as_bool(self) -> bool {
        matches!(self, ToggleArg::On)
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the background desktop sync daemon (default when no subcommand is given).
    Daemon {
        /// Relay WebSocket URL used when creating a new identity.
        #[arg(long)]
        relay: Option<String>,
    },
    /// Show daemon connection status, or Waybar/Polybar JSON with `--json`.
    Status,
    /// List trusted devices in the group.
    Devices,
    /// Open a pairing slot, print a UTF-8 QR code + `WEFT1:` code, and wait for a device.
    Pair,
    /// Join the group of the device showing a `WEFT1:` pairing code.
    Join {
        /// The `WEFT1:` code shown on the other device.
        code: String,
    },
    /// Remove a trusted device by hex ID, unique prefix, or name.
    Remove { device: String },
    /// Change this computer's announced device name (1..=64 UTF-8 bytes).
    Rename { name: String },
    /// Pause clipboard synchronization in both directions.
    Pause,
    /// Resume clipboard synchronization.
    Resume,
    /// Send the current clipboard text immediately to all paired devices.
    Clip,
    /// Send one or more files to a trusted device.
    Send {
        device: String,
        #[arg(required = true, num_args = 1..)]
        files: Vec<PathBuf>,
    },
    /// Open an `http://` or `https://` link on a trusted device.
    Url { device: String, url: String },
    /// Make a trusted device ring.
    Ring { device: String },
    /// Stop a ring started on a trusted device.
    StopRing { device: String },
    /// Stop ringing this computer ("Found it").
    FoundIt,
    /// Send a ping notification to a trusted device.
    Ping {
        device: String,
        message: Option<String>,
    },
    /// List SMS conversations on a phone, or read messages in `--thread <id>`.
    Messages {
        device: String,
        #[arg(long)]
        thread: Option<i64>,
    },
    /// Send an SMS through a paired phone.
    SendSms {
        device: String,
        #[arg(long)]
        to: String,
        text: String,
    },
    /// Manage remote shortcut commands phones may trigger on this computer.
    Commands {
        #[command(subcommand)]
        action: CommandsCmd,
    },
    /// Allow or forbid remote pointer/keyboard/presenter input from `<device>`.
    Control {
        device: String,
        #[arg(value_enum)]
        state: ToggleArg,
    },
    /// Show all feature switches, or toggle `<feature>` `on` or `off`.
    Features {
        feature: Option<String>,
        #[arg(value_enum, requires = "feature")]
        state: Option<ToggleArg>,
    },
    /// Show the current relay URL, or switch to a new `<wss://...>` relay when unpaired.
    Relay { url: Option<String> },
    /// Leave the current device group.
    Leave,
    /// Check for or install updates from GitHub Releases (`linux-update.json`).
    Update {
        /// Check whether a newer release is available (default).
        #[arg(long, conflicts_with = "install")]
        check: bool,
        /// Download, verify SHA-256 & ELF64 x86_64, atomically install, and restart.
        #[arg(long)]
        install: bool,
    },
    /// Write `~/.config/systemd/user/weft-desktop.service` and `~/.local/share/applications/weft-desktop.desktop`.
    InstallDesktop {
        /// Also reload user systemd units and `enable --now weft-desktop.service`.
        #[arg(long)]
        enable: bool,
    },
}

#[derive(Subcommand)]
enum CommandsCmd {
    /// List configured shortcut commands.
    List,
    /// Add a shortcut command (`<name> <command-line>`).
    Add { name: String, line: String },
    /// Remove a shortcut command by ID or name.
    Remove { query: String },
    /// List ready-made Linux desktop presets (or add one with `--add <key>`).
    Presets {
        /// Immediately add the matching preset (`lock`, `suspend`, `mute`, `play-pause`).
        #[arg(long)]
        add: Option<String>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let is_daemon = matches!(cli.command, None | Some(Cmd::Daemon { .. }));
    let default_level = if is_daemon { "info" } else { "warn" };

    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_ansi(io::IsTerminal::is_terminal(&io::stderr()))
        .with_target(false)
        .without_time()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("WEFT_LOG")
                .unwrap_or_else(|_| default_level.into()),
        )
        .init();

    let json = cli.json;
    match execute(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            if json {
                println!("{}", serde_json::json!({ "error": msg }));
            } else {
                eprintln!("weft-desktop: error: {msg}");
            }
            ExitCode::FAILURE
        }
    }
}

async fn execute(cli: Cli) -> Result<(), String> {
    let resolve_state = || -> Result<PathBuf, String> {
        cli.state_dir
            .clone()
            .or_else(default_state_dir)
            .ok_or_else(|| {
                "could not determine state directory: pass --state-dir or set XDG_STATE_HOME / HOME"
                    .to_owned()
            })
    };

    match cli.command.unwrap_or(Cmd::Daemon { relay: None }) {
        Cmd::Daemon { relay } => {
            let state_dir = resolve_state()?;
            run_daemon(state_dir, relay).await
        }
        Cmd::InstallDesktop { enable } => {
            let paths = DesktopPaths::from_env().ok_or_else(|| {
                "HOME is not set; cannot resolve systemd/applications paths".to_owned()
            })?;
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            install_desktop(&paths, &exe, enable, &SystemRunner).map_err(|e| e.to_string())?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "service_file": paths.service_file,
                        "desktop_file": paths.desktop_file,
                        "enabled": enable,
                    })
                );
            } else {
                println!("Wrote {}", paths.service_file.display());
                println!("Wrote {}", paths.desktop_file.display());
                if enable {
                    println!("Enabled and started weft-desktop.service");
                } else {
                    println!(
                        "Run `systemctl --user daemon-reload && systemctl --user enable --now weft-desktop.service` (or pass `--enable`) to start automatically."
                    );
                }
            }
            Ok(())
        }
        Cmd::Commands {
            action: CommandsCmd::Presets { add: None },
        } => {
            if cli.json {
                let items: Vec<_> = COMMAND_PRESETS
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "key": p.key,
                            "name": p.name,
                            "line": p.line,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string(&items).unwrap());
            } else {
                for preset in COMMAND_PRESETS {
                    println!("{:<12} {:<24} {}", preset.key, preset.name, preset.line);
                }
            }
            Ok(())
        }
        Cmd::Update { check: _, install } => {
            let state_dir = resolve_state()?;
            if let Ok(client) = IpcClient::from_state_dir(&state_dir) {
                let req = if install {
                    IpcRequest::UpdateInstall
                } else {
                    IpcRequest::UpdateCheck
                };
                let resp = client.request(req).await.map_err(|e| e.to_string())?;
                return print_response(resp, cli.json);
            }
            // Offline / standalone fallback when the daemon is not running.
            let snap = tokio::task::spawn_blocking(move || {
                let updater = Updater::new(state_dir, |_| {}, || {});
                if install {
                    updater.install_now()
                } else {
                    updater.check_now()
                }
            })
            .await
            .map_err(|e| e.to_string())?;
            print_response(IpcResponse::Update { snapshot: snap }, cli.json)
        }
        other => {
            let state_dir = resolve_state()?;
            let client = IpcClient::from_state_dir(&state_dir).map_err(|e| e.to_string())?;
            run_ipc_command(client, other, cli.json).await
        }
    }
}

async fn run_daemon(state_dir: PathBuf, relay: Option<String>) -> Result<(), String> {
    let config = DaemonConfig::system(state_dir.clone(), relay);
    let handle = match daemon::start(config).await {
        Ok(handle) => handle,
        Err(DaemonError::AlreadyRunning(dir)) => {
            tracing::info!(state_dir = %dir.display(), "daemon is already running");
            return Ok(());
        }
        Err(err) => return Err(err.to_string()),
    };

    let restart_exe = tokio::select! {
        restart = handle.wait() => restart,
        () = wait_shutdown_signal() => {
            tracing::info!("shutting down weft-desktop daemon");
            // Signal was received while handle was consumed by select branch drop:
            // wait_shutdown_signal only resolves while daemon is still live, so we
            // request shutdown over IPC or watch. Let's structure without consuming handle!
            None
        }
    };

    if let Some(exe) = restart_exe {
        let _ = std::process::Command::new(exe)
            .arg("--state-dir")
            .arg(&state_dir)
            .arg("daemon")
            .spawn();
    }
    Ok(())
}

async fn wait_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut sigterm) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = sigterm.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

fn absolutize_file(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    Ok(cwd.join(path))
}

async fn run_ipc_command(client: IpcClient, cmd: Cmd, json: bool) -> Result<(), String> {
    match cmd {
        Cmd::Pair => run_pair(client, json).await,
        Cmd::Status => {
            let resp = client
                .request(IpcRequest::Status)
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Devices => {
            let resp = client
                .request(IpcRequest::Devices)
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Join { code } => {
            let resp = client
                .request(IpcRequest::Join { code })
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Remove { device } => {
            let resp = client
                .request(IpcRequest::Remove { device })
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Rename { name } => {
            let resp = client
                .request(IpcRequest::Rename { name })
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Pause => {
            let resp = client
                .request(IpcRequest::SetPaused { paused: true })
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Resume => {
            let resp = client
                .request(IpcRequest::SetPaused { paused: false })
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Clip => {
            let resp = client
                .request(IpcRequest::SendClipboardNow)
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Send { device, files } => {
            let resolved: Result<Vec<PathBuf>, String> =
                files.iter().map(|p| absolutize_file(p)).collect();
            let resp = client
                .request(IpcRequest::SendFiles {
                    device,
                    files: resolved?,
                })
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Url { device, url } => {
            let resp = client
                .request(IpcRequest::ShareUrl { device, url })
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Ring { device } => {
            let resp = client
                .request(IpcRequest::Ring { device })
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::StopRing { device } => {
            let resp = client
                .request(IpcRequest::StopRing { device })
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::FoundIt => {
            let resp = client
                .request(IpcRequest::FoundIt)
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Ping { device, message } => {
            let resp = client
                .request(IpcRequest::Ping { device, message })
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Messages { device, thread } => {
            let resp = client
                .request(IpcRequest::Messages { device, thread })
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::SendSms { device, to, text } => {
            let resp = client
                .request(IpcRequest::SendSms { device, to, text })
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Commands { action } => {
            let req = match action {
                CommandsCmd::List => IpcRequest::CommandsList,
                CommandsCmd::Add { name, line } => IpcRequest::CommandsAdd { name, line },
                CommandsCmd::Remove { query } => IpcRequest::CommandsRemove { query },
                CommandsCmd::Presets { add: Some(key) } => {
                    let preset = find_preset(&key).ok_or_else(|| {
                        format!(
                            "unknown preset '{key}' (run `weft-desktop commands presets` to list)"
                        )
                    })?;
                    IpcRequest::CommandsAdd {
                        name: preset.name.to_owned(),
                        line: preset.line.to_owned(),
                    }
                }
                CommandsCmd::Presets { add: None } => unreachable!(),
            };
            let resp = client.request(req).await.map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Control { device, state } => {
            let resp = client
                .request(IpcRequest::SetControl {
                    device,
                    allowed: state.as_bool(),
                })
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Features { feature, state } => {
            let req = match (feature, state) {
                (None, _) => IpcRequest::GetFeatures,
                (Some(feat), Some(st)) => IpcRequest::SetFeature {
                    feature: feat,
                    enabled: st.as_bool(),
                },
                (Some(feat), None) => {
                    return Err(format!("missing `on` or `off` after feature '{feat}'"));
                }
            };
            let resp = client.request(req).await.map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Relay { url } => {
            let req = match url {
                Some(url) => IpcRequest::SetRelay { url },
                None => IpcRequest::GetRelay,
            };
            let resp = client.request(req).await.map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Leave => {
            let resp = client
                .request(IpcRequest::Leave)
                .await
                .map_err(|e| e.to_string())?;
            print_response(resp, json)
        }
        Cmd::Daemon { .. } | Cmd::InstallDesktop { .. } | Cmd::Update { .. } => unreachable!(),
    }
}

async fn run_pair(client: IpcClient, json: bool) -> Result<(), String> {
    let started = client
        .request(IpcRequest::PairStart)
        .await
        .map_err(|e| e.to_string())?;
    let (code, expires_in_secs) = match started {
        IpcResponse::PairStarted {
            code,
            expires_in_secs,
        } => (code, expires_in_secs),
        IpcResponse::Error { message } => return Err(message),
        _ => return Err("unexpected response from daemon".into()),
    };

    // Pairing secret goes to stdout only, never to tracing logs.
    if json {
        let mut out = io::stdout().lock();
        let line = serde_json::json!({
            "event": "showing_code",
            "code": code,
            "expires_in_secs": expires_in_secs,
        });
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    } else {
        let qr = render_terminal_qr(&code).unwrap_or_default();
        let mut out = io::stdout().lock();
        if !qr.is_empty() {
            let _ = writeln!(out, "{qr}");
        }
        let _ = writeln!(out, "{code}");
        let _ = writeln!(
            out,
            "\nScan the QR code or run `weft-desktop join <code>` on the other device (expires in {}s)...",
            expires_in_secs
        );
        let _ = out.flush();
    }

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                let _ = client.request(IpcRequest::PairCancel).await;
                return Err("pairing cancelled".into());
            }
            polled = client.request(IpcRequest::PairWait { timeout_ms: 1500 }) => {
                let resp = polled.map_err(|e| e.to_string())?;
                match resp {
                    IpcResponse::PairOutcome { outcome: PairPollOutcome::Waiting { .. } } => continue,
                    other => return print_response(other, json),
                }
            }
        }
    }
}

fn print_response(resp: IpcResponse, json: bool) -> Result<(), String> {
    match resp {
        IpcResponse::Error { message } => Err(message),
        IpcResponse::Ack { message } => {
            if json {
                println!("{}", serde_json::json!({ "ok": true, "message": message }));
            } else {
                println!("{message}");
            }
            Ok(())
        }
        IpcResponse::Status { status } => {
            if json {
                println!("{}", serde_json::to_string(&status).unwrap());
            } else {
                println!("Weft {} — {}", status.version, status.status);
                println!("Device:  {} ({})", status.name, status.short_id);
                println!("Relay:   {}", status.relay_url);
                println!("Paused:  {}", if status.paused { "yes" } else { "no" });
                println!("Devices: {}", status.devices.len());
                for dev in &status.devices {
                    let label = dev.name.as_deref().unwrap_or("-");
                    let bat = dev
                        .battery
                        .and_then(|b| b.level)
                        .map(|l| format!("  {l}%"))
                        .unwrap_or_default();
                    println!("  {}  {}{bat}", dev.short_id, label);
                }
                if !status.transfers.is_empty() {
                    println!("Transfers:");
                    for t in &status.transfers {
                        let dir = if t.outgoing { "->" } else { "<-" };
                        println!("  {dir} {} ({}/{}) [{}]", t.name, t.done, t.size, t.stage);
                    }
                }
            }
            Ok(())
        }
        IpcResponse::Devices { devices } => {
            if json {
                println!("{}", serde_json::to_string(&devices).unwrap());
            } else if devices.is_empty() {
                println!("No trusted devices paired.");
            } else {
                for dev in devices {
                    let name = dev.name.as_deref().unwrap_or("-");
                    let plat = dev.platform.as_deref().unwrap_or("-");
                    let bat = dev
                        .battery
                        .and_then(|b| b.level)
                        .map(|l| format!("{l}%"))
                        .unwrap_or_else(|| "-".into());
                    let ctrl = if dev.control_allowed {
                        " [control]"
                    } else {
                        ""
                    };
                    println!("{}  {:<20} {:<10} {:<5}{ctrl}", dev.id, name, plat, bat);
                }
            }
            Ok(())
        }
        IpcResponse::PairOutcome { outcome } => match outcome {
            PairPollOutcome::Success {
                device_id,
                short_id,
                name,
                kind,
            } => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "ok": true,
                            "device_id": device_id,
                            "short_id": short_id,
                            "name": name,
                            "kind": kind,
                        })
                    );
                } else {
                    let who = name.as_deref().unwrap_or(&short_id);
                    println!("Paired with {who} ({device_id})");
                }
                Ok(())
            }
            PairPollOutcome::Failed { reason } => Err(reason),
            PairPollOutcome::Waiting { .. } => Ok(()),
        },
        IpcResponse::Messages { payload } => {
            if json {
                println!("{}", serde_json::to_string(&payload).unwrap());
            } else if let Some(t_id) = payload.thread {
                println!("Thread {t_id}:");
                for msg in &payload.messages {
                    let who = if msg.outgoing {
                        "Me"
                    } else {
                        msg.address.as_deref().unwrap_or("Peer")
                    };
                    println!("  [{who}] {}", msg.body);
                }
                for pending in payload.outbox.iter().filter(|o| o.thread == Some(t_id)) {
                    println!("  [Me · {}] {}", pending.state, pending.text);
                }
            } else if payload.conversations.is_empty() {
                println!("No SMS conversations available.");
            } else {
                for conv in &payload.conversations {
                    let unread = if conv.unread { "*" } else { " " };
                    let snippet = conv.snippet.as_deref().unwrap_or("");
                    println!("{unread} [{}] {}: {snippet}", conv.thread, conv.title());
                }
            }
            Ok(())
        }
        IpcResponse::Commands { commands } => {
            if json {
                println!("{}", serde_json::to_string(&commands).unwrap());
            } else if commands.is_empty() {
                println!("No shortcut commands configured.");
            } else {
                for cmd in commands {
                    println!("{}  {:<24} {}", cmd.id, cmd.name, cmd.line);
                }
            }
            Ok(())
        }
        IpcResponse::Features { features } => {
            if json {
                println!("{}", serde_json::to_string(&features).unwrap());
            } else {
                let row = |k: &str, on: bool| {
                    println!("{:<16} {}", k, if on { "on" } else { "off" });
                };
                row("files", features.files);
                row("links", features.links);
                row("ring", features.ring);
                row("battery", features.battery);
                row("notifications", features.notifications);
                row("calls", features.calls);
                row("media", features.media);
                row("volume", features.volume);
                row("sms", features.sms);
                row("commands", features.commands);
            }
            Ok(())
        }
        IpcResponse::Relay { relay_url } => {
            if json {
                println!("{}", serde_json::json!({ "relay_url": relay_url }));
            } else {
                println!("{relay_url}");
            }
            Ok(())
        }
        IpcResponse::Left { told_peers } => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "ok": true, "told_peers": told_peers })
                );
            } else if told_peers {
                println!("Left the group.");
            } else {
                println!("Left the group locally (relay was unreachable).");
            }
            Ok(())
        }
        IpcResponse::Update { snapshot } => {
            if json {
                println!("{}", serde_json::to_string(&snapshot).unwrap());
                if let UpdateState::Failed { error, .. } = snapshot.state {
                    return Err(error.to_string());
                }
                return Ok(());
            }
            match snapshot.state {
                UpdateState::UpToDate | UpdateState::Idle => {
                    println!("Weft {} is up to date.", snapshot.installed_version);
                    Ok(())
                }
                UpdateState::Available { info } => {
                    println!(
                        "Update available: {} (installed: {}). Run `weft-desktop update --install`.",
                        info.version_name, snapshot.installed_version
                    );
                    Ok(())
                }
                UpdateState::Restarting { info } | UpdateState::Installing { info } => {
                    println!("Installed Weft {}.", info.version_name);
                    Ok(())
                }
                UpdateState::Checking | UpdateState::Downloading { .. } => {
                    println!("Update in progress.");
                    Ok(())
                }
                UpdateState::Failed { error, .. } => Err(error.to_string()),
            }
        }
        IpcResponse::PairStarted { .. } => Ok(()),
    }
}
