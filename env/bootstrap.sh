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

sdk="${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}"
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
echo "==> bootstrap done. Now: make verify"
