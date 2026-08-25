# ADR-0002 — The core layer has a foundation, and the foundation is not a pond
**Tags:** architecture, layering, rust, ponds

**Status:** accepted (2026-08-23); amended 2026-08-24 (the tree frame).
**Context:** E117 Phase 2 lands the first real crate in `core/`; the layering
must be on record before a second reader mis-shelves it. Extends ADR-0001 (one
shared functional core, per-pond domain cores, thin shells); changes none of it.

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

## The foundation is a tree, and groups are its first-built aspect

(Amended 2026-08-24, from the P4 design session.) The foundation's model is
the **social tree**, and the tree is person-rooted: the person is the
proverbial root and literal trunk, and everything else is drawn outward from
that center — direct connections at radius one, their connections at radius
two, mutuals as paired symmetry, mutuals-of-mutuals as the shape you can see.
There is no global tree; every tree is rooted at someone. That is the
data-model echo of a property the architecture already had epistemically
(local truth, peer equality — every node folds its own view from its own
position); the model here catches the data up to the epistemology.

A **group**, in this frame, is a place where several trees' branches
interweave and agree to shared rules — a derived enclosure with a charter,
not the primitive. What `social-tree-core` holds today is that group aspect
(membership, standing, charters, admission): built first because it is the
hardest part, but one aspect, not the definition. The crate's name names the
destination. The earlier croft-chat build got this wrong in the other
direction — it built groups and forgot the person-outward root — and this
section exists so that mistake is not repeatable by omission.

The **connections aspect** is also foundation-layer, and lands beside the
fold as its own module(s) under its own plan (discovery E134):

- **edges outward** — follows/mutuals and the radius model (treesocial is
  this aspect's presentation surface);
- **vouches/bindings** — the DID↔persona binding as a recorded human act
  (discovery E120);
- **capability grants as edge facts** — callability attaches per pair at
  the rendered-principal seam (the call-core section below); named here,
  not designed;
- **personal annotations** — mute and block. These hang on the *edge*,
  never on the enclosure, so they are cross-group by construction (mute
  someone for thirty days and it applies across every shared group), and
  they are local truth — never folded, no quorum, no wire; being
  local-only, a duration may even be clock-denominated where shared
  governance facts never may.

Two drifts refused in advance, the same species this ADR already refuses one
of: a connection must never be modeled as a group (contacts-as-a-group-of-
two), and a personal annotation must never be bolted into group governance.
Both would smuggle the enclosure shape into the one place the model says the
person is the center. The layering above the foundation is unchanged: ponds
consume both aspects as projections, and chat is the first manifestation of
the layers on top.

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
