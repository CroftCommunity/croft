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
   **DONE 2026-08-17, validated on-device:** the Pixel consumed the live
   invite link (explicit-component VIEW intent; the manifest offers the app
   for `connect.croft.ing/redeem`, chooser-based until assetlinks), resolved
   everything from public records, and the call connected via our relay then
   upgraded to direct — roles reversed from earlier runs (Pixel caller).
   Engine: `caps/` (32 tests) + `UrlHttp` + `redeemInvite` (lazy-on-tap per
   D1; the TTL cache layer arrives with M2, where repeat lookups exist).
2. **M2 — callability resolver** at the rendered-principal seam:
   `resolveHandle → resolvePds → listEndpoints → grants → derived state`.
   Needs decision D1 (below) first.
3. **M3 — OAuth identity proof** (`provenDid` via atproto OAuth against the
   caller's PDS). Biggest lift; unlocks `mutuals` / `registeredCallers`.
   **DONE 2026-08-17, validated on-device** (plan:
   `plans/2026-08-17-2-plan-m3-identity-proof.md`): live OAuth against the
   bsky.social entryway from the Samsung — PAR + hand-rolled ES256 DPoP +
   PKCE, client metadata hosted at
   `connect.croft.ing/oauth-client-metadata.json`, redirect on
   `ing.croft.connect:/oauth` (the spec fixes the scheme to the client_id
   host reversed) — DID surviving force-stop, and the flip observed both
   directions with the live fixtures: signed in → callable via
   `m3registered`; signed out → may-not-permit immediately (identity-keyed
   cache). Candidate `v0.4.0-rc.1`.
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

## M1 test bed (live since 2026-08-17)

One callee account is enough for M1 — a ticket admits by possession, so the
caller needs no identity. Published and verified end to end (all reads
unauthenticated, exactly as the client will do them):

- Callee repo: `ngvalidation2112.bsky.social` = `did:plc:xyfhcaweaeyew3zrgk6jaln7`,
  PDS `stropharia.us-west.host.bsky.network` (resolved via plc.directory).
- `ing.croft.iroh.endpoint` rkey `self`: the Samsung test device's EndpointId
  (`14af214d…c5ab`), `homeRelay https://relay.croft.ing:8443`.
- `ing.croft.call.grant` rkey `m1ticket`: `ticket` matcher, `devices:["self"]`,
  no policyRef.
- The ticket secret and account credentials live in `CroftC/.env`
  (git-ignored; the meta-repo ignores everything at the root by design). The
  invite link for on-device testing is
  `https://connect.croft.ing/redeem?repo=ngvalidation2112.bsky.social&grant=m1ticket#<secret>`.
- Verified: secret hashes to the stored `secretHash` (MATCH), grant + endpoint
  readable with no auth, handle→DID→PDS chain resolves.
- Two-accounts becomes necessary only at M3 (`mutuals` needs a proven caller
  DID and a bidirectional follow).

**Extended for M2/M3 (same day):** the second account
(`bobzmudacroft.bsky.social` = `did:plc:l5xigmplwu7eyxjobjr23iza`, the caller
identity) is mutual with the callee (follow records both ways; verified via
`app.bsky.graph.getRelationships` — both `following` and `followedBy`
populated, the exact shape `Evaluate.areMutuals` consumes). The callee repo
gained two identity grants alongside `m1ticket`:
`m3registered` (`registeredCallers`, dids=[caller], devices=["self"]) and
`m3mutuals` (`mutuals`, all devices). Pre-OAuth, the resolver honestly
derives MayNotPermit against these; with a proven DID (M3) both admit —
the fixtures for that flip are now live.

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
