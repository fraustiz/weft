# Code signing policy

[Français](CODE_SIGNING_POLICY.fr.md)

Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org).

> **Status:** Weft is applying to the SignPath Foundation's free code signing program for open-source projects. Until the project is accepted, the Windows executable is published unsigned and SmartScreen warns on first launch. This policy describes how releases are signed once it is.

## What is signed

SignPath Foundation signs one file per release: the Windows app, `weft-X.Y.Z-windows.exe`.

- It is built from the source code of this repository ([github.com/fraustiz/weft](https://github.com/fraustiz/weft), MIT license) and its open-source dependencies, on GitHub-hosted runners, by the [`.github/workflows/app-release.yml`](.github/workflows/app-release.yml) workflow, when a release tag `app-vX.Y.Z` is pushed.
- The workflow submits the executable to SignPath, and an approver (see below) approves every signing request by hand before it is signed. Manual runs of the workflow (dry runs) never submit a signing request.
- The signed executable is the one published on [GitHub Releases](https://github.com/fraustiz/weft/releases), and the one whose SHA-256 digest goes in `windows-update.json`, which installed copies check before updating themselves.
- Its file properties name the product `Weft`, with the release version.

The certificate is never used for anything else: no third-party software, and no build made outside that workflow.

The other apps are signed without SignPath:

- **macOS:** `Weft-X.Y.Z-mac.zip` is ad-hoc signed and not notarized. Its updates are verified by [Sparkle](https://sparkle-project.org) with an EdDSA signature against [`macos/sparkle-public-key.txt`](macos/sparkle-public-key.txt).
- **Android:** `weft-X.Y.Z.apk` is signed with the project's release key.
- **iOS:** `Weft-X.Y.Z-ios.ipa` carries an ad hoc signature only; testers re-sign it with their own Apple ID when they sideload it.

## Team roles

| Role | Members |
| --- | --- |
| Committers and reviewers | [fraustiz](https://github.com/fraustiz) |
| Approvers | [fraustiz](https://github.com/fraustiz) |

- **Committers** may push to the repository. Changes from anyone else are reviewed by a committer before they are merged.
- **Reviewers** review contributions from people outside the team.
- **Approvers** approve each signing request in SignPath, after checking that it comes from a release tag of this repository.

All team members must use multi-factor authentication (MFA) for their GitHub account and their SignPath account.

## Privacy

Weft moves data between your own devices: that is its purpose, so it uses the network. It contains no telemetry, analytics, crash reporting or advertising, and it sends nothing to any system other than those below.

- **Relay.** Paired devices exchange messages through a WebSocket relay. The default is the public relay `wss://weft.frohub.eu/v1`, run by the project; you can run your own ([`relay/README.md`](relay/README.md)). Everything you send (clipboard, files, notifications, device names, other features) is end-to-end encrypted on your device: the relay cannot read it. The relay does see metadata: your IP address, the identifiers of your devices (derived from their public keys) and which group they belong to, and when and how much each device sends to which other device. It stores one record per device (its identifier and group number), deleted when the device leaves its group. When a group member is offline, it keeps the encrypted group-change and device-name messages for it until delivery, for at most 30 days and 64 messages per device. Everything else, files included, is never stored: it is dropped when the recipient is offline. Its logs never contain message content.
- **Local network.** When paired devices share a local network, they tell each other their private IP addresses (end-to-end encrypted, through the relay) and connect directly. Each device listens on a local TCP port and only talks to devices it has paired with.
- **Update checks.** The macOS, Android and Windows apps check [GitHub Releases](https://github.com/fraustiz/weft/releases) for a new version at most once a day, and download it only when you choose to install it. Automatic checks can be turned off in each app's settings. These requests go to GitHub, under [GitHub's privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement). The iOS app does not check for updates.

The website [weft.frohub.eu](https://weft.frohub.eu) measures its audience with [Umami](https://umami.is), self-hosted on the project owner's own server: no cookies, anonymous aggregated statistics only, nothing shared with third parties.

## Uninstalling

Before uninstalling, you can remove the device from your group (in Weft on this device or on another one): the relay then deletes its record.

### Windows

1. In Weft's settings, turn off **Start with Windows**. This removes the `Weft` value under `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`.
2. Quit Weft: right-click its icon in the notification area, then **Quit**.
3. Delete `weft-windows.exe` (and `weft-windows.exe.old` or `weft-windows.exe.new` next to it, if an update left one).
4. Delete the folder `%APPDATA%\weft` (or the folder set in `WEFT_STATE`): it holds this device's keys, paired devices and settings.
5. Delete the registry key `HKEY_CURRENT_USER\Software\Classes\AppUserModelId\Weft.Windows`, which gives Weft's notifications their name and icon.

The same from PowerShell, after quitting Weft:

```powershell
Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name Weft -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force "$env:APPDATA\weft"
Remove-Item -Recurse -Force 'HKCU:\Software\Classes\AppUserModelId\Weft.Windows'
```

If you allowed Weft through Windows Defender Firewall, you can also remove it from the allowed apps. Files you received stay in your Downloads folder.

### macOS

1. In Weft's settings, turn off **Open at Login**.
2. Quit Weft from its menu bar icon (**Quit Weft**).
3. Move `Weft.app` to the Trash.
4. Delete `~/Library/Application Support/Weft`, and, if present, `~/Library/Preferences/fr.frohub.weft.mac.plist` and `~/Library/Caches/fr.frohub.weft.mac`.

### Android

Uninstall Weft like any other app (long-press its icon, then **Uninstall**). Android deletes its data with it; files you received stay in Downloads.
