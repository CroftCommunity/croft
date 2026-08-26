#!/usr/bin/env bash
# The AVD, created from env/avd.yml. Destroying and recreating it must be a
# non-event -- if it ever feels risky, this script has failed.
#
# First actually run 2026-08-26 (P7 Phase 0, D3): boots headless, the AVD comes
# up, adb reaches it. That run is also what exposed the SDK-discovery bug fixed
# below -- the script had never been executed on a machine that had an SDK.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
avd_yml="$here/avd.yml"
val() { sed -n "s/^[[:space:]]*$1:[[:space:]]*\"\{0,1\}\([^\"#]*\)\"\{0,1\}.*/\1/p" "$avd_yml" | head -1 | sed 's/[[:space:]]*$//'; }

# Find the SDK the same way verify.sh does, and for the same reason: Homebrew's
# cask installs to /opt/homebrew/share/android-commandlinetools, not the Android
# Studio default. Defaulting to the Studio path alone made `make emulator` fail
# with "emulator: command not found" on a machine bootstrapped exactly as
# documented -- while `make verify` passed, because verify.sh already probed both.
# One discovery rule, in both scripts, or the pair disagrees again.
# `cmdline-tools` is the marker: this machine has a stub ~/Library/Android/sdk
# holding platform-tools but no SDK proper, and that stub must not win.
sdk="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
if [ -z "$sdk" ]; then
  for cand in "$HOME/Library/Android/sdk" "/opt/homebrew/share/android-commandlinetools"; do
    [ -d "$cand/cmdline-tools" ] && { sdk="$cand"; break; }
  done
  sdk="${sdk:-$HOME/Library/Android/sdk}"
fi
name="$(val name)"; package="$(val package)"; device="$(val device)"
export PATH="$sdk/emulator:$sdk/platform-tools:$sdk/cmdline-tools/latest/bin:$PATH"

ensure_avd() {
  if ! avdmanager list avd 2>/dev/null | grep -q "Name: $name"; then
    echo "==> creating AVD $name ($package, $device)"
    echo "no" | avdmanager create avd -n "$name" -k "$package" -d "$device" --force
  fi
}

case "${1:-start}" in
  start)
    ensure_avd
    mode="${2:---headless}"
    args=(-avd "$name" -no-boot-anim)
    [ "$mode" = "--headless" ] && args+=(-no-window)
    echo "==> booting $name ${mode}"
    emulator "${args[@]}" >/tmp/croft-emulator.log 2>&1 &
    echo "==> waiting for device"
    adb wait-for-device
    # Boot completion, not just device presence: adb answers long before the
    # framework is up, and installing into a half-booted device fails oddly.
    until [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; do sleep 2; done
    echo "==> $name ready"
    ;;
  nuke)
    echo "==> killing and deleting $name"
    adb emu kill 2>/dev/null || true
    avdmanager delete avd -n "$name" 2>/dev/null || true
    echo "==> gone. 'make emulator' brings it back."
    ;;
  *) echo "usage: emulator.sh [start [--headless|--window] | nuke]"; exit 2;;
esac
