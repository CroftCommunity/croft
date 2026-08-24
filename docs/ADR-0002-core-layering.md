# ADR-0002 — The core layer has a foundation, and the foundation is not a pond

**Status:** accepted (2026-08-23). **Context:** E117 Phase 2 lands the first real
crate in `core/`; the layering must be on record before a second reader
mis-shelves it. Extends ADR-0001 (one shared functional core, per-pond domain
cores, thin shells); changes none of it.

## The two-layer core

ADR-0001's `core/` was drawn as a row of peer ponds — an app's brain per
domain, `(state, intent) -> (state, effects)`, pure. The first crate to land
(`social-tree-core`) is **not** one of them. It is the **substrate the ponds
stand on**: groups, membership, standing, the governance fold, its wire codecs
and projections — the Drystone social tree, which every pond consumes and no
pond owns.

```
core/
  social-tree-core     FOUNDATION — the social tree; ponds consume it
  call-core            pond (see below)
  feed-core            pond
  chat-core            pond (arrives at E117 P5)
```

A pond answers "what does this app do"; the foundation answers "who is here,
in what group, with what standing." Mistaking the foundation for a fourth pond
is how someone one day gives a pond its own copy of membership — the drift
this ADR exists to refuse.

## Where call-core sits

Croft doctrine says **"Calling is a capability, not a pond"** — while the
skeleton has carried `core/call-core` since before the doctrine was written.
Resolution: **call-core stays, as a pond, and the doctrine sentence is about
authority, not module layout.** Calling-as-capability means the *right to
call someone* is a grant resolved against identity and standing (the M-series
callability machinery) — an authority question, answered by the foundation
and the caps layer, never by a pond's own state. The pond named `call-core`
is the call *experience*: session state, media intents, the in-call UI's
brain. It consumes callability; it never computes it. If a future reading of
call-core starts computing grants, that is the violation — not the
directory's existence.

## The two admissions (the paragraph E117's reviews required)

The program now has two live things called "admission," at two layers:

- **Fabric admission** — croft-stack / the relay (D3, M4): who may use the
  relay's *resources*. A deployment-scoped capability check: service-auth
  mint, sponsorship and device-scope claims, denylist, QoS ceiling.
- **Group admission** — Part 2 §10.2.2's A-series, realized through
  `social-tree-core` (and Phase 4's key layer): who is a *member* of a group.
  A governance decision, rights-bearing, center-free, where A3 — validity
  MUST NOT imply admission — is the load-bearing invariant.

They share a word and nothing else. **The relay admits traffic, never
members; no signal from fabric admission is ever an input to the A-series.**
Wiring one into the other would hand a center authority over membership — the
S16 failure class one layer down — and is refused here, in advance, by
architecture record.

## The effect-composition rule (fixed once, per the vet's O7)

Two Elm-shaped layers stack here — substrate and pond — and this joint has no
prior art to copy, so the rule is decided once, now, not re-derived per pond:

1. **One substrate instance per shell.** The shell owns it, beside the ports.
2. **Ponds hold projections, never substrate state.** A pond's `update`
   receives the substrate views it needs (membership, standing, group lists)
   as read-only inputs. A pond that caches or copies substrate state is
   wrong by construction.
3. **Ponds speak to the substrate through effects.** A pond's effect enum
   may carry a substrate intent (`Social(...)` wrapper variant — the
   `Cmd.map` shape); the shell executes it against the one instance and
   feeds refreshed projections back on the next turn.
4. **The substrate never knows pond types.** Dependency points one way.

## Consequences

- The workspace gains its first member; G6 (`make gate` green) is armed, G7
  (CI runs the same gate) rides `.github/workflows/ci.yml`.
- `social-tree-core` is AGPL-3.0-or-later (owner decision 2026-08-21,
  consistent with A14), pure by mechanical enforcement (clippy
  disallowed-methods, the wasm32 and no-default-features CI arms).
- The redb realization stays in the discovery corpus
  (`local_storage_projection`), consuming this crate pinned by commit — the
  evidence machinery stays with the experiments; the mechanism lives here.
- Doc coverage rides the ratchet: `warn(missing_docs)` now, deny when clean.
