# croft — repo open items

> Known work only — items whose shape is already decided, and which may therefore be
> proposed as work. Anything still an open question (decide / verify / investigate /
> reconcile) belongs in the backlog of record, `discovery/alpha/ROADMAP_TODO.md`,
> however small or operational it is. Tracking scheme: `CroftC/.claude/TRACKING.md`;
> the two piles and why: its § "Two piles". Cross-reference E-numbers where an item
> here implements a backlog row.

Client-side work local to this repo. Dated plans live in `plans/`; device-run evidence
in `ops/RUNBOOK-*.md` and `sessions/`.

## Open

- [x] **Phone-to-phone over our port on BOTH sides.** D3.3 (2026-09-21) was device-verified
  on the Pixel in both directions against `croft-arc` on the laptop (runbook §17); the
  Samsung was pattern-locked and carried the CI-signed v0.5.0, so putting the D3 build on
  it was a fresh install (key wiped → `self` re-published, browser sign-in). RUN the same
  evening once the owner unlocked it: both directions connected direct, endings verbatim,
  no relay line for either phone (runbook §17). [device done 2026-09-21: samsung↔pixel over
  call-transport-iroh on production, both directions]
- [ ] **A backgrounded phone is not callable, and the caller only learns it after 20 s.**
  Surfaced by the §17 two-phone run: the Pixel had been backgrounded, its endpoint was shut
  down (the iroh Android guidance the app follows — no foreground service), and the
  Samsung's dial timed out with "no answer within 20s". Honest on both screens, but the
  product question — stay callable while backgrounded — is a foreground service plus
  push-to-wake, a later phase. Recorded so the next device run foregrounds the callee
  first. (since 2026-09-21) [device: android x2]
- [x] **A relayed call over our port.** RUN 2026-09-23 (runbook §17, last block): the Pixel
  on LTE, the Samsung on Wi-Fi, both directions — connected `relayed relay:https://relay.croft.ing:8443/`
  on both sides, then holepunched to direct across the carrier NAT within ~5 s, endings
  verbatim, no relay line during the call. A call that STAYS relayed was not observed;
  the relayed path is exercised and reported. [device done 2026-09-23: pixel on LTE ↔
  samsung on Wi-Fi over call-transport-iroh, both directions, relayed then direct]
- [ ] **The camp flaps on LTE.** Observed 2026-09-23 (§17, last block): the Pixel on
  cellular lost and regained its relay attachment every 15–30 s between calls — `home
  relay: NOT ATTACHED` / attached, the journal showing `usage` closes and fresh
  `admitted … sponsorship=` re-admits with the remembered pass. The screen tracked it
  honestly; calls placed while attached connected. Not seen on Wi-Fi in any run. Cause not
  established (carrier idle timeout? the IPv6 path?) — measure with a longer idle on LTE
  and the relay's keepalive interval in hand before naming one (iroh pings its relay
  every 15 s, `PING_INTERVAL` in `socket/transports/relay/actor.rs`; the observed period
  is 15–30 s). The phone-side instrument is missing: **E128** (native iroh logging on
  Android produces nothing, `discovery/alpha/ROADMAP_TODO.md`) is exactly the gap this
  measurement hits, so the plan runs the laptop's `croft-arc callee --wait 600` with
  `RUST_LOG=iroh=debug` on the Pixel's hotspot against a Wi-Fi control, and reads the
  relay journal's `usage … duration_ms` per connection. (since 2026-09-23)
  [device: android=pixel]

- [x] **A dial drops the caller's camp — reachability lost on every Connect.** FIXED 2026-09-08; **DEVICE-VERIFIED 2026-09-14** (runbook §16). [device done 2026-09-14: one Connect tap, one line in the relay journal across the whole call — no teardown, no re-admit; and the first connected call of the arc, with the E129 endings verbatim on both screens]

  **The fix:** the relay auth token belongs to the endpoint, so changing it costs
  a `stop()`/`start()`. `rebindWithToken` swapped unconditionally, and the
  tokenless dial path called `rebindWithToken(null)` over a live camping pass — a
  strict downgrade that enforce refuses by definition. `DialAdmission.rebind` is
  now a pure decision (`RebindPolicyTest`, 5 rows): a dial never lowers
  admission. Two adjacent faults fixed with it — a failed rebind returned `null`
  and the dial proceeded against a dead endpoint (both call sites now stop and
  say so), and the resulting refusal rendered as `dial failed: null`.

  **The device run is DONE (2026-09-14).** The closing condition was written in
  advance and met exactly: a phone dialled without losing its camp, and the relay
  journal carried one line in total from the Connect tap through the connected
  call and the hang-up. The call connected — the first in this arc — and the E129
  endings read verbatim on both screens. Runbook §16.

  Three findings came with it, none of them the fix: OAuth sessions did not
  survive **six days** idle (§15.2 measured ~11), so re-sign-in is step 0 of every
  device run rather than a contingency; `adb install -r` over the released APK
  **cleared app data and with it the persisted iroh secret key**, giving the phone
  a new endpoint id that its published record no longer named — silent
  unreachability under enforce, repaired by re-publishing `rkey=self`; and a piped
  `gradlew … | tail` reported exit 0 over a failed build, the exact shape
  VERIFICATION.md names.
  Found on hardware 2026-09-08 under enforcement (`ops/RUNBOOK-two-device-call-test.md`
  §15.3). One Connect tap tears down the caller's camped relay connection — the relay
  logs `actor errored "Stream terminated, exiting"` and a `usage` close one second
  after the tap — and the phone then re-attaches. **The re-attach is not reliable:**
  observed recovering with its pass in 4 s, and on the first dial going **tokenless**,
  refused ~20 times with `no_token` over four minutes, recovering only on a full app
  restart. The never-dialled callee held one admitted connection for 14 minutes in the
  same window, so this is the dial path, not the network or the relay.

  Under open mode this was invisible (a tokenless re-attach was admitted anyway); under
  enforce it costs real reachability. Fix before v0.5.0 can be called complete under
  enforcement. The dial should reuse the camped connection, or at minimum re-present the
  cached pass on the re-attach it causes.

- [x] **`dial failed: null` — a refusal with no words.** FIXED 2026-09-08. Observed 2026-09-08 (§15.3).
  `docs/ENFORCEMENT-SCENARIOS.md` Dial posture requires "MUST REFUSE — never dials,
  words on screen"; `null` is not words, and the matrix walk cannot catch it because the
  mapping it pins is correct — the message never arrives. Same shape as the P7 S1 uniffi
  finding (a fieldless error variant crossing FFI with an empty `message`); check that
  cause first. The call never connected this run, so everything below it is unproven.

- [ ] **A dead OAuth refresh token still reads `Signed in`.** [device: android]
  Observed 2026-09-08 (§15.2): after ~10 days idle the refresh token was invalid
  (`invalid_grant` from `bsky.social/oauth/token`), camp setup never ran, and the phone
  was unreachable under enforce — while the account card said `Signed in` with the right
  DID. The camp line was honest ("NOT camped … calls cannot reach this device"); the two
  lines contradict each other and only one is actionable. This is the concrete case the
  **E135(b)** wording decision should be made against, and it also refutes the prepared
  step-0 check ("shows the handle field instead of Signed in") — that cannot see this
  state.

- [ ] **Schedule the OAuth refresh so an idle phone keeps its session (E113).** [device: android]
  `AuthManager.freshAccessToken()` refreshes on-foreground only (M4b), so an app that is
  not opened for days never refreshes, and a refresh token has a lifetime: dead after
  ~10 days idle (§15.2, `invalid_grant`), dead after 6 days (§16, re-sign-in became step 0
  of every device run), alive after 7 (§17, 2026-09-21: "access token stale; refreshing"
  → "session refreshed"). The lifetime is therefore NOT established by idle days alone —
  measure it before choosing a period: the PDS's refresh-token lifetime and inactivity
  rule for a public client, from its OAuth metadata and one deliberate idle soak. Then a
  periodic refresh (WorkManager) inside that window, decision rules in the core first.
  Elevated from roadmap E113 by the triage queue (row 6, 2026-09-14); a roadmap row is
  never proposable as work, this row is. Pairs with the E135(b) row above: the refresh
  that does die must then read as dead, not `Signed in`. (since 2026-09-25)

- [ ] **Adopt openmls 0.9.0 / openmls_rust_crypto 0.6.0 — ordinary work, not urgent.** [device: android]
  Our pins are exact and deliberate (`=0.8.1`, `=0.5.1`, "the exact versions the
  experiments resolved"). The 0.9 line landed 2026-08-25 and brings `hpke-rs` 0.7.0,
  which pins `libcrux-sha3 =0.0.10` and retires most of `osv-scanner.toml`.

  **Why this is not an emergency** — settled 2026-08-29, see the next item. The upgrade
  is worth doing on its own merits (staying on a maintained MLS line, and shedding a
  file of exceptions), but it is an MLS stack change on a client that shipped four days
  ago, so it carries a device re-validation obligation: §12 rungs against staging
  enforce, then §13 on production. It should be planned, not slipped in.

  Version chain, measured, so nobody re-derives it:
  ```
  libcrux-sha3 0.0.10  <- hpke-rs 0.7.0  <- openmls_rust_crypto 0.6.0  <- openmls_traits 0.6.0  <- openmls 0.9.0
  ```
  There is no shorter path. `hpke-rs 0.6.1` requires `libcrux-sha3 ^0.0.8`, and under
  cargo's semver rules a `^0.0.x` requirement admits **only** `0.0.x` — so 0.0.10
  cannot be reached from anywhere in the 0.6 line, with or without `cargo update`.

## Settled

- [x] **The seven libcrux/unmaintained advisories are all unreachable in the shipped
  APK — recorded, not fixed (2026-08-29).** Found by the workspace supply-chain sweep;
  the initial report called them urgent on the strength of CVSS 8.2 plus crate-level
  reachability. Reading each advisory's own `[affected.functions]` and checking call
  sites changed the answer. Every one carries CVSS 4.0 `VC:N/VI:N/VA:H` — **no
  confidentiality impact, no integrity impact**, availability only. They are panic and
  correctness bugs, not key-compromise bugs.

  | advisory | crate | why it does not reach us |
  |---|---|---|
  | RUSTSEC-2026-0207 | libcrux-sha3 0.0.8 | affects incremental `Shake*Xof::squeeze`; hpke-rs calls one-shot `shake256::<32>/<64>` (kem.rs:154,158) |
  | RUSTSEC-2026-0208 | libcrux-sha3 0.0.8 | AVX2 → x86-64; the APK is **arm64-v8a only** |
  | RUSTSEC-2026-0212 | libcrux-secrets 0.0.5 | affects `Select::select`/`Swap::swap`; **zero call sites** in libcrux-traits or libcrux-sha3, which use only the `U8` newtype |
  | RUSTSEC-2026-0209/0210/0211 | libcrux-aesgcm 0.0.7 | unenabled optional dep via `hpke-rs-libcrux`; in the lock, in **no resolved tree on any target** |
  | RUSTSEC-2026-0124 | libcrux-chacha20poly1305 0.0.7 | same |
  | RUSTSEC-2026-0173 · RUSTSEC-2024-0384 | proc-macro-error2 · instant | unmaintained-crate notices; build-time or transitive, no fix to apply here |

  Each is a dated, reasoned, expiring entry in `osv-scanner.toml` (expiry 2026-11-29),
  per `CroftC/.claude/SUPPLY-CHAIN.md` rule 9. `osv-scanner scan source -L Cargo.lock`
  reports **No issues found** with it and 9 vulnerabilities without.

  **Two conditions that invalidate this analysis, stated because an exception that
  silently stops applying is worse than none:**
  1. **Shipping any x86_64 artifact** — a desktop shell, an emulator-targeted build —
     makes RUSTSEC-2026-0208's AVX2 path executable. Re-check on adding a target, not
     only at the expiry date.
  2. **Enabling the `hpke-rs-libcrux` feature** pulls the whole libcrux-aead tree in
     for real, and four exceptions above stop being true.

- [x] **`connect/android` needs the Gradle dependency locking this repo got.** Tracked
  in `connect/TODO.md` (landed 2026-08-29); noted here because the two Android builds
  are siblings and the fix is the same three lines.

## Device queue — a note for the next session in this repo (2026-08-30)

The device-testing needs in this file are **registered in the workspace device queue**
(`CroftC/.claude/TESTBED.md` § The device queue; `CroftC/.claude/DEVICE-QUEUE.md` is
generated from the `[device: …]` tags). Nothing here was removed or reworded: a tag was
appended to the line that records each need, or a pointer bullet was added below where the
need sits inside a longer item. Going forward:

- a new item that needs a phone carries a tag — `[device: android]`, `[device: android x2, ios]`,
  `[device: android=samsung]` when the check is about that unit (tokens: TESTBED's table);
- a run that fulfils one turns its tag into `[device done YYYY-MM-DD: …]` in the same commit as
  the evidence;
- when you next touch an item registered by a pointer bullet, fold the tag into the item and
  drop the pointer — the pointer is scaffolding for the migration, not the shape.

`bash CroftC/.claude/bin/device-queue.sh --have samsung` shows what a phone in hand can seat.
