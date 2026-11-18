# Weft for Linux (`weft-desktop`)

`weft-desktop` is the Linux desktop daemon and control CLI for Weft. It runs as a lightweight user service, synchronizes the clipboard across Wayland and X11, forwards desktop notifications, integrates with PipeWire/PulseAudio and MPRIS2 media players, accepts remote pointer/keyboard/presenter input from authorized devices, and exposes a JSON-ready CLI for Waybar, Polybar, and shell bindings.

## Runtime Dependencies

`weft-desktop` is a single static-friendly Rust binary that calls standard desktop utilities directly (never through a shell) when present:

| Feature | Wayland | X11 / Fallback | Package (Debian/Ubuntu) |
| --- | --- | --- | --- |
| Clipboard sync & password-manager exclusion (`x-kde-passwordManagerHint`) | `wl-paste`, `wl-copy` | `xclip` or `xsel` | `wl-clipboard` / `xclip` |
| Desktop notifications & removal | `gdbus` (`org.freedesktop.Notifications`) | `notify-send` | `libglib2.0-bin`, `libnotify-bin` |
| Ring alarm playback | `pw-play` | `paplay` or `aplay` | `pipewire-bin` / `pulseaudio-utils` / `alsa-utils` |
| System volume control | `wpctl` | `pactl` | `wireplumber` / `pulseaudio-utils` |
| Media players (MPRIS2) | `playerctl` | `gdbus` | `playerctl` |
| Remote input & presenter | `ydotool` + `wtype` | `xdotool` | `ydotool`, `wtype` / `xdotool` |
| Shared web links | `xdg-open` | `xdg-open` | `xdg-utils` |

## Installation & `systemd --user` Setup

Place `weft-desktop` on your `PATH` (for example `~/.local/bin/weft-desktop`), then generate the user unit and desktop entry:

```sh
weft-desktop install-desktop --enable
```

Without `--enable`, `install-desktop` only writes:
- `~/.config/systemd/user/weft-desktop.service`
- `~/.local/share/applications/weft-desktop.desktop`

You can inspect logs or manage the daemon with `systemctl --user`:

```sh
systemctl --user status weft-desktop.service
journalctl --user -u weft-desktop.service -f
```

State and IPC credentials (`daemon.json`, mode `0600`) live in `$XDG_STATE_HOME/weft` (default `~/.local/state/weft`, override with `--state-dir <DIR>`).

## Common Commands

```sh
weft-desktop status                  # Human-readable summary (or --json)
weft-desktop pair                    # Show UTF-8 QR code + WEFT1: code and wait
weft-desktop join WEFT1:...          # Join another device's group
weft-desktop devices                 # List paired devices and battery/network info
weft-desktop clip                    # Send current clipboard text immediately
weft-desktop pause / resume          # Pause or resume sync
weft-desktop send <device> file.pdf  # Send files to a paired device
weft-desktop url <device> https://example.com
weft-desktop ring <device>           # Ring a paired phone or computer
weft-desktop found-it                # Stop ringing this computer
weft-desktop commands presets        # List ready-made Linux shortcut commands
weft-desktop control <device> on     # Allow remote touchpad/keyboard from device
weft-desktop update --check          # Check GitHub Releases for updates
```

## Waybar Configuration Snippet

`weft-desktop status --json` outputs `text`, `alt`, `class`, and `tooltip` fields tailored for Waybar's `custom` module:

```jsonc
"custom/weft": {
    "exec": "weft-desktop status --json",
    "return-type": "json",
    "interval": 5,
    "on-click": "weft-desktop clip",
    "on-click-right": "weft-desktop pause"
}
```
