//! Installs the `systemd --user` service (`weft-desktop.service`) and XDG
//! desktop entry (`weft-desktop.desktop`), enabling the unit only when
//! `--enable` is passed.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::runner::Runner;

/// Paths where desktop integration files are written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopPaths {
    pub service_file: PathBuf,
    pub desktop_file: PathBuf,
}

impl DesktopPaths {
    pub fn from_env() -> Option<Self> {
        let env_dir = |key: &str| {
            std::env::var_os(key)
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty() && p.is_absolute())
        };
        let home = env_dir("HOME").or_else(|| env_dir("USERPROFILE"))?;
        let config_home = env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        let data_home =
            env_dir("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local").join("share"));
        Some(Self::under(&config_home, &data_home))
    }

    pub fn under(config_home: &Path, data_home: &Path) -> Self {
        Self {
            service_file: config_home
                .join("systemd")
                .join("user")
                .join("weft-desktop.service"),
            desktop_file: data_home.join("applications").join("weft-desktop.desktop"),
        }
    }
}

fn quote_exec_arg(exe: &Path) -> String {
    let raw = exe.to_string_lossy();
    let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

pub fn render_systemd_unit(exe: &Path) -> String {
    let exec = quote_exec_arg(exe);
    format!(
        "[Unit]\n\
Description=Weft Desktop Sync Daemon\n\
Documentation=https://github.com/fraustiz/weft\n\
After=graphical-session.target network-online.target\n\
Wants=network-online.target\n\
\n\
[Service]\n\
Type=simple\n\
ExecStart={exec} daemon\n\
Restart=on-failure\n\
RestartSec=5\n\
\n\
[Install]\n\
WantedBy=default.target\n"
    )
}

pub fn render_desktop_entry(exe: &Path) -> String {
    let exec = quote_exec_arg(exe);
    format!(
        "[Desktop Entry]\n\
Type=Application\n\
Version=1.0\n\
Name=Weft\n\
GenericName=Device Sync\n\
Comment=Share clipboard, files, and notifications across your devices\n\
Exec={exec} daemon\n\
Terminal=false\n\
Categories=Utility;Network;\n\
StartupNotify=false\n"
    )
}

/// Writes `weft-desktop.service` and `weft-desktop.desktop`, and optionally
/// reloads & enables the user unit via `systemctl --user` when `enable` is true.
pub fn install_desktop(
    paths: &DesktopPaths,
    exe: &Path,
    enable: bool,
    runner: &dyn Runner,
) -> io::Result<()> {
    if let Some(parent) = paths.service_file.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = paths.desktop_file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&paths.service_file, render_systemd_unit(exe))?;
    fs::write(&paths.desktop_file, render_desktop_entry(exe))?;

    if enable {
        let reload = runner.run("systemctl", &["--user", "daemon-reload"], None)?;
        if !reload.success {
            return Err(io::Error::other(format!(
                "systemctl --user daemon-reload failed: {}",
                String::from_utf8_lossy(&reload.stderr).trim()
            )));
        }
        let enabled = runner.run(
            "systemctl",
            &["--user", "enable", "--now", "weft-desktop.service"],
            None,
        )?;
        if !enabled.success {
            return Err(io::Error::other(format!(
                "systemctl --user enable --now weft-desktop.service failed: {}",
                String::from_utf8_lossy(&enabled.stderr).trim()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::runner::FakeRunner;

    #[test]
    fn writes_unit_and_desktop_files_without_enabling_by_default() {
        let temp = TempDir::new().unwrap();
        let paths = DesktopPaths::under(&temp.path().join("cfg"), &temp.path().join("data"));
        let exe = Path::new("/home/alice/.local/bin/weft-desktop");
        let runner = FakeRunner::new(&["systemctl"]);

        install_desktop(&paths, exe, false, &runner).unwrap();
        assert!(paths.service_file.is_file());
        assert!(paths.desktop_file.is_file());
        assert!(runner.calls().is_empty());

        let unit = fs::read_to_string(&paths.service_file).unwrap();
        assert!(unit.contains("ExecStart=\"/home/alice/.local/bin/weft-desktop\" daemon"));
        let desktop = fs::read_to_string(&paths.desktop_file).unwrap();
        assert!(desktop.contains("Exec=\"/home/alice/.local/bin/weft-desktop\" daemon"));

        install_desktop(&paths, exe, true, &runner).unwrap();
        let calls = runner.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].args, ["--user", "daemon-reload"]);
        assert_eq!(
            calls[1].args,
            ["--user", "enable", "--now", "weft-desktop.service"]
        );
    }
}
