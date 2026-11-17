<p align="center">
  <img src="brand/final/weft-lockup-dark.svg" alt="weft" width="360">
</p>

# Weft

**Your clipboard, files and notifications, woven across your devices — even on networks that keep them apart.**

Weft is an open-source, KDE Connect–style bridge between **Android**, **macOS** and **Windows**. Instead of relying on the local network, devices talk through an end-to-end encrypted relay on the internet, so it keeps working on office, school or hotel Wi-Fi where devices can't see each other.

> **Status: planning.** Nothing is implemented yet. The design is in [`docs/PLAN.md`](docs/PLAN.md) (in French).

## What it will do

| Feature | Milestone |
| --- | --- |
| Clipboard sync (text) — automatic computer → phone, one tap phone → computer | MVP |
| Send files and links between devices | After MVP |
| Android notifications on your computers | V1 |
| Battery level, ring my phone, incoming calls | V1 |
| Media controls | V2 |

On Android, reading the clipboard in the background isn't allowed since Android 10. Like KDE Connect, Weft sends it with one tap: a Quick Settings tile, a notification action or the share sheet. No root, no ADB tricks.

## How it works

```
Android (Kotlin) ─┐                          ┌─ macOS (Swift)
                  ├── WSS 443 ── relay ── WSS 443 ──┤
Windows (Rust) ───┘     (weft.frohub.eu)     └─ …
```

- **Native apps:**
  - Android: Kotlin + Jetpack Compose, Material 3.
  - macOS: Swift + SwiftUI.
  - Windows: Rust + WinUI 3, via [windows-reactor](https://github.com/microsoft/windows-rs/tree/master/crates/libs/reactor).
- **Relay:** written in Rust, shipped as a Docker image. A public instance will run at `weft.frohub.eu`, and anyone can self-host their own; the relay address is configurable in the apps.
- **No accounts:** each install has its own keys, and devices pair with a QR code.

## Security model

- **End-to-end encryption** with libsodium (XChaCha20-Poly1305 with authenticated headers, `crypto_kx` session keys). The relay never sees clipboard contents, files or notifications. It only sees which devices talk to each other, when, and how much.
- **The relay is not trusted.** Each device keeps its own signed list of the devices in its group, so the relay cannot add or swap a device.
- **Pairing:**
  - The QR code carries a one-time secret that never goes through the relay.
  - Without a camera, devices compare a 6-digit code backed by a commitment, as in Bluetooth numeric comparison.
- **Known limit:** no forward secrecy in v1.

## Distribution

No app stores for now. Builds will be published on GitHub Releases:
- the macOS app is not notarized: open it with right-click → Open;
- the Windows build is unsigned, so SmartScreen will warn on first launch.

### Releases and updates

The macOS and Android apps update themselves from GitHub Releases. The Mac app uses [Sparkle](https://sparkle-project.org) and reads `releases/latest/download/appcast.xml`; the Android app reads `releases/latest/download/android-update.json`.

To cut a release, push a tag `app-vX.Y.Z`:

```sh
git tag app-v1.2.0 && git push origin app-v1.2.0
```

[`.github/workflows/app-release.yml`](.github/workflows/app-release.yml) builds both apps at version `X.Y.Z` (build number and `versionCode`: the commit count), and publishes the release with `Weft-X.Y.Z-mac.zip`, `appcast.xml`, `weft-X.Y.Z.apk` and `android-update.json`. Run it by hand (Actions › App release › Run workflow) for a dry run that builds and signs without publishing.

Repository secrets it needs: `SPARKLE_ED_PRIVATE_KEY`, `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`.

Mac updates are checked with an EdDSA key: Sparkle installs an update only if its archive is signed with the private key matching the public key built into the app. That public key is [`macos/sparkle-public-key.txt`](macos/sparkle-public-key.txt). The private key, and the Android keystore with its passwords, stay with the maintainer, in a keychain or password manager, and in the repository secrets: never in the repository. Lose the Sparkle private key and installed Macs can no longer update themselves; leak it and anyone can ship them an update. `macos/scripts/update-e2e.sh` checks the whole update locally, with a temporary key.

## Brand

The logo is hand-drawn SVG in two colours: sunset orange `#F26A21` on ink `#17130F` or cream `#F6EFE6`. No AI-generated images. See [`brand/`](brand/).

## License

[MIT](LICENSE) — the whole project: apps, relay, protocol, logo and docs.
