#!/usr/bin/env bash
# Populate the UNSET checksums in toolchain.yml from the real artifacts.
#
# Exists so that no checksum is ever typed from memory or inferred. A guessed
# checksum looks like verification and performs none, which is strictly worse
# than an honest UNSET that fails the gate.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
manifest="$here/toolchain.yml"
val() { sed -n "s/^[[:space:]]*$1:[[:space:]]*\"\{0,1\}\([^\"#]*\)\"\{0,1\}.*/\1/p" "$manifest" | head -1 | sed 's/[[:space:]]*$//'; }

gv="$(sed -n '/^gradle:/,/^[a-z]/p' "$manifest" | sed -n 's/^[[:space:]]*version:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
url="https://services.gradle.org/distributions/gradle-${gv}-bin.zip"

echo "==> fetching Gradle ${gv} checksum from the publisher"
# Gradle publishes .sha256 alongside each distribution -- take theirs rather than
# hashing a download, so a corrupted fetch cannot launder itself into the pin.
sum="$(curl -fsSL "${url}.sha256")"
[ -n "$sum" ] || { echo "could not retrieve checksum"; exit 1; }
echo "    $sum"

tmp="$(mktemp)"
sed "s|^  distribution_sha256: \"UNSET\"|  distribution_sha256: \"${sum}\"|" "$manifest" > "$tmp" && mv "$tmp" "$manifest"
echo "==> recorded into toolchain.yml — review the diff before committing"
grep -n "distribution_sha256" "$manifest"
