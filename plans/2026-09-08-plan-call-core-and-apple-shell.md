# Plan — filling `call-core` and standing up the apple shell (roadmap R1–R4)

**Status:** ACCEPTED 2026-09-10. Three passes complete, **Phase 0 CLOSED** — D1 was probed
2026-09-08, and D4/D5/D6 were walked with the owner 2026-09-10 (see *Decisions*). Pass 3's
escalation is discharged: the severities were reviewed. **R1 may start.** Phase 0's D1 is already RUN and green; D4–D6 are
not. Nothing below Phase 0 should start until D4–D6 are answered, because two of them can
change the shape of R1.

**Parent:** `2026-09-08-plan-one-stream-calling-and-chat.md`. That document sequences the
whole stream (R0–R8) and holds the decisions spanning both workstreams; this one owns
**R1–R4** and nothing else. R0/R0b are landed. R5–R8 are the parent's.

**Naming note:** no `-N` ordinal, per `CroftC/.claude/TRACKING.md` (retired 2026-08-29 — a
counter two invisible sessions cannot share). The `phase-plan` skill prescribes one and
also says to match an existing project convention; this is that case.

## Problem Statement

**Finding a client defect costs two physical phones and a forty-second cycle, and the
reason is structural rather than a coverage gap.** §15 (2026-09-08) found that one Connect
tap tore down the caller's camping pass, leaving a phone unreachable for four minutes. That
defect was nine days old, live under enforcement the whole time, and no tier below two
devices could see it:

| Tier | Could it see it? | Why not |
|---|---|---|
| Kotlin unit / journey tests | No | `FixtureExchange` fakes the ports; no real endpoint exists to tear down |
| `cargo test` | No | Calling is not in our Rust at all |
| croft-stack's `attach_probe` | No | It attaches and holds; it never dials |
| Emulator | Partially | One endpoint, NAT'd networking |
| Two physical phones | **Yes** | The most expensive tier we have |

The cause is that **the calling app reaches iroh through upstream Kotlin bindings, not
through our Rust**, and two independent iroh integrations live in one APK:

```
CALLING (released v0.5.0)   Kotlin 2,902 LOC ─→ computer.iroh (upstream, Maven) ─→ iroh
SOCIAL  (P7 S2, dev module) Kotlin ─→ our libcroft_ffi.so ─→ our Rust ─→ iroh
```

`core/call-core/` has been an empty `.gitkeep` since 2026-08-11 and is not a workspace
member. `shell/` has never had an occupant.

**What we want:** the calling arc — sign in, camp, be refused, mint, be admitted, dial,
hang up — exercisable on a laptop against the real relay, in seconds.

## Reasoning

**Why not write the macOS client natively in Swift.** Quicker to start, wrong thing built.
Every defect §15 found lives in the Kotlin client's own paths; a reimplementation would
almost certainly not reproduce them, and its green would mean "the Mac client works", not
"the shipped client works". We would have built a second thing to trust rather than a
cheaper way to check the first.

**Why the port is smaller than "port the app" sounds.** The decision logic is **216 lines**
(`CampAdmission.kt` 96, `DialAdmission.kt` 120) and is already pure — injected ports,
effects at the edges, journey tests over real ports. The shape the core wants is the shape
the Kotlin already has. The `caps/` engine beside it is 1,194 lines and is **explicitly not
in scope** (see *Adjacent, out of scope*).

**Why R1 alone will feel like nothing happened, and the guard that comes with it.** R1
buys shared *rules*. The §15 defect lived in the **endpoint lifecycle**, which is R2. A
plan that stopped at R1 would be worth doing and would not solve the problem it was written
for. Worse, **R1 produces code whose only caller is its own test suite until R3 wires it** —
which is the dead-code shape the phase template exists to prevent. That is accepted
deliberately, as the price of staying additive while croftcall bakes, and it comes with a
stated expiry: *if R3 has not started when R1 lands, R1 is dead code and should be reverted
rather than left to rot.* This is the single most likely way this plan goes wrong.

Pass 2 extends that guard one layer up: **R2 without R3 is the same shape** — a port whose
only caller is its own test suite. The expiry covers R1 and R2 together, and the honest
reading is that R1–R3 are one unit of work that happens to land in three commits, not three
independently valuable phases.

**Porting is a two-way street** (owner, 2026-09-10): *"we want kotlin to be in good shape as
we port it so we are porting good code."* So when the port surfaces something wrong in the
Kotlin — and it will, because reading code closely enough to translate it is how today's
`Ok(_)` and `rebindWithToken` defects were both found — **the fix lands on BOTH sides**, not
only the new one. Otherwise the matrix reports "drift" when what actually happened is that
one side is right and the other is fossilised, and the signal that is supposed to catch
divergence instead records it as normal.

**Why additive.** The calling app is released, baking under enforcement, and carries an
unverified fix. Switching it onto a fresh core in the same motion means two moving things
and no way to attribute a regression. The core runs as a second implementation graded by
the same matrix; the duplication is the cost of being able to say which layer broke.

**What this never buys.** A laptop cannot answer NAT traversal across networks, cellular,
or mobile lifecycle. This plan lowers the cost of everything *below* the two-device line;
it does not move the line. The failure mode to guard against is a green desktop arc read as
"calling works" — the same misreading §13 made of attributed `usage` lines.

## Verified Assumptions

Everything here was read or run firsthand on 2026-09-08. Anything not listed is unverified.

- **`core/call-core/` is empty and not a member.** `find` reports 0 `.rs` files; `Cargo.toml`
  `members` lists six crates and call-core is not among them. Same for `core/feed-core/` and
  `shell/`.
- **The decision logic is 216 lines.** `wc -l` on `CampAdmission.kt` (96) and
  `DialAdmission.kt` (120). `caps/` is 1,194 across 14 files.
- **Two iroh integrations coexist in one APK.** `android/app` imports `computer.iroh.*`
  (Maven, bundles `libiroh_ffi.so`); `android/social` goes through our `libcroft_ffi.so`.
  Confirmed by import scan and `build.gradle.kts`. Two applicationIds: `ing.croft.call`,
  `ing.croft.social`.
- **D1 is answered and green** — see Phase 0. `ports/transport-iroh/tests/two_relay_modes.rs`,
  landed in croft #11.
- **iroh is 1.1 on the Rust side** (`ports/transport-iroh/Cargo.toml`), and the API shape
  used by the probe is `iroh::endpoint::presets::Minimal`, `RelayMap::try_from_iter`,
  `RelayMode::Custom` — all compile-verified, not inferred from docs.
- **`attach_probe` is an EXAMPLE target**, at
  `croft-stack/relay/source/crates/croft-relay-bin/examples/attach_probe.rs`. Cargo examples
  are not importable by dependents, so R3 **cannot** consume it as a pinned git dependency,
  and per `SHARED-CODE.md` rule 1 must not copy it either. R3 writes its own attach against
  the iroh API; that is not a copy of our code, it is use of a third-party API.
- **The relay is in `enforce`** (`/etc/iroh-relay/croft-relay.toml`, read on the box), binary
  `8e287cb7…` = croft-relay v0.2.0, unit up since the 2026-08-30 flip converge.
- **The matrix walker is Kotlin-only.** `EnforcementMatrixTest` (`:app` testDebugUnitTest)
  walks `docs/ENFORCEMENT-SCENARIOS.md` and resolves `PIN:` entries to Kotlin test names.
  **There is no Rust walker**, which is why D5 exists.

## Documentation Impact

- `docs/ENFORCEMENT-SCENARIOS.md` — the `PIN:` syntax gains a way to name a Rust test
  alongside a Kotlin one. **R1** (blocked on D5).
- `CLAUDE.md` (croft) — the status paragraph says `shell/` is skeleton and the calling app
  calls none of the core. **R1** corrects the second clause, **R4** the first. Both in the
  phase that makes the sentence false, not a docs phase at the end.
- `CHANGELOG.md` — an `[Unreleased]` entry per phase that changes what a consumer runs.
  R3 and R4 do; R1 and R2 do not (no shipped artifact changes).
- `README.md` — R3 adds a command a person can run; R4 adds an app. Both phases own their
  own entry.
- `ops/JOURNAL.md` — R3 and R4 add toolchain requirements (a macOS target, possibly a
  second Rust target). G4 makes that journalled with reason and outcome.
- `docs/adr/0004-…` — the calling transport port as a port, with D1's reasoning. **R2**
  (Pass 2 finding; precedent is `0003-keylayer-port.md`).
- `.github/workflows/ci.yml` — **R1** adds `call-core` to `core-purity`; **R2** adds the new
  port to the clippy/fmt job. Pass 2 finding: not a doc, but the same failure mode — a
  reference that goes stale silently.
- `plans/2026-09-08-plan-one-stream-calling-and-chat.md` — the parent's R1–R4 lines point
  here; no edit needed unless a phase is dropped or renamed.

## Concurrency Map

**All four phases are sequential.** R2 depends on R1's types, R3 on both, R4 on R3.
Nothing here is parallelisable and no phase needs a re-entry checklist.

The one genuine shared-state concern is **the production relay**, which R2 and R3 both
touch as a live dependency, and which peer sessions and the bake also use. Contract for
both phases: read-only against production (attach, camp, dial, hang up — no admin, no
config, no converge), and **claim `testbed--relay-live` in `.coordination/claims/`** before
any run that holds a connection for more than a few seconds, so a peer reading the journal
does not attribute our endpoint to a device.

## Phases

### Phase 0 — Discovery

D1 is **RUN**. D4–D6 are not, and two of them can change R1's shape, so this phase gates
everything below it.

- [x] **D1: can a relay-disabled endpoint and a relay-attaching one share a process?**
  - **Probe:** bind `GossipTransport` (severed) and a raw `iroh::Endpoint` with
    `RelayMode::Custom` pointed at TEST-NET-1 in one process; let the neighbour retry; assert
    the severed transport's dial card carries no relay-shaped address.
  - **Result (2026-09-08, croft #11):** **they coexist.** Severance held.
  - **Consequence:** the sibling-port recommendation in D1 below stands on the architectural
    argument alone, **not** on impossibility, and P7 S2's separation claim is not undermined.
  - **Disposition:** `keep-as-fixture` — landed as
    `ports/transport-iroh/tests/two_relay_modes.rs`.

- [x] **D4: is every admission decision actually pure, or does one reach for I/O inline?**
  **ANSWERED 2026-09-10 — yes, and the risky part was already handled.**
  - **Probe:** read `CampAdmission.kt` and `DialAdmission.kt` end to end and list every
    decision point, classifying each as `(state, input) -> (decision, effects)` or as
    something that awaits. Cross-check against `MainViewModel.dialCallee`, which is where
    the effects are actually performed today.
  - **Success criteria:** a written list of decision points with zero unclassified entries.
    A single decision that must await is not a blocker — it is a finding that changes R1's
    signature, and better found now than in the middle of the port.
  - **Result:** `CampAdmission.plan(signedIn, cached, nowMs)` and
    `action(outcome, nowMs)` take **`nowMs` as a parameter**. The one thing that could have
    forced a signature change — deciding "is my pass still good?" needs the time — is
    already injected rather than read. The Kotlin was written to core discipline before it
    ever lived in a core, so the port is a translation.
  - **Two translation details, neither structural.** `failureNote(t: Throwable)` takes a JVM
    type and becomes an error enum in Rust. And the decisions consume `Admit.CampOutcome`
    from `caps/`, which is NOT being ported — so `call-core` carries its own copy of the
    **outcome shape** (minted / refused-with-reason / unavailable / bad-request) while the
    thing that performs the HTTP call stays Kotlin. The core knows what answers are
    possible, not how to get one. That is the seam, and it is clean.
  - **Disposition:** `throwaway` — the output is the paragraph above.

- [x] **D5: how does the matrix grade two implementations?**
  **DECIDED 2026-09-10 (owner): (a) now, (b) eventually.**
  - **Probe:** read `EnforcementMatrixTest.kt` and the `PIN:` grammar in
    `docs/ENFORCEMENT-SCENARIOS.md`. Decide between (a) extending the syntax so one row can
    name both a Kotlin and a Rust test, with the Kotlin walker checking only its own and a
    new Rust walker checking only its own; (b) a single Rust walker that parses the file and
    the Kotlin walker retiring; (c) two files, which is rejected on sight because two
    matrices is the drift this whole stream exists to stop.
  - **Decision:** **(a)** — one row names both sides; two walkers, each checking its own.
    A row gains `RUST:<path>` beside its `PIN:<file>::\`test\``, the Kotlin walker keeps
    checking `PIN:` and a new Rust walker checks `RUST:`. **(b) is the destination** — the
    Kotlin walker retires once Android is on the core (D3) — but not now: retiring the
    shipped app's gate to serve unshipped code removes enforcement where it currently
    matters. **(c) two files is rejected**; two matrices is the drift this stream exists to
    stop.
  - **Owner's framing, which is why (b) is a destination and not a maybe:** *"this is all
    alpha, so I would rather prioritize forward functionality than preserve historical."*
  - **One-sided rows:** a row naming only `PIN:` passes, but the COUNT of such rows is
    asserted and may only shrink — the shape forage used for its nine pre-rule lexicon
    entries (reached zero) and the `ing.croft.*` register used 2026-09-08. It is the only
    option that lets the port land row by row while keeping incomplete coverage visible
    instead of silent. *Recorded as the default rather than as an owner decision — the
    a/b question was answered explicitly, this sub-question was not, and it is cheap to
    overturn.*
  - **Disposition:** `throwaway`.

- [x] **D6: what does R3 need that croft-stack already has, and how may it travel?**
  **ANSWERED 2026-09-10 — nothing travels, and the read found a better constraint.**
  - **Probe:** read `attach_probe.rs` and list what it does that R3 also needs (mint a
    token against admit, attach, present it). For each, decide: already in `call-core` after
    R1, reimplement against the iroh/HTTP API, or genuinely needs croft-stack code.
  - **Success criteria:** a list where nothing is marked "copy". Verified already: it is an
    **example** target, so a pinned git dependency cannot reach it — meaning "extract it
    into a library crate in croft-stack, then pin that" is the only compliant path if any
    item genuinely needs croft-stack's code, and that is a croft-stack-side change to
    negotiate, not a croft-side decision.
  - **Disposition:** `throwaway`.

**Done when:** ~~D4, D5 and D6 are answered…~~ **MET 2026-09-10.** All four discovery
items are resolved, the phases they touched are edited, and the plan is accepted. See
*Decisions* for each answer and *Review Log* for what changed as a result.

---

### R1 — `call-core`: the decision rules, dual-graded

**Goal:** the camp and dial admission rules exist in Rust as a pure core, graded by the
same matrix that grades the Kotlin, with Android untouched.

**Changes:**
- [ ] `core/call-core/Cargo.toml` + `src/lib.rs` — new crate, added to workspace `members`
- [ ] `src/model.rs` — the state a decision reads (session presence, cached pass + expiry,
      grant/proof availability) and the decision/effect types
- [ ] `src/camp.rs` — `CampAdmission`'s rules: mint / reuse / re-mint at margin / degrade
      with words, one arm per matrix row
- [ ] `src/dial.rs` — `DialAdmission`'s rules including `rebind` (R0's fix, ported with its
      reason intact — a dial never lowers admission)
- [ ] `docs/ENFORCEMENT-SCENARIOS.md` — `PIN:` syntax extended per D5
- [ ] the Rust matrix walker per D5
- [ ] **`core/call-core/clippy.toml`** — the purity lints, copied from
      `core/social-tree-core/clippy.toml`. Pass 2 finding: they are a **per-crate** config,
      so without this file a clock read in `call-core` is not a defect, it is invisible
- [ ] **`.github/workflows/ci.yml`** — add `call-core` to the `core-purity` job's wasm arm,
      clippy and fmt. Pass 2 finding: every Rust gate names crates explicitly with `-p`, so
      a new crate lands **outside all of them** and CI stays green over it
- [ ] `CLAUDE.md` — the "calling app calls none of it" clause becomes accurate again

**Call chain:** `core/call-core` tests → `camp::decide` / `dial::decide`. **This phase has
no production caller** — the first is R3. Stated rather than hidden: it is the additive
constraint's cost, and the expiry in *Reasoning* applies.

**Wiring test:** the Rust matrix walker, RED at phase start (no rows resolve to Rust tests)
and GREEN at end (every row that names a Rust test resolves to one that exists). This is
the phase's anti-dead-code gate: the walker proves the rules are reachable from the
document that defines them, which is the only consumer they have until R3.

**Depends on:** D4 (signature), D5 (grading).

**Read-set:** `android/app/src/main/java/ing/croft/call/CampAdmission.kt`,
`.../DialAdmission.kt`, `.../MainViewModel.kt`, `.../caps/Admit.kt`,
`docs/ENFORCEMENT-SCENARIOS.md`, `android/app/src/test/java/ing/croft/call/EnforcementMatrixTest.kt`.

**Write-set:** `core/call-core/**` (including its own `clippy.toml`), `Cargo.toml`,
`.github/workflows/ci.yml`, `docs/ENFORCEMENT-SCENARIOS.md`, `CLAUDE.md`, and the new Rust
walker's file.

**Shared-state contract:** no mutable state beyond the write-set. No network, no relay, no
device. Pure `cargo test`.

**Risks:** the port drifts from the Kotlin while both are live — mitigated by the shared
matrix, which is the whole point of dual-grading. Second risk: scope creep into `caps/`
(1,194 lines); the boundary is that `call-core` decides, `caps` fetches, and fetching stays
Kotlin in this phase.

**Done when:**
1. **Behavioural:** the enforcement matrix grades both implementations — a row failing in
   Rust fails the build even though the Kotlin passes.
2. **Verification:** the **walker**, not the module — `cargo test -p call-core --test
   enforcement_matrix` (Pass 3 finding: an earlier draft said `cargo test -p call-core`,
   which is the isolated-module command the quality gate explicitly flags; it proves the
   rules work alone and nothing about whether the matrix reaches them). Plus
   `./gradlew :app:testDebugUnitTest` still green, the deliberate perturbation of one
   matrix row watched to fail the Rust walker specifically, and
   `cargo check -p call-core --target wasm32-unknown-unknown` + `cargo clippy -p call-core`
   green in CI **watched to fail** against a deliberately inserted `SystemTime::now`. A
   purity gate nobody has watched reject a clock read is not a purity gate.

**Tests first, and the boundaries named** (Pass 3). Write the walker and the row tests
before the rules. The rules are branching code with real edges, so single-point assertions
would survive a one-line mutation — name the edges up front:
- **re-mint margin:** inside the margin mints; *exactly at* the boundary mints; outside
  reuses. Three points, not one. The Kotlin matrix already pins the boundary case
  (`a pass exactly at the margin boundary still mints`) and the Rust must too, or the
  ported rule can drift by one comparison operator with both suites green.
- **refusal mapping:** each refusal reason maps to its own words — asserted per reason, not
  "a refusal produces some words".
- **the R0 rule:** a dial with a live pass and a tokenless want KEEPS; identical token
  keeps; a different non-null token swaps. The interesting case is the one that regressed.

**Validation:** *Narrow.* Tests are sufficient — no I/O, no shipped artifact changes.

---

### R2 — the calling transport port

**Goal:** the endpoint lifecycle in Rust — bind, camp with a pass, dial — such that the
§15 defect class is reachable by `cargo test` instead of by two phones.

**Changes:**
- [ ] a new sibling port per D1's recommendation (name TBD at execution; **not** a mode
      inside `ports/transport-iroh`, whose relay-disabled guarantee is structural)
- [ ] bind with a persisted secret key, `RelayMode::Custom` at our relay, auth token carried
- [ ] the camp/dial lifecycle, with R0's rule enforced *at the port* rather than only in the
      decision layer
- [ ] **`docs/adr/0004-…`** — Pass 2 finding: a new port has precedent,
      `docs/adr/0003-keylayer-port.md` is "the key layer is a port". D1's reasoning is the
      ADR's content and should live there, not only in a plan that will be archived
- [ ] **`.github/workflows/ci.yml`** — the new port added to the clippy/fmt job beside
      `transport-iroh` (it is not a pure core, so it belongs in that job, not core-purity)
- [ ] `CLAUDE.md` — the ports list

**Call chain:** `call-core` decision → port `attach(token)` / `dial(peer)` → iroh
`Endpoint`. R1's rules become the port's caller, which is what retires R1's dead-code
status.

**Wiring test:** **the §15 regression, as a test.** Camp against the enforcing relay,
dial, and assert the camped connection survives — no `no_token` after the dial, no `usage`
close one second later. RED against a port that rebinds unconditionally; GREEN against one
that does not. Marked `:live` (per `VERIFICATION.md`, a `:live` suffix is a recorded reason
not to gate) and run deliberately, not in CI.

**Tests first** (Pass 3): the `:live` regression is written and RED against the current
rebinding behaviour before the port is written. It is the phase's reason for existing, so
writing it second would be writing it to fit.

**Depends on:** R1.

**Read-set:** `android/app/.../net/CallPeer.kt` (the lifecycle being ported),
`ports/transport-iroh/src/transport.rs` (the bind pattern), `core/call-core/**`.

**Write-set:** the new port's crate, `Cargo.toml`, `.github/workflows/ci.yml`,
`docs/adr/0004-…`, `CLAUDE.md`.

**Shared-state contract:** **touches the production relay.** Read-only in the operational
sense — attach, camp, dial, hang up; no admin, no config, no converge. Claim
`testbed--relay-live` before any run holding a connection beyond a few seconds. Binds no
local ports beyond iroh's ephemeral UDP. Uses a test account's credentials from
`CroftC/.env`, never committed.

**Risks:** the biggest is a false green — a test that passes because the relay admitted us
for a reason unrelated to what we think. Mitigation: assert on the **relay's own journal
lines**, not just on client-side success, exactly as §15 did.

**Observability** (Pass 3). §15 was diagnosed from the *relay's* journal, not the client's:
app-tagged logcat was drowned in system noise and the successful mint is silent at every
client layer. So the port must emit what the relay cannot see — attach attempted, token
presented or not, rebind decided and why, connection closed and by whom — at `debug`, with
the endpoint id on every line so a run can be correlated with the journal by eye. Without
that, a failure here is diagnosed the same expensive way §15 was.

**Done when:**
1. **Behavioural:** a `cargo test` run on this laptop reproduces the §15 defect against a
   port that has it, and passes against the port that does not.
2. **Verification:** the `:live` regression test, plus the relay journal showing
   `admitted … sponsorship=` and **no** `no_token` in the dial window.

**Validation:** *Broad.* Tests plus live integration; check the relay journal, confirm the
data flow, verify the refusal paths (tokenless, wrong key) not just the happy one.

---

### R3 — the headless arc binary (the fast loop)

**Goal:** one command on a laptop walks the whole calling arc against production, with no
phone and no adb.

**Changes:**
- [ ] a `bin/` target (crate name TBD) — sign in / load session, camp, dial a named peer,
      hang up, and print what it observed
- [ ] session handling: reuse a stored OAuth session; refuse honestly when the refresh
      token is dead (the §15.2 case — `Signed in` over a dead session is the defect this
      must not reproduce)
- [ ] `README.md` + `CHANGELOG.md` + `ops/JOURNAL.md` entries

**Call chain:** `main` → session load → `call-core::camp::decide` → port `attach` →
`call-core::dial::decide` → port `dial` → relay. This is the first end-to-end chain in the
plan and the reason R1 stops being dead code.

**Wiring test:** the binary itself, run against production, asserting on the relay journal.
A test that runs the binary and greps the journal for this endpoint's `admitted … sponsorship=`.

**Depends on:** R1, R2, D6.

**Read-set:** `croft-stack/relay/source/crates/croft-relay-bin/examples/attach_probe.rs`
(read for what it does — **not** copied; it is an example target and unimportable),
`android/app/.../caps/Admit.kt` and `.../identity/AuthManager.kt` for the mint and session
shapes.

**Write-set:** the new binary crate, `Cargo.toml`, `README.md`, `CHANGELOG.md`,
`ops/JOURNAL.md`.

**Shared-state contract:** as R2, plus it holds a real OAuth session for a test account.
Credentials from `CroftC/.env`; never echoed, never committed. Claim `testbed--relay-live`.

**Risks:** the tempting one is letting this become a second client rather than an
instrument. Guard: it prints observations and exits; it holds no UI state and makes no
decision the core does not make.

**Observability** (Pass 3). This binary IS the diagnostic surface, so its output is a
deliverable rather than a side effect: each step of the arc printed with its outcome and
the endpoint id, and — the part that matters — **the silence cases named out loud**. A
successful mint is silent at every layer; a binary that prints nothing between "camping"
and "admitted" reproduces the exact ambiguity that made §13 misread its own evidence.

**Done when:**
1. **Behavioural:** running one command on this laptop produces
   `admitted endpoint_id=… sponsorship=…` in the production journal, then a dial, then a
   hang-up — with no phone attached.
2. **Verification:** that command, plus the journal lines it caused, pasted into the Review
   Log with timestamps.

**Validation:** *Broad.* Live against production under enforcement.

---

### R4 — `shell/`'s first occupant: the macOS app

**Goal:** a person can do R3's arc without a terminal, and `shell/` stops being a `.gitkeep`.

**Changes:**
- [ ] `shell/<name>/` — the macOS shell, per D2 (headless first, UI second)
- [ ] the FFI surface it consumes — whether `ffi/` grows a calling object beside
      `ChatSession` or a second crate appears is an execution decision, recorded when made
- [ ] `CLAUDE.md`, `README.md`, `CHANGELOG.md`, `ops/JOURNAL.md`

**Call chain:** macOS UI event → FFI → `call-core` → port → relay.

**Wiring test:** a test at the FFI boundary that drives camp-then-dial through the same
surface the UI uses — not the core directly. If the test can reach the core without going
through the FFI, it is not testing the wiring.

**Depends on:** R3.

**Read-set:** `ffi/src/lib.rs` (the `ChatSession` pattern), `shell/`, R3's binary.

**Write-set:** `shell/**`, possibly `ffi/**`, `Cargo.toml`, the four docs above.

**Shared-state contract:** as R3, plus a macOS build toolchain — journalled per G4.

**Risks:** scope. A UI invites features. This phase's job is parity with R3's arc, nothing
more; anything else is a later plan.

**Done when:**
1. **Behavioural:** a person clicks in a macOS app and the production relay journal shows
   that endpoint admitted, dialling, and hanging up.
2. **Verification:** the FFI-boundary wiring test, plus the journal lines.

**Validation:** *Broad*, and explicitly **not** a substitute for a device run — see the
line in *Reasoning* about the two-device tier.

---

## Adjacent, explicitly OUT of this plan's scope

- **`caps/` (1,194 lines)** — the callability engine, ticket redemption, OAuth flow, DPoP,
  the XRPC surface. `call-core` decides; `caps` fetches. Porting it is a separate plan and
  probably follows R4, not R1.
- **R5 (the rendered-principal seam), R6, R7, R8** — the parent plan's.
- **Android switching onto the core (D3)** — deliberately undecided until after R3.
- **Chat.** `chat-core` and `call-core` never merge; per-pond cores is law.

## Open Questions

**All closed 2026-09-10** — walked with the owner one at a time. Severities below were the
recommendations; the resolutions are in *Decisions*, and Pass 3's escalation (agent-set
severities never reviewed) is discharged.

- ~~`[BLOCKING]` **D4** — is every admission decision pure?~~ **RESOLVED by reading the
  code**, not by asking: `nowMs` is already a parameter. It never needed to be a question,
  which is itself the lesson — a probe that can be answered by opening the file should be
  run before it is escalated.
- ~~`[BLOCKING]` **D5** — how does the matrix grade two implementations?~~ **DECIDED (a)
  now, (b) eventually.**
- ~~`[PHASE-GATED (R3)]` **D6** — what may travel from croft-stack?~~ **RESOLVED: nothing
  does.** Also resolved by reading rather than asking.
- ~~`[ADVISORY]` **D2** — first macOS artifact.~~ **CONFIRMED: headless first.**
- ~~`[ADVISORY]` **D3** — does Android ever switch?~~ **DECIDED: yes; timing after R3.**
  Upgraded from advisory in effect — it changes what R1 and R2 optimise for.

One sub-item carried as a default rather than an owner decision: **what happens to a matrix
row that names only the Kotlin side.** The a/b question was answered explicitly; this was
not. The default is "passes, but the count is pinned and may only shrink". Cheap to
overturn; see D5.

## Decisions

### D1 — where does the calling endpoint live?

**Options.** (a) Extend `ports/transport-iroh` with a calling mode. (b) A new sibling port.

**Recommendation: (b), a separate port.** `ports/transport-iroh` sets `RelayMode::Disabled`
and contacts no relay **by construction rather than by care** — CLAUDE.md's words — and P7
S2's separation claim rests on it. A crate containing both endpoint kinds downgrades that
guarantee to a convention. The shared surface is small (`Endpoint` construction,
`EndpointId`); most of the crate's 1,277 lines are codec that calling does not want.

**Probe RUN 2026-09-08 — they can coexist, so (b) is a judgment, not a necessity.**
`ports/transport-iroh/tests/two_relay_modes.rs`. A relay-attaching endpoint binds beside the
severed transport, retries for 1.5 s, and the severed dial card still carries no
relay-shaped address. **The recommendation therefore stands entirely on the architectural
argument** — the plan must not be read as though the probe forced it — and **P7 S2's
separation claim is not undermined**, which mattered because the two apps already share a
phone.

**The probe's limit, stated.** It asserts what this repo already means by severance — no
relay-shaped address escaping into a dial card — which is a property of what the endpoint
*publishes*, not proof it never contacts a relay. A stronger claim needs traffic
observation. If (a) is ever chosen anyway, that is the evidence to go and get first.

### D2 — what is the first macOS artifact? **CONFIRMED 2026-09-10 (owner).**

**Headless (R3) first, UI (R4) after.** R3 is what removes adb from the
loop; whether a person can *use* it is a different question from whether we can *test* it,
and UI work should not block the loop that motivated the plan. A headless binary can also
run in CI; a window never will.

**The limit, stated so R4 is not treated as garnish.** A headless binary can print "NOT
camped on relay", but nothing judges whether those are the right words in the right place.
Screen honesty — the rule that cost two device runs — is a **UI** rule, so R4 is where the
honesty invariant gets its second surface. Second, not optional.

### D3 — does Android switch onto the core? **DECIDED 2026-09-10 (owner): YES. Timing after R3.**

**This changed on 2026-09-10 and the change is load-bearing.** The question used to be "does
it switch *at all*", to be answered after R3. It is now **"assume yes; decide the timing
after R3"** — because D5's answer implies it. Retiring the Kotlin walker (D5's "(b)
eventually") only makes sense if the Kotlin implementation stops being what Android runs.

**Why the distinction is not pedantry — it changes what R1 and R2 optimise for:**

| If D3 were "maybe" | D3 is "yes, timing TBD" |
|---|---|
| design the core for the macOS shell; worry about Android's FFI later, if ever | **design the core to cross the Android FFI from the start**, because it will |

P7 has already paid and proven that cost once for chat — the uniffi surface, the arm64
cross-compile, a real device `dlopen`ing the result — so it is a known quantity rather than
a risk.

**A consolidation nobody had costed.** Today a phone carrying both apps carries **two** iroh
copies: upstream's `libiroh_ffi.so` in the calling app, ours inside `libcroft_ffi.so` in the
social one. One app on one core is **one** copy. P7's measured 6.3 MB → 29.6 MB is the cost
of putting iroh in our Rust *once*; calling joining it does not pay that twice.

**The real cost, named honestly:** we take over a surface n0 currently maintains for free,
including the Android cross-compile. That is the thing to weigh at R3, not whether to go.

## Review Log

**Pass 1 — 2026-09-08.** Expanded from a shape sketch into a phase plan against the
`phase-plan` template. D1's probe result was already in the sketch and is carried forward
unchanged. Three things the expansion surfaced that the sketch did not have:

1. **R1 has no production caller until R3** — the dead-code shape the template's "call
   chain" field exists to catch. Recorded with an expiry rather than hidden, and R1's
   wiring test is now the Rust matrix walker specifically so the phase has a gate at all.
2. **`attach_probe` is an example target**, so R3 cannot take it as a pinned git dependency
   and must not copy it. That turns D6 from a vague "what can we reuse" into a concrete
   question with a compliant path (extract to a library crate in croft-stack, then pin) and
   a lead time, because it is another repo.
3. **The matrix walker is Kotlin-only**, so "dual-graded" was an assertion with no
   mechanism. D5 now has to answer it before R1 can have a wiring test.

**Pass 2 — gap analysis, 2026-09-08.** Claims checked against the code rather than the
plan. Four findings, two of which change phase contents:

1. **Every Rust gate names crates explicitly with `-p`.** `ci.yml`'s `core-purity` job runs
   `cargo check -p social-tree-core --target wasm32-unknown-unknown`, `-p chat-core`, and
   clippy/fmt per crate. **A new crate lands outside all of them and CI stays green over
   it.** For `call-core` that is not cosmetic: WASM-cleanliness and no-clock are the
   defining properties of a core in this repo, and they would simply not be enforced.
   Added to R1's changes, write-set and done-when — including *watching* the purity gate
   reject a deliberately inserted `SystemTime::now`, because a gate nobody has watched
   fail is indistinguishable from one that is not wired.
2. **The purity lints are a per-crate `clippy.toml`.** They live in
   `core/social-tree-core/clippy.toml` (`disallowed-methods` / `disallowed-types` on
   `SystemTime` and `Instant`). Clippy reads the nearest one, so `call-core` needs its own
   copy or the lints do not apply. *Observed while checking, and not this plan's to fix:*
   **`chat-core` has no `clippy.toml`** — it is wasm-checked but not clock-linted today.
3. **A new port has ADR precedent.** `docs/adr/0003-keylayer-port.md` is "the key layer is
   a port". R2's D1 reasoning belongs in an ADR, not only in a plan that will be archived.
   Added to R2 and to Documentation Impact.
4. **Ships-alone coherence: R2 has R1's problem one layer up.** A port whose only caller is
   its test suite is the same dead code. The expiry guard now covers R1 and R2 together,
   and the plan says plainly that R1–R3 are one unit of work landing in three commits.

Dependencies re-verified: R2 genuinely consumes R1's decision types; R3 consumes both; R4
consumes R3. Concurrency map unchanged — all sequential, no parallel set to audit for
write-set disjointness, and the one real shared resource (the production relay) already has
an invariant-shaped contract rather than a mechanism-shaped one.

**Pass 3 — quality gates, 2026-09-08.** Five checks, four fixed in place and one escalated.

1. **R1's verification command was the flagged anti-pattern.** It read
   `cargo test -p call-core` — the isolated-module form the gate explicitly names as a plan
   defect, because it proves the rules work alone and nothing about whether anything reaches
   them. Replaced with the walker command. Worth noting that this plan *argued* against dead
   code at length and then specified a verification that could not have caught it.
2. **Boundary cases were unnamed**, so R1's test specifications would have survived a
   one-line mutation. The re-mint margin is branching code with a real edge — the Kotlin
   matrix already pins `a pass exactly at the margin boundary still mints` — and a ported
   rule can drift by one comparison operator with both suites green. Edges now named for
   the margin, the refusal mapping and the R0 rule.
3. **No observability was planned for R2/R3**, in a plan whose motivating incident was
   diagnosed *from the relay's journal* because client logging was useless. Both phases now
   specify what to emit, including naming the silence cases — a successful mint is silent at
   every layer, and a binary that prints nothing between "camping" and "admitted" rebuilds
   the ambiguity that made §13 misread itself.
4. **TDD ordering was implied, not stated.** R1 and R2 now say which test is written first
   and why writing it second would mean writing it to fit.
5. **ESCALATED — the open-question severities are agent-set and unconfirmed.** D4 and D5 are
   marked BLOCKING on my judgement, D6 PHASE-GATED, D2/D3 ADVISORY. Pass 3 requires the user
   to see and confirm these before execution starts. **This plan is not ready to execute
   until that happens**, independent of anything else in it.

Coherence check passed: the plan still solves the problem in its Problem Statement, scope
has not crept (`caps/` is explicitly out, and R5–R8 stayed with the parent), and the
reasoning reconstructs from the document alone.

*Three passes complete.* ~~Execution is gated on item 5.~~

**Phase 0 closed — 2026-09-10.** The four discovery items walked with the owner one at a
time, in plain English, each grounded in the code before being discussed rather than after.

**Two of the three "blocking" questions were answered by opening a file.** D4 (are the
decisions pure?) was settled by reading `CampAdmission.kt` — `nowMs` is already a parameter.
D6 (what may travel from croft-stack?) was settled by reading `attach_probe.rs` — 33 lines,
every import third-party, nothing of ours in it. **Both had been escalated to the owner as
questions when they were probes.** Worth recording as a planning lesson: Pass 1 marked them
BLOCKING on the reasoning that they *could* change R1's shape, which was true, and then did
not spend the ten minutes that would have shown they did not.

**D6's real output was not the answer to its question.** Reading the probe showed it
attaches with `iroh_relay::client::ClientBuilder` — a relay client — while the app binds an
`iroh::Endpoint` with a `RelayMode`. Different layers. The probe proves *the relay accepts
this token*; it proves nothing about *the app's endpoint attaching*, which is exactly where
§15's defect lived. R3 now carries that as a constraint: go through `Endpoint`, not the
probe's shape, however convenient 33 lines look.

**D3 moved from a question to a premise**, and that propagates. "Assume Android switches;
decide when after R3" means R1 and R2 design for the Android FFI from the start rather than
treating it as a later maybe. It also surfaced a consolidation nobody had costed: a phone
with both apps carries two iroh copies today, and one app on one core carries one.

**A standing rule came out of D5's framing** and is now in *Reasoning*: the port is a
two-way street. Defects the port surfaces in the Kotlin get fixed on both sides, or the
matrix records divergence as normal instead of catching it.
