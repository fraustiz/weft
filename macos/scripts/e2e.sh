#!/bin/bash
# End-to-end check of Weft.app against a local relay and the command-line
# client, through the real pasteboard.
#
#   macos/scripts/build-app.sh      # once, or after a change
#   macos/scripts/e2e.sh
#
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
# 5. `printf 'hello cli' | pbcopy`: `weft listen` must print it.
#
# The pasteboard is saved first (every item and type, scripts/pasteboard.js)
# and given back at the end, after the app has quit so it cannot send it.
# Everything started here is stopped and the temporary directory deleted,
# on success, failure or Ctrl-C. Exits non-zero on failure.
#
# Environment: WEFT_E2E_TIMEOUT, seconds per step (default 20).
set -euo pipefail

macos="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$macos/.." && pwd)"
app="$macos/build/Weft.app"
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

[[ -x "$app/Contents/MacOS/Weft" ]] || { echo "no $app: run scripts/build-app.sh first" >&2; exit 1; }
if pgrep -x Weft >/dev/null; then
    # Another Weft would see the test's copies, and could send them to a
    # real group.
    echo "Weft is already running: quit it first" >&2
    exit 1
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
    if [[ $saved == 1 ]]; then
        osascript -l JavaScript "$macos/scripts/pasteboard.js" restore "$tmp/pasteboard" >/dev/null \
            || echo "warning: the pasteboard could not be restored" >&2
    fi
    if [[ $status != 0 || $failed == 1 ]]; then
        # Logs never hold clipboard text or codes, apart from the two test
        # strings.
        for log in relay app join listen; do
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

osascript -l JavaScript "$macos/scripts/pasteboard.js" save "$tmp/pasteboard" >/dev/null
saved=1

say "relay on 127.0.0.1:$port"
WEFT_BIND="127.0.0.1:$port" WEFT_RELAY_NAME=127.0.0.1 WEFT_DB="$tmp/relay.sqlite3" \
    "$relay_bin" >"$tmp/relay.log" 2>&1 &
pids+=($!)
wait_for "the relay" curl -sf "http://127.0.0.1:$port/healthz" -o /dev/null

say "Weft.app, with a temporary state"
WEFT_DEV_STATE="$tmp/mac" WEFT_DEV_RELAY="$relay_url" WEFT_DEV_CODE_FILE="$tmp/code" \
    WEFT_DEV_START_PAIRING=1 WEFT_LANG=en \
    "$app/Contents/MacOS/Weft" >"$tmp/app.log" 2>&1 &
pids+=($!)
wait_for "a pairing code from the app" test -s "$tmp/code"

say "pairing the command-line client"
"$weft" --state "$tmp/cli" init --relay "$relay_url" --name "e2e CLI" >/dev/null 2>"$tmp/join.log"
"$weft" --state "$tmp/cli" join "$(cat "$tmp/code")" >/dev/null 2>>"$tmp/join.log" \
    || fail "weft join failed"

say "CLI -> Mac: weft send \"hello mac\""
"$weft" --state "$tmp/cli" send "hello mac" 2>>"$tmp/join.log"
pasteboard_is() { [[ "$(pbpaste)" == "$1" ]]; }
wait_for "\"hello mac\" on the pasteboard" pasteboard_is "hello mac"
say "ok: pbpaste shows \"hello mac\""

say "Mac -> CLI: pbcopy \"hello cli\""
"$weft" --state "$tmp/cli" listen >"$tmp/listen.out" 2>"$tmp/listen.log" &
pids+=($!)
wait_for "weft listen to connect" grep -q "listening for clipboard text" "$tmp/listen.log"
printf 'hello cli' | pbcopy
wait_for "\"hello cli\" from weft listen" grep -qF '"text":"hello cli"' "$tmp/listen.out"
say "ok: weft listen printed \"hello cli\""

say "PASS"
