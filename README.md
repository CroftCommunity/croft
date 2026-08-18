# croft

The Croft client. One shared functional core, thin per-platform shells.

**Status: shared core is skeleton; the android app runs and is released.** The
functional core, the cross-platform shells, and the toolchain contract are here;
the **core** is not yet implemented. The **android app** — the inherited
croftcall client — builds, launches, and is published as **`v0.4.0`** (Latest):
it camps on the croft relay (`relay.croft.ing:8443`), reports the live
connection path, redeems exchange invite links, and proves caller identity
via atproto OAuth, surfacing derived callability on the callee card
(Phase 11 M1–M3). Rungs 0–3 and the Phase 11 milestones validated on real
devices 2026-08-17 (see `ops/RUNBOOK-two-device-call-test.md` and
`plans/2026-08-17-2-plan-m3-identity-proof.md`). It is not yet rebuilt on
the shared core.

## The shape

```
SHARED (pure, no I/O, no async, no clock — WASM-clean)
  core/      per-pond domain cores:  (state, intent) -> (state, effects)
  shell/     cross-platform composition — owns the "rendered principal" seam
  design/    tokens and primitives
  ports/     the I/O contracts as traits; held by the shell, never called by a core

PER-PLATFORM SHELLS (thin — the only place platform code lives)
  web/       leptos/wasm + effects
  android/   kotlin + effects          ← first target
  apple/     swift + effects           ← follows android
  ffi/       uniffi bindings for the mobile shells
```

Effects are **data**, never function calls. A core emits an effect request; its
shell performs it and feeds the result back as a new intent. That is what keeps
the core pure, synchronous, testable, and WASM-clean.

The full reasoning is `docs/ADR-0001-client-architecture.md`. It is adopted, not
invented here — the decision was made in `discovery` on 2026-06-22 and
demonstrated in code across three platforms before this repo existed.

## What continuity means here

You get **logic continuity, not UI continuity.** Every shell consumes the same
core verbatim, so behaviour cannot drift between web, Android and Apple. The UI
is deliberately native per platform: desktop reuses the web shell because it is a
browser engine anyway, mobile does not, because lifecycle, background execution
and P2P cannot be driven from a webview.

## Calling is a capability, not a pond

A pond has a timeline, a population, an idiom. Calling has none — it is an
**action on a principal**. So it lives in `core/call-core/` and attaches to the
shell's *rendered principal* seam: wherever a view model carries a DID, the call
affordance can attach. One seam, not one integration per pond.

The directory site that resolves handle → endpoint is a separate thing that
stands on its own (`CroftCommunity/connect`); calling here is the integration
with it, not a copy of it.

## Platform posture

Read `docs/PLATFORM-POSTURE.md` before promising anything about background
behaviour. Short version: iOS cannot hold a background socket, so device-to-device
P2P is opportunistic, never deterministic, and the honest promise is **best
effort, stated plainly — and not forever.**

## Environment

`env/` is the toolchain as code: pinned versions, checksums, one command up, and a
verify step that **refuses** rather than warns.

```
make bootstrap     # zero -> working, idempotent
make verify        # diffs installed against declared; exits non-zero on drift
make emulator      # create/boot the AVD from its definition
make emulator-nuke # destroy it — restoring is meant to be a non-event
```

The point of pinning is not rigidity. It is that a wrecked environment is one
command from restored, so experiments stop being scary.

## How this repo keeps its own records

The infrastructure is treated as a product in its own right, because scaffolding
that grows without a record becomes archaeology.

| File | Answers |
|---|---|
| `CHANGELOG.md` | what changed for a consumer |
| `ops/JOURNAL.md` | what we did to the environment, and **why** — failures included |
| `docs/VERSIONING.md` | three clocks: the product, the contract, the toolchain |
| `CLAUDE.md` → Commit gates | what must be true to commit, and when each gate turns on |

The journal's rule is the one that carries the weight: **record the reason, not
just the command.** A command without its why is archaeology; a reason without its
command is a story.

Gates **ratchet** — each is recorded alongside the trigger that turns it on, so
the repo neither pretends to enforce what it cannot yet run, nor drifts to 1.0
with no gates at all.

## Licence

AGPL-3.0, matching the rest of the Croft estate.
