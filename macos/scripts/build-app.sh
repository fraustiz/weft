#!/bin/bash
# Builds Weft.app, the macOS app, with the Command Line Tools only.
#
#   macos/scripts/build-app.sh
#
# 1. the Rust core, release, universal (scripts/build-rust.sh), and its
#    Swift bindings;
# 2. `swift build -c release` for arm64 and x86_64, joined with lipo;
# 3. build/Weft.app: Info.plist (fr.frohub.weft.mac, LSUIElement, macOS 14
#    or later, version from Cargo and Git), the app icon rendered from
#    brand/final/weft-icon-dark.svg and the menu bar template from
#    brand/final/weft-mark.svg (headless Chrome, then sips and iconutil);
# 4. mediaremote-adapter (BSD-3-Clause, macos/THIRD_PARTY_NOTICES.md), which
#    reads what plays on this Mac: fetched at a pinned commit into
#    build/vendor, built with CMake (universal), and bundled as
#    Contents/Frameworks/MediaRemoteAdapter.framework,
#    Contents/Helpers/MediaRemoteAdapterTestClient and
#    Contents/Resources/MediaRemoteAdapter/ (its Perl script and licence);
# 5. Sparkle.framework (in-app updates, MIT, pinned in Package.swift), with
#    its Autoupdate tool, Updater.app and XPC services, in
#    Contents/Frameworks; Info.plist gets the appcast's URL and the EdDSA
#    public key updates are checked with (Weft.app only, see below);
# 6. an ad-hoc signature (codesign -s -), inside out: not signed by a
#    developer, so the first launch from Finder needs right-click › Open.
#    Sparkle accepts an ad-hoc signed update because the archive is signed
#    with the EdDSA key: the key, not the code signature, carries the trust.
#
# Idempotent: run it again after any change. The icons are rendered again
# only when their SVG changed. Prints the path of the app.
#
# Needs CMake for the adapter (brew install cmake).
#
# Environment: WEFT_ARCHS="aarch64-apple-darwin" for an arm64-only build;
# CHROME=/path/to/chrome if Chrome is elsewhere; WEFT_DEV_BUNDLE=1 for
# build/Weft Dev.app instead, a development copy with its own identifier
# (fr.frohub.weft.mac.dev): it shares no notification permission,
# preferences or Launch Services entry with an installed Weft, so it can
# run next to it (scripts/e2e.sh uses it). It never updates itself.
#
# Versions: CFBundleShortVersionString is WEFT_VERSION if set (the release
# workflow passes X.Y.Z from the tag app-vX.Y.Z), otherwise the version of
# crates/weft-ffi; CFBundleVersion is the number of commits, which only
# grows: Sparkle compares it to pick the newer build.
#
# Updates: the EdDSA public key is SPARKLE_PUBLIC_ED_KEY if set, otherwise
# macos/sparkle-public-key.txt. Without one the app is built without
# SUFeedURL and does not update itself; a release build (WEFT_VERSION set)
# without one fails.
set -euo pipefail

macos="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$macos/.." && pwd)"
brand="$root/brand/final"
build="$macos/build"
if [[ "${WEFT_DEV_BUNDLE:-}" == 1 ]]; then
    app="$build/Weft Dev.app"
    bundle_id=fr.frohub.weft.mac.dev
    display_name="Weft Dev"
else
    app="$build/Weft.app"
    bundle_id=fr.frohub.weft.mac
    display_name=Weft
fi
icons="$build/icons"
chrome="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
read -r -a rust_archs <<<"${WEFT_ARCHS:-aarch64-apple-darwin x86_64-apple-darwin}"
feed_url=https://github.com/fraustiz/weft/releases/latest/download/appcast.xml

if [[ -n "${WEFT_VERSION:-}" && ! "$WEFT_VERSION" =~ ^[0-9]+(\.[0-9]+){0,2}$ ]]; then
    echo "WEFT_VERSION must look like 1.2.3, not $WEFT_VERSION" >&2
    exit 1
fi
public_key="${SPARKLE_PUBLIC_ED_KEY:-}"
if [[ -z "$public_key" && -s "$macos/sparkle-public-key.txt" ]]; then
    public_key="$(tr -d '[:space:]' <"$macos/sparkle-public-key.txt")"
fi
if [[ -n "$public_key" && ! "$public_key" =~ ^[A-Za-z0-9+/]{43}=$ ]]; then
    echo "the Sparkle public key is not a base64 Ed25519 key: $public_key" >&2
    exit 1
fi
if [[ -n "${WEFT_VERSION:-}" && -z "$public_key" && "${WEFT_DEV_BUNDLE:-}" != 1 ]]; then
    echo "a release build needs the Sparkle public key (macos/sparkle-public-key.txt or SPARKLE_PUBLIC_ED_KEY)" >&2
    exit 1
fi

"$macos/scripts/build-rust.sh" release

echo "==> swift build (release)"
swift_archs=()
for rust_arch in "${rust_archs[@]}"; do
    arch="${rust_arch%%-*}"
    [[ "$arch" == aarch64 ]] && arch=arm64
    swift_archs+=(--arch "$arch")
done
(cd "$macos" && swift build -c release "${swift_archs[@]}" --product Weft)
binary="$(cd "$macos" && swift build -c release "${swift_archs[@]}" --show-bin-path)/Weft"

# mediaremote-adapter v0.7.7. To move to another version, change the commit
# and read its README for changes to the `stream`, `send` and `seek`
# commands (Sources/Weft/NowPlaying.swift, Sources/WeftKit/NowPlayingAdapter.swift).
adapter_repo=https://github.com/ungive/mediaremote-adapter.git
adapter_commit=e3ff5021eb0875858bd05f48d2e9ba2e962d1cf6
adapter_src="$build/vendor/mediaremote-adapter"
adapter_build="$build/vendor/mediaremote-adapter-build"
echo "==> mediaremote-adapter ${adapter_commit:0:12}"
command -v cmake >/dev/null || { echo "CMake is needed to build mediaremote-adapter: brew install cmake" >&2; exit 1; }
if [[ "$(git -C "$adapter_src" rev-parse HEAD 2>/dev/null)" != "$adapter_commit" ]]; then
    if [[ ! -d "$adapter_src/.git" ]]; then
        rm -rf "$adapter_src"
        mkdir -p "$build/vendor"
        git clone --quiet "$adapter_repo" "$adapter_src"
    fi
    git -C "$adapter_src" cat-file -e "$adapter_commit^{commit}" 2>/dev/null \
        || git -C "$adapter_src" fetch --quiet origin
    git -C "$adapter_src" -c advice.detachedHead=false checkout --quiet --force "$adapter_commit"
    rm -rf "$adapter_build"
fi
# Exactly the pinned sources, nothing changed in them.
[[ "$(git -C "$adapter_src" rev-parse HEAD)" == "$adapter_commit" \
    && -z "$(git -C "$adapter_src" status --porcelain)" ]] \
    || { echo "$adapter_src is not a clean checkout of $adapter_commit" >&2; exit 1; }
# Its CMakeLists builds x86_64 and arm64 whatever WEFT_ARCHS says.
# Its compiler warnings are upstream's: shown only if the build fails.
adapter_log="$build/vendor/mediaremote-adapter-build.log"
if ! { cmake -S "$adapter_src" -B "$adapter_build" -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_OSX_DEPLOYMENT_TARGET=14.0 && cmake --build "$adapter_build"; } >"$adapter_log" 2>&1; then
    cat "$adapter_log" >&2
    exit 1
fi
adapter_framework="$adapter_build/MediaRemoteAdapter.framework"
adapter_test_client="$adapter_build/MediaRemoteAdapterTestClient"
for file in "$adapter_framework/MediaRemoteAdapter" "$adapter_test_client"; do
    [[ -x "$file" ]] || { echo "the adapter build made no $file" >&2; exit 1; }
done

# Sparkle.framework, universal, from the XCFramework SwiftPM fetched for the
# version pinned in Package.swift.
sparkle_framework="$macos/.build/artifacts/sparkle/Sparkle/Sparkle.xcframework/macos-arm64_x86_64/Sparkle.framework"
[[ -x "$sparkle_framework/Versions/B/Sparkle" ]] \
    || { echo "no Sparkle.framework at $sparkle_framework (swift package resolve)" >&2; exit 1; }

# Renders an HTML page with headless Chrome into a PNG of size $3.
render() {
    "$chrome" --headless --disable-gpu --hide-scrollbars --allow-file-access-from-files \
        --default-background-color=00000000 --window-size="$3,$3" \
        --screenshot="$2" "file://$1" >/dev/null 2>&1
    [[ -s "$2" ]] || { echo "rendering $1 failed" >&2; exit 1; }
}

mkdir -p "$icons"
if [[ ! -s "$icons/Weft.icns" || "$brand/weft-icon-dark.svg" -nt "$icons/Weft.icns" \
    || "$0" -nt "$icons/Weft.icns" ]]; then
    echo "==> app icon"
    # The square icon inside the macOS grid: 824 of 1024 points, rounded.
    cat >"$icons/icon.html" <<EOF
<!doctype html><html><head><style>
html, body { margin: 0; background: transparent; }
div { position: absolute; left: 100px; top: 100px; width: 824px; height: 824px;
      border-radius: 185px; overflow: hidden; }
img { width: 100%; height: 100%; display: block; }
</style></head><body><div><img src="file://$brand/weft-icon-dark.svg"></div></body></html>
EOF
    render "$icons/icon.html" "$icons/icon-1024.png" 1024
    iconset="$icons/Weft.iconset"
    rm -rf "$iconset" && mkdir -p "$iconset"
    for size in 16 32 128 256 512; do
        sips -z "$size" "$size" "$icons/icon-1024.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
        double=$((size * 2))
        sips -z "$double" "$double" "$icons/icon-1024.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
    done
    iconutil -c icns "$iconset" -o "$icons/Weft.icns"
fi

if [[ ! -s "$icons/MenuBarIcon@2x.png" || "$brand/weft-mark.svg" -nt "$icons/MenuBarIcon@2x.png" \
    || "$0" -nt "$icons/MenuBarIcon@2x.png" ]]; then
    echo "==> menu bar icon"
    # The mark alone, black on transparent (a template image), cropped to
    # the square 80 units wide around it, drawn ten times too large.
    cat >"$icons/mark.html" <<EOF
<!doctype html><html><head><style>
html, body { margin: 0; background: transparent; overflow: hidden; }
img { position: absolute; width: 450px; height: 450px; left: -31.5px; top: -63px; }
</style></head><body><img src="file://$brand/weft-mark.svg"></body></html>
EOF
    render "$icons/mark.html" "$icons/mark-360.png" 360
    sips -z 36 36 "$icons/mark-360.png" --out "$icons/MenuBarIcon@2x.png" >/dev/null
    sips -z 18 18 "$icons/mark-360.png" --out "$icons/MenuBarIcon.png" >/dev/null
fi

echo "==> ${app##*/}"
version="${WEFT_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/crates/weft-ffi/Cargo.toml" | head -1)}"
build_number="$(git -C "$root" rev-list --count HEAD 2>/dev/null || echo 1)"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/Weft"
cp "$icons/Weft.icns" "$icons/MenuBarIcon.png" "$icons/MenuBarIcon@2x.png" "$app/Contents/Resources/"
# The adapter: the framework is loaded by /usr/bin/perl, never linked.
mkdir -p "$app/Contents/Frameworks" "$app/Contents/Helpers" "$app/Contents/Resources/MediaRemoteAdapter"
ditto "$adapter_framework" "$app/Contents/Frameworks/MediaRemoteAdapter.framework"
cp "$adapter_test_client" "$app/Contents/Helpers/"
cp "$adapter_src/bin/mediaremote-adapter.pl" "$adapter_src/LICENSE" "$app/Contents/Resources/MediaRemoteAdapter/"
# Sparkle, linked by the app (@rpath = @executable_path/../Frameworks).
ditto "$sparkle_framework" "$app/Contents/Frameworks/Sparkle.framework"
# Updates: the release app only. The development copy has no feed, so it
# never replaces itself, let alone an installed Weft.
updates_plist=""
if [[ "${WEFT_DEV_BUNDLE:-}" != 1 && -n "$public_key" ]]; then
    updates_plist="    <key>SUFeedURL</key><string>$feed_url</string>
    <key>SUPublicEDKey</key><string>$public_key</string>
    <key>SUEnableAutomaticChecks</key><true/>
    <key>SUScheduledCheckInterval</key><integer>86400</integer>
"
elif [[ "${WEFT_DEV_BUNDLE:-}" != 1 ]]; then
    echo "warning: no Sparkle public key, this build will not update itself" >&2
fi
cp "$macos/THIRD_PARTY_NOTICES.md" "$app/Contents/Resources/"
# The languages of the interface (WeftKit/Language.swift), so the system's
# own menus and panels follow them too.
for language in en fr; do
    mkdir -p "$app/Contents/Resources/$language.lproj"
    printf '"CFBundleDisplayName" = "%s";\n' "$display_name" >"$app/Contents/Resources/$language.lproj/InfoPlist.strings"
done
cat >"$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>$display_name</string>
    <key>CFBundleExecutable</key><string>Weft</string>
    <key>CFBundleIconFile</key><string>Weft</string>
    <key>CFBundleIdentifier</key><string>$bundle_id</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleLocalizations</key><array><string>en</string><string>fr</string></array>
    <key>CFBundleName</key><string>Weft</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$build_number</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>Copyright © 2026 Fraustiz. MIT License.</string>
$updates_plist</dict>
</plist>
EOF
plutil -lint -s "$app/Contents/Info.plist"

echo "==> codesign (ad hoc)"
# Inside out: what the app holds first, the app last. Sparkle in the order
# its documentation gives for signing it again (Documentation › Sandboxing
# and Code Signing): XPC services, Autoupdate, Updater.app, the framework;
# the Downloader keeps its entitlements.
sparkle="$app/Contents/Frameworks/Sparkle.framework"
codesign --force --sign - --timestamp=none -o runtime "$sparkle/Versions/B/XPCServices/Installer.xpc"
codesign --force --sign - --timestamp=none -o runtime --preserve-metadata=entitlements \
    "$sparkle/Versions/B/XPCServices/Downloader.xpc"
codesign --force --sign - --timestamp=none -o runtime "$sparkle/Versions/B/Autoupdate"
codesign --force --sign - --timestamp=none -o runtime "$sparkle/Versions/B/Updater.app"
codesign --force --sign - --timestamp=none -o runtime "$sparkle"
codesign --force --sign - --timestamp=none "$app/Contents/Frameworks/MediaRemoteAdapter.framework"
codesign --force --sign - --timestamp=none "$app/Contents/Helpers/MediaRemoteAdapterTestClient"
codesign --force --sign - --timestamp=none "$app"
codesign --verify --deep --strict "$app"

# A build is not an install: Launch Services must not take it for the
# copy in Applications ("Open at Login" would find two and give up).
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -u "$app" 2>/dev/null || true

echo "==> $(lipo -archs "$app/Contents/MacOS/Weft"), version $version ($build_number)"
echo "    adapter: $(lipo -archs "$app/Contents/Frameworks/MediaRemoteAdapter.framework/MediaRemoteAdapter")"
echo "    Sparkle: $(lipo -archs "$sparkle/Versions/B/Sparkle"), $([[ -n "$updates_plist" ]] && echo "updates from $feed_url" || echo "no updates")"
echo "$app"
