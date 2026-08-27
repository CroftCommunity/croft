#!/usr/bin/env bash
# Build the ffi cdylib for THIS machine, generate its Kotlin bindings, and run
# the JVM wiring test against both.
#
# This is S0's binding-test command. It exists as a script rather than a README
# incantation because the three steps have to agree about paths — the bindgen
# reads the library it is generating from, and JNA loads the same file at test
# time. Splitting them across a human's shell history is how they drift.
#
# The android arm64 load is a DIFFERENT rung and lives in
# `env/build-croft-ffi-android.sh`. This one answers "are the bindings right";
# that one answers "does the library load on the target ABI". Both are required
# and neither substitutes for the other.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"

# Homebrew's cargo shadows rustup on PATH and has no wasm std; the repo's own
# rule is to resolve through rustup, always.
cargo="$(rustup which cargo)"
export RUSTC="$(rustup which rustc)"

out="$root/ffi/kotlin/build/generated/uniffi"
libdir="$root/target/debug"

case "$(uname -s)" in
  Darwin) libname="libcroft_ffi.dylib" ;;
  Linux)  libname="libcroft_ffi.so" ;;
  *) echo "unsupported host: $(uname -s)" >&2; exit 2 ;;
esac

echo "==> building the cdylib"
"$cargo" build -p croft-ffi

lib="$libdir/$libname"
[ -f "$lib" ] || { echo "no cdylib at $lib" >&2; exit 1; }

echo "==> generating Kotlin bindings from $libname"
rm -rf "$out"
mkdir -p "$out"
"$cargo" run -p croft-ffi --features cli --bin uniffi-bindgen -- \
  generate --library "$lib" --language kotlin --out-dir "$out" --no-format

echo "==> running the JVM wiring test"
cd "$root/ffi/kotlin"
./gradlew test --console=plain -Dcroft.ffi.libdir="$libdir" "$@"
