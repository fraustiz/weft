#!/bin/bash
# End-to-end check of in-app updates (Sparkle) on this Mac, with no
# network but the loopback and nothing of an installed Weft touched.
#
#   macos/scripts/build-app.sh          # once, or after a change
#   macos/scripts/update-e2e.sh [path/to/Weft.app]
#
# From the built app it makes two copies under its own identifier
# (fr.frohub.weft.mac.updatetest), in a temporary directory: A (build 1,
# 0.0.1) and B (build 2, 0.0.2). Each gets a temporary EdDSA key, its feed
# at http://127.0.0.1:$port/appcast.xml, a state directory of its own
# (Info.plist LSEnvironment: WEFT_DEV_STATE, WEFT_DEV_RELAY on a closed
# port, so no identity is made on the public relay) and WEFT_DEV_UPDATE=now
# (Sources/Weft/Development.swift: check at launch, install at once).
# Both are signed ad hoc again, as a release is.
# 1. B is zipped as the release workflow does (ditto -c -k --keepParent),
#    and listed by generate_appcast with the temporary private key given on
#    stdin; a local HTTP server serves both;
# 2. A is launched: Sparkle must fetch the appcast, download B, check its
#    EdDSA signature, install it in place of A and relaunch it. Passes when
#    the bundle on disk is build 2, still valid for codesign, and the app
#    running from it is build 2 in a new process;
# 3. C (build 3), signed with ANOTHER key, replaces B in the appcast; B is
#    launched again: Sparkle must download C and refuse it, the bundle on
#    disk staying build 2.
#
# Everything started here is stopped, and the temporary directory, the
# test identifier's preferences and caches, and Launch Services' entries
# for the copies, deleted, on success, failure or Ctrl-C. Exits non-zero on
# failure. Environment: WEFT_UPDATE_E2E_PORT (default 18765).
set -euo pipefail

macos="$(cd "$(dirname "$0")/.." && pwd)"
source_app="${1:-$macos/build/Weft.app}"
tools="$macos/.build/artifacts/sparkle/Sparkle/bin"
port="${WEFT_UPDATE_E2E_PORT:-18765}"
id=fr.frohub.weft.mac.updatetest
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

say() { printf '==> %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

[[ -x "$source_app/Contents/MacOS/Weft" ]] || fail "no app at $source_app: run scripts/build-app.sh"
[[ -d "$source_app/Contents/Frameworks/Sparkle.framework" ]] || fail "$source_app has no Sparkle.framework"
[[ -x "$tools/generate_appcast" ]] || fail "no Sparkle tools in $tools: swift package resolve"
if lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
    fail "port $port is taken: set WEFT_UPDATE_E2E_PORT"
fi

tmp="$(mktemp -d -t weft-update-e2e)"
server=""
appcast_cache="$HOME/Library/Caches/Sparkle_generate_appcast"
caches_before="$(ls "$appcast_cache" 2>/dev/null || true)"
cleanup() {
    osascript -e "tell application id \"$id\" to quit" >/dev/null 2>&1 || true
    sleep 1
    pkill -f "$tmp/install/WeftUpdateTest.app/Contents/" 2>/dev/null || true
    [[ -n "$server" ]] && kill "$server" 2>/dev/null || true
    for app in "$tmp"/*/WeftUpdateTest.app; do "$lsregister" -u "$app" >/dev/null 2>&1 || true; done
    defaults delete "$id" >/dev/null 2>&1 || true
    rm -rf "$HOME/Library/Caches/$id" "$HOME/Library/HTTPStorages/$id" "$HOME/Library/Preferences/$id.plist"
    # What generate_appcast cached for these archives, and only that.
    for entry in $(ls "$appcast_cache" 2>/dev/null || true); do
        grep -qxF "$entry" <<<"$caches_before" || rm -rf "${appcast_cache:?}/$entry"
    done
    [[ -n "$caches_before" ]] || rmdir "$appcast_cache" 2>/dev/null || true
    rm -rf "$tmp"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

# An Ed25519 key pair in Sparkle's format: the 32-byte seed and the public
# key, base64. CryptoKit, so nothing to install.
key_pair() {
    swift -e 'import CryptoKit
let key = Curve25519.Signing.PrivateKey()
print(key.rawRepresentation.base64EncodedString())
print(key.publicKey.rawRepresentation.base64EncodedString())' 2>/dev/null
}
say "temporary keys"
{ read -r private_key; read -r public_key; } < <(key_pair)
{ read -r wrong_key; read -r _; } < <(key_pair)
[[ -n "$public_key" && -n "$wrong_key" ]] || fail "could not make a key pair (swift -e)"
(umask 077; printf '%s\n' "$private_key" >"$tmp/private.txt"; printf '%s\n' "$wrong_key" >"$tmp/wrong.txt")

# copy DIR BUILD VERSION: the test copy of the app in DIR.
copy() {
    local app="$1/WeftUpdateTest.app"
    mkdir -p "$1"
    ditto "$source_app" "$app"
    local plist="$app/Contents/Info.plist"
    plutil -replace CFBundleIdentifier -string "$id" "$plist"
    plutil -replace CFBundleVersion -string "$2" "$plist"
    plutil -replace CFBundleShortVersionString -string "$3" "$plist"
    plutil -replace SUFeedURL -string "http://127.0.0.1:$port/appcast.xml" "$plist"
    plutil -replace SUPublicEDKey -string "$public_key" "$plist"
    plutil -replace SUAutomaticallyUpdate -bool YES "$plist"
    plutil -replace LSEnvironment -json "{
        \"WEFT_DEV_STATE\": \"$tmp/state\",
        \"WEFT_DEV_DOWNLOADS\": \"$tmp/downloads\",
        \"WEFT_DEV_RELAY\": \"ws://127.0.0.1:9/v1\",
        \"WEFT_DEV_NO_OPEN\": \"1\",
        \"WEFT_DEV_UPDATE\": \"now\"
    }" "$plist"
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :SUPublicEDKey' "$app/Contents/Info.plist")" == "$public_key" ]] \
        || fail "could not edit $app/Contents/Info.plist"
    codesign --force --sign - --timestamp=none "$app" 2>/dev/null
    codesign --verify --deep --strict "$app" || fail "$app does not verify"
}

# appcast DIR KEYFILE: zips DIR's app into www and lists it there.
appcast() {
    local version
    version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$1/WeftUpdateTest.app/Contents/Info.plist")"
    rm -rf "$tmp/www" && mkdir -p "$tmp/www"
    (cd "$1" && ditto -c -k --keepParent WeftUpdateTest.app "$tmp/www/WeftUpdateTest-$version-mac.zip")
    "$tools/generate_appcast" --ed-key-file - --download-url-prefix "http://127.0.0.1:$port/" \
        -o "$tmp/www/appcast.xml" "$tmp/www" <"$2" >/dev/null
}

installed_build() {
    /usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$tmp/install/WeftUpdateTest.app/Contents/Info.plist"
}

# The pid and build Launch Services shows for the running test app.
running() {
    lsappinfo info -only pid -app "$id" 2>/dev/null | sed -n 's/.*=\([0-9][0-9]*\).*/\1/p'
}

say "A (build 1) and B (build 2)"
mkdir -p "$tmp/state" "$tmp/downloads"
copy "$tmp/install" 1 0.0.1
copy "$tmp/b" 2 0.0.2
appcast "$tmp/b" "$tmp/private.txt"
grep -q 'sparkle:edSignature=' "$tmp/www/appcast.xml" || fail "the appcast has no EdDSA signature"

say "serving http://127.0.0.1:$port/"
# The server serves whatever $tmp/www holds when asked.
python3 -I -m http.server "$port" --bind 127.0.0.1 --directory "$tmp/www" >"$tmp/server.log" 2>&1 &
server=$!
for _ in $(seq 1 50); do curl -fsS -o /dev/null "http://127.0.0.1:$port/appcast.xml" 2>/dev/null && break; sleep 0.2; done

say "launching A: it must update itself to B and relaunch"
open -n "$tmp/install/WeftUpdateTest.app"
first=""
for _ in $(seq 1 50); do first="$(running)"; [[ -n "$first" ]] && break; sleep 0.2; done
[[ -n "$first" ]] || fail "A did not start"
for _ in $(seq 1 90); do [[ "$(installed_build)" == 2 ]] && break; sleep 1; done
[[ "$(installed_build)" == 2 ]] || fail "A was not replaced by B (server log: $(cat "$tmp/server.log"))"
codesign --verify --deep --strict "$tmp/install/WeftUpdateTest.app" || fail "the installed B does not verify"
second=""
for _ in $(seq 1 30); do
    second="$(running)"
    [[ -n "$second" && "$second" != "$first" ]] && break
    sleep 1
done
[[ -n "$second" && "$second" != "$first" ]] || fail "B was installed but not relaunched"
running_version="$(lsappinfo list | grep -A8 "\"$id\"" | sed -n 's/.*Version="\([^"]*\)".*/\1/p' | head -1)"
[[ "$running_version" == 2 ]] || fail "the relaunched app is build $running_version, not 2"
say "OK: A (pid $first) installed B and relaunched it (pid $second, build 2)"

say "C (build 3) signed with another key: B must refuse it"
osascript -e "tell application id \"$id\" to quit" >/dev/null 2>&1 || true
for _ in $(seq 1 30); do [[ -z "$(running)" ]] && break; sleep 0.5; done
copy "$tmp/c" 3 0.0.3
appcast "$tmp/c" "$tmp/wrong.txt"
: >"$tmp/server.log"
open "$tmp/install/WeftUpdateTest.app"
for _ in $(seq 1 60); do grep -q 'GET /WeftUpdateTest-0.0.3-mac.zip' "$tmp/server.log" && break; sleep 1; done
grep -q 'GET /WeftUpdateTest-0.0.3-mac.zip' "$tmp/server.log" || fail "B did not download C"
sleep 10
[[ "$(installed_build)" == 2 ]] || fail "C, signed with another key, was installed"
say "OK: C was downloaded and refused, B stays"
say "passed"
