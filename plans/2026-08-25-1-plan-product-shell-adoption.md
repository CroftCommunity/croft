# Product-shell adoption: the shells consume the core (E117 P7's successor plan)

`Status: DRAFT (Pass 1, 2026-08-25). Awaiting owner Pass 2/3 per the house discipline.`
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

### S0 — the uniffi surface: the core crosses the FFI line

A `shell/` crate (`shell/uniffi` or the existing `shell/ing` skeleton, decided at build
time) exposing `social-tree-core` + `chat-core` to Kotlin via uniffi: the substrate
instance, the pond's `Intent`/`Effect`/`update`/`project` loop, and the redb store port
compiled into the Rust side (storage lives under the app's files dir; redb is pure Rust
and crosses no FFI). The effect-composition rule (ADR-0002) becomes concrete here: **one
substrate instance per shell, owned by the shell, ports beside it** — the uniffi object
graph is exactly that sentence.

*Done when:* a Kotlin unit test drives create-group → send → project through the
bindings on JVM; the arm64 emulator loads the native library (the inherited client's
packaging-bug class is exactly what the emulator CAN answer); no android app code
touched yet.

### S1 — the android chat surface, behind a flag

A new UI surface (module/package beside `ing.croft.call`, e.g. `ing.croft.social`) —
groups list, timeline, the truthful membership panel, mute — consuming S0's bindings,
gated behind a build flag so release builds of the calling app are byte-boring. The P6
renderings port from the TUI: CONTESTED as "membership pending resolution", the voided
row, marked-never-dropped muted lines, the fork banner as a blocking surface. The
**lost-race UX** debt (two concurrent admissions; the losing side's rendering) lands
here with the E116 leftovers — this is the phase that owed it a home.

*Done when:* the journey harness (the M4 workflow-test pattern, adopted as-is) drives
group-create → invite → message → panel-truth on JVM against the real bindings; a
screenshot run on the arm64 emulator shows the surface; the calling app's existing
tests are untouched and green.

### S2 — the keylayer joins the product: sealed chat on-device

`ports/keylayer-openmls` wired through the shell: group creation seats real MLS, the
invite path carries the real Welcome, messages seal/open at AEAD grade on-device, and
the token-return arc runs device-to-device — the E117 P6 loopback ladder climbed one
rung. Storage: the openmls provider persists under the app dir beside redb. The honest
rung after this phase: chat is MLS-sealed on hardware; transport between devices rides
the existing sync machinery first (iroh-gossip as in the TUI), the relay path being
fabric admission's territory and out of scope.

*Done when:* two real devices (the Samsung + the borrowed Pixel, per the standing rig)
exchange sealed messages in a group one of them planted; a departure + token return
runs device-to-device; per-plane rungs stated in the runbook style.

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

*Done when:* the ADR is accepted; the binding fact type is built and pinned in the core
(a Vouch specialization or its own type — decided in the ADR); the connect proposal doc
exists for the owner to carry.

### S4 — the rendered-principal joint (the ONE calling-track touchpoint)

Only after M4's operational close (production enforce flipped, or the owner declares
the window open): the callee/caller cards learn to render social context — "in 2 groups
with you", the binding-fact-backed display name — read-only, through the
rendered-principal seam ADR-0001 named. Calling still computes nothing from group data
(the caps engine unchanged); this is presentation composition, the first visible
payoff of the two tracks sharing one substrate.

*Done when:* a device run shows social context on a call card with the caps engine's
tests untouched; the coordination claim protocol was used for every `android/` commit.

### S5 — the web probe (and apple, named only)

openmls-on-wasm moves from compile-proof to runtime-proof: the wasm build of the core +
keylayer executes in a browser (seal/open round-trip in a headless page), and a thin
web shell spike renders the chat pond's `project()` output. Croft-pwa precedent applies
for scaffolding. Apple: the uniffi surface is the preparation; committing an apple
shell is its own future decision, recorded here as named-not-committed exactly as P7
was in E117.

*Done when:* the browser round-trip runs in CI or a recorded local run with the honest
rung stated; a one-screen web spike exists or the blocker is named.

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

## Open questions (for Pass 2/3)

1. **Flag mechanics (S1):** build-time flag (product flavor) vs runtime hidden entry —
   flavors keep release bytes identical (recommended); runtime flags ship dormant code.
2. **The S2 transport rung:** iroh-gossip device-to-device first (recommended, reuses
   the proven TUI machinery) vs waiting for a relay-carried path (fabric-admission
   entanglement — recommended against inside this plan).
3. **S3's fact shape:** specialize the existing `Vouch` (0x000A) with a context tag vs
   a new fact type — leaning specialize, but the ADR decides.
4. **Apple:** stays named-only here — confirm, or promote a minimal SwiftUI spike into
   S5.
5. **Plan home check:** this plan lives in croft (the client repo owns client plans, the
   M-series precedent); discovery gets a ROADMAP row pointing here. Confirm.

## Review log

- **2026-08-25 — Pass 1 (draft).** Written against croft main `07a6b61` (post-§13-bake
  note), core through `9f7d0c6` (GroupState v4, the 2026-08-25 decision set). The
  standing constraint recorded from the owner's words: croftcall is running through
  testing with edges in both croft and connect.
