#!/usr/bin/env bash
# Cross-compile libcroft_ffi.so for Android and install it into the app's
# jniLibs, then verify the emulator can actually load it.
#
# The build recipe is `build-iroh-android.sh`'s, generalized. That script's
# header lists the five things that must line up and records that each was found
# by failing; all five apply identically here, so they are not restated — read
# it, then read this. The differences from that script are only these: the crate
# is ours and in this workspace (no clone, no pinned upstream tag), and this one
# does not stop at producing a file.
#
# **Producing the `.so` is not the test.** The class of bug the emulator exists
# to catch — the one that ate the inherited croftcall client — is a native
# library that builds cleanly and fails to load on the target ABI. A build that
# succeeds proves the compiler was happy; only a load proves the device is. So
# with an emulator running, this pushes the library and dlopens it. arm64-v8a is
# required rather than preferred for exactly that reason: an x86_64 image can
# mask a packaging bug that arm64 reproduces, and a wrong ABI makes the emulator
# lie.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
manifest="$here/toolchain.yml"
val() { sed -n "s/^[[:space:]]*$1:[[:space:]]*\"\{0,1\}\([^\"#]*\)\"\{0,1\}.*/\1/p" "$manifest" | head -1 | sed 's/[[:space:]]*$//'; }

# The same candidate probe `verify.sh` uses. Honouring only ANDROID_SDK_ROOT is
# what broke `make emulator` on a machine bootstrapped the documented way
# (ops/JOURNAL.md, 2026-08-26).
SDK="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
if [ -z "$SDK" ]; then
  for cand in "/opt/homebrew/share/android-commandlinetools" "$HOME/Library/Android/sdk"; do
    [ -d "$cand/ndk" ] && { SDK="$cand"; break; }
  done
  SDK="${SDK:-/opt/homebrew/share/android-commandlinetools}"
fi

NDK_VER="$(val ndk_version)"
API="$(val min_sdk)"
CHANNEL="$(val channel)"
TC="$SDK/ndk/$NDK_VER/toolchains/llvm/prebuilt/darwin-x86_64"

[ -d "$TC" ] || { echo "NDK toolchain missing: $TC (run: make bootstrap)"; exit 1; }

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

  echo "==> building croft-ffi for $target (API $API)"
  env \
    PATH="$TC/bin:$(dirname "$RUSTC_BIN"):$PATH" \
    "CC_${envsuffix}=$clang" \
    "AR_${envsuffix}=$TC/bin/llvm-ar" \
    "CARGO_TARGET_${upper}_LINKER=$clang" \
    "$CARGO_BIN" build --release -p croft-ffi --target "$target"

  so="$root/target/$target/release/libcroft_ffi.so"
  [ -f "$so" ] || { echo "no .so at $so"; exit 1; }

  out="$root/android/app/src/main/jniLibs/$abi"
  mkdir -p "$out"
  cp "$so" "$out/"
  echo "==> installed $out/libcroft_ffi.so ($(wc -c < "$out/libcroft_ffi.so") bytes)"
  file "$out/libcroft_ffi.so"
done

# --- the load, which is the actual test -------------------------------------

if [ "${1:-}" = "--no-emulator" ]; then
  echo "==> skipping the emulator load (--no-emulator)"
  exit 0
fi

export PATH="$SDK/platform-tools:$PATH"
if ! adb get-state >/dev/null 2>&1; then
  echo
  echo "==> NO DEVICE. The library built, which is not the same as it loading."
  echo "    Start one with 'make emulator' and re-run, or pass --no-emulator to"
  echo "    say out loud that you are only building."
  exit 1
fi

abi="$(adb shell getprop ro.product.cpu.abi | tr -d '\r')"
if [ "$abi" != "arm64-v8a" ]; then
  echo "==> refusing: attached device is $abi, not arm64-v8a."
  echo "    An x86_64 image can mask a packaging bug that arm64 reproduces."
  exit 1
fi

echo "==> pushing to the device and dlopening it"
loader="$here/../.build/dlopen-probe"
mkdir -p "$(dirname "$loader")"
cat > "$loader.c" <<'PROBE'
// Deliberately no JVM in this path. It isolates "does this cdylib load and
// resolve on android arm64" from every gradle, JNA and packaging question, so
// a failure here has exactly one possible cause.
#include <dlfcn.h>
#include <stdio.h>
int main(void) {
    void *h = dlopen("/data/local/tmp/libcroft_ffi.so", RTLD_NOW);
    if (!h) { printf("DLOPEN FAILED: %s\n", dlerror()); return 1; }
    // A symbol from OUR surface, not just any symbol: a library that loads but
    // whose exports are missing is a library that will fail at the first call.
    void *s = dlsym(h, "uniffi_croft_ffi_fn_constructor_chatsession_open");
    if (!s) { printf("DLSYM FAILED: %s\n", dlerror()); return 1; }
    printf("LOADED AND RESOLVED\n");
    return 0;
}
PROBE
"$TC/bin/aarch64-linux-android${API}-clang" "$loader.c" -o "$loader"
adb push "$root/android/app/src/main/jniLibs/arm64-v8a/libcroft_ffi.so" /data/local/tmp/ >/dev/null
adb push "$loader" /data/local/tmp/dlopen-probe >/dev/null
adb shell chmod 755 /data/local/tmp/dlopen-probe
result="$(adb shell /data/local/tmp/dlopen-probe | tr -d '\r')"
echo "==> $result"
[ "$result" = "LOADED AND RESOLVED" ] || exit 1

adb shell rm -f /data/local/tmp/dlopen-probe /data/local/tmp/libcroft_ffi.so
echo "==> the emulator loaded our library and resolved our symbol."
