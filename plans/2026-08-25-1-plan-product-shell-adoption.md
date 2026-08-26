# Product-shell adoption: the shells consume the core (E117 P7's successor plan)

`Status: READY FOR EXECUTION (Passes 1-3 complete; all seven questions closed by the`
`owner 2026-08-26). Start at Phase 0; its findings return to the owner before S0.`
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

Six phases, S0–S5, plus a Phase 0 of discovery probes. The order is: bindings first
(pure additive), then an android chat surface behind a build flavor (new source set, no
calling contact), then real MLS on-device, then the two *designed* joints — the E120
binding fact and the rendered-principal seam — and only then the web and apple spikes.
Every phase is RED-first under the existing harness disciplines (the workflow-harness
journey pattern for android; the core's pins for anything that touches core), and every
phase closes on a wiring test that runs through an entry point, never on component
tests alone.

### Phase 0 — discovery probes (Discovery Exemption applies; load execute.md § Discovery Exemption first)

Every remaining unknown below is toolchain- or dependency-shaped and resolvable by a
probe. **Pass 3 resolved the cheap half during planning** (§ Verified assumptions,
Pass 3 block) — what survives here is what genuinely needs code run against it.

- [ ] **D1: Does OUR uniffi round-trip work end to end?**
  - **Standing on (Pass 3, verified):** the android app *already ships and loads a
    uniffi-generated Rust cdylib* — `computer.iroh:iroh` is uniffi-bound, JNA
    5.14.0 is already a declared dependency ("uniffi requires JNA >= 5.12",
    `android/app/build.gradle.kts:85`), and `libiroh_ffi.so` (arm64-v8a, 18MB) sits
    in `android/app/src/main/jniLibs/` loading on real hardware through the M4 runs.
    The packaging path is proven; what is unproven is *our* crate through it.
  - **Probe:** Minimal crate at `ffi/` exporting one function over one core type;
    generate Kotlin; call it from a JVM test. Pin the uniffi version compatible
    with the JNA already on the classpath and record it.
  - **Success criteria:** The JVM test calls Rust and asserts the returned value.
  - **Disposition:** `promote` — the spike crate becomes S0's scaffold; TDD applies
    when S0 builds the real surface on it (S0 is the named follow-up phase).
- [ ] **D2: What does persistent MLS state require on openmls =0.8.1?**
  - **Probe:** Read `openmls_traits` v0.5's storage/provider traits and
    `openmls_rust_crypto` v0.5.1's keystore. Determine the sanctioned persistence
    path: implement the storage trait over a file/redb backend, vs serialize the
    memory keystore. Verified fact going in: the current provider is
    **memory-only** — S2's "provider persists under the app dir" is unbuilt work.
  - **Success criteria:** A named strategy with the trait surface listed, sized
    (hours vs days), recorded in this plan before S2 starts.
  - **Disposition:** `throwaway` (notes into this doc; implementation is S2's).
- [ ] **D3: Does our cdylib cross-compile and load on arm64?**
  - **Resolved during Pass 3 (no longer open):** the rust targets
    `aarch64-linux-android` + `armv7-linux-androideabi` are installed and pinned in
    `rust-toolchain.toml`; the NDK is declared in `env/toolchain.yml`
    (`ndk;29.0.14206865`) and installed by `make bootstrap`; **cargo-ndk is not
    needed** — `env/build-iroh-android.sh` is a working per-ABI cross-compile
    script (direct NDK clang via `CC_*`/`AR_*`/linker env vars, `rustup which
    cargo`, copy the `.so` into `jniLibs/<abi>/`) and is the template to copy. No
    AVD exists on this machine, which is a non-event: `make emulator` creates it
    from `env/avd.yml`.
  - **Remaining probe:** run that template against the D1 crate and load it on the
    arm64 emulator (`make emulator`, then the instrumentation/scratch load).
  - **Success criteria:** the emulator loads our `.so` and returns a value.
  - **Disposition:** `promote` — the build script generalizes into S0's build step.
- [ ] **D4: What does promoting the corpus redb adapter cost?**
  - **Probe:** Read `discovery/alpha/experiments/local_storage_projection`'s store
    code; list what is croft-shaped (the Store realization over redb) vs
    corpus-only (projection/edge code that stays).
  - **Success criteria:** A file-level lift list; the promote decision is already
    made (Q6), so this sizes and scopes it rather than deciding it.
  - **Disposition:** `throwaway` (feeds S0).
- [ ] **D5: What does the apple spike need? (new — Q4 promoted the spike into S5)**
  - **Standing on (Pass 3, verified):** Xcode 26.3 and Swift 6.2.4 are installed;
    `aarch64-apple-darwin` is an installed rust target, but **no iOS target is**
    (`aarch64-apple-ios` / `aarch64-apple-ios-sim` absent) and `rust-toolchain.toml`
    does not pin one.
  - **Probe:** decide the spike's surface — macOS SwiftUI (runs on the installed
    toolchain today, no signing story) vs iOS simulator (needs the target added to
    `rust-toolchain.toml` + `env/toolchain.yml`, both files in one commit per the
    file's own rule). Build the D1 crate for the chosen target and call it from
    Swift.
  - **Success criteria:** Swift calls Rust and asserts a value, on a named target.
  - **Disposition:** `promote` — becomes S5's apple spike.

*Done when:* D1–D5 findings are recorded in Verified Assumptions, and S0–S5's items
are adjusted here if any probe contradicts them (Phase 0 is the only phase allowed to
restructure later phases). **Checkpoint:** report findings to the owner before S0
starts — a Phase 0 that changes the plan materially is a decision point, not a
formality.

### S0 — the uniffi surface: the core crosses the FFI line

A crate under **`ffi/`** — the home the repo README already assigns ("uniffi bindings
for the mobile shells"; `shell/`, `web/`, `apple/` are `.gitkeep` placeholders, there
is no existing skeleton crate — Pass 2 correction) — exposing `social-tree-core` +
`chat-core` to Kotlin via uniffi: the substrate instance, the pond's
`Intent`/`Effect`/`update`/`project` loop (`core/chat-core/src/{model,update,project}.rs`),
and a **redb Store realization on the Rust side** (storage lives under the app's files
dir; redb is pure Rust and crosses no FFI). The redb adapter does not exist in croft
today — it lives in the discovery corpus (`local_storage_projection`); **Q6 decided
(owner, 2026-08-26): promote it** into `ports/store-redb`, D4 sizing the lift and
separating what is croft-shaped (the Store realization) from what stays corpus-only
(projection/edge code). The effect-composition rule (ADR-0002) becomes concrete here:
**one substrate instance per shell, owned by the shell, ports beside it** — the uniffi
object graph is exactly that sentence.

**Changes (test-first in this order):**
- [ ] the `ffi/` crate from D1's scaffold — RED: the Kotlin wiring test below, written
  against the intended binding surface before the surface exists.
- [ ] `ports/store-redb` promoted from the corpus — RED: the promoted adapter's own
  pins run in croft *before* it is wired to the ffi crate. Promoted code gets TDD in
  this phase; it does not inherit the corpus's green as a free pass.
- [ ] workspace `members` gains both crates (root `Cargo.toml`).
- [ ] the android cross-compile step generalized from `env/build-iroh-android.sh`
  (D3's template) so the `.so` lands in `jniLibs/arm64-v8a/`.
- [ ] CI arms for both crates (read `CroftC/.claude/CI-PATTERN.md` before touching the
  workflow; follow the existing clippy `-D warnings --force-warn missing_docs` and fmt
  patterns in `.github/workflows/ci.yml`).
- [ ] `README.md` repo-map wording (`ffi/` is no longer a placeholder) and `CLAUDE.md`
  status line — in this phase's landing commit, per G2.

**Test specifics (mutation-resistant, not single-point):** the store port's pins name
the edges, not one happy value — empty store reads, a key absent vs present, a
round-trip after reopen, and a write that overwrites vs appends. The binding pins
assert the *refusal* paths cross the FFI intact (a typed core refusal surfaces as a
typed Kotlin error, not a swallowed null) — an FFI layer that only proves happy paths
is the classic place errors go to die, and this repo's rule is fail-loud.
**Call chain:** Kotlin test → generated bindings → ffi crate → `chat_core::update`/`project`
→ `social_tree_core::evaluate` → redb store port.
**Wiring test:** a Kotlin JVM test driving create-group → send → project through the
generated bindings and asserting the projected timeline — RED before the surface
exists, GREEN at phase end. Component tests alone do not close this phase.
**Depends on:** Phase 0 (D1, D3, D4).
**Write-set:** `ffi/**`, `ports/store-redb/**`, root `Cargo.toml`,
`.github/workflows/ci.yml`, `README.md` (map wording), `CLAUDE.md` (status line),
`env/build-*.sh` (the generalized cross-compile step).
**Shared-state contract:** no android app code touched; no ports bound; work in a
croft worktree; no shared mutable state beyond the write-set.
**Risks:** uniffi type-mapping friction on enum-heavy `Intent`/`Effect`; native-lib
loading on JVM differs from android (D3 covers the android half); a uniffi version
that disagrees with the JNA 5.14.0 already on the classpath (D1 pins for
compatibility — a second JNA on the classpath is a packaging bug waiting to happen).
**Observability:** the ffi crate logs at its boundary — every refusal crossing the FFI
carries the core's own words (the fail-loud rule), and the android side routes them to
logcat under a distinct tag so `make logcat` shows the seam. Rust-side: `tracing` at
DEBUG for the crossing, WARN for refusals. Not INFO for everything.
**Debugging readiness:** if S0 breaks, the failure is one of three things and the plan
should be able to tell them apart — the Rust build (cargo output), the packaging (does
the `.so` exist in `jniLibs/`, `make crash` on load failure), or the binding contract
(the JVM test fails with a type error). Name which in the phase's landing note.
**Validation:** moderate — wiring test + unit tests + a manual run of the JVM harness
+ the emulator load.

*Done when:* **(behavioral)** a Kotlin unit test drives create-group → send → project
through the bindings on JVM, and the arm64 emulator loads the native library (the
inherited client's packaging-bug class is exactly what the emulator CAN answer);
**(verification)** the JVM binding-test command (exact gradle/cargo invocation named
during execution) runs green, and the emulator load is a recorded run; no android app
code touched yet.

### S1 — the android chat surface, behind a flag

A new UI surface (module/package beside `ing.croft.call`, e.g. `ing.croft.social`) —
groups list, timeline, the truthful membership panel, mute — consuming S0's bindings,
gated by a **gradle product flavor** (**Q1 decided, owner 2026-08-26**): the social
surface compiles into dev builds only, so release builds of the calling app stay
byte-identical to today. That is the standing constraint's cheapest guarantee — the
app under test cannot contain code it never compiled. The P6 renderings port from the
TUI: CONTESTED as "membership pending resolution", the voided row, marked-never-dropped
muted lines, the fork banner as a blocking surface. The **lost-race UX** debt (two
concurrent admissions; the losing side's rendering) lands here with the E116 leftovers
— this is the phase that owed it a home.

**Test specifics (name the edges):** the panel renderings are branching code and get
boundary assertions, not single points — a member seated / pending resolution / voided
/ banned each render their own words; a muted line is *marked and still present* (never
dropped — the P6 rule); the fork banner blocks rather than decorates. E116's four
obligations are testable claims here: the factual fork statement, the exposure
disclosure, the three response registers reachable with mute the lightest, and
returner-side "admission voided" legibility.

**Call chain:** the flag-gated entry (launcher/nav entry visible only under the flag)
→ `ing.croft.social` screens → S0 bindings → the core loop.
**Wiring test:** a `SocialJourneyTest` in the existing harness home
(`android/app/src/test/java/ing/croft/call/workflow/` — six `*JourneyTest.kt` files
set the pattern) driving group-create → invite → message → panel-truth through the
real bindings, not mocks of them.
**Depends on:** S0.
**Write-set:** the new flavor's source set (`android/app/src/<flavor>/java/ing/croft/social/**`),
its test sources, `android/app/build.gradle.kts` (the flavor block — the single line
the calling build reads). **Nothing under `ing/croft/call/`** — that gradle file is a
contested surface: claim first, smallest possible diff.
**Shared-state contract:** claim `croft--android-social-surface` in
`CroftC/.coordination/claims/` before the first `android/` commit (rule 4); the M4
track owns `android/` ambient state until its operational close.
**Risks:** the gradle flavor block is the one write the calling track can see —
smallest possible diff, claim held, and a release build compared against a
pre-change build as the proof; uniffi-generated Kotlin ergonomics pushing toward a
hand-written facade layer.
**Observability:** the social surface logs under its own logcat tag, distinct from the
calling app's, so a `make logcat` session can tell the two tracks apart at a glance —
which matters precisely because they now share a process in dev builds.
**Debugging readiness:** the flavor boundary is the first thing to check on any
weirdness — "does this reproduce in the release variant?" separates a social-surface
bug from a calling-track bug in one build. Record that check in the phase notes.
**Validation:** moderate — journey test + full existing suite + a manual emulator
walk (`make emulator`, `make screenshot`) + a release-variant build proving absence.

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

**Q2 decided (owner, 2026-08-26): iroh-gossip device-to-device.** The transport is the
machinery P6 already converged the whole arc over at loopback; no relay contact at
all, so fabric admission (M4's live subject) and group admission stay severed by
construction rather than by discipline. The honest rung this phase reaches: *sealed
chat between two devices over gossip* — nothing about NAT-hard networks or offline
delivery is claimed, and the app already carries iroh on both sides of the FFI.

**Test specifics (name the edges):** persistence is boundary behavior and gets
boundary tests — state after a clean restart, state after a kill mid-epoch, and a
second device joining *after* the first restarted. A single "it round-trips" assertion
would survive almost any mutation to the persistence path.
**Call chain:** the S1 surface → bindings → `ports/keylayer-openmls`
(`seal`/`open`/both admission paths) → persistent provider storage + redb, both under
the app files dir.
**Wiring test:** the S1 journey extended to sealed round-trip (seal on one substrate,
open on another) at JVM grade; the device run is the validation tier above it.
**Depends on:** S1; D2's strategy; testbed claim (`testbed--<resource>` per
`CroftC/.claude/TESTBED.md`) before the device run.
**Write-set:** `ports/keylayer-openmls/**` (persistence), the ffi crate, the S1
package. No calling-path files.
**Shared-state contract:** device runs claim the testbed devices; no relay contact —
transport rides iroh-gossip per Q2, the relay path is fabric admission's territory.
**Risks:** MLS state corruption across app restarts (the persistence work's whole
point — test kill-and-relaunch explicitly); openmls storage trait friction on 0.8.1.
**Observability:** epoch transitions, seal/open, and every refusal log with the group
and epoch — an MLS desync is unreadable without them, and this is the first phase
where two independent devices can disagree. WARN on any epoch mismatch; DEBUG on the
normal transitions.
**Debugging readiness:** the device run gets a runbook section like §11/§12 before it
runs, with per-rung expected output, so a failure names its rung instead of "it didn't
work". Checkpoint: JVM-grade sealed round-trip green *before* any device is touched.
**Validation:** broad — wiring test + kill-and-relaunch persistence test + the
two-device run recorded runbook-style, plus a bounded mutation audit of the new
persistence module at phase close (house rule for non-trivial new modules).

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
Concurrency map).
**Write-set:** `docs/adr/0004-<slug>.md` + the `CroftC/.claude/DECISIONS.md` registry
row (same change, per the house rule), `core/social-tree-core/src/**` (the fact type),
`core/social-tree-core/WIRE-REGISTER.md` (the wire-visible payload),
`docs/proposals/connect-contract-v3-group-grants.md`.
**Shared-state contract:** no android, no connect, no calling-path files; core work in
a croft worktree.
**Risks:** scope creep toward E134's whole connections aspect — this phase builds the
E120 binding fact ONLY; the rest of E134 is its own plan. Second risk: the proposal
drifting from sketch into specification — it proposes, connect disposes.
**Observability:** core crates take no logging (the purity rule — no I/O in a core);
refusals are typed values the shell renders. Nothing to add here, and that is the
correct answer rather than a skipped check.
**Debugging readiness:** the fact type's pins are the checkpoint; a failure is
localized to the fold by construction.
**Validation:** narrow-to-moderate — core pins + `cargo test -p social-tree-core`
+ wasm arm still green + a bounded mutation audit of the new fact's arms.

**Q3 decided (owner, 2026-08-26): the ADR decides the fact shape, with the lean toward
specializing `Vouch` recorded.** The owner's reasoning — *"I like [specialize] but I'm
not sure yet based on how it's used"* — makes usage the deciding information, and the
ADR beat is where usage is in view. This is not a deferral of the question; it is the
question routed to the instrument that can answer it. The ADR must state which it
chose and why, either way.
**Q7 decided (owner, 2026-08-26): the proposal lives at
`croft/docs/proposals/connect-contract-v3-group-grants.md`** and the owner carries it
to connect. No P7 commit lands in the connect repo. **Already done (2026-08-26):** a
pointer note is filed in connect's new `TODO.md` (connect `9d257bc`, local) so the
proposal is expected there rather than a surprise — it names what will arrive, that
contract v2 stays canonical, and that adoption is connect's decision on its own
schedule.
**Test specifics (name the edges):** a binding fact is governance-adjacent, so the pins
cover its whole lifecycle, not its happy path — recorded, revoked, re-recorded after
revocation, and composed against §11.8 standing in both directions (a binding held by
an excluded principal; a standing change after a binding). Single-point assertions on
a fact type with revocation semantics are exactly what mutation testing eats.

*Done when:* **(behavioral)** the ADR is accepted and states the chosen fact shape with
its reasoning; the binding fact type is built and pinned in the core; the connect
proposal doc exists at the agreed path for the owner to carry; **(verification)**
`cargo test -p social-tree-core` green including the new lifecycle pins; the ADR +
`CroftC/.claude/DECISIONS.md` registry row landed in one change.

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
**Test specifics (name the edges):** the render is conditional code, so the edges are
the cases — no shared groups (renders nothing, not "0 groups"), one, several; a
binding fact present vs absent (display name falls back, never blanks); and the
negative that matters most, *a call from someone with no social data renders exactly
as it does today*.
**Observability:** no new logging on the calling path — this phase adds a renderer,
and a renderer that logs is a renderer doing too much.
**Debugging readiness:** the phase's own checkpoint is the caps diff being empty; run
it before the device run, not after.
**Validation:** moderate — journey test + full suite + a device run.

*Done when:* **(behavioral)** a device run shows social context on a call card;
**(verification)** the caps engine's tests untouched and green
(`./gradlew :app:testDebugUnitTest`), the caps/ diff empty, and the coordination
claim protocol used for every `android/` commit.

### S5 — the web probe and the apple spike

openmls-on-wasm moves from compile-proof to runtime-proof: the wasm build of the core +
keylayer executes in a browser (seal/open round-trip in a headless page), and a thin
web shell spike renders the chat pond's `project()` output. Croft-pwa precedent applies
for scaffolding.

**Q4 decided (owner, 2026-08-26): apple gets a minimal SwiftUI spike here** — one
screen rendering `project()` output through the S0 uniffi bindings. Its value is
falsifying a claim the plan otherwise only asserts: that the FFI surface is genuinely
platform-neutral rather than Kotlin-shaped. If Swift needs a different surface than
Kotlin did, S0's design has a flaw worth knowing before an apple shell is ever
committed. **Still not committed here:** a full apple shell, a signing story, or App
Store anything — the spike is a probe with a screen, and D5 picks its target (macOS on
the installed toolchain, or iOS-simulator with the rust target added to
`rust-toolchain.toml` *and* `env/toolchain.yml` in one commit, per that file's own
two-sources-of-truth rule). If the spike blocks, name the blocker and move on — a
blocked spike is a finding, not a failed phase.

**Call chain:** headless browser page → wasm module (core + keylayer, js features per
the existing target config in `ports/keylayer-openmls/Cargo.toml`) → seal/open; the
spike page → `chat_core::project` output rendered.
**Wiring test:** the browser round-trip itself (wasm-pack test --headless or
equivalent — the runner is execution's choice, recorded).
**Depends on:** S0 (the surface shape); D5 (apple target); independent of S1–S4
otherwise.
**Write-set:** `web/**`, `apple/**`, wasm build config, `.github/workflows/ci.yml` (a
keylayer wasm arm, if CI-hosted — CI-PATTERN.md read-first applies), and — only if D5
chooses iOS — `rust-toolchain.toml` + `env/toolchain.yml` together.
**Shared-state contract:** none beyond the write-set; live-tier browser runs stay out
of push CI per `CroftC/.claude/WEB-TESTING.md`.
**Risks:** openmls-on-wasm is compile-proof only today — runtime may surface getrandom
/time/storage gaps the compile never sees; that discovery is this phase's point.
Apple-side: adding an iOS rust target touches the two toolchain files the environment
refuses on if they disagree — change both or neither.
**Observability:** browser console + the honest-rung statement; the spikes are
observed by running them, not by instrumenting them.
**Debugging readiness:** the two spikes are independent — a web blocker does not stop
the apple half and vice versa; record each rung separately.
**Validation:** narrow — the recorded round-trip run; the spikes are spikes, and their
findings (including "blocked, here is why") are the deliverable.

*Done when:* **(behavioral)** seal/open round-trips in a real browser context, and a
Swift screen renders `project()` output through the bindings; **(verification)** the
browser round-trip runs in CI or a recorded local run with the honest rung stated; a
one-screen web spike and a one-screen apple spike each exist or name their blocker.

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
- **Phase 0 survives Pass 3 even though half of it got answered during planning**
  (Pass 3 § Verified assumptions) because what remains — our crate through uniffi, the
  openmls persistence surface, the corpus lift, the apple target — cannot be answered
  by reading. The half that could be answered by reading was, which is the rule the
  skill states: resolve during planning what planning can resolve.
- **The apple spike is a falsification device, not a product step.** S0 asserts a
  platform-neutral FFI surface; only a second language calling it can test that
  assertion, and it is far cheaper to learn at S5 than after an apple shell is
  committed.

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

### Pass 3 additions (2026-08-26 — probes run during planning)

The single most useful finding: **this repo already ships and loads a uniffi-generated
Rust cdylib on real hardware.**

- `computer.iroh:iroh:1.0.0` is uniffi-bound, and `android/app/build.gradle.kts:85`
  declares `net.java.dev.jna:jna:5.14.0@aar` with the comment *"uniffi requires JNA >=
  5.12"*. **JNA is already on the classpath** — our bindings add no new dependency.
- `android/app/src/main/jniLibs/arm64-v8a/libiroh_ffi.so` (18MB) is committed and
  loading through every M4 device run. The jniLibs packaging path — the E100
  crash class the plan feared — is proven, not hypothetical.
- `env/build-iroh-android.sh` cross-compiles a Rust cdylib per-ABI **without
  cargo-ndk**: direct NDK clang through `CC_*`/`AR_*`/linker env vars, `rustup which
  cargo`, then copy into `jniLibs/<abi>/`. It is the template S0 copies.
- Toolchain state: `aarch64-linux-android` + `armv7-linux-androideabi` +
  `wasm32-unknown-unknown` installed; `rust-toolchain.toml` pins channel 1.97.1 and
  those targets; `env/toolchain.yml` declares `ndk;29.0.14206865` and `make bootstrap`
  installs it. **cargo-ndk is absent and not needed.** No AVD exists locally — a
  non-event, `make emulator` creates it from `env/avd.yml`.
- Apple: Xcode 26.3 + Swift 6.2.4 installed; `aarch64-apple-darwin` present, **no iOS
  rust target installed or pinned** — D5 decides macOS vs iOS-simulator.
- `make` targets that execution should use rather than reinvent: `bootstrap`, `verify`,
  `gate`, `emulator` (headless), `emulator-ui`, `screenshot` (headless capture),
  `install`, `run`, `crash`, `logcat`.

**Still not verified (why Phase 0 survives Pass 3):** our own crate through uniffi
(D1), the openmls persistence trait surface (D2), our cdylib loading on arm64 (D3),
the corpus adapter's lift size (D4), the apple spike's target (D5), and
openmls-on-wasm at runtime (S5's whole point).

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
- `croft/docs/proposals/connect-contract-v3-group-grants.md` — created by S3 (new
  directory; `docs/` currently holds `adr/`, `CONTRACT.md`, `PLATFORM-POSTURE.md`).
- `connect/TODO.md` — **already done (2026-08-26, connect `9d257bc`, local)**: the
  repo had no TODO file, so one was created carrying the workspace scope header
  (`CroftC/.claude/TRACKING.md` § Repo TODO scope header, which audit check 9 greps
  for) plus the note that S3's proposal is coming and that no P7 commit lands there.
- `env/build-iroh-android.sh` → a generalized sibling for our cdylib (S0); if S5
  chooses iOS, `rust-toolchain.toml` + `env/toolchain.yml` change together.
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

## Decisions (all seven questions walked and closed, owner, 2026-08-26)

No open questions remain. Each decision is folded into the phase it governs; this is
the index.

| # | Question | Decision | Governs |
|---|---|---|---|
| Q1 | Flag mechanics | **Gradle product flavor** — the social surface compiles into dev builds only; release builds stay byte-identical | S1 |
| Q2 | S2 transport rung | **iroh-gossip device-to-device** — no relay contact, the admissions stay severed by construction | S2 |
| Q3 | Binding fact shape | **The S3 ADR decides**, lean toward specializing `Vouch` recorded; usage is the deciding information | S3 |
| Q4 | Apple | **A minimal SwiftUI spike joins S5** — it falsifies "the FFI surface is platform-neutral"; a full apple shell stays uncommitted | S5 |
| Q5 | Plan home | **croft** — the client repo owns client plans (M-series precedent); discovery's E137 points here | all |
| Q6 | redb provenance | **Promote the corpus adapter** into `ports/store-redb`; D4 sizes the lift; promoted code still gets TDD in S0 | S0 |
| Q7 | Proposal home | **`croft/docs/proposals/connect-contract-v3-group-grants.md`**, carried to connect by the owner; connect's `TODO.md` note already filed | S3 |

Two decisions carry consequences worth restating because they are the ones a future
reader will question:

- **Q3 is routed, not deferred.** The owner's *"I like [specialize] but I'm not sure
  yet based on how it's used"* identifies the missing input precisely — usage — and
  the ADR beat is the one place that has it. The ADR must record which shape it chose
  and why; an ADR that restates the two options without choosing has not discharged
  this.
- **Q4 changed the plan's shape.** Apple moved from "named-not-committed" to a real
  spike, which is why D5 exists: an apple spike that discovers its own toolchain
  requirements mid-phase is the rework Phase 0 is for. The spike's *finding* is the
  deliverable — including a blocker, honestly named.

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

### Pass 3: Quality gates — 2026-08-26

Run in the same session as the question walk (the owner ran the walk here rather than
in a fresh context) against croft main `a51457f`.

**TDD ordering:**
- S0's changes became an ordered, test-first checklist; the promoted redb adapter is
  explicitly told it does *not* inherit the corpus's green — it re-earns it in croft.
- "Test specifics" added to S0–S4 naming **boundary cases, not single points**: store
  edges and FFI refusal paths (S0); the four membership renderings and marked-not-
  dropped mute (S1); restart / kill-mid-epoch / late-joiner persistence (S2); the
  binding fact's full revocation lifecycle (S3); the no-social-data negative (S4).
  Single-point assertions on branching code survive mutation, which is the planning-
  side version of the check `tdd-guardian` runs at execution.

**Observability:**
- Added per phase, including the two phases whose honest answer is *none*: cores take
  no logging (the purity rule) and S4 adds a renderer, not a logger. S0 logs the FFI
  boundary with the core's own refusal words; S1 takes its own logcat tag so the two
  tracks are distinguishable in one process; S2 logs epoch transitions because an MLS
  desync is unreadable without them.

**Debugging readiness:**
- Each phase names how a failure localizes: S0's three-way split (build / packaging /
  binding contract), S1's release-variant reproduction check, S2's runbook rungs
  written *before* the device run, S4's caps-diff check run *before* the device run.
- Phase 0 gained an explicit owner checkpoint before S0 starts.

**Validation calibration:**
- Unchanged where it fit; S2 and S3 gained bounded mutation audits at phase close per
  the house rule for non-trivial new modules.

**Concurrency honesty:**
- Map re-checked after Pass 3's write-set edits. S0 gained `CLAUDE.md` and `env/`
  entries, S5 gained `apple/**` and the conditional toolchain pair — no new overlap
  with the {S1‖S3} set, which stays disjoint (S1: android source set + one gradle
  file; S3: docs/adr, core, proposals). Contracts are stated as invariants
  ("does not touch calling-path files", "binds no ports"), not mechanisms.

**Discovery:**
- **D3 was resolved during planning** rather than deferred — the NDK is declared and
  bootstrap-installed, cargo-ndk is unnecessary, and `env/build-iroh-android.sh` is a
  working cross-compile template. What remains of D3 is only running it against our
  crate. D1 shrank the same way: the repo already loads a uniffi cdylib with JNA on
  the classpath.
- **D5 added** (apple target) because Q4 promoted the spike into S5.
- Every task carries a disposition; both `promote` tasks name their follow-up phase
  (D1/D3 → S0, D5 → S5).

**Coherence:**
- Scope grew in exactly one place — the apple spike — by owner decision, recorded with
  its reasoning. Everything else is depth on the Pass 2 base.

**Documentation impact:**
- Three additions: the proposals directory (S3 creates it), `connect/TODO.md` (done
  2026-08-26, with the workspace scope header audit check 9 requires), and the
  `env/` build script plus the conditional toolchain pair. Every listed file has a
  phase that owns it; no trailing docs phase exists.

**Confirmed ready:** yes — all seven questions closed by the owner, no BLOCKING items
outstanding. Execution starts at Phase 0, whose findings return to the owner before S0.
