# Weft for iOS

Native iOS 17+ SwiftUI app and a Foundation-based `WeftMobileKit` package.
Pair by QR/code, share clipboard text/images and files, control another device,
and receive files while the app is active. iOS suspends background sockets:
this client does not promise background clipboard monitoring or delivery.

## Build and validate on a Mac

Requires full Xcode with an iOS SDK and XcodeGen (`brew install xcodegen`).

```sh
swift test --package-path ios
bash ios/scripts/build-sim.sh
```

The first command tests the portable core on macOS; the second compiles the
UIKit/SwiftUI app **and** share extension for the simulator. Neither command
launches a simulator or modifies a real device. Build outputs stay in
`ios/.build` and `ios/build`; remove them when no build uses them.

For a physical device, open `ios/Weft.xcodeproj` after generation, select a
development team for both targets, and register the `group.eu.frohub.weft`
App Group. The share extension stages text, web links, images and regular files
in that group; open Weft and select the destination to send them. It queues at
most 20 items (200 MiB total), with at most 10 files per share and 100 MiB per
file. Text is limited to 256 KiB. A queued file is removed only after the app
has made a private transfer copy. Finished transfer copies are cleaned during
maintenance; interrupted copies expire after 24 hours. No credentials are
shared with the extension. iOS state is AES-GCM encrypted with a device-bound
Keychain key; development plaintext state is migrated when opened.

This initial implementation has not yet been compiled or run on Apple hardware
from the Windows development host. Do not publish it until both build commands
and pairing/transfer tests on an iOS device pass.
