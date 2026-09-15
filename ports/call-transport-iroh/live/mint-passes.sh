#!/usr/bin/env bash
# Mint the two camping passes the `:live` §15 regression needs, against the
# STAGING enforce listener, with no atproto in the loop.
#
# How (croft runbook §12, the host-side proven loop in croft-stack
# `croft-relay-bin/tests/camp_enforce_loop.rs`): run croft-stack's own
# croft-relay-admit locally, signing with the STAGING mint key and trusting a
# rig-local camp key for exactly the two rig endpoint ids; prove possession of
# that key to /campToken; keep the passes. The staging relay verifies the
# staging key, so it honours these mints and nothing else does.
#
# Inputs (environment):
#   CROFT_STACK                 path to the croft-stack checkout (default: ../../../../croft-stack)
#   CROFT_STAGING_MINT_KEY      the staging mint's private half, hex PKCS#8 (CroftC/.env)
#   CROFT_LIVE_CALLEE_SECRET_HEX, CROFT_LIVE_CALLER_SECRET_HEX
#                               the rig endpoints' iroh secrets, 32 bytes hex
#                               (default: fixed rig seeds; rig material, never identities)
#
# Output: `export CROFT_LIVE_*=…` lines on stdout — eval them into the shell
# that runs the test. Secrets and passes go to stdout ONLY; nothing is logged.
#
# This script calls croft-stack's binaries; it copies none of its code
# (SHARED-CODE.md rule 1). It is rig tooling, like the runbook's LAN admit.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
croft="$(cd "$here/../../.." && pwd)"
stack="${CROFT_STACK:-$croft/../croft-stack}"
: "${CROFT_STAGING_MINT_KEY:?CROFT_STAGING_MINT_KEY is not set (CroftC/.env)}"
export CROFT_LIVE_CALLEE_SECRET_HEX="${CROFT_LIVE_CALLEE_SECRET_HEX:-$(printf '15%.0s' $(seq 32))}"
export CROFT_LIVE_CALLER_SECRET_HEX="${CROFT_LIVE_CALLER_SECRET_HEX:-$(printf '16%.0s' $(seq 32))}"

export PATH="$HOME/.cargo/bin:$PATH"
# Every cargo call below runs with its cwd INSIDE the repo it builds, so the
# rustup proxy resolves that repo's rust-toolchain.toml (croft 1.97.1,
# croft-stack 1.94.1). Observed 2026-09-14 with `--manifest-path` from here:
# croft-stack was rebuilt under croft's toolchain, every build script's
# `rustc` then resolved by ITS cwd to the default channel, and a dozen
# concurrent rustup syncs raced each other into a half-installed `stable`.

# 1. The rig endpoint ids the passes bind to.
ids="$(cd "$croft" && cargo run -q -p call-transport-iroh --example rig_ids)"
callee_id="$(sed -n 's/^callee=//p' <<<"$ids")"
caller_id="$(sed -n 's/^caller=//p' <<<"$ids")"
>&2 echo "rig: callee=$callee_id caller=$caller_id"

# 2. The rig's local camp key + two fresh proofs (one jti each; a proof lives 50 s).
stack_src="$stack/relay/source"
proof() { (cd "$stack_src" && cargo run -q -p croft-relay-admit --example camp_proof -- 15 did:web:admit.croft.ing "$1"); }
p1="$(proof "r2-live-callee-$(date +%s)")"
p2="$(proof "r2-live-caller-$(date +%s)")"
pubkey="$(sed -n 's/^verifying_key_hex=//p' <<<"$p1")"
proof_callee="$(sed -n 's/^proof=//p' <<<"$p1")"
proof_caller="$(sed -n 's/^proof=//p' <<<"$p2")"

# 3. A local admit trusting that key for exactly those two endpoints, signing with the staging key.
(cd "$stack_src" && cargo build -q -p croft-relay-admit --bin croft-relay-admit)
admit_bin="$stack_src/target/debug/croft-relay-admit"
tmp="$(mktemp -d)"
trap 'kill "${admit_pid:-}" 2>/dev/null || true; rm -rf "$tmp"' EXIT
cat >"$tmp/admit.toml" <<EOF
bind = "127.0.0.1:0"

[store]
mode = "memory"

[index]
mode = "transparent"

[mint]
appview_base = "http://127.0.0.1:9"
plc_base = "http://127.0.0.1:9/plc"
aud = "did:web:admit.croft.ing"
lxm = "ing.croft.relay.grantCall"
issuer = "https://admit.croft.ing"
token_ttl_secs = 900
signing_key_env = "CROFT_ADMIT_SIGNING_KEY"

[camp]
lxm = "ing.croft.relay.campToken"
token_ttl_secs = 43200

[[camp.local_keys]]
verifying_key_hex = "$pubkey"
endpoints = ["$callee_id", "$caller_id"]
EOF
CROFT_ADMIT_SIGNING_KEY="$CROFT_STAGING_MINT_KEY" "$admit_bin" --config "$tmp/admit.toml" >"$tmp/admit.out" 2>/dev/null &
admit_pid=$!
for _ in $(seq 100); do grep -q '^listening on ' "$tmp/admit.out" 2>/dev/null && break; sleep 0.1; done
addr="$(sed -n 's/^listening on //p' "$tmp/admit.out")"
[ -n "$addr" ] || { >&2 echo "the local admit did not come up"; exit 1; }

# 4. Mint.
mint() {
  curl -sS -X POST "http://$addr/campToken" -H 'content-type: application/json' \
    -d "{\"endpoint\":\"$1\",\"proof\":{\"localKey\":\"$2\"}}" \
  | python3 -c 'import json,sys; d=json.load(sys.stdin); t=d.get("token") or sys.exit("mint refused: %s" % d); print(t)'
}
pass_callee="$(mint "$callee_id" "$proof_callee")"
pass_caller="$(mint "$caller_id" "$proof_caller")"

echo "export CROFT_LIVE_CALLEE_SECRET_HEX=$CROFT_LIVE_CALLEE_SECRET_HEX"
echo "export CROFT_LIVE_CALLER_SECRET_HEX=$CROFT_LIVE_CALLER_SECRET_HEX"
echo "export CROFT_LIVE_CALLEE_PASS=$pass_callee"
echo "export CROFT_LIVE_CALLER_PASS=$pass_caller"
>&2 echo "minted two passes (staging key, 43200 s TTL); eval this script's stdout, then:"
>&2 echo "  cargo test -p call-transport-iroh --test live_s15_regression -- --ignored --nocapture"
