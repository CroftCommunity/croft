# Phase 11 M4 — call-time admission: the cap becomes a relay token (plan)

**Status: DRAFT — written 2026-08-20, the day the relay side finished its
build surface** (croft-stack Phase 8: caps evaluation, service-auth verify,
`/grantCall` mint on the running binary, usage transport, declared deploy —
Review Log in `discovery/alpha/plans/2026-08-07-1-plan-croft-relay-tiered-admission.md`,
chunks C–E).

## Problem statement

M1–M3 made the *social* layer real: a callee publishes grants, a caller
redeems a ticket or proves a DID, and the client derives callability. But
the relay still carries anyone — `RelayConfig.authToken` is null, and the
admission the whole ladder exists for is not wired. The relay side is now
built and waiting: `POST /grantCall` on croft-admit takes a cap (grant rkey
+ proof) and mints a short-lived relay token (sponsorship + device scope)
bound to the caller's EndpointId. M4 is the client half: acquire the proof,
call the mint, put the token on the wire, and validate the refusals as
carefully as the admits.

## What already exists (verified 2026-08-20)

- **Server (croft-stack, built + tested, NOT yet activated in production):**
  `/grantCall` request shape `{callee, grant, endpoint, proof:{ticket|
  serviceAuth}, device?}` → `{token}` or `{error: no_cap|cap_not_found|
  cap_revoked|cap_mismatch|jwt_invalid|replay|quota_exhausted|bad_request|
  unavailable}`. Source of truth: `croft-stack/relay/source/crates/
  croft-relay-admit/src/mint.rs` (external-API rule: field names from there,
  never inferred). Service-auth: `getServiceAuth` with
  `aud=did:web:admit.croft.ing`, `lxm=ing.croft.relay.grantCall`, single-use
  `jti`, 60 s lifetime.
- **Client:** `caps/` engine (redeem, callability, OAuth) behind `Http`/
  `HttpForm` ports; `AuthManager` holds the DPoP-bound OAuth session
  (`provenDid`, access + refresh tokens); `CroftRelay.config()` pins
  `authToken = null`; `CallPeer` builds the node once at foreground with a
  persisted secret key (same EndpointId across restarts, proven).
- **Gaps this plan closes:** the ticket secret is verified at redeem and
  **dropped** — call-time mint needs it retained; `OAuthFlow.refresh` exists
  but nothing calls it (ROADMAP_TODO **E113** — the mint is the first
  load-bearing consumer of a live session); there is no authed XRPC GET
  (getServiceAuth needs the DPoP access token against the caller's own PDS);
  nothing calls `/grantCall`; nothing sets `authToken`.
- **Production posture:** relay.croft.ing runs `croft-relay` v0.1.1 at
  `admission = "open"` — tokens are verified and attributed when presented,
  never required. croft-admit is declared (`services/croft-admit.toml`) but
  not activated. M4 can therefore land and validate *presentation and
  attribution* against production without any enforcement risk, and prove
  *enforcement* against a local/staging enforce pair.

## Approach — chunks, each independently testable

1. **M4a — the admit client, and the secret survives redeem.**
   `caps/Admit.kt`: pure request/response mapping for `/grantCall` (typed
   refusals, fail closed on unknown shapes) over a new `HttpJson` post port
   (UrlHttp gains a JSON POST; `HttpForm` stays form-only). `Redeem` returns
   and `Callee` retains the ticket secret (in-memory state, encrypted-prefs
   persistence only if a later chunk shows re-dial-after-restart matters).
   No network in tests; the admit fixture is canned JSON from the real
   server's shapes.
2. **M4b — proof acquisition.** Ticket path: the retained secret, no
   identity. Identity path: authed XRPC `com.atproto.server.getServiceAuth`
   against the caller's PDS — new `Xrpc.getServiceAuth(http, accessToken,
   dpopKeyPair, aud, lxm)` with DPoP proof (reuses `Dpop`); **E113 lands
   here**: `AuthManager.freshAccessToken()` refreshes via
   `OAuthFlow.refresh` when the stored token is stale (single-use rotating
   refresh token — persist the NEW pair before first use of the old one is
   possible to lose), called on-foreground and before-mint. The open probe
   rides this chunk: **does OAuth scope `atproto` authorize getServiceAuth?**
   (The croft-stack probe used an app-password session.) If the entryway
   refuses, the fallback is a scope bump in the client metadata — a connect
   Pages change, named here so it is not discovered in a debugger.
3. **M4c — the token reaches the wire.** Mint-at-dial: `dialCallee` becomes
   resolve-proof → mint → re-bind the node with
   `RelayConfig(authToken = token)` → dial. Re-bind = the existing
   stop/start (persisted key ⇒ same EndpointId — assert it, since the mint
   bound the token to that id). **No in-call renewal in v1, by design:** the
   relay verifies the token at attach (`on_connect`) only; an established
   connection outlives token expiry, and the next dial mints fresh. Renewal
   machinery would be speculative.
4. **M4d — validation ladder.** (a) Robolectric: the full mint matrix
   against canned fixtures (every refusal reason surfaces as a distinct,
   honest UI state — "not permitted" ≠ "network failed"). (b) Emulator
   against a LOCAL croft-relay(enforce)+croft-admit pair — the first
   end-to-end enforcement loop, zero production risk. (c) Two devices
   against production (admission=open): the call works exactly as v0.4.0
   and the relay journal shows `admitted sponsorship=…` attribution — the
   observable win. (d) The production enforce flip stays owner-gated and
   OUT of this plan (croft-stack activation prerequisites + open question
   O1 below).

## Reasoning

- **Mint-at-dial, not mint-at-redeem:** the token is EndpointId-bound and
  short-lived; minting early buys nothing and widens the window a revoked
  grant keeps working (the server's whole revocation design is fresh reads
  at mint).
- **Ticket-first again (M4a before M4b):** the ticket path exercises
  mint → token → wire with zero OAuth moving parts, exactly the M1 logic
  that made the ladder debuggable.
- **Local-enforce before production-open (M4d order):** enforcement
  refusals are the risky surface; proving them against a disposable pair
  means production only ever sees the already-proven presentation path.
- **The client treats the token as opaque** (D3): no claim parsing on the
  phone; sponsorship/scope are the relay's business.

## Open questions (named now, owned by a chunk or explicitly deferred)

- **O1 — the callee's own token under enforce** ("mint to both parties",
  D3): admission at attach applies to the callee's camping connection too,
  and `/grantCall` as built returns only the caller's token. Deferred past
  M4's production posture (open mode admits token-less camping); must be
  settled before the production enforce flip. Candidates: the callee mints
  against its own repo with a self-proof; or the mint returns a second
  token the caller relays in-band; or camping admission rides membership
  rather than grants.
- **O2 — scope `atproto` vs getServiceAuth** — resolved by M4b's on-phone
  probe, fallback named there.
- **O3 — per-callee proof storage:** v1 keeps the ticket secret in-memory
  with the Callee card; durable multi-callee cap storage (the "wallet") is
  a later phase with its own privacy design.

## Coordinates

- Server truth: `croft-stack .../src/mint.rs`, `.../tests/mint_binary.rs`.
- Contract: connect `docs/contract.md` §7; handoff `docs/PHASE11-HANDOFF.md`.
- Decisions: D3 record in the tiered-admission plan Review Log (2026-08-19).
- Backlog: ROADMAP_TODO E113 (refresh scheduling — closed by M4b).
