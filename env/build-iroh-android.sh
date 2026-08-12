#!/usr/bin/env bash
# Build libiroh_ffi.so for Android and install it into the app's jniLibs.
#
# WHY THIS EXISTS: `computer.iroh:iroh` on Maven Central is a Kotlin/JVM artifact.
# Its jar carries darwin/linux/win32 natives and NO Android ABI directory --
# iroh-ffi's own README.kotlin.md lists it under "Kotlin / JVM" and documents
# Android as a separate build-it-yourself path. There is no published Android AAR.
# Without this script the app installs fine and dies on launch with
# `UnsatisfiedLinkError: dlopen failed: library "libiroh_ffi.so" not found`.
#
# FIVE things must line up. Each was found by failing, in this order:
#   1. Rust linker      .cargo/config.toml or CARGO_TARGET_*_LINKER
#   2. C compiler       CC_/AR_ -- `ring` has a C build script and `cc` looks for
#                       an unversioned "aarch64-linux-android-clang" that does not exist
#   3. API level        encoded in the clang wrapper NAME; must match minSdk
#   4. Rust toolchain   resolved per-DIRECTORY; a checkout without a
#                       rust-toolchain.toml falls back to the rustup default
#   5. WHICH rustc      Homebrew's rustc shadows rustup AND is the same version,
#                       with a different sysroot carrying zero Android targets
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
manifest="$here/toolchain.yml"
val() { sed -n "s/^[[:space:]]*$1:[[:space:]]*\"\{0,1\}\([^\"#]*\)\"\{0,1\}.*/\1/p" "$manifest" | head -1 | sed 's/[[:space:]]*$//'; }

SDK="${ANDROID_SDK_ROOT:-/opt/homebrew/share/android-commandlinetools}"
NDK_VER="$(val ndk_version)"
FFI_TAG="$(val iroh_ffi_tag)"
API="$(val min_sdk)"
CHANNEL="$(val channel)"
TC="$SDK/ndk/$NDK_VER/toolchains/llvm/prebuilt/darwin-x86_64"
work="${IROH_FFI_SRC:-$root/.build/iroh-ffi}"

[ -d "$TC" ] || { echo "NDK toolchain missing: $TC (run: make bootstrap)"; exit 1; }

# --- source, pinned to a tag ------------------------------------------------
if [ ! -d "$work/.git" ]; then
  mkdir -p "$(dirname "$work")"
  echo "==> cloning iroh-ffi $FFI_TAG"
  git clone --depth 1 --branch "$FFI_TAG" https://github.com/n0-computer/iroh-ffi "$work"
else
  echo "==> reusing $work"
fi

# --- toolchain, resolved absolutely ----------------------------------------
export RUSTUP_TOOLCHAIN="$CHANNEL"
RUSTC_BIN="$(rustup which rustc)"
CARGO_BIN="$(rustup which cargo)"
case "$RUSTC_BIN" in
  *"/.rustup/toolchains/"*) : ;;
  *) echo "refusing: rustup resolved to a non-rustup rustc: $RUSTC_BIN"; exit 1 ;;
esac
export RUSTC="$RUSTC_BIN"

for abi_target in "arm64-v8a:aarch64-linux-android:aarch64_linux_android:AARCH64_LINUX_ANDROID"; do
  abi="${abi_target%%:*}"; rest="${abi_target#*:}"
  target="${rest%%:*}"; rest="${rest#*:}"
  envsuffix="${rest%%:*}"; upper="${rest#*:}"
  clang="$TC/bin/${target}${API}-clang"
  [ -x "$clang" ] || { echo "no clang wrapper: $clang"; exit 1; }

  echo "==> building $target (API $API) with $(basename "$RUSTC_BIN")"
  env \
    PATH="$TC/bin:$(dirname "$RUSTC_BIN"):$PATH" \
    "CC_${envsuffix}=$clang" \
    "AR_${envsuffix}=$TC/bin/llvm-ar" \
    "CARGO_TARGET_${upper}_LINKER=$clang" \
    "$CARGO_BIN" build --release --target "$target" --manifest-path "$work/Cargo.toml"

  out="$root/android/app/src/main/jniLibs/$abi"
  mkdir -p "$out"
  cp "$work/target/$target/release/libiroh_ffi.so" "$out/"
  echo "==> installed $out/libiroh_ffi.so ($(wc -c < "$out/libiroh_ffi.so") bytes)"
done

echo
echo "==> done. Rebuild the app: cd android && ./gradlew assembleDebug"
