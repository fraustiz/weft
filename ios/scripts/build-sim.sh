#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
command -v xcodegen >/dev/null || { echo 'Install XcodeGen (brew install xcodegen) first.' >&2; exit 1; }
xcodegen generate --spec project.yml
xcodebuild -project Weft.xcodeproj -scheme Weft -sdk iphonesimulator \
  -destination 'generic/platform=iOS Simulator' -derivedDataPath build/DerivedData \
  CODE_SIGNING_ALLOWED=NO build
