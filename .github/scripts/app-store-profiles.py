#!/usr/bin/env python3
"""CI only: makes sure an App Store provisioning profile exists for each
bundle ID, tied to the Apple Distribution certificate the job imported, and
installs it where Xcode looks.

    app-store-profiles.py CERT_SHA1 APP_GROUP BUNDLE_ID...

Prints one line per bundle ID, "BUNDLE_ID PROFILE_NAME", for the build
settings and the export options.

Uses the App Store Connect API key of the release workflow:
    API_KEY_PATH, API_KEY_ID, API_ISSUER_ID

A profile is named after the bundle ID and the certificate ("Weft CI
eu.frohub.weft.ios 8AC7FC5C"); each run deletes the one it finds and makes
it again, so it always has the App ID's current capabilities. The bundle
IDs, their capabilities and the App Group, linked to both App IDs, are
registered in Certificates, Identifiers & Profiles: the profile takes its
entitlements from there, and the script fails if APP_GROUP is not in them.

Needs PyJWT and cryptography (pip install pyjwt cryptography).
"""

import base64
import hashlib
import json
import os
import plistlib
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

import jwt

API = "https://api.appstoreconnect.apple.com/v1"


def token() -> str:
    key = Path(os.environ["API_KEY_PATH"]).read_text()
    now = int(time.time())
    return jwt.encode(
        {"iss": os.environ["API_ISSUER_ID"], "iat": now, "exp": now + 15 * 60, "aud": "appstoreconnect-v1"},
        key,
        algorithm="ES256",
        headers={"kid": os.environ["API_KEY_ID"], "typ": "JWT"},
    )


def call(method: str, path: str, query: dict | None = None, body: dict | None = None) -> dict:
    url = API + path
    if query:
        url += "?" + urllib.parse.urlencode(query)
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(url, data=data, method=method)
    request.add_header("Authorization", "Bearer " + token())
    if data is not None:
        request.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            raw = response.read()
            # DELETE answers 204, with no body.
            return json.loads(raw) if raw else {}
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        sys.exit(f"App Store Connect API: {method} {path}: HTTP {error.code}: {detail}")


def distribution_certificate(sha1: str) -> str:
    """The API id of the Apple Distribution certificate whose SHA-1 is sha1."""
    sha1 = sha1.upper()
    certificates = call("GET", "/certificates", {"filter[certificateType]": "DISTRIBUTION", "limit": "200"})
    for certificate in certificates["data"]:
        content = base64.b64decode(certificate["attributes"]["certificateContent"])
        if hashlib.sha1(content).hexdigest().upper() == sha1:
            return certificate["id"]
    sys.exit(f"no Apple Distribution certificate with SHA-1 {sha1} in the team")


def bundle_id(identifier: str) -> str:
    found = call("GET", "/bundleIds", {"filter[identifier]": identifier, "limit": "200"})
    for item in found["data"]:
        if item["attributes"]["identifier"] == identifier:
            return item["id"]
    sys.exit(f"the bundle ID {identifier} is not registered (Certificates, Identifiers & Profiles › Identifiers)")


def profile(identifier: str, certificate_id: str, sha1: str, group: str) -> tuple[str, bytes]:
    """A new App Store profile for identifier: the one a previous run made is
    deleted first, since a profile keeps the capabilities the App ID had
    when it was made."""
    name = f"Weft CI {identifier} {sha1[:8].upper()}"
    found = call("GET", "/profiles", {"filter[name]": name, "filter[profileType]": "IOS_APP_STORE"})
    for item in found["data"]:
        if item["attributes"]["name"] == name:
            call("DELETE", f"/profiles/{item['id']}")
    created = call("POST", "/profiles", body={
        "data": {
            "type": "profiles",
            "attributes": {"name": name, "profileType": "IOS_APP_STORE"},
            "relationships": {
                "bundleId": {"data": {"type": "bundleIds", "id": bundle_id(identifier)}},
                "certificates": {"data": [{"type": "certificates", "id": certificate_id}]},
            },
        }
    })
    content = base64.b64decode(created["data"]["attributes"]["profileContent"])
    groups = read_plist(content)["Entitlements"].get("com.apple.security.application-groups", [])
    if group not in groups:
        sys.exit(f"the profile of {identifier} lacks the App Group {group}: in Certificates, "
                 f"Identifiers & Profiles, open the App ID {identifier}, App Groups › Configure, "
                 f"tick {group} and save")
    return name, content


def read_plist(content: bytes) -> dict:
    # The profile is a CMS envelope around a plist: read it without
    # verifying the signature (Xcode does).
    start, end = content.index(b"<?xml"), content.index(b"</plist>") + len(b"</plist>")
    return plistlib.loads(content[start:end])


def install(content: bytes) -> None:
    uuid = read_plist(content)["UUID"]
    if not re.fullmatch(r"[0-9A-Fa-f-]{36}", uuid):
        sys.exit(f"unexpected profile UUID {uuid!r}")
    home = Path.home()
    # Xcode 16 and later read the first, older ones the second.
    for directory in (home / "Library/Developer/Xcode/UserData/Provisioning Profiles",
                      home / "Library/MobileDevice/Provisioning Profiles"):
        directory.mkdir(parents=True, exist_ok=True)
        (directory / f"{uuid}.mobileprovision").write_bytes(content)


def main() -> None:
    if len(sys.argv) < 4:
        sys.exit(__doc__)
    sha1, group, identifiers = sys.argv[1], sys.argv[2], sys.argv[3:]
    certificate_id = distribution_certificate(sha1)
    for identifier in identifiers:
        name, content = profile(identifier, certificate_id, sha1, group)
        install(content)
        print(identifier, name)


if __name__ == "__main__":
    main()
