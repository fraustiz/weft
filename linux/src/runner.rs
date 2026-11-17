//! External process runner abstraction so every Linux desktop adapter
//! (`wl-clipboard`, `xclip`, `gdbus`, `notify-send`, `playerctl`, `wpctl`,
//! `pactl`, `ydotool`, `wtype`, `xdotool`) can be tested deterministically on
//! any OS without spawning system binaries.

use std::collections::{BTreeSet, VecDeque};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// Captured result of running an external command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmdOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl CmdOutput {
    pub fn ok(stdout: impl Into<Vec<u8>>) -> Self {
        Self {
            success: true,
            code: Some(0),
            stdout: stdout.into(),
            stderr: Vec::new(),
        }
    }

    pub fn ok_str(stdout: &str) -> Self {
        Self::ok(stdout.as_bytes().to_vec())
    }

    pub fn err(code: i32, stderr: &str) -> Self {
        Self {
            success: false,
            code: Some(code),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

/// Runs external helper utilities directly (never through a shell).
pub trait Runner: Send + Sync {
    /// Whether `program` can be found in `PATH`.
    fn has_program(&self, program: &str) -> bool;

    /// Executes `program` with `args`, feeding optional `stdin` and capturing
    /// its stdout and stderr.
    fn run(&self, program: &str, args: &[&str], stdin: Option<&[u8]>) -> io::Result<CmdOutput>;

    /// Executes `program` with `args`, feeding optional `stdin` while attaching
    /// stdout and stderr to `/dev/null` (`Stdio::null()`).
    ///
    /// Required for X11 clipboard writers (`xclip -i`, `xsel -i`), which fork a
    /// background selection owner that inherits open stdout/stderr pipes if
    /// `.output()` is used.
    fn run_quiet(&self, program: &str, args: &[&str], stdin: Option<&[u8]>) -> io::Result<bool> {
        self.run(program, args, stdin).map(|out| out.success)
    }

    /// Spawns `program` in the background without waiting for completion
    /// (used for `xdg-open`).
    fn spawn_detached(&self, program: &str, args: &[&str]) -> io::Result<()> {
        self.run_quiet(program, args, None).map(|_| ())
    }
}

/// Real OS process runner.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemRunner;

impl SystemRunner {
    pub fn find_in_path(program: &str) -> Option<PathBuf> {
        if program.is_empty() {
            return None;
        }
        let candidate = PathBuf::from(program);
        if candidate.components().count() > 1 {
            return candidate.is_file().then_some(candidate);
        }
        let path_var = std::env::var_os("PATH")?;
        for dir in std::env::split_paths(&path_var) {
            let full = dir.join(program);
            if is_executable_file(&full) {
                return Some(full);
            }
            #[cfg(windows)]
            for ext in ["exe", "cmd", "bat"] {
                let with_ext = dir.join(format!("{program}.{ext}"));
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
        None
    }
}

fn is_executable_file(path: &std::path::Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

impl Runner for SystemRunner {
    fn has_program(&self, program: &str) -> bool {
        Self::find_in_path(program).is_some()
    }

    fn run(&self, program: &str, args: &[&str], stdin: Option<&[u8]>) -> io::Result<CmdOutput> {
        let mut cmd = Command::new(program);
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
        if stdin.is_some() {
            cmd.stdin(Stdio::piped());
        } else {
            cmd.stdin(Stdio::null());
        }
        let mut child = cmd.spawn()?;
        if let Some(input) = stdin
            && let Some(mut pipe) = child.stdin.take()
        {
            // Ignore BrokenPipe if the child exits early; exit status tells.
            let _ = pipe.write_all(input);
        }
        let output = child.wait_with_output()?;
        Ok(CmdOutput {
            success: output.status.success(),
            code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    fn run_quiet(&self, program: &str, args: &[&str], stdin: Option<&[u8]>) -> io::Result<bool> {
        let mut cmd = Command::new(program);
        cmd.args(args).stdout(Stdio::null()).stderr(Stdio::null());
        if stdin.is_some() {
            cmd.stdin(Stdio::piped());
        } else {
            cmd.stdin(Stdio::null());
        }
        let mut child = cmd.spawn()?;
        if let Some(input) = stdin
            && let Some(mut pipe) = child.stdin.take()
        {
            let _ = pipe.write_all(input);
        }
        let status = child.wait()?;
        Ok(status.success())
    }

    fn spawn_detached(&self, program: &str, args: &[&str]) -> io::Result<()> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

/// One recorded invocation on [`FakeRunner`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedCall {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: Option<Vec<u8>>,
}

type HandlerFn = Box<dyn Fn(&str, &[&str], Option<&[u8]>) -> io::Result<CmdOutput> + Send + Sync>;

/// Deterministic fake command runner for unit and integration tests.
pub struct FakeRunner {
    programs: Mutex<BTreeSet<String>>,
    calls: Mutex<Vec<RecordedCall>>,
    queued: Mutex<VecDeque<io::Result<CmdOutput>>>,
    handler: Option<HandlerFn>,
}

impl Default for FakeRunner {
    fn default() -> Self {
        Self::new(&[])
    }
}

impl FakeRunner {
    pub fn new(available_programs: &[&str]) -> Self {
        Self {
            programs: Mutex::new(available_programs.iter().map(|&s| s.to_owned()).collect()),
            calls: Mutex::new(Vec::new()),
            queued: Mutex::new(VecDeque::new()),
            handler: None,
        }
    }

    pub fn with_handler(
        available_programs: &[&str],
        handler: impl Fn(&str, &[&str], Option<&[u8]>) -> io::Result<CmdOutput> + Send + Sync + 'static,
    ) -> Self {
        Self {
            programs: Mutex::new(available_programs.iter().map(|&s| s.to_owned()).collect()),
            calls: Mutex::new(Vec::new()),
            queued: Mutex::new(VecDeque::new()),
            handler: Some(Box::new(handler)),
        }
    }

    pub fn push_output(&self, output: CmdOutput) {
        self.queued.lock().unwrap().push_back(Ok(output));
    }

    pub fn push_error(&self, err: io::Error) {
        self.queued.lock().unwrap().push_back(Err(err));
    }

    pub fn calls(&self) -> Vec<RecordedCall> {
        self.calls.lock().unwrap().clone()
    }

    pub fn take_calls(&self) -> Vec<RecordedCall> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }
}

impl Runner for FakeRunner {
    fn has_program(&self, program: &str) -> bool {
        self.programs.lock().unwrap().contains(program)
    }

    fn run(&self, program: &str, args: &[&str], stdin: Option<&[u8]>) -> io::Result<CmdOutput> {
        self.calls.lock().unwrap().push(RecordedCall {
            program: program.to_owned(),
            args: args.iter().map(|&s| s.to_owned()).collect(),
            stdin: stdin.map(<[u8]>::to_vec),
        });
        if let Some(next) = self.queued.lock().unwrap().pop_front() {
            return next;
        }
        if let Some(handler) = &self.handler {
            return handler(program, args, stdin);
        }
        Ok(CmdOutput::ok(Vec::new()))
    }
}
