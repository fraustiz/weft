#!/bin/bash
# Renders the iOS app icon (1024x1024, opaque) from brand/final/weft-icon-dark.svg
# with headless Chrome, like macos/scripts/build-app.sh. Run it again after the
# SVG changes. CHROME=/path/to/chrome if Chrome is elsewhere.
set -euo pipefail
cd "$(dirname "$0")/.."
svg="$(cd ../brand/final && pwd)/weft-icon-dark.svg"
out="Resources/Assets.xcassets/AppIcon.appiconset/AppIcon.png"
chrome="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cat >"$tmp/icon.html" <<HTML
<!doctype html><html><head><style>html,body{margin:0;background:#17130F}
img{display:block;width:1024px;height:1024px}</style></head>
<body><img src="file://$svg"></body></html>
HTML
"$chrome" --headless --disable-gpu --hide-scrollbars --allow-file-access-from-files \
    --default-background-color=17130Fff --window-size=1024,1024 \
    --screenshot="$tmp/icon.png" "file://$tmp/icon.html" >/dev/null 2>&1
[[ -s "$tmp/icon.png" ]] || { echo "rendering $svg failed" >&2; exit 1; }
cp "$tmp/icon.png" "$out"
echo "$out"
