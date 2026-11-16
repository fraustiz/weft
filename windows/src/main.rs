//! The Weft application for Windows. Logs go to stderr, at the level set
//! by `WEFT_LOG`; release builds have no console, so they only show when
//! stderr is redirected.

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

    use weft_cli::{StateDir, default_state_dir};
    use weft_windows::tray::{self, Link};
    use weft_windows::worker::{self, Command};
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };

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
        tracing::info!("already running on this state directory");
        return ExitCode::SUCCESS;
    }
    // Without it the menu is drawn for 96 dpi and stretched on scaled
    // displays.
    // SAFETY: called before any window exists; a failure changes nothing.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let link = Arc::new(Link::default());
    let on_text = {
        let link = link.clone();
        move |text| link.remote_text(text)
    };
    let (worker, worker_thread) = match worker::spawn(StateDir::new(state_dir), on_text) {
        Ok(started) => started,
        Err(err) => {
            tracing::error!(error = %err, "could not start");
            return ExitCode::FAILURE;
        }
    };

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
        let worker = worker.clone();
        std::thread::Builder::new()
            .name("weft-tray".into())
            .spawn(move || tray::run(worker, link))
    };
    match tray_thread {
        // Returns when the user quits.
        Ok(thread) => {
            let _ = thread.join();
        }
        Err(err) => tracing::error!(error = %err, "could not start the tray thread"),
    }
    worker.send(Command::Shutdown);
    let _ = worker_thread.join();
    ExitCode::SUCCESS
}
