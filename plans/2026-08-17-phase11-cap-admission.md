# Phase 11 — cap/admission on the client (plan)

**Status: ACTIVE — D1/D2 decided by the owner 2026-08-17; M1 underway.**
Written 2026-08-17, the day rungs 0–3 all went green and v0.2.0 shipped.

**D1 decided:** lazy-on-tap **plus** a TTL cache — resolution happens only on
explicit user action (nothing resolves on render), and the derived result is
cached with a TTL so repeated actions against the same principal do not
re-leak a lookup. The cache holds *derived state*, not raw records.
**D2 decided:** the engine is Kotlin in `croft/android` now, written pure
(no Android imports, effects at the edges) so the shared Rust core can absorb
it later without a rewrite of the semantics.
**D3 remains open** (relay token semantics — design against the croft-stack
admission plan, blocks M4 only).

## Problem statement

The call path is proven (v0.2.0: our relay carries the call, the path is
observable), but *anyone with an EndpointId can dial anyone* — there is no
admission. The contract for who-may-call is built and canonical on
`CroftCommunity/connect` (contract v2: per-device endpoints + grants /
matchers / policies), with a reference engine in `connect/web/resolver.js`
and a handoff spec in `connect/docs/PHASE11-HANDOFF.md`. The client half —
callability resolution, identity proof, call-time evaluation — is unbuilt.

## What already exists (verified 2026-08-17)

- `DeepLink` captures `device` + `grant` (handoff item 4 is mostly done;
  what remains is *consuming* them at call time).
- `RelayConfig.authToken` (iroh-ffi 1.0.0) becomes an `Authorization: Bearer`
  header on the relay upgrade — the transport-level admission hook, pinned in
  `CroftRelay` with a test, currently null.
- `connect/web/resolver.js` exports the full engine to mirror:
  `resolveHandle` / `resolvePds` / `listEndpoints` / `fetchGrant`,
  `verifyTicketSecret` / `redeemTicket`,
  `evaluateMatcher` / `evaluateRules` / `evaluateGrant` (§7, fails closed).
- The vocabulary bridge (handoff): callability is **derived** —
  "does any grant admit me and do its rules still hold" — not a three-value
  field. `callable` / `not-listed` / `may-not-permit` are outcomes of
  `evaluateGrant`, not a lookup.

## Approach — milestones, each cut/validated/promoted like rungs 0–3

Ordered so every milestone is independently testable on devices, and the
OAuth lift (the biggest unknown) is not load-bearing for the first two:

1. **M1 — ticket redeem, no identity.** A `ticket` grant admits without
   `provenDid` (the secret in the invite-link fragment is the proof). Client:
   parse invite link → `verifyTicketSecret` → dial with the grant carried.
   This exercises grants end-to-end with zero OAuth.
2. **M2 — callability resolver** at the rendered-principal seam:
   `resolveHandle → resolvePds → listEndpoints → grants → derived state`.
   Needs decision D1 (below) first.
3. **M3 — OAuth identity proof** (`provenDid` via atproto OAuth against the
   caller's PDS). Biggest lift; unlocks `mutuals` / `registeredCallers`.
4. **M4 — call-time `evaluateGrant` as an effect** and the relay-side
   enforcement wire-up (`usesSoFar` / `grantExists` come from relay/CISS
   Membership, not the page; `authToken` starts carrying whatever the
   admission token turns out to be). Coordinates with croft-stack.

## Reasoning

- **Ticket-first (M1 before M3)** because it delivers a working, testable
  admission story without the OAuth dependency, and it is the grant type the
  invite-link flow already produces on the connect side.
- **Mirror `resolver.js`, don't reinvent** — the handoff names it the
  reference engine; a Kotlin port with the same test vectors keeps the two
  halves provably aligned. (External-API rule: field names come from
  `resolver.js` and `contract.md`, never inferred.)
- **Milestone = candidate** per `ops/RELEASING.md`, same ladder discipline
  that just worked for rungs 0–3.

## Decisions needed before code (the owner's, not the plan's)

- **D1 — resolution strategy** (blocks M2): lazy-on-tap vs cached-TTL vs
  batched. This is the metadata-leak decision the changelog carries as Open —
  resolving callability is PDS lookups that reveal who you are looking at.
- **D2 — where the engine lives**: Kotlin in `croft/android` now (matches the
  app as it exists), vs starting the shared Rust core (the architecture the
  repo is *for*, but `core/` is skeleton and nothing runs on it). The handoff
  says "effect + port, never awaited in a core" — which is a constraint on
  shape *if* the core is used, not a mandate to build the core now.
- **D3 — relay enforcement semantics** (blocks M4): what the relay actually
  checks per contract §7 and what the Bearer token contains — belongs to the
  croft-stack/relay plan (`discovery` tiered-admission plan) and should be
  designed against it, not guessed here.
