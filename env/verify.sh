#!/usr/bin/env bash
# Verify the installed toolchain against env/toolchain.yml.
#
# THIS REFUSES, IT DOES NOT WARN.
#
# That is the whole design. `fun` shipped .nvmrc AND an `engines` field, both of
# which only warned, and a developer ran the wrong Node for a full day while CI
# was green. The fix there was engine-strict=true so npm REFUSES. Same rule here:
# every check below exits non-zero. A warning scrolls past; an exit code does not.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
manifest="$here/toolchain.yml"
fail=0

red()  { printf '\033[31mFAIL\033[0m  %s\n' "$1"; fail=1; }
ok()   { printf '\033[32m ok \033[0m  %s\n' "$1"; }
note() { printf '      %s\n' "$1"; }

# Minimal YAML reads. The manifest is deliberately flat enough not to need a
# parser -- a dependency here would itself need pinning.
val() { sed -n "s/^[[:space:]]*$1:[[:space:]]*\"\{0,1\}\([^\"#]*\)\"\{0,1\}.*/\1/p" "$manifest" | head -1 | sed 's/[[:space:]]*$//'; }

echo "verifying against $manifest"
echo

# ---- checksums must not be UNSET -------------------------------------------
# Checked first: an unset checksum means the pin is decorative, and every check
# below it is being read as stronger than it is.
if grep -q 'UNSET' "$manifest"; then
  red "toolchain.yml still contains UNSET checksums"
  note "run: make record-checksums   (then review and commit the values)"
  note "a guessed checksum looks like verification and performs none"
fi

# ---- rust -------------------------------------------------------------------
want_rust="$(val channel)"
if command -v rustup >/dev/null 2>&1; then
  # Resolve through rustup, never bare `cargo`: Homebrew's cargo shadows rustup
  # on PATH here and has no wasm std. fun/tools/build-wasm.sh documents the same
  # trap, and it cost three CI round trips during that repo's gate bring-up.
  # Ask the toolchain rustup resolves IN THIS REPO -- that is what a build will
  # actually use, and it is what rust-toolchain.toml exists to control.
  got_rust="$("$(rustup which rustc 2>/dev/null)" --version 2>/dev/null | awk '{print $2}')"
  if [ "$got_rust" = "$want_rust" ]; then ok "rust $got_rust"; else
    red "rust: want $want_rust, got ${got_rust:-none}"
  fi
  for t in $(sed -n '/^  targets:/,/^[a-z]/p' "$manifest" | sed -n 's/^[[:space:]]*-[[:space:]]*"\{0,1\}\([^"]*\)"\{0,1\}/\1/p'); do
    # Per-toolchain, not global: a target installed for `stable` is not installed
    # for the pinned channel, and only the pinned one will build.
    if rustup target list --toolchain "$want_rust" --installed 2>/dev/null | grep -qx "$t"; then ok "rust target $t"; else
      red "rust target missing: $t"
    fi
  done
else
  red "rustup not found"
fi

# ---- android sdk ------------------------------------------------------------
# Search the places the SDK actually lands, not just the Android Studio default:
# Homebrew's cask installs to /opt/homebrew/share/android-commandlinetools, which
# bootstrap discovered the hard way.
sdk="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
if [ -z "$sdk" ]; then
  for cand in "$HOME/Library/Android/sdk" "/opt/homebrew/share/android-commandlinetools"; do
    [ -d "$cand/cmdline-tools" ] && { sdk="$cand"; break; }
  done
  sdk="${sdk:-$HOME/Library/Android/sdk}"
fi
if [ ! -d "$sdk" ]; then
  red "Android SDK not found at $sdk (set ANDROID_SDK_ROOT)"
else
  ok "Android SDK at $sdk"
  sdkmanager="$sdk/cmdline-tools/latest/bin/sdkmanager"
  if [ ! -x "$sdkmanager" ]; then
    red "sdkmanager not found at $sdkmanager"
  else
    installed="$("$sdkmanager" --list_installed 2>/dev/null || true)"
    while IFS= read -r pkg; do
      [ -z "$pkg" ] && continue
      if grep -qF "$pkg" <<<"$installed"; then ok "sdk pkg $pkg"; else
        red "sdk pkg missing: $pkg"
      fi
    done < <(sed -n '/^  sdk_packages:/,/^  [a-z#]/p' "$manifest" | sed -n 's/^[[:space:]]*-[[:space:]]*"\([^"]*\)"/\1/p')
  fi
fi

# ---- the ABI trap -----------------------------------------------------------
# Not a performance check. An x86_64 image can hide a native-packaging bug that
# arm64 reproduces, so a wrong ABI here makes the emulator lie about exactly the
# class of bug we most need it to catch.
want_abi="$(val abi)"
if [ -n "$want_abi" ]; then
  if [ -d "$sdk/system-images" ] && find "$sdk/system-images" -maxdepth 3 -type d -name "$want_abi" | grep -q .; then
    ok "system image ABI $want_abi"
  else
    red "no $want_abi system image — an x86_64 image can MASK native-packaging bugs"
  fi
fi

# ---- adb --------------------------------------------------------------------
if command -v adb >/dev/null 2>&1 || [ -x "$sdk/platform-tools/adb" ]; then ok "adb present"; else
  red "adb not found"
fi

# ---- gradle wrapper ---------------------------------------------------------
root="$(cd "$here/.." && pwd)"
if [ -f "$root/android/gradlew" ]; then
  ok "gradle wrapper committed"
  if grep -q "distributionSha256Sum" "$root/android/gradle/wrapper/gradle-wrapper.properties" 2>/dev/null; then
    ok "wrapper pins a distribution checksum"
  else
    red "wrapper has no distributionSha256Sum — the version is pinned, the bytes are not"
  fi
else
  red "no gradle wrapper at android/gradlew — builds would use whatever gradle is on PATH"
  note "this machine's PATH gradle is 9.4.1 on JDK 26; the project wants JDK 17"
fi

echo
if [ "$fail" -ne 0 ]; then
  echo "toolchain verification FAILED — run 'make bootstrap', or fix the items above."
  exit 1
fi
echo "toolchain verified."
