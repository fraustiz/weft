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
# 4. an ad-hoc signature (codesign -s -): not signed by a developer, so the
#    first launch from Finder needs right-click › Open.
#
# Idempotent: run it again after any change. The icons are rendered again
# only when their SVG changed. Prints the path of the app.
#
# Environment: WEFT_ARCHS="aarch64-apple-darwin" for an arm64-only build;
# CHROME=/path/to/chrome if Chrome is elsewhere; WEFT_DEV_BUNDLE=1 for
# build/Weft Dev.app instead, a development copy with its own identifier
# (fr.frohub.weft.mac.dev): it shares no notification permission,
# preferences or Launch Services entry with an installed Weft, so it can
# run next to it (scripts/e2e.sh uses it).
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
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/crates/weft-ffi/Cargo.toml" | head -1)"
build_number="$(git -C "$root" rev-list --count HEAD 2>/dev/null || echo 1)"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/Weft"
cp "$icons/Weft.icns" "$icons/MenuBarIcon.png" "$icons/MenuBarIcon@2x.png" "$app/Contents/Resources/"
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
</dict>
</plist>
EOF
plutil -lint -s "$app/Contents/Info.plist"

echo "==> codesign (ad hoc)"
codesign --force --sign - --timestamp=none "$app"
codesign --verify --strict "$app"

echo "==> $(lipo -archs "$app/Contents/MacOS/Weft"), version $version ($build_number)"
echo "$app"
