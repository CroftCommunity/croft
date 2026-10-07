# Plan — the connections module: one "are we mutuals" rule, then mute (E134 slice 1)

**Status:** DRAFT 2026-10-07 — Pass 1 (croftc-06). The three framing decisions are the
owner's, made 2026-10-07 (see *Decisions*); nothing below them is reviewed. **No code before
review.** Phase 0 is discovery only and may run on the owner's go.

**Naming note:** no `-N` ordinal, per `CroftC/.claude/TRACKING.md` § Plan files.

**Source rows:** discovery `alpha/ROADMAP_TODO.md` **E134** (the connections aspect of the
social tree — "currently homeless"), **E120** (DID ↔ persona-key binding — the second
consumer, NOT in this plan), **E146** (ring fan-out, measured); croft `docs/adr/0002-core-layering.md`
(amended 2026-08-24: connections land "beside the fold as its own module(s) under its own
plan").

## Problem Statement

**Who may call you is decided by one rule written three times, and that rule asks a
company's index.** Bob's grant says *my mutuals may call me*. When Alice dials, three
independent implementations of the calling contract's matcher (connect `docs/contract.md`
§2/§7) can be asked whether she qualifies:

| Where | Language | Role | How it learns "mutual" |
|---|---|---|---|
| croft-stack `croft-relay-admit/src/caps.rs` + `mint.rs:428–443` | Rust | **the authority** — mints or refuses | `resolver.mutuals()` → AppView `getRelationships` |
| croft `android/app/.../caps/Callability.kt` (`areMutuals`, l.86–95) | Kotlin | courtesy pre-check (greys the button, says why) | AppView `getRelationships` |
| connect `web/resolver.js` | JS | the reference engine; the invite page | AppView `getRelationships` |

Three consequences, each already visible in code:

1. **They have drifted.** On a lookup that fails, admit refuses `Unavailable`
   (`mint.rs:435`); the Android check collapses a missing relationship into *not mutual*
   (`Callability.kt:91–92` returns `false`). On a malformed `expires.at`, admit denies — "a
   **deliberate divergence** from the reference engine" (`caps.rs` header). Drift is only
   caught where an enforcement row happens to pin it.
2. **The foundation has no place for any of it.** `social-tree-core` holds the group
   aspect only (E134: membership, standing, charters, admission). Follows, mutuals and the
   radius outward — what ADR-0002's amendment calls "the person outward is the root" — have
   no module, so each consumer re-derives them. The one connections fact that exists, the
   personal mute set, lives in **`chat-core`** (`model.rs:101–103`), so calling cannot see
   it: a muted person can still ring you.
3. **"Local authority first" has a centralised dependency on the live call path.** Every
   implementation asks Bluesky's AppView. The people's own follow records
   (`app.bsky.graph.follow` in each PDS) are the source of truth, and forage's
   *Direct social tree PDS query* beta already reads them — but nothing in the calling
   track can.

**What we want:** one pure rule for "may this person call me," built on a connections
module that records *who says* two people are connected and *when*, used by the server,
the Android app and the macOS window alike — and a mute that calls honour too.

## Reasoning

- **Why the mutuals rule first, not mute (owner, D2).** It is live in production,
  triplicated, and already has a server-side enforcement row to grade against
  (croft-stack `docs/ENFORCEMENT-SCENARIOS.md:45`: *Mutuals grant, actual mutual — MUST
  ADMIT (one-way follower MUST REFUSE)*, `PIN:mint.rs::a_mutual_mints_and_a_one_way_follower_does_not`).
  Mute exists, works, and has no production consumer; moving it is cheap once the module
  exists, so it is phase C6 of this plan rather than a plan of its own.
- **Why edges carry their source (owner, D1).** The core must not care whether a follow
  came from the AppView or from a PDS — but a reader of a verdict must be able to ask. An
  edge that says *AppView reported this at T* versus *Alice's PDS holds this record at T*
  is the provenance tenet ("machines compute provenance, never utility") applied to the
  graph, and it is what lets phase C5 add the PDS source without touching the rule.
- **Why admit depends on the core rather than keeping its own copy (owner, D3).** One
  rule cannot drift. The owner chose this knowing the costs (a declared cross-repo edge, a
  pinned rev, a toolchain constraint, server re-deploys following core changes); phases
  C3 and Phase 0 exist to pay them deliberately rather than discover them.
- **Why not a wasm path for connect's JS engine here.** Whether web consumers take the Rust
  core is the open Track A question 1 (canvas 2026-09-28). This plan keeps `resolver.js`
  and registers it as a copy (SHARED-CODE.md: "a copy is debt with a name").
- **No atproto types in `social-tree-core` (E120's standing discipline).** The
  connections module speaks opaque subject ids; DIDs live in the calling crates and ports.
  This is why the rule splits across two crates (see Approach).

## Approach

```
                 ┌─────────────── ports (impure) ───────────────┐
                 │ AppView source (C1)      PDS-direct source (C5)│
                 └──────────────┬──────────────────┬─────────────┘
                     Edge{from,to,kind:Follow,source,observed_at}
                                ▼
 social-tree-core::connections  (pure, opaque SubjectId, no clock, no DIDs)
   mutual(a, b, edges) -> Verdict { Mutual | OneWay(dir) | Neither | Unknown(why) }
   annotations: mute set (C6, local truth, never folded, never sent)
                                │
                                ▼
 call-core::grant  (pure; the contract §2/§3 matcher + rules, DIDs allowed here)
   admits(grant, proof, connections verdict, now) -> Admit | Refuse(reason)
        │                         │                          │
        ▼                         ▼                          ▼
 croft-stack admit (C3)    croft-ffi → Android (C4)    croft-ffi → macOS (C4)
 git-pinned croft rev      replaces Callability.kt     shell/apple
```

The matcher moves into `call-core` (it is calling-contract semantics and must see DIDs);
the graph question moves into `social-tree-core::connections` (foundation, opaque). The
fold is untouched: connections are **not** governance facts and never enter it (ADR-0002's
two refused drifts: no contacts-as-a-group-of-two, no annotation in group governance).

## Decisions (owner, 2026-10-07 — walked one at a time in session croftc-06)

- **D1 — where follows come from: BOTH, AppView the default.** The core is
  source-agnostic; every edge records its source and observation time. Phase C1 keeps the
  AppView, so nothing changes on the live call path. Phase C5 adds the PDS-direct source
  and **measures call-time latency on real accounts** before any decision to flip the
  default (the E146 lesson: a hop looked cheap until measured — 630× at two hops).
- **D2 — first slice: the mutuals rule, then mute.** C1–C4 are the rule; C6 moves mute.
- **D3 — admit depends on the core** (croft-stack pins croft at a git rev, as it pins
  CISS today: `croft-relay-admit/Cargo.toml:45`). Recommended alternative was two copies
  plus a shared truth table; the owner chose the dependency.

## Verified Assumptions (checked 2026-10-07; re-check before relying)

- Toolchains differ: croft `rust-toolchain.toml` = **1.97.1**; croft-stack = **1.94.1**.
  `social-tree-core` and `call-core` are `edition = "2021"`; neither declares a
  `rust-version`. Whether they build on 1.94.1 is **unverified** — Phase 0, P0.1.
- admit's matcher is already pure with the network deferred: `MatcherVerdict::CheckMutuals`
  (`caps.rs:110–118`), resolved in `mint.rs:430–441`. It is the best-shaped of the three
  and is the port source.
- `call-core` today holds dial-side proof selection only (`dial.rs`: ticket / grant rkey),
  not the matcher.
- `chat-core`'s mute: `model.rs:101–103` (`muted: BTreeSet<PrincipalId>`), toggled in
  `update.rs:78–79`, seeded and persisted by the shell (`model.rs:142,165`).
- `PrincipalId` is `[u8; 32]` (`social-tree-core/src/model.rs:62`) — a persona-key
  identity, not a DID.

## Concurrency Map

- **session-state (croftc-be, `worktrees/session-state`, branch `claude/session-state`)**
  — its plan (`2026-09-28-plan-session-state.md`, unlanded at `94dd8b9`) leans to a
  `call-core::session` module. **C2 writes `call-core::grant` in the same crate.** C2 does
  not start until session-state has landed or the two sessions agree the module and
  `lib.rs` boundary in a message. C1 (`social-tree-core`) does not collide.
- croft-stack: C3 touches `croft-relay-admit` and the deploy pins — a **claim**
  (`croft-stack--admit-pin`) and the staging-first rule (`DEPLOYMENT.md`).
- connect: C4 touches nothing in connect beyond a SHARED-CODE register row.

## Phases

### Phase 0 — Discovery (no production code)

- **P0.1** Do `social-tree-core` and `call-core` build and pass under **1.94.1**? Run in a
  scratch worktree with that toolchain explicitly (`rustup run 1.94.1 cargo test -p …`).
  Outcome decides C3's shape: declare `rust-version` and add a CI leg, or bump croft-stack.
- **P0.2** Can croft-stack's CI fetch a git dependency on `CroftCommunity/croft` the way it
  fetches CISS (public repo, same mechanism)? One throwaway branch, read the gate log.
- **P0.3** **The truth table.** Walk all three matchers (`caps.rs`, `Callability.kt`,
  `resolver.js`) against one table of cases — mutual / one-way each direction / neither /
  blocked-either-way / deactivated / lookup unreachable / malformed `expires.at` /
  unknown matcher / unknown rule / empty device scope — and record each engine's answer.
  Every divergence becomes an Open Question for the owner, not a silent pick.
- **P0.4** Probe what `getRelationships` actually returns for blocked, deactivated and
  not-found actors (a probe script printing raw responses — CLAUDE.md § External APIs).
  The Kotlin and admit code read only `following`/`followedBy`; what else is there decides
  whether `Verdict` needs a `Blocked` arm.

### C1 — `social-tree-core::connections`: edges and the mutual verdict

Pure, RED first, wasm32-clean (`core-purity`). `SubjectId` (opaque), `Edge`,
`Source { AppView, Pds }`, `observed_at` as a supplied value (the core has no clock),
`mutual(a, b, edges) -> Verdict`, where `Unknown(why)` is distinct from `Neither` — the
honesty invariant: "I could not ask" never reads as "no". Rows pinned from P0.3's table.
Mutation round after green (`cargo mutants`, detached scratch worktree).

### C2 — `call-core::grant`: the contract matcher, ported from admit

Port `caps.rs`'s matcher and rules (its fail-closed choices intact, unless P0.3's owner
answers change one) into `call-core`, consuming C1's `Verdict` for `Mutuals`. Dual-graded:
the P0.3 table plus admit's existing tests re-pointed at the ported code. ENFORCEMENT
matrix rows for the client half added with `RUST:` pins.

### C3 — admit depends on the core

croft-stack pins croft at a rev for `call-core` + `social-tree-core`; `caps.rs`'s matcher
is deleted and `mint.rs` calls `call-core::grant`. Every existing croft-stack enforcement
row must still pass **unchanged** — a changed row is a semantics change and stops the
phase. ARCHITECTURE.md declares the new edge (croft → croft-stack, build-time). The bump
rule is written down: a croft rev bump in croft-stack is a decision (audit check 47c
already reminds), and **a matcher semantics change ships to admit before any client relies
on it** (server-first, as the contract's own ordering). Deploy: staging first — staging
rehearses the next artifact — then production, owner-authorized.

### C4 — the clients adopt it

`croft-ffi` exposes `call-core::grant`; Android's `Callability.kt` keeps only its port (the
AppView lookup, now producing `Edge`s) and renders the core's verdict, including
`Unknown` as words rather than *not callable*. The macOS shell uses the same. Device
verification on both phones (claim `testbed--*`; TESTBED.md step 0 re-sign-in). connect's
`resolver.js` stays JS: a SHARED-CODE register row + TODO.

### C5 — the PDS-direct source, measured

A `Pds` source port reading `app.bsky.graph.follow` via `listRecords` (forage
`js/substrates/pds-graph.js` is prior art, not a dependency). **Measure** call-time
latency for real accounts (median 337 / p90 1,530 follows per E146's crawl) with and
without a cache; the default flips only on the owner's call against the numbers.

### C6 — mute moves to connections

The mute set leaves `chat-core` for `social-tree-core::connections` as a local annotation
(never folded, never sent; a duration may be wall-clock locally, per E134). Chat keeps
honouring it; calling starts to. What a muted caller *experiences* is an Open Question.

## Open Questions

1. **The failed-lookup divergence:** admit refuses `Unavailable`, Android says *not
   mutual*. Recommended: `Unknown` everywhere, rendered as words — but it is the owner's.
2. **`expires.at` malformed:** admit denies, the reference engine admits. Which is the
   contract? (Likely a connect contract clarification — v2.1 or v3.)
3. **`SubjectId`:** a fresh opaque newtype over a hashed DID, or reuse `PrincipalId`? The
   E120 binding (later plan) is what eventually relates the two; reusing now would
   pre-empt it. Recommended: a fresh newtype.
4. **A muted caller (C6):** does the call fail, ring silently, or go unanswered with no
   signal? Each says something different to Alice.
5. **The lexicon questions from connect #10** (`type` vs `$type`, datetime format, rkey
   kinds) touch the same records; settle them before C2 freezes a parser.

## Adjacent, explicitly OUT of scope

E120's binding fact and contract v3 (the next consumers); radius beyond one hop (E146:
a background walk with an "as of" stamp, not a slider stop); forage's ring (TS; Track A
question 1); R5–R8 of the one-stream plan; publishing `ing.croft.*` lexicons (#22).

## Documentation Impact

ADR-0002 (connections now has an occupant), croft-stack ARCHITECTURE card + DEPLOYMENT
pins, both ENFORCEMENT-SCENARIOS files, SHARED-CODE register (`resolver.js`), E134's
roadmap row (retire-by partially met after C1/C6), croft `CHANGELOG.md` per phase.

## Review Log

- 2026-10-07 — Pass 1 drafted (croftc-06) from the owner's three decisions. Awaiting
  owner review; Phase 0 may run on the owner's go.
