# Changelog

All notable changes to `croft`. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning follows `docs/VERSIONING.md` — **three clocks**, of which this file
tracks the product and the contract.

Environment commands and their reasoning live in `ops/JOURNAL.md`, not here. The
changelog answers *what changed for a consumer*; the journal answers *what we did
to the environment and why*.

## [Unreleased]

Phase 11 **M4 — the client side is complete and device-validated**
(plan: `plans/2026-08-20-1-plan-m4-call-time-admission.md`). M4a + M4b
landed 2026-08-20; M4c (mint-at-dial) and M4d's first device runs landed
2026-08-21 (runbook §11: real mint from a phone, minted-token dial, live
revocation refused with words, identity-proof mint on-device). M4e
(camp-at-attach) landed under tests 2026-08-23, the three call-endings
(E129) the same day, and the **§12 enforce rehearsal ran 2026-08-24 with
every rung green on hardware**: the signed-out camp refused, sign-in →
self-minted camping pass → admitted with attribution, the first call
with both sides holding passes on an enforcing relay, the endings'
words verbatim on both screens, and the sign-out negative. versionCode 5 / versionName 0.5.0 published as **v0.5.0-rc.1**
2026-08-24; **versionCode 6** re-cut as **rc.2** 2026-08-28 carrying the
E135(a) device fix below.

Then the operational distance closed almost entirely (2026-08-25/26):
croft-admit + its private store ACTIVATED on the box, production relay
promoted to the v0.2.0 candidate in OPEN mode verifying the REAL admit
key — **the bake is live** — and `admit.croft.ing` answered its first
phones. **§13 steps 2 + 4 RAN 2026-08-26** with the published rc.1 APK
on both devices (runbook §13 results): a production camp mint, the
`endpoint_unbound` caller posture with its words in both places, and an
attributed production call, ended with E129's words verbatim. **Corrected
2026-08-28 (E150): the attributed `usage` lines cited there prove a token
was PRESENTED, not admitted — the relay attributes denials too. The
production relay in fact REJECTS both phones' passes, no pass has ever
been admitted there, and the enforce flip is blocked until one is.** Remaining: the bake days, then the owner's
one-word enforce flip (croft-stack `TODO.md`), plus E135(b) —
caller-side camp posture (filed as "E130" in this repo's earlier notes;
the roadmap renumbered it when openprices claimed E130).

### Added (M4, unreleased)
- **The camped line is honest (E135(a)).** The screen no longer claims
  "camped on relay" the app cannot see — a refused attach reads "NOT
  camped on relay; calls cannot reach this device". Landed 2026-08-25
  against `addr().relayUrl()`, which §13 step 3 then proved blind; see
  the Fixed entry below for the signal that actually works.
- **The enforcement scenario matrix, client half**
  (`docs/ENFORCEMENT-SCENARIOS.md` + `EnforcementMatrixTest` riding
  `testDebugUnitTest`): ~30 posture rows — what must dial, camp, degrade
  tokenless WITH words, and what the screen must say — each pinned to a
  named test, gated against drift (a missing pin or a GAP row fails the
  build; the doc is a declared test input so a doc-only edit re-runs the
  gate). The server half, where refusal bites, is croft-stack
  `docs/ENFORCEMENT-SCENARIOS.md`.
- **M4e camp-at-attach: the callee mints its own pass.** `CampAdmission` +
  `Admit.campToken` + the ViewModel trigger at attach: the pass is cached
  by the wire's `expiresIn` (the JWT stays opaque, decision D3), expiry
  re-mints, and a refusal camps tokenless WITH words — the relay stays the
  gate. The camp journey covers the full arc: OAuth session →
  method-bound proof → mint → cache → expiry re-mint → revocation →
  outage.
- **The three call-endings (E129), each with words.** The app holds the
  live `Connection` and a `closed()`-watcher ends the state honestly —
  hang up ("you ended the call"), the remote end, and error each land on
  screen; before it, a remote ending left the UI stuck at Connected.
  Ended keeps the endpoint bound: still camped, still callable. The
  endings' words were read verbatim on both phones in §12.
- `DialAdmission` + mint-at-dial in `dialCallee`: refusals never dial and
  say why; an admit outage dials tokenless with a note (the relay is the
  gate); v1 callees dial exactly as before. `CallPeer.rebindWithToken`
  checks EndpointId stability across the token swap.
- Debug-build rig overrides (`-PcroftRelayUrl`/`-PcroftAdmitBase`) and a
  debug-only cleartext manifest — production defaults untouched.
- `caps/Admit` — the admit client: `POST /grantCall` on croft-admit, typed
  refusal taxonomy (refused ≠ unavailable ≠ bad request), token kept opaque.
- `caps/ServiceAuth` — the caller proof: DPoP-authed `getServiceAuth` at the
  caller's own PDS (`ath`-bound proofs, resource-server nonce dance).
- `AuthManager.freshAccessToken` — single-use refresh rotation, the new pair
  persisted before return; wired on-foreground (closes discovery E113).
- The **workflow harness** (first-class per the plan): `FixtureExchange`
  stands in for every backend incl. a full OAuth authorization server;
  journeys drive the REAL ports over real sockets (ticket, identity,
  session, callability — including the callability-vs-mint disagreement
  rows and revocation stories via mutable fixture state).
- `Redeem` retains the ticket secret (the call-time possession proof).

### Changed (M4, unreleased)
- OAuth scope is now `atproto transition:generic` — under OAuth,
  `getServiceAuth` requires an RPC permission the bare scope lacks, and
  bsky.social does not yet advertise granular `rpc:` scopes (plan O2,
  resolved from PDS source). Existing sessions must re-sign-in to mint.

### Fixed (M4, unreleased)
- **The camped claim is now device-true (E135(a)).** The screen said
  "ready, camped on relay" while an enforcing relay refused every
  attach — the earlier fix polled `addr().relayUrl()`, which reports the
  CONFIGURED relay in both states, so its (correct) mapping was fed a
  blind input. The truth source is `Endpoint.online()`, chosen by
  refuting the alternatives on hardware: `watchHomeRelay` throws "there
  is no reactor running" exactly as `conn.watchPaths()` does, and
  `stats()`'s `relay_home_change` reads 1 whether attached or refused.
  Verified both ways on a phone — refused → "NOT camped on relay; calls
  cannot reach this device", attached → "camped" (runbook §13 step 3,
  `ops/JOURNAL.md` 2026-08-28).
- **A cancelled camp no longer speaks.** A rebind mid-mint rendered
  "camping pass setup failed: Job was cancelled" on screen; cancellation
  is a lifecycle event, not a refusal, and now propagates untouched.
- **The refresh-token race the §12 run earned.** The foreground
  best-effort refresh and the camp mint's before-mint refresh raced the
  SINGLE-USE refresh token; the entryway answered 400 invalid_grant
  ("refresh token rotated concurrently") and the loser's mint died.
  Found on hardware, reproduced RED in the workflow harness (the fixture
  now enforces single-use rotation exactly like the entryway), fixed by
  serializing `freshAccessToken` behind a mutex with a double-check so
  the loser rides the winner's fresh pair.

### Added
- Repo skeleton: shared-core/per-platform-shell layout (`core/`, `shell/`,
  `design/`, `ports/`, `ffi/`, `web/`, `android/`, `apple/`).
- `docs/ADR-0001-client-architecture.md` — the architecture, restated from the
  ADR Accepted in `discovery` 2026-06-22 and demonstrated in code before this
  repo existed.
- `docs/PLATFORM-POSTURE.md` — the per-platform promise. T14 answered: **best
  effort, stated plainly, and not forever**, with three revisit triggers.
- `docs/CONTRACT.md` — points at `CroftCommunity/connect` as canonical rather
  than restating the lexicon, so the contract cannot silently fork.
- `docs/VERSIONING.md` — the three clocks and their different obligations.
- `env/` — the toolchain as code: pinned versions and checksums, an idempotent
  `bootstrap`, a `verify` that **exits non-zero** on drift, and the emulator as a
  definition rather than a pet.
- `rust-toolchain.toml` — added because `env/verify.sh` caught this repo
  resolving to `stable` instead of the declared 1.97.1 on its first run.
- `ops/JOURNAL.md` — the environment command log.

### Decided
- **Calling is a capability, not a pond**, attaching to the shell's *rendered
  principal* seam — one seam rather than one integration per pond.
- **Callability is three states**, not a boolean (`callable` / `not-listed` /
  `may-not-permit`), because it is the relay's admission model and "not listed"
  is the normal case.
- **`arm64-v8a` is required**, not preferred: an x86_64 emulator image can mask
  the native-packaging bug class already suspected in the inherited croftcall
  client.
- **Android first**, Apple second — the code exists there, and it is the more
  forgiving background story.

### Open
- ~~Callability resolution strategy (lazy / cached / batched)~~ — **decided
  2026-08-17 (D1, parent Phase 11 plan): lazy-on-tap plus a TTL cache of
  derived state**, because an always-visible call icon leaks *who you are
  looking at*. Shipped in 0.3.0 (lazy redeem) and 0.4.0 (the cache).
- The push dependency (APNs/FCM in the delivery path of a no-central-operator
  project) — a values question, deliberately not optimised away.
- Whether contract ownership eventually moves here from `connect`.

### Fixed
- `env/bootstrap.sh` — three defects found by its first real run: it assumed a
  usable JDK (the machine has only 8, and `sdkmanager` requires 17+); it gated on
  `/usr/libexec/java_home -v 17`, which **returns the wrong JDK and exits 0** when
  17 is absent; and it could not converge when a JDK cask is left half-installed
  (brew records it, the disk does not have it, `brew install` then no-ops).
- `env/verify.sh` — now searches Homebrew's SDK root, where the cask actually
  installs, not only the Android Studio default.

### Fixed (app)
- **The launch crash is fixed.** `computer.iroh:iroh` is a Kotlin/JVM artifact
  with no Android ABI directory, so `libiroh_ffi.so` was absent and
  `MainViewModel` died on `UnsatisfiedLinkError`. `env/build-iroh-android.sh` now
  cross-compiles it from pinned iroh-ffi source into `jniLibs/arm64-v8a/`. The app
  launches, publishes a real EndpointId, and reports "ready, camped on relay".
- **`env/verify.sh` had a hole and no longer does.** It asserted the rustc
  *version*, which Homebrew's rust matches exactly while shipping a different
  sysroot with zero cross-compile targets — so it passed on a machine where every
  Android build failed. It now asserts the sysroot and, per target, that the
  compiler can actually emit for it (`--print target-libdir`).

### Verified working
- `make bootstrap` completes and is idempotent (a second run no-ops every SDK
  package). It installs Temurin 17 when absent, and stops with an exact command
  when a JDK cask is left half-installed by an unanswered sudo prompt.
- `make verify` — 9 failures before bootstrap, 1 after. The remaining failure (no
  Gradle wrapper) is correct: it arrives with the `android/` shell.
- `make record-checksums` — Gradle 8.13 checksum taken from the publisher's
  `.sha256`, so a corrupted download cannot launder itself into the pin.
- `make emulator` — AVD `croft-dev` boots headless on **arm64-v8a**, API 35,
  `sys.boot_completed=1`, `adb` attached. Confirmed through `adb` rather than
  trusting the script's own "ready" message.
- `make screenshot` — real 1080x2400 capture from a *headless* emulator, so UI is
  inspectable with no window and no human.

### Runs now
- The **android app** (the inherited croftcall client, `ing.croft.call`) builds
  and launches; single-node behaviour — EndpointId, relay-camp, deep-link — is
  emulator-verified per `ops/RUNBOOK-two-device-call-test.md`. Published as
  two-device-test candidate **`v0.1.0-rc.1`**, since validated and promoted to
  `v0.1.0` (see below). The Gradle wrapper and `android/` shell have landed; the
  E100 launch crash is rooted.
- **One-app consolidation** (2026-08-16): `connect/android` retired at connect
  v0.2.0; `croft/android` is the sole Croft Call app. `DeepLink` captures the
  connect contract-v2 params (`device`/`grant`). Release process:
  `ops/RELEASING.md`.

### Not yet true
- The **shared core** does not run yet — `core/`, `shell/`, `ports/` are still
  skeleton, and the android app is not rebuilt on them. `verify` still fails on
  the parts that have not landed.

## [0.4.0] — 2026-08-18

Phase 11 **M2 + M3**. Validated on-device 2026-08-17; promoted 2026-08-18.

### Added (app)
- **Identity proof: `provenDid` via atproto OAuth, and callability made
  visible.** Sign in with an atproto handle from the This-device card: the
  app resolves the PDS, discovers the auth server, and runs the mandatory
  PAR + DPoP (ES256, hand-rolled — no JOSE dependency) + PKCE dance through
  the default browser; the redirect returns on `ing.croft.connect:/oauth`
  (the spec ties a native client's custom scheme to the client_id hostname
  reversed — the client metadata lives at
  `https://connect.croft.ing/oauth-client-metadata.json`). Tokens and the
  DPoP keypair persist in EncryptedSharedPreferences and never reach logs;
  the proven DID survives restarts, and the flip was observed live both
  directions (signed in: callable via grant `m3registered`; signed out:
  may-not-permit, immediately). Scope is `atproto` alone: identity, not
  writes. Plan + live findings:
  `plans/2026-08-17-2-plan-m3-identity-proof.md`.
- **The callability resolver (M2), now surfaced.** `Callability.resolve`
  derives `callable / not-listed / may-not-permit` per the handoff ("does
  any grant admit me and do its rules still hold" — never a lookup), with
  matchers mirrored from `resolver.js` (`ticket` / `mutuals` via AppView
  `getRelationships` / `registeredCallers`). The callee card carries the
  derived line, resolved lazily on callee arrival (decision D1) and
  TTL-cached 5 min per (principal, identity) — a different proven DID can
  never read another identity's answer. versionCode 4, versionName 0.4.0.

## [0.3.0] — 2026-08-17

Phase 11 **M1**.

### Added (app)
- **Ticket redeem: an exchange invite link is now a callable contact.** The
  app offers itself for `https://connect.croft.ing/redeem` links (chooser-based
  until assetlinks lands; `croftcall://` unchanged) and runs contract §6
  redemption: resolve the repo (handle → DID → PDS), fetch the grant, verify
  the ticket secret against `secretHash`, enforce the redeem-time rule subset
  (`expires` only — use-based rules are call-time facts), read the chosen
  device's endpoint record, and populate the callee card carrying
  `grant`+`device` for the §7 re-check. The engine (`ing.croft.call.caps`) is
  a Kotlin mirror of connect's `resolver.js` — same test vectors, same
  fail-closed semantics (unknown matcher/rule types deny, never crash), the
  network behind one injected `Http` port, the clock an input. Resolution is
  lazy-on-tap (decision D1): nothing resolves on render. Validated on-device
  2026-08-17 against a live test repo: invite link → callee card from public
  records alone → call connected via our relay, upgraded to direct. 57 unit
  tests green. versionCode 3, versionName 0.3.0.

## [0.2.0] — 2026-08-17

Cut and validated the same day as 0.1.0 — the ladder's rung 3. The artifact is
byte-identical to candidate `v0.2.0-rc.1` (sha256 `f19e2b9f…204b60`), promoted
after the split-network run through our own relay went green.

### Changed (app) — rung 3
- **The endpoint camps on our relay.** `endpointOptions` now sets
  `relayMode = RelayMode.custom(...)` over `CroftRelay.config()` —
  `https://relay.croft.ing:8443`, QUIC address discovery on udp/7824, both
  pinned by test against what the relay's own front page advertises
  (nonstandard ports, so a bare URL would dial the defaults and miss). The
  preset stays: iroh-ffi applies it first as the baseline (it installs the
  crypto provider) and explicit fields win, so only the relay moves — n0's
  discovery services remain in use. `authToken` is the Phase 11 hook and stays
  null. Validated on-device 2026-08-17, split networks (WiFi caller, LTE
  callee): ~4 s to connected, both sides' first path
  `relayed https://relay.croft.ing:8443/`, then both upgraded to a
  cross-network direct path — our relay carried the call *and* the holepunch
  upgrade out of it worked. versionCode 2, versionName 0.2.0.

### Added (app)
- **The call screen and logcat now say which path a call is using** — `direct
  <addr>` / `relayed <url>` / `path unknown`, from the connection's own
  `paths()` snapshots, re-read every 2 s while connected so a post-connect
  migration shows up (verified on-device: a callee that connected relayed
  upgraded to direct two seconds later, and the line followed). This closes the
  "direct or relayed: unknown" gap the 2026-08-17 two-device test had to
  record, and is the instrument rung 3 (our own relay) needs. `PathSummary`
  reports only what a selected snapshot actually says — no inference.
  `Connection.watchPaths()` was tried first and fails from Kotlin ("no reactor
  running"; iroh-ffi 1.0.0), hence the poll.

### Changed (build)
- Gradle now provisions its JDK via toolchains + the foojay resolver —
  previously `env/toolchain.yml` claimed this but nothing was wired, and builds
  ran on whatever JAVA_HOME held. Unit tests run on a JDK 21 launcher because
  the iroh 1.0.0 jar ships Java-21 bytecode; compile (and the APK) stay at 17.

## [0.1.0] — 2026-08-17

The first full release of Croft Call. The artifact is byte-identical to
candidate `v0.1.0-rc.1` (sha256 `c3fbc013…843a987`), promoted after the
two-device call test went green.

### Validated
- **The two-device call test passed all three rungs** (Pixel 9 Pro + Samsung
  SM-S947U1, 2026-08-17): each device alone reached "ready, camped on relay"
  with its own EndpointId; same-WiFi dial connected with the `croft-call/0`
  hello exchange in both directions; split-network dial (WiFi caller → LTE
  callee) connected in ~4 s. Direct-vs-relayed path: **unknown** — the binding
  logs nothing that names it. Full results: `ops/RUNBOOK-two-device-call-test.md` §8.
- The inferred iroh Kotlin `accept`/`connect`/stream surface has now run against
  a real peer — the risk called out in the runbook's §1 is retired.

### Released
- `v0.1.0` (Latest) cut from the rc.1 commit; the `v0.1.0-rc.1` prerelease
  pruned per `ops/RELEASING.md`.
