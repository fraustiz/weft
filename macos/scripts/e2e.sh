#!/bin/bash
# End-to-end check of the macOS app against a local relay and the
# command-line client: the pasteboard both ways, then packets and files.
#
#   WEFT_DEV_BUNDLE=1 macos/scripts/build-app.sh      # once, or after a change
#   macos/scripts/e2e.sh
#
# It runs build/Weft Dev.app, the development copy with its own bundle
# identifier, so an installed Weft keeps its notification permission,
# preferences and Launch Services entry (WEFT_E2E_APP picks another app).
# 1. starts a relay on 127.0.0.1:8095 (WEFT_RELAY_NAME=127.0.0.1, a temporary
#    WEFT_DB);
# 2. launches build/Weft.app with a temporary state directory, pointed at
#    ws://127.0.0.1:8095/v1, and the pairing sheet open. These go through the
#    development-only variables of Sources/Weft/Development.swift
#    (WEFT_DEV_STATE, WEFT_DEV_RELAY, WEFT_DEV_START_PAIRING); with none set,
#    the app uses Application Support and wss://weft.frohub.eu/v1;
# 3. reads the pairing code the app shows from WEFT_DEV_CODE_FILE (a file in
#    the private temporary directory: the only place the app ever writes a
#    code out) and pairs a command-line client with it (`weft init`, then
#    `weft join`);
# 4. `weft send "hello mac"`: the text must reach the pasteboard (pbpaste);
# 5. `weft packet` (a hidden development command) sends the CLI's
#    capabilities, a `ping` and a `share.url`: the app must log both notices
#    in WEFT_DEV_LOG (kinds only; WEFT_DEV_NO_OPEN keeps the link from
#    opening the browser);
# 6. `weft send-file` (hidden too): the file must land, whole, in the
#    app's WEFT_DEV_DOWNLOADS folder, and the app log a fileReceived;
# 7. `printf 'hello cli' | pbcopy`: `weft listen` must print it;
# 8. the app offers WEFT_DEV_SEND_FILE once it exists: `weft listen` must
#    print the `file.offer`. The CLI cannot receive files, so the transfer
#    itself stops there.
#
# The pasteboard steps (4 and 7) are skipped when another Weft runs (an
# installed one, with a real group): it would send the test texts to it.
# The pasteboard is saved first (every item and type, scripts/pasteboard.js)
# and given back at the end, after the app has quit so it cannot send it.
# Everything started here is stopped and the temporary directory deleted,
# on success, failure or Ctrl-C. Exits non-zero on failure.
#
# Environment: WEFT_E2E_TIMEOUT, seconds per step (default 20).
set -euo pipefail

macos="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$macos/.." && pwd)"
app="${WEFT_E2E_APP:-$macos/build/Weft Dev.app}"
port=8095
relay_url="ws://127.0.0.1:$port/v1"
step_timeout="${WEFT_E2E_TIMEOUT:-20}"

say() { printf '==> %s\n' "$*"; }
fail() {
    printf 'FAIL: %s\n' "$*" >&2
    failed=1
    exit 1
}

# --- Preconditions -----------------------------------------------------------

[[ -x "$app/Contents/MacOS/Weft" ]] \
    || { echo "no $app: run WEFT_DEV_BUNDLE=1 scripts/build-app.sh first" >&2; exit 1; }
if pgrep -f "$app/Contents/MacOS/Weft" >/dev/null; then
    echo "$app is already running: quit it first" >&2
    exit 1
fi
pasteboard=1
if pgrep -x Weft >/dev/null; then
    # Another Weft would see the test's copies, and could send them to a
    # real group: the pasteboard is left alone.
    pasteboard=0
    say "another Weft is running: the pasteboard steps are skipped"
fi
if lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
    echo "port $port is already in use" >&2
    exit 1
fi

say "building the relay and the command-line client (debug)"
(cd "$root" && cargo build -q -p weft-relay -p weft-cli)
weft="$root/target/debug/weft"
relay_bin="$root/target/debug/weft-relay"

# --- Cleanup -----------------------------------------------------------------

tmp="$(mktemp -d "${TMPDIR:-/tmp}/weft-e2e.XXXXXX")"
chmod 700 "$tmp"
mkdir -m 700 "$tmp/pasteboard"
pids=()
failed=0
saved=0

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    for pid in "${pids[@]+"${pids[@]}"}"; do
        kill "$pid" 2>/dev/null || true
    done
    for pid in "${pids[@]+"${pids[@]}"}"; do
        # The app closes its connection politely: give it a few seconds.
        for _ in $(seq 50); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
        kill -9 "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    done
    if [[ $saved == 1 && $pasteboard == 1 ]]; then
        osascript -l JavaScript "$macos/scripts/pasteboard.js" restore "$tmp/pasteboard" >/dev/null \
            || echo "warning: the pasteboard could not be restored" >&2
    fi
    if [[ $status != 0 || $failed == 1 ]]; then
        # Logs never hold clipboard text or codes, apart from the two test
        # strings.
        for log in relay app join listen dev; do
            [[ -s "$tmp/$log.log" ]] || continue
            echo "--- $log.log (last lines)" >&2
            tail -n 15 "$tmp/$log.log" | sed 's/WEFT1:[A-Za-z0-9_-]*/WEFT1:<code>/g' >&2
        done
    fi
    rm -rf "$tmp"
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

# Waits until "$@" succeeds, at most $step_timeout seconds.
wait_for() {
    local what="$1"
    shift
    local deadline=$((SECONDS + step_timeout))
    until "$@"; do
        ((SECONDS < deadline)) || fail "timed out waiting for $what"
        sleep 0.2
    done
}

# --- Run ---------------------------------------------------------------------

if [[ $pasteboard == 1 ]]; then
    osascript -l JavaScript "$macos/scripts/pasteboard.js" save "$tmp/pasteboard" >/dev/null
    saved=1
fi

say "relay on 127.0.0.1:$port"
WEFT_BIND="127.0.0.1:$port" WEFT_RELAY_NAME=127.0.0.1 WEFT_DB="$tmp/relay.sqlite3" \
    "$relay_bin" >"$tmp/relay.log" 2>&1 &
pids+=($!)
wait_for "the relay" curl -sf "http://127.0.0.1:$port/healthz" -o /dev/null

say "${app##*/}, with a temporary state"
mkdir -m 700 "$tmp/downloads" "$tmp/outgoing"
WEFT_DEV_STATE="$tmp/mac" WEFT_DEV_RELAY="$relay_url" WEFT_DEV_CODE_FILE="$tmp/code" \
    WEFT_DEV_START_PAIRING=1 WEFT_LANG=en WEFT_DEV_DOWNLOADS="$tmp/downloads" \
    WEFT_DEV_LOG="$tmp/dev.log" WEFT_DEV_NO_OPEN=1 WEFT_DEV_SEND_FILE="$tmp/outgoing/from-mac.txt" \
    "$app/Contents/MacOS/Weft" >"$tmp/app.log" 2>&1 &
pids+=($!)
wait_for "a pairing code from the app" test -s "$tmp/code"

say "pairing the command-line client"
"$weft" --state "$tmp/cli" init --relay "$relay_url" --name "e2e CLI" >/dev/null 2>"$tmp/join.log"
"$weft" --state "$tmp/cli" join "$(cat "$tmp/code")" >/dev/null 2>>"$tmp/join.log" \
    || fail "weft join failed"

cli() { "$weft" --state "$tmp/cli" "$@" 2>>"$tmp/join.log"; }
logged() { grep -qx "$1" "$tmp/dev.log" 2>/dev/null; }

if [[ $pasteboard == 1 ]]; then
    say "CLI -> Mac: weft send \"hello mac\""
    cli send "hello mac"
    pasteboard_is() { [[ "$(pbpaste)" == "$1" ]]; }
    wait_for "\"hello mac\" on the pasteboard" pasteboard_is "hello mac"
    say "ok: pbpaste shows \"hello mac\""
fi

say "CLI -> Mac: capabilities, ping, share.url (weft packet)"
cli packet '{"type":"capabilities","platform":"cli","incoming":["state.request","ping","file.offer"],"outgoing":["state.request","ping","share.url","file.offer","file.ack","file.done","file.cancel"]}'
cli packet '{"type":"ping"}'
wait_for "the app to log the ping" logged "notice ping"
say "ok: the app logged the ping"
cli packet '{"type":"share.url","url":"https://example.org/weft-e2e"}'
wait_for "the app to log the link" logged "notice link"
logged "link not opened (development)" || fail "the link was not handled"
say "ok: the app logged the link (not opened: WEFT_DEV_NO_OPEN)"

say "CLI -> Mac: weft send-file"
head -c 300000 /dev/urandom >"$tmp/from-cli.bin"
cli send-file "$tmp/from-cli.bin"
received() { cmp -s "$tmp/from-cli.bin" "$tmp/downloads/from-cli.bin"; }
wait_for "the file in the app's Downloads" received
wait_for "the app to log the file" logged "notice fileReceived"
say "ok: from-cli.bin (300000 bytes) landed whole in the Downloads folder"

say "Mac -> CLI: weft listen"
"$weft" --state "$tmp/cli" listen >"$tmp/listen.out" 2>"$tmp/listen.log" &
pids+=($!)
wait_for "weft listen to connect" grep -q "listening for clipboard text" "$tmp/listen.log"
if [[ $pasteboard == 1 ]]; then
    printf 'hello cli' | pbcopy
    wait_for "\"hello cli\" from weft listen" grep -qF '"text":"hello cli"' "$tmp/listen.out"
    say "ok: weft listen printed \"hello cli\""
fi
printf 'from the Mac\n' >"$tmp/outgoing/from-mac.txt"
offered() { grep -F '"type":"file.offer"' "$tmp/listen.out" | grep -qF '"name":"from-mac.txt"'; }
wait_for "the app's file.offer at weft listen" offered
say "ok: weft listen got the file.offer (the CLI receives no files: transfer not run)"

say "PASS"
