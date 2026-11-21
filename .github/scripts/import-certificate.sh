#!/bin/bash
# CI only: imports a signing certificate (a .p12 with its private key) into
# a keychain of its own, adds that keychain to the search list, and prints
# the SHA-1 of the identity whose name starts with KIND and that belongs to
# TEAM_ID. The SHA-1, not the name, goes to the log: the name of an
# individual developer's certificate is the developer's legal name.
#
#   identity="$(.github/scripts/import-certificate.sh)"
#
#   P12           the .p12 file, base64
#   P12_PASSWORD  its password
#   KIND          "Developer ID Application" or "Apple Distribution"
#   TEAM_ID       the Apple team, e.g. FTXAB58KDV
#   KEYCHAIN      the keychain to create ($RUNNER_TEMP/signing.keychain-db)
#
# Delete the keychain when the job ends: security delete-keychain "$KEYCHAIN".
set -euo pipefail
: "${P12:?}" "${P12_PASSWORD?}" "${KIND:?}" "${TEAM_ID:?}" "${KEYCHAIN:?}"

password="$(openssl rand -hex 24)"
if [[ ! -f "$KEYCHAIN" ]]; then
    security create-keychain -p "$password" "$KEYCHAIN"
    security set-keychain-settings -lut 21600 "$KEYCHAIN"
    printf '%s' "$password" >"$KEYCHAIN.password"
    chmod 600 "$KEYCHAIN.password"
    # Search this keychain too, after the ones already there.
    existing=()
    while IFS= read -r line; do
        line="${line#"${line%%[![:space:]]*}"}"
        existing+=("${line//\"/}")
    done < <(security list-keychains -d user)
    security list-keychains -d user -s "$KEYCHAIN" "${existing[@]}"
fi
password="$(cat "$KEYCHAIN.password")"
security unlock-keychain -p "$password" "$KEYCHAIN"

p12="$(mktemp)"
trap 'rm -f "$p12"' EXIT
printf '%s' "$P12" | base64 --decode >"$p12"
security import "$p12" -k "$KEYCHAIN" -f pkcs12 -P "$P12_PASSWORD" \
    -T /usr/bin/codesign -T /usr/bin/security -T /usr/bin/productbuild >/dev/null
# Lets codesign use the key without a prompt nobody would answer.
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$KEYCHAIN" >/dev/null

# Lines look like: 1) 0123…ABCD "Developer ID Application: Name (TEAMID)"
sha1="$(security find-identity -v -p codesigning "$KEYCHAIN" \
    | awk -v kind="\"$KIND: " -v team="($TEAM_ID)\"" 'index($0, kind) && index($0, team) { print $2; exit }')"
if [[ ! "$sha1" =~ ^[0-9A-F]{40}$ ]]; then
    echo "no valid \"$KIND\" identity of team $TEAM_ID in the certificate" >&2
    exit 1
fi
echo "$sha1"
