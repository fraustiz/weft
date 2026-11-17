//! The Weft application for Windows. Logs go to stderr, at the level set
//! by `WEFT_LOG`; release builds have no console, so they only show when
//! stderr is redirected.
//!
//! Four threads: the main one runs the window, the tray thread owns the
//! notification icon and the clipboard, the worker talks to the relay, and
//! the system thread runs the toasts, the players and the volume.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::process::ExitCode;

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!("Weft for Windows only runs on Windows");
    ExitCode::from(1)
}

#[cfg(windows)]
fn main() -> ExitCode {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use weft_cli::{StateDir, default_state_dir};
    use weft_windows::notify::Notifier;
    use weft_windows::system::{self, System};
    use weft_windows::tray::{self, Link};
    use weft_windows::update::{self, Updater};
    use weft_windows::worker::{self, Command, Hooks};
    use weft_windows::{autostart, control, files, toast, ui, window};
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };

    /// How long the worker gets to close the connection politely.
    const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);

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

    let state_dir = std::env::var_os("WEFT_STATE")
        .map(PathBuf::from)
        .or_else(default_state_dir);
    let Some(state_dir) = state_dir else {
        tracing::error!("no state directory: set WEFT_STATE");
        return ExitCode::FAILURE;
    };
    if !tray::single_instance(&state_dir) {
        if tray::show_running_instance(&state_dir) {
            tracing::info!("already running on this state directory: window shown");
        } else {
            tracing::info!("already running on this state directory");
        }
        return ExitCode::SUCCESS;
    }
    update::cleanup_old_executable(&state_dir);
    let hidden = std::env::args().any(|arg| arg == autostart::STARTUP_ARG);
    // Without it the menu is drawn for 96 dpi and stretched on scaled
    // displays.
    // SAFETY: called before any window exists; a failure changes nothing.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    // Before any window, so they all carry the app's identity.
    if let Err(err) = toast::register(&state_dir) {
        tracing::warn!(error = %err, "no app identity: notifications may not show");
    }

    let link = Arc::new(Link::default());
    let (system, system_inbox) = System::new();
    let updater = {
        let notifier = Notifier {
            system: system.clone(),
        };
        let restart_link = link.clone();
        Updater::new(
            state_dir.clone(),
            move |version| notifier.update_available(version),
            move || {
                restart_link.quit();
                window::quit();
            },
        )
    };
    let hooks = {
        let text_link = link.clone();
        let notifier = Notifier {
            system: system.clone(),
        };
        Hooks {
            on_text: Box::new(move |text| text_link.remote_text(text)),
            on_notice: Arc::new(move |notice| notifier.handle(notice)),
            // Without a known Downloads folder, files stay next to the state.
            downloads: files::downloads_dir().unwrap_or_else(|| state_dir.join("Received")),
            on_input: {
                let mut injector = control::Injector::default();
                Box::new(move |input| injector.apply(input))
            },
            run_command: Box::new(control::run_command),
            // Every feature of the core is implemented here.
            implemented: None,
        }
    };
    let (worker, worker_thread) = match worker::spawn(StateDir::new(state_dir.clone()), hooks) {
        Ok(started) => started,
        Err(err) => {
            tracing::error!(error = %err, "could not start");
            return ExitCode::FAILURE;
        }
    };

    system::start(&system, system_inbox, worker.clone(), link.clone());
    updater.check_if_due();

    // The tray thread waits on window messages, not on the worker: this
    // thread turns each new snapshot into a message for it.
    {
        let (worker, link) = (worker.clone(), link.clone());
        std::thread::spawn(move || {
            let mut generation = 0;
            while let Some(published) = worker.wait_newer(generation) {
                generation = published.generation;
                link.snapshot_changed();
            }
        });
    }

    let tray_thread = {
        let (worker, link, updater, state_dir) = (
            worker.clone(),
            link.clone(),
            updater.clone(),
            state_dir.clone(),
        );
        std::thread::Builder::new()
            .name("weft-tray".into())
            .spawn(move || {
                // Returns when the user quits; the window goes with it.
                tray::run(worker, link, updater, &state_dir);
                window::quit();
            })
    };
    let tray_thread = match tray_thread {
        Ok(thread) => thread,
        Err(err) => {
            tracing::error!(error = %err, "could not start the tray thread");
            updater.stop();
            worker.send(Command::Shutdown);
            return ExitCode::FAILURE;
        }
    };

    let shell = ui::Shell {
        worker: worker.clone(),
        link: link.clone(),
        updater: updater.clone(),
        hidden,
    };
    // Returns once the window is closed for good, by Quit or by the
    // system. Without WinUI the app still works from the tray.
    if let Err(err) = ui::run(shell) {
        tracing::error!(error = %err, "the window could not start; the tray icon stays");
    } else {
        link.quit();
    }
    let _ = tray_thread.join();
    updater.stop();

    worker.send(Command::Shutdown);
    let deadline = Instant::now() + SHUTDOWN_WAIT;
    while !worker_thread.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    if !worker_thread.is_finished() {
        tracing::warn!("the connection did not close in time");
    }

    if let Some(exe) = updater.take_restart_exe() {
        tray::release_single_instance();
        if let Err(err) = std::process::Command::new(&exe).spawn() {
            tracing::error!(error = %err, "could not start the updated executable");
        }
    }
    ExitCode::SUCCESS
}
