# The `:live` rig — the §15 regression against the staging enforce listener

`tests/live_s15_regression.rs` is the child plan's R2 wiring test: camp two
endpoints on `relay.croft.ing:8444` with passes, dial tokenless over the live
pass, and read the relay's own journal for `no_token`. It is `#[ignore]`d and
named `live` because it holds a real connection and reads a journal over ssh
(`VERIFICATION.md`: the suffix is the recorded reason it is not on the gate).

## Before a run

1. **Claim the relay.** Drop `CroftC/.coordination/claims/testbed--relay-live.md`
   (template in `COORDINATION.md` § Claims) so a peer reading the journal does not
   attribute the rig's endpoints to a phone. Delete it after.
2. **Have the staging mint key.** `CROFT_STAGING_MINT_KEY` lives in `CroftC/.env`
   (never in a repo, never in a log). Load it into the shell without echoing:
   `set -a; source /path/to/CroftC/.env; set +a`.
3. **Have croft-stack beside this repo** (or point `CROFT_STACK` at it). The
   script runs croft-stack's own `croft-relay-admit` and `camp_proof` example;
   it copies nothing (`SHARED-CODE.md` rule 1).
4. **Have `ssh croft-vps`** working (croft-stack `ansible/inventory.ini`). The
   default journal command is
   `ssh croft-vps sudo -n journalctl -u croft-relay-staging -o cat --no-pager --since <ts>`
   (`sudo`: the login user is not in `systemd-journal`, and without it journalctl
   prints "No entries" plus a hint rather than an error — an empty set graded green);
   override with `CROFT_LIVE_JOURNAL` if the host or unit differs.

## A run

```sh
eval "$(bash ports/call-transport-iroh/live/mint-passes.sh)"
cargo test -p call-transport-iroh --test live_s15_regression -- --ignored --nocapture
```

The script mints two camping passes (43200 s TTL) for two fixed rig endpoints
(`CROFT_LIVE_{CALLEE,CALLER}_SECRET_HEX`, seeds `0x15…`/`0x16…` by default —
rig material, never identities, and never published: the test binds with
`Discovery::None`). The passes are signed by the staging key, so only the
staging listener honours them; production is never touched.

## Reading a failure

- **"the callee's pass must camp it"** — the pass is not for that endpoint id,
  has expired, or was signed by the wrong key. Re-mint. If a fresh mint still
  fails, check the staging listener's `[token] verification_pubkey_hex` against
  the public half of the key you signed with.
- **"a dial never lowers admission (R0)"**, with `denied … reason="no_token"`
  lines for the caller — the §15 defect: the port tore the endpoint down on a
  tokenless want and re-attached with nothing. This is the RED the test exists
  to produce against a port that has the bug.
- **"§15's signature appeared in the journal during the dial"** — the camp did
  not survive the dial; read the printed journal lines.
- **the journal command failed** — ssh. The test is only as good as its
  instrument, and it says so rather than passing over an empty journal.
