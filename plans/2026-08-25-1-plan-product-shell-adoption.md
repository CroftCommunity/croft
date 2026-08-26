# Product-shell adoption: the shells consume the core (E117 P7's successor plan)

`Status: DRAFT (Pass 2 applied, 2026-08-25). Awaiting owner question walk + Pass 3.`
`Execution target: a fresh executor context under the phase-plan skill`
`(~/.claude/coding-agents/skills/phase-plan/execute.md). This document is the sole`
`handoff artifact — everything the executor needs is in this file or reachable from it.`
`Predecessor: discovery plans/2026-08-20-1-plan-social-tree-core.md (E117, phases 1–6`
`executed and closed 2026-08-25). This is the plan that E117 §4 named and refused to`
`commit: "product shells consuming them — its own plan with its own constraints per`
`platform."`

## Problem statement

The shared core is real and nothing a user touches consumes it. `core/social-tree-core`
holds the governance fold and the admission machinery; `core/chat-core` is the chat pond;
`ports/keylayer-openmls` runs both admission paths on real MLS at loopback — all of it
green, none of it on a phone. Meanwhile the one shipping app (`croft/android`, the
inherited croftcall client) speaks *calling only*: the M-series caps engine, contract v2
against `CroftCommunity/connect`, relay admission against croft-stack. Chat exists only
as the discovery-side TUI dev harness on pinned crates.

The gap this plan closes: the shells adopt the core — the android app first, because it
exists and ships — without destabilizing the calling track, and the two tracks meet at
exactly one designed joint (the rendered-principal seam) instead of growing into each
other.

## The standing constraint (owner, 2026-08-25): croftcall is LIVE and mid-validation

**Croftcall is currently running through testing and has edges in both the croft and
connect repos.** The §12 enforce rehearsal ran all-green on hardware 2026-08-24; what
remains on M4 is *operational* (croft-admit activation, the production enforce flip —
owner-gated, croft-stack `TODO.md`) plus small closures (optimistic-Ready honesty,
caller-side camp posture, E125–E128). The bake/§13 validation work is ongoing in a
concurrent session.

Consequences, binding on every phase below:

1. **Additive only, until the named joint.** No phase touches the calling path: not the
   caps engine (`android/.../caps/`), not `AuthManager`/OAuth, not the relay/camp
   admission code, not `DeepLink`, not the contract. The adoption work rides new modules
   beside the calling code.
2. **The contract belongs to connect.** Contract v2 is canonical there and the app under
   test speaks it. Any change calling-grants-derived-from-group-standing would imply is a
   *contract conversation* (G8: stated version + visible degrade path), drafted as a
   proposal to connect — never a P7 unilateral.
3. **The two admissions stay severed** (croft ADR-0002): fabric admission (relay/camp,
   M4's whole subject) and group admission (the A-series, this plan's subject) share a
   word and nothing else. No signal crosses.
4. **Coordination discipline:** all work in worktrees; the android app module is a
   contested surface while the M4 track is hot — claims in `.coordination/claims/`
   before any commit that touches `android/`, and the S4 joint phase explicitly waits
   for M4's operational close.

## Approach

Six phases, S0–S5. The order is: bindings first (pure additive), then an android chat
surface behind a flag (new module, no calling contact), then real MLS on-device, then
the two *designed* joints — the E120 binding fact and the rendered-principal seam — and
only then the web probe. Apple stays named-not-committed. Every phase is RED-first under
the existing harness disciplines (the workflow-harness journey pattern for android; the
core's pins for anything that touches core).

### Phase 0 — discovery probes (Discovery Exemption applies; load execute.md § Discovery Exemption first)

Every remaining unknown below is toolchain- or dependency-shaped and resolvable by a
probe. Nothing in croft has ever used uniffi; the openmls provider's keystore is
in-memory; the redb adapter lives only in the discovery corpus. Committing S0–S2
without these probes risks exactly the mid-phase rework Phase 0 exists to prevent.

- [ ] **D1: Does a uniffi round-trip work end to end on this machine?**
  - **Probe:** Minimal crate at `ffi/` (proc-macro style, current uniffi release —
    record the version chosen) exporting one function over one core type
    (`PrincipalId` or a trivial struct); generate Kotlin; call it from a JVM test
    (JNA loads the cdylib). Record the exact toolchain steps that worked.
  - **Success criteria:** The JVM test calls Rust and asserts the returned value.
  - **Disposition:** `promote` — the spike crate becomes S0's scaffold; TDD applies
    when S0 builds the real surface on it.
- [ ] **D2: What does persistent MLS state require on openmls =0.8.1?**
  - **Probe:** Read `openmls_traits` v0.5's storage/provider traits and
    `openmls_rust_crypto` v0.5.1's keystore. Determine the sanctioned persistence
    path: implement the storage trait over a file/redb backend, vs serialize the
    memory keystore. Verified fact going in: the current provider is
    **memory-only** — S2's "provider persists under the app dir" is unbuilt work.
  - **Success criteria:** A named strategy with the trait surface listed, sized
    (hours vs days), recorded in this plan before S2 starts.
  - **Disposition:** `throwaway` (notes into this doc; implementation is S2's).
- [ ] **D3: Is the android native toolchain present and can the emulator load our lib?**
  - **Probe:** `rustup target list --installed` (want `aarch64-linux-android`),
    cargo-ndk / NDK presence, then build the D1 crate for aarch64-linux-android and
    load it in a scratch activity or instrumentation test on the arm64 emulator.
  - **Success criteria:** The emulator loads the `.so` and returns a value; or the
    missing toolchain pieces are named with install steps.
  - **Disposition:** `throwaway`.
- [ ] **D4: What does promoting the corpus redb adapter cost?**
  - **Probe:** Read `discovery/alpha/experiments/local_storage_projection`'s store
    code; list what is croft-shaped (the Store realization over redb) vs
    corpus-only (projection/edge code that stays).
  - **Success criteria:** A promote-vs-rewrite recommendation with the file list.
  - **Disposition:** `throwaway` (feeds S0; open question Q6 decides).

*Done when:* D1–D4 findings are recorded in Verified Assumptions, Q6 is decidable,
and S0–S2's items are adjusted here if any probe contradicts them (Phase 0 is the
only phase allowed to restructure later phases).

### S0 — the uniffi surface: the core crosses the FFI line

A crate under **`ffi/`** — the home the repo README already assigns ("uniffi bindings
for the mobile shells"; `shell/`, `web/`, `apple/` are `.gitkeep` placeholders, there
is no existing skeleton crate — Pass 2 correction) — exposing `social-tree-core` +
`chat-core` to Kotlin via uniffi: the substrate instance, the pond's
`Intent`/`Effect`/`update`/`project` loop (`core/chat-core/src/{model,update,project}.rs`),
and a **redb Store realization on the Rust side** (storage lives under the app's files
dir; redb is pure Rust and crosses no FFI). The redb adapter does not exist in croft
today — it lives in the discovery corpus (`local_storage_projection`); Q6 decides
promote-vs-rewrite and this phase lands it (likely `ports/store-redb`). The
effect-composition rule (ADR-0002) becomes concrete here: **one substrate instance per
shell, owned by the shell, ports beside it** — the uniffi object graph is exactly that
sentence.

**Changes:** the `ffi/` crate (from D1's scaffold); the redb store port; workspace
`members` gains both crates (root `Cargo.toml`); CI arms for both (read
`CroftC/.claude/CI-PATTERN.md` before touching the workflow; follow the existing
clippy `--force-warn missing_docs` and fmt patterns in `.github/workflows/ci.yml`).
**Call chain:** Kotlin test → generated bindings → ffi crate → `chat_core::update`/`project`
→ `social_tree_core::evaluate` → redb store port.
**Wiring test:** a Kotlin JVM test driving create-group → send → project through the
generated bindings and asserting the projected timeline — RED before the surface
exists, GREEN at phase end. Component tests alone do not close this phase.
**Depends on:** Phase 0 (D1, D3, D4).
**Write-set:** `ffi/**`, `ports/store-redb/**` (or the name Q6 yields), root
`Cargo.toml`, `.github/workflows/ci.yml`, `README.md` (map wording).
**Shared-state contract:** no android app code touched; no ports bound; work in a
croft worktree; no shared mutable state beyond the write-set.
**Risks:** uniffi type-mapping friction on enum-heavy `Intent`/`Effect`; native-lib
loading on JVM differs from android (D3 covers the android half).
**Validation:** moderate — wiring test + unit tests + a manual run of the JVM harness.

*Done when:* **(behavioral)** a Kotlin unit test drives create-group → send → project
through the bindings on JVM, and the arm64 emulator loads the native library (the
inherited client's packaging-bug class is exactly what the emulator CAN answer);
**(verification)** the JVM binding-test command (exact gradle/cargo invocation named
during execution) runs green, and the emulator load is a recorded run; no android app
code touched yet.

### S1 — the android chat surface, behind a flag

A new UI surface (module/package beside `ing.croft.call`, e.g. `ing.croft.social`) —
groups list, timeline, the truthful membership panel, mute — consuming S0's bindings,
gated behind a build flag so release builds of the calling app are byte-boring. The P6
renderings port from the TUI: CONTESTED as "membership pending resolution", the voided
row, marked-never-dropped muted lines, the fork banner as a blocking surface. The
**lost-race UX** debt (two concurrent admissions; the losing side's rendering) lands
here with the E116 leftovers — this is the phase that owed it a home.

**Call chain:** the flag-gated entry (launcher/nav entry visible only under the flag)
→ `ing.croft.social` screens → S0 bindings → the core loop.
**Wiring test:** a `SocialJourneyTest` in the existing harness home
(`android/app/src/test/java/ing/croft/call/workflow/` — six `*JourneyTest.kt` files
set the pattern) driving group-create → invite → message → panel-truth through the
real bindings, not mocks of them.
**Depends on:** S0; Q1 (flag mechanics) decided.
**Write-set:** the new package/module (`ing.croft.social` or the module Q1 shapes),
its test sources, gradle config for the flag/flavor. **Nothing under
`ing/croft/call/`** — any gradle-file line the calling build reads is a contested
surface: claim first.
**Shared-state contract:** claim `croft--android-social-surface` in
`CroftC/.coordination/claims/` before the first `android/` commit (rule 4); the M4
track owns `android/` ambient state until its operational close.
**Risks:** gradle flavor config touching shared build files (the one write the
calling track can see — smallest possible diff, claim held); uniffi-generated Kotlin
ergonomics pushing toward a hand-written facade layer.
**Validation:** moderate — journey test + full existing suite + a manual emulator
walk of the surface.

*Done when:* **(behavioral)** the journey harness drives group-create → invite →
message → panel-truth on JVM against the real bindings, and a screenshot run on the
arm64 emulator shows the surface; **(verification)**
`./gradlew :app:testDebugUnitTest` fully green — the calling app's existing tests
untouched and passing is part of the gate, and a release-variant build shows the
surface absent.

### S2 — the keylayer joins the product: sealed chat on-device

`ports/keylayer-openmls` wired through the shell: group creation seats real MLS, the
invite path carries the real Welcome, messages seal/open at AEAD grade on-device, and
the token-return arc runs device-to-device — the E117 P6 loopback ladder climbed one
rung. Storage: the openmls provider persists under the app dir beside redb. The honest
rung after this phase: chat is MLS-sealed on hardware; transport between devices rides
the existing sync machinery first (iroh-gossip as in the TUI), the relay path being
fabric admission's territory and out of scope.

**Pass 2 correction:** the openmls provider's keystore is in-memory today
(`openmls_rust_crypto` =0.5.1) — persistence is work this phase BUILDS, on the
strategy D2 named. "The openmls provider persists under the app dir" is the done
state, not the starting state.
**Call chain:** the S1 surface → bindings → `ports/keylayer-openmls`
(`seal`/`open`/both admission paths) → persistent provider storage + redb, both under
the app files dir.
**Wiring test:** the S1 journey extended to sealed round-trip (seal on one substrate,
open on another) at JVM grade; the device run is the validation tier above it.
**Depends on:** S1; D2's strategy; Q2 (transport rung) decided; testbed claim
(`testbed--<resource>` per `CroftC/.claude/TESTBED.md`) before the device run.
**Write-set:** `ports/keylayer-openmls/**` (persistence), the ffi crate, the S1
package. No calling-path files.
**Shared-state contract:** device runs claim the testbed devices; no relay contact —
transport rides iroh-gossip per Q2, the relay path is fabric admission's territory.
**Risks:** MLS state corruption across app restarts (the persistence work's whole
point — test kill-and-relaunch explicitly); openmls storage trait friction on 0.8.1.
**Validation:** broad — wiring test + kill-and-relaunch persistence test + the
two-device run recorded runbook-style.

*Done when:* **(behavioral)** two real devices (the Samsung + the borrowed Pixel, per
the standing rig) exchange sealed messages in a group one of them planted; a departure
+ token return runs device-to-device; state survives an app restart;
**(verification)** the JVM sealed-round-trip test green plus the device run recorded
with per-plane rungs stated in the runbook style.

### S3 — the E120 binding fact: DID ↔ persona as a recorded human act

The designed joint between the calling identity (atproto DID, proven by OAuth — M3's
`provenDid`) and the social identity (persona keys). Design first, ADR before code, per
the house pattern: the **binding is a vouch-shaped fact** — a human judgment the system
records, never computes (Part 1 §2.0) — with shape, revocation, and composition with
§11.8 standing all specified. The prize it unlocks: calling grants derived from group
membership and standing. That derivation is the **contract conversation**: this phase
produces the design + a proposal document for connect (contract v3 sketch: what the
grant record would say, the degrade path for v2 peers), and *stops there* — no contract
change ships from this plan.

**Call chain:** (core-side) ingest → `evaluate` → the binding fact recorded on the
fold; no product caller yet in this phase — S4 is the first renderer, and that is
deliberate: the fact type's wiring test is core-side pins, its product wiring is S4's.
**Wiring test:** core pins proving the binding fact round-trips the fold (recorded,
revoked, composed with §11.8 standing) — RED-first per the house discipline.
**Depends on:** nothing after Phase 0 — S3 is order-independent of S1/S2 (see
Concurrency map); Q3 (fact shape) is decided by this phase's ADR.
**Write-set:** `docs/adr/0004-<slug>.md` + the `CroftC/.claude/DECISIONS.md` registry
row (same change, per the house rule), `core/social-tree-core/src/**` (the fact type),
`core/social-tree-core/WIRE-REGISTER.md` (any wire-visible payload), the proposal doc
(location: Q7).
**Shared-state contract:** no android, no connect, no calling-path files; core work in
a croft worktree.
**Risks:** scope creep toward E134's whole connections aspect — this phase builds the
E120 binding fact ONLY; the rest of E134 is its own plan.
**Validation:** narrow-to-moderate — core pins + `cargo test -p social-tree-core`
+ wasm arm still green.

*Done when:* **(behavioral)** the ADR is accepted; the binding fact type is built and
pinned in the core (a Vouch specialization or its own type — decided in the ADR); the
connect proposal doc exists for the owner to carry; **(verification)**
`cargo test -p social-tree-core` green including the new pins; the ADR + registry row
landed in one change.

### S4 — the rendered-principal joint (the ONE calling-track touchpoint)

Only after M4's operational close (production enforce flipped, or the owner declares
the window open): the callee/caller cards learn to render social context — "in 2 groups
with you", the binding-fact-backed display name — read-only, through the
rendered-principal seam ADR-0001 named. Calling still computes nothing from group data
(the caps engine unchanged); this is presentation composition, the first visible
payoff of the two tracks sharing one substrate.

**Call chain:** the existing call-card composables → a read-only social-context
provider backed by the S0 bindings → the core's projections (+ S3's binding fact for
display identity). Rendering only; the caps engine computes exactly what it computed
before.
**Wiring test:** a journey test asserting a call card renders social context when the
substrate holds shared groups — and renders unchanged when it holds none.
**Depends on:** S1, S3, **and M4's operational close (hard gate — owner declares the
window open)**.
**Write-set:** the call-card UI files under `ing/croft/call/ui/` (the ONE sanctioned
calling-track touch) + the social package. Nothing under `caps/`, `identity/`, `net/`.
**Shared-state contract:** claim before every `android/` commit; verify with
`git diff --stat <phase-base>..HEAD -- android/app/src/main/java/ing/croft/call/caps`
empty at phase end.
**Risks:** the seam tempting computation (a grant, a sort order, a filter derived
from group data) — ADR-0001's line is render-only; anything more is a new decision.
**Validation:** moderate — journey test + full suite + a device run.

*Done when:* **(behavioral)** a device run shows social context on a call card;
**(verification)** the caps engine's tests untouched and green
(`./gradlew :app:testDebugUnitTest`), the caps/ diff empty, and the coordination
claim protocol used for every `android/` commit.

### S5 — the web probe (and apple, named only)

openmls-on-wasm moves from compile-proof to runtime-proof: the wasm build of the core +
keylayer executes in a browser (seal/open round-trip in a headless page), and a thin
web shell spike renders the chat pond's `project()` output. Croft-pwa precedent applies
for scaffolding. Apple: the uniffi surface is the preparation; committing an apple
shell is its own future decision, recorded here as named-not-committed exactly as P7
was in E117.

**Call chain:** headless browser page → wasm module (core + keylayer, js features per
the existing target config in `ports/keylayer-openmls/Cargo.toml`) → seal/open; the
spike page → `chat_core::project` output rendered.
**Wiring test:** the browser round-trip itself (wasm-pack test --headless or
equivalent — the runner is execution's choice, recorded).
**Depends on:** S0 (the surface shape); independent of S1–S4 otherwise.
**Write-set:** `web/**`, wasm build config, `.github/workflows/ci.yml` (a keylayer
wasm arm, if CI-hosted — CI-PATTERN.md read-first applies).
**Shared-state contract:** none beyond the write-set; live-tier browser runs stay out
of push CI per `CroftC/.claude/WEB-TESTING.md`.
**Risks:** openmls-on-wasm is compile-proof only today — runtime may surface getrandom
/time/storage gaps the compile never sees; that discovery is this phase's point.
**Validation:** narrow — the recorded round-trip run; the spike is a spike.

*Done when:* **(behavioral)** seal/open round-trips in a real browser context;
**(verification)** the browser round-trip runs in CI or a recorded local run with the
honest rung stated; a one-screen web spike exists or the blocker is named.

## Reasoning

- **Android first** because it is the only shell that exists and ships; adopting there
  proves the FFI, storage, and lifecycle questions that apple/web inherit.
- **Flag-gated additive modules** because the calling track is mid-validation with
  hardware runs and an owner-gated production flip — the cheapest way to guarantee
  non-interference is that release builds cannot see the new code.
- **Bindings before UI** because the E117 P2/P4 lesson repeats: the seam design (what
  crosses the FFI, where state lives) is the risk; screens are not.
- **The keylayer at S2, not S1** so the first android increment has no new native
  dependencies — packaging risk (the E100 launch-crash class) is isolated to its own
  phase.
- **E120 as design-first with a stops-there rule** because the contract is connect's and
  the app speaking it is under test; a proposal doc is the largest artifact this plan
  may produce on that front.
- **S4 last-but-one and gated on M4's close** because it is the only phase that touches
  the calling surface at all, and the standing constraint says that surface is hot.

## Verified assumptions (Pass 2, 2026-08-25 — everything below read firsthand)

- **Android paths as named:** package `ing.croft.call` with `caps/`, `identity/AuthManager.kt`,
  `DeepLink.kt`; the journey harness is real at
  `android/app/src/test/java/ing/croft/call/workflow/` (six `*JourneyTest.kt` files).
- **Core loop as named:** `chat-core` `Intent`/`Effect` (`src/model.rs:127,148`),
  `update` (`src/update.rs:11`), `project` (`src/project.rs:12`).
- **Keylayer surface as named:** `ports/keylayer-openmls/src/lib.rs` — `create_group`,
  `key_package_bytes`, `deposit_key_package`, `join_from_welcome`, `store_token`,
  `group_info_with_tree`, `return_via_external_commit`, `enact_departure`, `seal`,
  `open`, `member_count`. Pins: openmls `=0.8.1`, openmls_rust_crypto `=0.5.1`
  (wasm target gets `js` features, already configured).
- **The provider keystore is IN-MEMORY** (openmls_rust_crypto 0.5.1) — S2 persistence
  is unbuilt work (Pass 2 correction; D2 probes the strategy).
- **`Vouch = 0x000A`** exists (`core/social-tree-core/src/model.rs:219`) with
  `VouchStrength`/`VouchPayload` — Q3's "specialize" option is real.
- **Shell dirs are `.gitkeep` placeholders** (`shell/`, `ffi/`, `web/`, `apple/`);
  there is no `shell/ing` skeleton crate. The README (`README.md:40`) assigns `ffi/`
  as "uniffi bindings for the mobile shells" — S0 corrected to build there.
- **redb exists only in the discovery corpus** (`local_storage_projection`); croft's
  core notes it explicitly ("the redb realization lives in the discovery corpus",
  `core/social-tree-core/src/lib.rs:11`). S0 must land a croft-side Store realization.
- **Workspace members** today: the two core crates + keylayer
  (root `Cargo.toml`); CI has wasm check arms for both core crates and the clippy
  `-D warnings --force-warn missing_docs` pattern (`.github/workflows/ci.yml`).
- **Roadmap rows as cited:** E116 (four presentation obligations), E120 (binding
  seam, "retire by: the binding-fact design in Phase 7's successor plan"), E137
  (this plan) — all read in `discovery/alpha/ROADMAP_TODO.md`.

**Not verified (Phase 0 exists because of these):** any uniffi behavior (never used in
this workspace), cargo-ndk/NDK/emulator toolchain presence, the openmls persistence
trait surface, openmls-on-wasm at runtime.

## Documentation impact

Scheduled in the phase that makes the reference stale, not a trailing docs phase:

- `README.md` — repo-map wording when `ffi/` (S0) and `web/` (S5) stop being
  placeholders; owned by those phases.
- `CLAUDE.md` (croft) — the status-honesty section ("core is real; shell/ skeleton")
  goes stale at S0/S1; each of those phases updates it in its landing commit.
- `docs/adr/0004-<slug>.md` + `CroftC/.claude/DECISIONS.md` registry row — S3, one
  change (the house ADR rule).
- `core/social-tree-core/WIRE-REGISTER.md` — S3, if the binding fact is wire-visible
  (it is, either shape).
- `.github/workflows/ci.yml` — S0 (new crate arms) and S5 (keylayer wasm arm);
  `CroftC/.claude/CI-PATTERN.md` read-before-touching applies both times.
- `discovery/alpha/ROADMAP_TODO.md` — E137 phase notes as phases land; E116 retires
  through S1 (renderings on a product surface), E120 through S3/S4; the close-out
  entry updates all three rows.
- Grepped for other references to `shell/ing` (the corrected claim): only this plan
  carried it.

## Concurrency map

```
Sequential spine: Phase 0 → S0 → S1 → S2 → S4 → S5
S3 is order-independent: any time after Phase 0, parallel-safe beside S1/S2.
S4 additionally gates on M4's operational close (external, owner-declared).
```

Parallel set {S1‖S3} (opt-in, only if two sessions run):
- **Disjoint write-sets:** S1 writes `android/**` + gradle config; S3 writes
  `docs/adr/`, `core/social-tree-core/**`, the proposal doc. No overlap. (S1 *reads*
  core via the S0 crate at a pinned rev; S3's core additions are additive types, and
  S1 rebases its pin only at a phase boundary.)
- **Shared-state contract:** both in their own croft worktrees; S1 holds the
  android claim, S3 holds none; neither touches the other's write-set or any
  calling-path file; no ports, no daemons.
- **Re-entry verification:** croft main HEAD unchanged from pre-dispatch SHA;
  `git worktree list` shows only expected trees; `git -C croft status` clean.

A single executor context runs the spine sequentially and may take S3 wherever it
best fits (e.g., while a device or toolchain blocker on S1/S2 resolves).

## Adjacent registers — explicitly OUT of this plan's scope

Named so the executor does not wander into them; each has its own home:

- **The M4 operational close** (croft-admit activation, production enforce flip) —
  owner-gated, croft-stack `TODO.md`, other sessions'. S4 *waits* on it; nothing here
  advances it.
- **E134, the connections aspect** — its own future plan. S3 builds the E120 binding
  fact only.
- **The mutation burn-down** (`core/social-tree-core/MUTATION.md`, 398 registered
  survivors) — corpus-side register, untouched here. New modules this plan adds get
  their own bounded mutation audit at their phase close, per the house rule.
- **HeadAck over real transport** (E112 residual) — freshness stays a documented
  caller input in `admit_return`; retiring it is transport work outside this plan.
- **The push + pin swap** — croft pushes are owner-gated; at push, the discovery
  `file:///…/CroftC/croft` pins swap to `git@github-personal:CroftCommunity/croft.git`
  at the same rev. Execution never pushes on its own.

## Notes for the executing context

- Run under the phase-plan skill's `execute.md`: commit per phase item at stable
  points, no stubs, wiring test RED before GREEN, Review Log entry per phase.
- All work in a dedicated worktree
  (`git -C croft worktree add ../worktrees/croft/<slug> -b claude/<slug>`); never
  uncommitted state in the shared checkout; stage explicitly, never `-A`.
- `android/` is a contested surface until M4's operational close: claim file in
  `CroftC/.coordination/claims/` before the first commit that touches it (S1, S2
  device wiring, S4); testbed claims (`testbed--<resource>`) before device runs.
- Device rig, accounts, and serial gotchas: `CroftC/.claude/TESTBED.md`.
- Pushes and anything remote: owner-gated, always ask.

## Open questions (for Pass 2/3)

1. [RECOMMENDED: PHASE-GATED S1] **Flag mechanics:** build-time flag (product flavor)
   vs runtime hidden entry. *Flavors keep release bytes identical — the standing
   constraint's cheapest guarantee; runtime flags ship dormant code into the app
   under test.*
2. [RECOMMENDED: PHASE-GATED S2] **The S2 transport rung:** iroh-gossip
   device-to-device first vs waiting for a relay-carried path. *Gossip reuses the
   proven TUI machinery; a relay path entangles fabric admission — recommended
   against inside this plan.*
3. [RECOMMENDED: ADVISORY] **S3's fact shape:** specialize the existing `Vouch`
   (0x000A, `VouchPayload` verified) with a context tag vs a new fact type.
   *Leaning specialize; the S3 ADR is the deciding instrument either way, so this
   can resolve during execution.*
4. [RECOMMENDED: ADVISORY] **Apple:** stays named-only here — confirm, or promote a
   minimal SwiftUI spike into S5. *Nothing downstream depends on it.*
5. [RECOMMENDED: BLOCKING] **Plan home check:** this plan lives in croft (the client
   repo owns client plans, the M-series precedent); discovery's row E137 points here.
   *Blocking only in the trivial sense that execution starts from this file's home —
   a one-word confirm closes it.*
6. [RECOMMENDED: PHASE-GATED S0] **redb adapter provenance (new, Pass 2):** promote
   the corpus `local_storage_projection` redb Store realization into croft
   (`ports/store-redb`) vs write fresh. *Leaning promote — it is proven, C-series-
   vetted, same author; D4 sizes it before the decision is due.*
7. [RECOMMENDED: ADVISORY] **Connect proposal doc location (new, Pass 2):** the S3
   proposal (contract v3 sketch) needs a named home — leaning croft
   `docs/proposals/connect-contract-v3-group-grants.md`, carried to connect by the
   owner. *Any location works; naming it just keeps the executor from inventing one.*

## Review log

- **2026-08-25 — Pass 1 (draft).** Written against croft main `07a6b61` (post-§13-bake
  note), core through `9f7d0c6` (GroupState v4, the 2026-08-25 decision set). The
  standing constraint recorded from the owner's words: croftcall is running through
  testing with edges in both croft and connect.

### Pass 2: Gap analysis — 2026-08-25

Run against croft main `168f5d6`, targeted at execution by a fresh context under the
phase-plan skill (owner direction: "targeted for opus to execute using phased plan
skill").

**Found (factual, verified against the code):**
- S0's location claim was wrong: no `shell/ing` skeleton exists (all four shell dirs
  are `.gitkeep`); the README already assigns `ffi/` as the uniffi home. Corrected.
- The redb Store realization does not exist in croft — corpus-only. S0 gained the
  explicit item + Q6 (promote vs rewrite; D4 sizes it).
- The openmls provider keystore is in-memory (`openmls_rust_crypto` =0.5.1): S2's
  "provider persists" was stated as existing behavior but is unbuilt work. Corrected;
  D2 probes the strategy.
- Everything else the plan named checked out firsthand (android paths, journey
  harness, core loop fns, keylayer API, Vouch 0x000A, E116/E120/E137 rows) — recorded
  in the new Verified Assumptions section.

**Concurrency:**
- Map added (was absent). Spine sequential; S3 declared order-independent with
  disjoint write-sets as an opt-in parallel set {S1‖S3}; S4's external M4 gate on
  the map.

**Changed:**
- Phase 0 (discovery probes D1–D4, dispositions declared) inserted — uniffi, openmls
  persistence, android toolchain, redb sizing are all unverified and cheap to probe.
- Per-phase execution fields added to S0–S5 (call chain, wiring test, depends-on,
  write-set, shared-state contract, risks, validation, two-tier done-when) — additive;
  Pass 1 prose retained.
- New sections: Verified Assumptions, Documentation Impact, Concurrency Map, Adjacent
  registers (out-of-scope fence: M4 ops, E134, mutation burn-down, HeadAck, push/pin
  swap), Notes for the executing context (worktrees, claims, owner-gated pushes).
- Open questions severity-tagged; Q6 (redb provenance) and Q7 (proposal doc home)
  added.

**Confirmed:**
- The phase order and the standing-constraint rule set held up unchanged; no phase
  reordered, no Pass 1 prose rewritten beyond the three factual corrections.
