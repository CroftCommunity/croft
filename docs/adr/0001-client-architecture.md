# ADR-0001 — Shared functional core + per-platform shells
**Tags:** architecture, client, rust, shells

**Status:** Adopted 2026-08-11. **Not decided here** — restated from
`discovery/alpha/thinking/app/client-architecture-adr.md`, Accepted 2026-06-22.

**Why restate rather than link:** this repo is where the decision is *executed*, and
a reader here should not have to reconstruct it from another repo. The reasoning
below is a faithful restatement; `discovery` remains the origin and holds the
derivation.

## Decision

One shared, pure functional core consumed by thin per-platform shells, with two
orthogonal callout axes.

```
SHARED (platform-agnostic, no I/O / async / clock; WASM-clean)
  core     pure (state, intent) -> (state, effects) + projection (model -> view model)
  shell    cross-platform composition (layout / slots / pinning)
  design   design system (tokens / primitives)
  <port>   the I/O contract as a trait, held by the shell, never called by a core

PER-PLATFORM SHELLS (thin)
  web      effects.rs + leptos
  android  effects.kt + kotlin host
  apple    effects.swift + swift host
```

**Axis 1 — platform.** Each shell supplies its own effects handler: it *performs*
the effect-requests the core emits **as data**, and feeds results back in as new
intents. Same core, different effect-performers.

**Axis 2 — implementation.** Adapters swap behind a port, orthogonally to
platform: a fixture-backed fake and a real HTTP adapter for the same port. The
core and shells are blind to which is wired.

## Structural decomposition: per-pond cores

**Per-pond domain cores unified by the shared `shell` composition layer** — not one
god-core (which would couple a feed read-model to an MLS group engine), and not
disconnected cores (which would re-fatten the shells). Per-pond concerns live
inside that pond's core, never smeared across a shared one.

### The cross-pond line, and why calling sits where it does

The origin ADR separates two things and defers one of them:

- **Awareness** — read-only surfacing of one pond's content inside another's view.
  Composition in the shell; the content carries a *reference*, the shell resolves
  it, nothing flows back. Cheap, expected.
- **Interactivity** — *acting* in pond A from pond B. Needs a **broker** that
  translates idioms, sitting between cores, never inside a pond core. Deferred
  "until a concrete cross-pond action is committed to."

**That condition is now met** (owner, 2026-08-11): a call affordance next to a
username, inside other ponds' views.

But calling is **not a pond** — a pond has a timeline, a population, an idiom, and
calling has none. It is an **action on a principal**. So it is modelled as a
capability core behind a port, attaching to a single shell seam:

> **The unit is a *rendered principal*.** Where a view model carries a DID, the
> call affordance can attach. One seam, not one integration per pond.

This avoids a broker entirely for this case. A broker is still the right answer
for genuine pond-to-pond actions (replying to Bluesky from chat); it is not needed
to act on a *principal*, because a principal is not pond-specific.

## Consequences accepted with this

- **Logic continuity, not UI continuity.** Shells cannot drift because they share
  the core verbatim; the UI is deliberately per-platform.
- **The mobile shells are native**, not webviews — lifecycle, background execution
  and P2P cannot be driven from a webview, and a native shell also avoids the
  App Store "minimum functionality" problem a wrapper invites.
- **The core must stay WASM-clean.** No I/O, no async, no clock. A clock read
  inside a core is the classic way this architecture rots.
- **Effects-as-data is load-bearing, not stylistic.** `update` is
  `(model, intent) -> (model, Vec<effect>)`, which an awaited port call cannot
  satisfy. Introducing one `async fn` into a core breaks the property that makes
  the whole thing testable.

## Prior art this rests on

Demonstrated before adoption in `discovery/alpha/experiments/croft-app-phase0/`
(core/shell/design/cli/web/desktop/bluesky) with 20 acceptance tests against the
pure core. The pattern is Crux-style functional-core / imperative-shell; the
mobile variant (native shell → Rust core over UniFFI) is the shape Delta Chat and
Berty both ship.

## What this ADR does not settle

- The mobile background/durability story — see `PLATFORM-POSTURE.md`.
- Whether `feed-core` here adopts the Social Tree backbone work in `discovery`
  (ROADMAP_TODO E62) or grows independently.
