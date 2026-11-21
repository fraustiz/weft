#!/bin/bash
# Sends a .zip, .dmg or .pkg to Apple's notary service, waits for the
# verdict and fails unless it is Accepted, printing the notary log then.
#
#   macos/scripts/notarize.sh path/to/Weft.zip
#
# Authenticates with an App Store Connect API key (Users and Access ›
# Integrations › App Store Connect API):
#   NOTARY_KEY_PATH   the AuthKey_XXXXXXXXXX.p8 file
#   NOTARY_KEY_ID     its key ID
#   NOTARY_ISSUER_ID  the issuer ID shown above the keys
#
# Notarizing does not change the file: staple the ticket afterwards
# (xcrun stapler staple) to the app or the disk image, so Gatekeeper finds
# it without going online.
set -euo pipefail

file="${1:?usage: notarize.sh FILE}"
: "${NOTARY_KEY_PATH:?}" "${NOTARY_KEY_ID:?}" "${NOTARY_ISSUER_ID:?}"
auth=(--key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER_ID")

echo "==> notarize ${file##*/}"
# notarytool exits non-zero on a rejection too: read the verdict instead.
result="$(xcrun notarytool submit "$file" "${auth[@]}" --wait --timeout 1h --output-format json || true)"
echo "$result"
id="$(jq -r '.id // empty' <<<"$result" 2>/dev/null || true)"
status="$(jq -r '.status // empty' <<<"$result" 2>/dev/null || true)"
if [[ "$status" != Accepted ]]; then
    echo "notarization of ${file##*/}: ${status:-no verdict}" >&2
    [[ -n "$id" ]] && xcrun notarytool log "$id" "${auth[@]}" >&2 || true
    exit 1
fi
