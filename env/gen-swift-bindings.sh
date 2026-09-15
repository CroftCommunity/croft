#!/usr/bin/env bash
# Build the ffi cdylib for THIS machine, generate its Swift bindings into the
# macOS shell, and run the shell's tests against both (R4).
#
# The same shape as gen-kotlin-bindings.sh, for the same reason: the bindgen
# reads the library it generates from, the package links the same file, and
# the tests load it — three steps that have to agree about one path.
#
# macOS only: it drives `swift`, and the shell is a macOS window.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"

[ "$(uname -s)" = Darwin ] || { echo "the apple shell builds on macOS only" >&2; exit 2; }

cargo="$(rustup which cargo)"
export RUSTC="$(rustup which rustc)"

libdir="$root/target/debug"
lib="$libdir/libcroft_ffi.dylib"
shell="$root/shell/apple"
gen="$shell/.generated"

echo "==> building the cdylib"
"$cargo" build -p croft-ffi
[ -f "$lib" ] || { echo "no cdylib at $lib" >&2; exit 1; }

echo "==> generating Swift bindings from libcroft_ffi.dylib"
rm -rf "$gen"
mkdir -p "$gen"
"$cargo" run -p croft-ffi --features cli --bin uniffi-bindgen -- \
  generate --library "$lib" --language swift --out-dir "$gen" --no-format
# SwiftPM's C target: the header and the modulemap (under the name SwiftPM
# looks for) in include/; the Swift half beside the target's placeholder.
mkdir -p "$shell/Sources/croft_ffiFFI/include" "$shell/Sources/CroftFFI/Generated"
rm -f "$shell/Sources/croft_ffiFFI/include/"*.h "$shell/Sources/croft_ffiFFI/include/module.modulemap" \
      "$shell/Sources/CroftFFI/Generated/"*.swift
cp "$gen/croft_ffiFFI.h" "$shell/Sources/croft_ffiFFI/include/"
cp "$gen/croft_ffiFFI.modulemap" "$shell/Sources/croft_ffiFFI/include/module.modulemap"
cp "$gen/croft_ffi.swift" "$shell/Sources/CroftFFI/Generated/"
rm -rf "$gen"

echo "==> running the Swift wiring test"
cd "$shell"
CROFT_FFI_LIBDIR="$libdir" swift test "$@"
