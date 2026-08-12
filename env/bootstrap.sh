#!/usr/bin/env bash
# Zero -> working toolchain. Idempotent: safe to run any time, converges.
#
# UNTESTED AS WRITTEN. There is no Android SDK on the machine this was authored
# on, so the first real run IS the test. Expect to fix something here; that is
# the point of it being in the repo rather than in someone's shell history.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
manifest="$here/toolchain.yml"
val() { sed -n "s/^[[:space:]]*$1:[[:space:]]*\"\{0,1\}\([^\"#]*\)\"\{0,1\}.*/\1/p" "$manifest" | head -1 | sed 's/[[:space:]]*$//'; }

# JDK FIRST. sdkmanager itself requires JDK 17+, and this machine's default is
# Temurin 8 -- found by bootstrap failing on its first real run.
#
# TRAP, verified here: `/usr/libexec/java_home -v 17` does NOT fail when 17 is
# absent. It returns the path of whatever JVM it does have (JDK 8) and exits 0.
# So version-gating on its exit status silently accepts the wrong JDK. Read the
# actual major version out of the JVM instead.
java_major() {
  local home="$1" v
  [ -x "$home/bin/java" ] || return 1
  v="$("$home/bin/java" -version 2>&1 | head -1 | sed -E 's/.*version "([0-9.]+).*/\1/')"
  case "$v" in
    1.*) echo "${v#1.}" | cut -d. -f1 ;;   # 1.8.0_482 -> 8
    *)   echo "$v" | cut -d. -f1 ;;        # 17.0.9    -> 17
  esac
}

find_jdk17() {
  local home major
  for home in $(/usr/libexec/java_home -V 2>&1 | sed -n 's|.*[[:space:]]\(/Library/Java.*\)$|\1|p'); do
    major="$(java_major "$home" 2>/dev/null || true)"
    [ -n "$major" ] && [ "$major" -ge 17 ] 2>/dev/null && { echo "$home"; return 0; }
  done
  return 1
}

JAVA_HOME="$(find_jdk17 || true)"
if [ -z "$JAVA_HOME" ]; then
  # A JDK cask installs a .pkg that requires sudo and prompts interactively.
  # That cannot be automated from here, and worse: if the prompt is abandoned,
  # brew still records the cask as INSTALLED while the JDK is absent from disk --
  # so a later `brew install` no-ops and the loop never converges. Detect that
  # split state explicitly and hand the human the exact command.
  if brew list --cask 2>/dev/null | grep -qx "temurin@17"; then
    cat <<'MSG'

  JDK 17 IS HALF-INSTALLED.

  Homebrew records temurin@17 as installed, but no JDK 17 exists on disk --
  the cask's .pkg needs sudo and its prompt was not completed. `brew install`
  will now no-op, so it must be a reinstall:

      brew reinstall --cask temurin@17

  Run that yourself (it will ask for your password), then re-run: make bootstrap

MSG
    exit 1
  fi
  echo "==> installing Temurin 17 (sdkmanager needs 17+; this machine has 8)"
  echo "    this cask runs a .pkg under sudo and WILL prompt for your password"
  brew install --cask temurin@17
  JAVA_HOME="$(find_jdk17 || true)"
fi
[ -n "$JAVA_HOME" ] || { echo "no JDK 17+ found after install attempt"; exit 1; }
export JAVA_HOME
echo "==> JAVA_HOME: $JAVA_HOME (major $(java_major "$JAVA_HOME"))"

# Homebrew's cask puts the SDK here, not in ~/Library/Android/sdk. Honour an
# explicit ANDROID_SDK_ROOT, else prefer whichever actually exists.
default_sdk="/opt/homebrew/share/android-commandlinetools"
[ -d "$HOME/Library/Android/sdk/cmdline-tools" ] && default_sdk="$HOME/Library/Android/sdk"
sdk="${ANDROID_SDK_ROOT:-$default_sdk}"
echo "==> Android SDK root: $sdk"

# 1. command-line tools (the only piece that cannot be pinned by checksum here --
#    Google does not publish stable per-version checksums for the zip).
if [ ! -x "$sdk/cmdline-tools/latest/bin/sdkmanager" ]; then
  echo "==> installing Android command-line tools via Homebrew cask"
  brew install --cask android-commandlinetools
  sdk="${ANDROID_SDK_ROOT:-/opt/homebrew/share/android-commandlinetools}"
fi

sdkmanager="$sdk/cmdline-tools/latest/bin/sdkmanager"
[ -x "$sdkmanager" ] || { echo "sdkmanager still not found at $sdkmanager"; exit 1; }

# 2. licences. Interactive by default; this accepts them non-interactively, which
#    is a deliberate choice for reproducibility -- read them once, then let the
#    script converge.
echo "==> accepting SDK licences"
yes | "$sdkmanager" --licenses >/dev/null 2>&1 || true

# 3. the declared packages, exactly.
echo "==> installing declared SDK packages"
while IFS= read -r pkg; do
  [ -z "$pkg" ] && continue
  echo "    $pkg"
  "$sdkmanager" "$pkg"
done < <(sed -n '/^  sdk_packages:/,/^  [a-z#]/p' "$manifest" | sed -n 's/^[[:space:]]*-[[:space:]]*"\([^"]*\)"/\1/p')

# 4. rust toolchain + targets
want_rust="$(val channel)"
echo "==> rust $want_rust + targets"
command -v rustup >/dev/null 2>&1 || { echo "rustup not installed: brew install rustup-init && rustup-init"; exit 1; }
rustup toolchain install "$want_rust" --component clippy rustfmt
for t in $(sed -n '/^  targets:/,/^[a-z]/p' "$manifest" | sed -n 's/^[[:space:]]*-[[:space:]]*"\{0,1\}\([^"]*\)"\{0,1\}/\1/p'); do
  rustup target add --toolchain "$want_rust" "$t"
done

echo
echo "==> bootstrap done."
echo "    Add to your shell profile so every tool agrees on the SDK root:"
echo "      export ANDROID_SDK_ROOT=\"$sdk\""
echo "      export JAVA_HOME=\"$JAVA_HOME\""
echo "    Now: make verify"
