#!/usr/bin/env bash
#
# End-to-end test of the Android app against the real relay and the CLI, on this computer.
#
#   1. builds weft-relay and weft (cargo) and installs the debug app and its tests (Gradle);
#   2. runs a relay on 127.0.0.1:8090 and makes it reachable from the device (adb reverse);
#   3. the CLI (device A) shows a pairing code, the app (device B) joins with it; each one then
#      has the other's name (Device name envelopes): `weft devices` shows the phone's name and
#      the app stores the CLI's;
#   4. the app sends its clipboard through the "Send clipboard" notification action, and
#      `weft listen` must print it; the app is renamed, and `weft devices` must show the new name;
#   5. `weft send` sends text, and the app must put it on the device clipboard (read back by an
#      instrumented test with the app's activity focused, as any foreground app reads it);
#   6. invitation: a fresh CLI device D shows a pairing code and the app, already in A's group,
#      adds D to it instead of leaving it. `weft devices` on D must list the phone and A, A must
#      trust D, and text sent by D must reach A (`weft listen`) and the device clipboard.
#
# The app steps are EndToEndTest methods, run with `am instrument` and their input passed as
# instrumentation arguments. Each step is a separate process, so the pairing is also reloaded
# from disk in between.
#
# Requirements: the emulator online in adb (e.g.
#   $ANDROID_HOME/emulator/emulator -avd weft-pixel -no-window -no-audio -no-snapshot-save &),
# Rust (cargo), a JDK. Every adb command targets that one device, emulator-5554 unless
# WEFT_E2E_SERIAL names another: other devices connected to adb are never touched. JAVA_HOME
# and ANDROID_HOME default to the Homebrew JDK 21 and ~/Library/Android/sdk. WEFT_E2E_PORT
# overrides the port (8090).
#
# Usage: android/scripts/e2e.sh        Exits 0 on success, non-zero on the first failure.
# Everything it starts (relay, CLI processes, adb reverse) and its temporary directory are
# cleaned up on exit, and the app's data is cleared.

set -euo pipefail

export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home}"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
SERIAL="${WEFT_E2E_SERIAL:-emulator-5554}"
# Also read by adb itself and by Gradle's device tasks.
export ANDROID_SERIAL="$SERIAL"
ADB_BIN="$ANDROID_HOME/platform-tools/adb"
PORT="${WEFT_E2E_PORT:-8090}"
RELAY_URL="ws://127.0.0.1:$PORT/v1"
APP_ID=fr.frohub.weft
RUNNER="$APP_ID.test/androidx.test.runner.AndroidJUnitRunner"
TEST_CLASS="$APP_ID.EndToEndTest"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANDROID_DIR="$(dirname "$SCRIPT_DIR")"
REPO_DIR="$(dirname "$ANDROID_DIR")"
WEFT="$REPO_DIR/target/debug/weft"
RELAY="$REPO_DIR/target/debug/weft-relay"

TMP="$(mktemp -d -t weft-e2e)"
STATE_A="$TMP/mac"
STATE_D="$TMP/desk"
PIDS=()

# adb, always on $SERIAL.
adb_() { "$ADB_BIN" -s "$SERIAL" "$@"; }

log() { printf '\n== %s\n' "$*"; }
fail() {
    printf '\nFAIL: %s\n' "$*" >&2
    exit 1
}

cleanup() {
    local status=$?
    for pid in "${PIDS[@]+"${PIDS[@]}"}"; do
        kill "$pid" 2>/dev/null || true
    done
    for pid in "${PIDS[@]+"${PIDS[@]}"}"; do
        wait "$pid" 2>/dev/null || true
    done
    adb_ reverse --remove "tcp:$PORT" >/dev/null 2>&1 || true
    adb_ shell pm clear "$APP_ID" >/dev/null 2>&1 || true
    rm -rf "$TMP"
    if [ "$status" -eq 0 ]; then echo "PASS"; fi
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

# Waits up to $1 seconds for the command in the remaining arguments to succeed.
wait_for() {
    local seconds=$1
    shift
    local deadline=$((SECONDS + seconds))
    until "$@"; do
        [ "$SECONDS" -lt "$deadline" ] || return 1
        sleep 0.5
    done
}

# Runs one EndToEndTest method; the remaining arguments are `-e key value` pairs, already
# quoted for the device shell. Fails unless exactly that test passed.
instrument() {
    local method=$1
    shift
    local out="$TMP/instrument-$method.txt"
    adb_ shell am instrument -w -r "$@" -e class "$TEST_CLASS#$method" "$RUNNER" >"$out" 2>&1 || true
    if ! grep -q "OK (1 test)" "$out" || grep -q "AssumptionViolatedException" "$out"; then
        grep -E "INSTRUMENTATION_STATUS: stack=|FAILURES|Error|Exception" "$out" | head -20 >&2 || true
        return 1
    fi
}

device_ready() {
    adb_ shell input keyevent KEYCODE_WAKEUP
    adb_ shell wm dismiss-keyguard
    adb_ shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS >/dev/null
}

booted() { [ "$(adb_ shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; }

log "checking the device $SERIAL"
[ "$(adb_ get-state 2>/dev/null)" = device ] || fail "$SERIAL is not online in adb"
wait_for 120 booted || fail "the device did not finish booting"
device_ready

log "building the relay and the CLI"
(cd "$REPO_DIR" && cargo build -q -p weft-relay -p weft-cli)

log "installing the app and its tests on $SERIAL"
(cd "$ANDROID_DIR" && ./gradlew -q :app:assembleDebug :app:assembleDebugAndroidTest)
adb_ install -r -t "$ANDROID_DIR/app/build/outputs/apk/debug/app-debug.apk" >/dev/null
adb_ install -r -t "$ANDROID_DIR/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk" >/dev/null
adb_ shell pm clear "$APP_ID" >/dev/null
adb_ shell pm grant "$APP_ID" android.permission.POST_NOTIFICATIONS 2>/dev/null || true

log "starting the relay on 127.0.0.1:$PORT"
adb_ reverse "tcp:$PORT" "tcp:$PORT" >/dev/null
WEFT_BIND="127.0.0.1:$PORT" WEFT_RELAY_NAME=127.0.0.1 WEFT_DB="$TMP/relay.sqlite3" \
    "$RELAY" >"$TMP/relay.log" 2>&1 &
PIDS+=($!)
wait_for 15 nc -z 127.0.0.1 "$PORT" 2>/dev/null || fail "the relay did not start (see relay.log)"

MAC_NAME="E2E Mac"
PHONE_NAME="E2E Pixel"
PHONE_RENAMED="E2E Pixel renamed"

# True if `weft devices` lists one device with exactly the name $1.
cli_shows_name() {
    "$WEFT" --state "$STATE_A" devices | grep -qE "^[0-9a-f]{32}  $1\$"
}

log "pairing: the CLI shows a code, the app joins"
A_ID="$("$WEFT" --state "$STATE_A" init --relay "$RELAY_URL" --name "$MAC_NAME" 2>/dev/null)"
"$WEFT" --state "$STATE_A" pair >"$TMP/pair.out" 2>"$TMP/pair.err" &
PAIR_PID=$!
PIDS+=("$PAIR_PID")
wait_for 20 grep -q '^WEFT1:' "$TMP/pair.out" || fail "weft pair printed no code"
CODE="$(grep -m1 '^WEFT1:' "$TMP/pair.out")"
device_ready
instrument join -e weftCode "$CODE" -e weftName "'$PHONE_NAME'" -e weftExpectPeerName "'$MAC_NAME'" ||
    fail "the app could not join, or did not get the CLI's name"
wait "$PAIR_PID" || fail "weft pair did not accept the app"
[ "$("$WEFT" --state "$STATE_A" devices | wc -l | tr -d ' ')" = 1 ] || fail "the CLI does not trust the app"
PHONE_ID="$("$WEFT" --state "$STATE_A" devices | cut -c1-32)"
echo "joined: the CLI trusts ${PHONE_ID:0:8}…"
cli_shows_name "$PHONE_NAME" || fail "weft devices does not show the phone's name"
echo "weft devices: $("$WEFT" --state "$STATE_A" devices | cut -c1-8)…  $PHONE_NAME"
echo "the app stores the CLI's name: $MAC_NAME"

log "phone -> mac: the notification action sends the clipboard"
"$WEFT" --state "$STATE_A" listen >"$TMP/listen.out" 2>"$TMP/listen.err" &
LISTEN_PID=$!
PIDS+=("$LISTEN_PID")
wait_for 15 grep -q 'listening' "$TMP/listen.err" || fail "weft listen did not connect"
PHONE_TEXT="from-the-phone-$RANDOM$RANDOM"
device_ready
instrument sendClipboard -e weftSendText "$PHONE_TEXT" || fail "the app could not send its clipboard"
wait_for 15 grep -q "\"text\":\"$PHONE_TEXT\"" "$TMP/listen.out" || fail "weft listen did not print the text"
echo "weft listen printed: $(grep -m1 "$PHONE_TEXT" "$TMP/listen.out")"

log "rename: the app sends its new name, weft listen stores it"
device_ready
instrument rename -e weftName "'$PHONE_RENAMED'" || fail "the app could not send its new name"
wait_for 15 cli_shows_name "$PHONE_RENAMED" || fail "weft devices does not show the new name"
echo "weft devices: $("$WEFT" --state "$STATE_A" devices | cut -c1-8)…  $PHONE_RENAMED"
kill "$LISTEN_PID"
wait "$LISTEN_PID" 2>/dev/null || true

log "mac -> phone: weft send lands on the device clipboard"
MAC_TEXT="hello from mac $RANDOM"
device_ready
# The test waits (up to 60 s) for the text on the clipboard; the relay only delivers to
# connected devices, so keep sending until the test sees it.
instrument receiveClipboard -e weftExpectText "'$MAC_TEXT'" &
RECEIVE_PID=$!
PIDS+=("$RECEIVE_PID")
while kill -0 "$RECEIVE_PID" 2>/dev/null; do
    "$WEFT" --state "$STATE_A" send "$MAC_TEXT" 2>>"$TMP/send.err" || true
    sleep 2
done
wait "$RECEIVE_PID" || fail "the text sent by the CLI did not reach the device clipboard"
echo "device clipboard: $MAC_TEXT"

log "invitation: the app adds a new CLI device to its group"
DESK_NAME="E2E Desk"
D_ID="$("$WEFT" --state "$STATE_D" init --relay "$RELAY_URL" --name "$DESK_NAME" 2>/dev/null)"
# A stays online: it gets the Device added and trusts D.
"$WEFT" --state "$STATE_A" listen >"$TMP/listen-a.out" 2>"$TMP/listen-a.err" &
LISTEN_A_PID=$!
PIDS+=("$LISTEN_A_PID")
wait_for 15 grep -q 'listening' "$TMP/listen-a.err" || fail "weft listen on A did not connect"
"$WEFT" --state "$STATE_D" pair >"$TMP/pair-d.out" 2>"$TMP/pair-d.err" &
PAIR_D_PID=$!
PIDS+=("$PAIR_D_PID")
wait_for 20 grep -q '^WEFT1:' "$TMP/pair-d.out" || fail "weft pair on D printed no code"
CODE_D="$(grep -m1 '^WEFT1:' "$TMP/pair-d.out")"
device_ready
instrument invite -e weftInviteCode "$CODE_D" -e weftExpectPeerName "'$DESK_NAME'" -e weftExpectDevices 2 ||
    fail "the app could not add D to its group"
wait "$PAIR_D_PID" || fail "weft pair on D did not accept the invitation"
echo "invited: the app added ${D_ID:0:8}… ($DESK_NAME)"

# True if `weft devices` with state $1 lists device $2.
cli_trusts() { "$WEFT" --state "$1" devices | grep -q "^$2"; }
wait_for 15 cli_trusts "$STATE_D" "$PHONE_ID" || fail "D does not trust the phone"
wait_for 15 cli_trusts "$STATE_D" "$A_ID" || fail "D does not trust A"
[ "$("$WEFT" --state "$STATE_D" devices | wc -l | tr -d ' ')" = 2 ] || fail "D does not trust exactly 2 devices"
"$WEFT" --state "$STATE_D" devices | grep -qE "^$PHONE_ID  $PHONE_RENAMED\$" || fail "D does not show the phone's name"
echo "weft devices on D: ${PHONE_ID:0:8}…  $PHONE_RENAMED, ${A_ID:0:8}… (A)"
wait_for 15 cli_trusts "$STATE_A" "$D_ID" || fail "A does not trust D"
echo "weft devices on A: ${PHONE_ID:0:8}…, ${D_ID:0:8}… (D)"

log "D -> A and D -> phone: text sent by the invited device"
DESK_TEXT="hello from desk $RANDOM"
device_ready
instrument receiveClipboard -e weftExpectText "'$DESK_TEXT'" &
RECEIVE_D_PID=$!
PIDS+=("$RECEIVE_D_PID")
while kill -0 "$RECEIVE_D_PID" 2>/dev/null; do
    "$WEFT" --state "$STATE_D" send "$DESK_TEXT" 2>>"$TMP/send-d.err" || true
    sleep 2
done
wait "$RECEIVE_D_PID" || fail "the text sent by D did not reach the device clipboard"
echo "device clipboard: $DESK_TEXT"
wait_for 15 grep -q "\"from\":\"$D_ID\".*\"text\":\"$DESK_TEXT\"" "$TMP/listen-a.out" ||
    fail "weft listen on A did not print the text from D"
echo "weft listen on A printed the text from ${D_ID:0:8}…"
kill "$LISTEN_A_PID"
wait "$LISTEN_A_PID" 2>/dev/null || true
