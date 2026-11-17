#!/bin/bash
# Builds the Rust client core (crates/weft-ffi) as a static library for the
# Swift package, and generates its UniFFI Swift bindings.
#
#   macos/scripts/build-rust.sh [release|debug]     (default: release)
#
# Output, all ignored by git:
#   macos/Rust/lib/libweft_ffi.a          universal arm64 + x86_64 (lipo)
#   macos/Sources/WeftFFI/weft_ffi.swift the generated bindings
#   macos/Sources/weft_ffiFFI/            their C header and module map
#
# WEFT_ARCHS="aarch64-apple-darwin" builds one architecture only. Files are
# replaced only when their content changes, so `swift build` stays
# incremental. Needs the Rust targets: rustup target add x86_64-apple-darwin
set -euo pipefail

profile="${1:-release}"
case "$profile" in
    release) cargo_profile=release; dir=release ;;
    debug) cargo_profile=dev; dir=debug ;;
    *) echo "usage: $0 [release|debug]" >&2; exit 2 ;;
esac

macos="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$macos/.." && pwd)"
read -r -a archs <<<"${WEFT_ARCHS:-aarch64-apple-darwin x86_64-apple-darwin}"

# Objects built for the app's minimum macOS, so the linker has nothing to
# say about them.
export MACOSX_DEPLOYMENT_TARGET=14.0

targets=()
for arch in "${archs[@]}"; do targets+=(--target "$arch"); done
echo "==> cargo build weft-ffi ($profile, ${archs[*]})"
(cd "$root" && cargo build -p weft-ffi --lib --profile "$cargo_profile" "${targets[@]}")

libs=()
for arch in "${archs[@]}"; do libs+=("$root/target/$arch/$dir/libweft_ffi.a"); done

# Replaces $2 with $1 only if they differ.
install_if_changed() {
    if ! cmp -s "$1" "$2"; then
        mkdir -p "$(dirname "$2")"
        cp "$1" "$2"
        echo "    updated ${2#"$macos"/}"
    fi
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

lipo -create "${libs[@]}" -output "$work/libweft_ffi.a"
install_if_changed "$work/libweft_ffi.a" "$macos/Rust/lib/libweft_ffi.a"

echo "==> uniffi-bindgen (Swift)"
(cd "$root" && cargo build -q -p weft-ffi --features bindgen --bin uniffi-bindgen)
"$root/target/debug/uniffi-bindgen" generate \
    --library "${libs[0]}" --language swift --out-dir "$work/generated" >/dev/null

install_if_changed "$work/generated/weft_ffi.swift" "$macos/Sources/WeftFFI/weft_ffi.swift"
install_if_changed "$work/generated/weft_ffiFFI.h" "$macos/Sources/weft_ffiFFI/weft_ffiFFI.h"
# The generated module map, plus the library to link.
awk '/^}$/ { print "    link \"weft_ffi\"" } { print }' \
    "$work/generated/weft_ffiFFI.modulemap" >"$work/module.modulemap"
install_if_changed "$work/module.modulemap" "$macos/Sources/weft_ffiFFI/module.modulemap"
echo "==> Rust core ready: $(lipo -archs "$macos/Rust/lib/libweft_ffi.a")"
