# Phase 11 M3 — identity proof: `provenDid` via atproto OAuth

Plan doc (phase-plan skill, Pass 1). Parent plan:
`plans/2026-08-17-phase11-cap-admission.md` (M3 is its third milestone).

## Problem Statement

The callability engine (M2, shipped `f24e622`) can evaluate identity
matchers — `mutuals` and `registeredCallers` — but the client has no way to
*prove* who the caller is. `CallerContext.provenDid` is always null, so
every identity grant honestly evaluates to MayNotPermit. The live fixtures
are already staged: the two test accounts are mutuals, and the callee repo
carries `m3registered` (naming the caller's DID) and `m3mutuals` grants.
M3 is the flip: obtain a proven DID on-device, feed it to the resolver, and
watch MayNotPermit become Callable.

The handoff (`connect/docs/PHASE11-HANDOFF.md` item 2) specifies the
mechanism: "an atproto **OAuth** session against the caller's PDS. The
engine consumes `provenDid`; obtaining it is client work."

Constraints:
- The app is `croft/android` (Kotlin, Compose, minSdk 26); the caps engine
  is pure Kotlin behind an injected `Http` port (decision D2 of the parent
  plan) and new engine code must keep that shape.
- Secrets discipline: tokens/keys must live in EncryptedSharedPreferences
  (already a dependency, used by `IdentityStore`) and never in logs.
- Each milestone ships as a candidate and validates on-device before
  promote (`ops/RELEASING.md` ladder, proven three times today).

## Reasoning

**Why OAuth and not app passwords.** An app-password `createSession` would
prove account control with far less machinery, and was considered as a
stepping stone ("M3a"). Rejected as the *plan of record* for three reasons:
(1) the contract handoff names OAuth explicitly — the client half should
match the contract owner's stated mechanism, not drift; (2) an app-password
UI trains users to type PDS credentials into a third-party app, which is
the exact habit atproto OAuth exists to end; (3) the stepping stone is
throwaway — none of its code (password UI, session refresh semantics)
survives into the OAuth build, so it buys validation time but pays it back
as deletion. It remains listed as a fallback under Open Questions in case
Phase 0 discovers OAuth is blocked for debug-signed clients.

**Why a hosted client-metadata document on connect.croft.ing.** atproto
OAuth identifies a client by a URL (`client_id` = the URL of a JSON
metadata document the auth server fetches). We need a stable https origin
we control that serves static JSON; `connect.croft.ing` is GitHub Pages,
deployed continuously from connect `main`, and is already the calling
contract's web property — the client metadata is contract-adjacent surface,
so it belongs there (same argument that put the exchange page there).
Alternative rejected: a new bucket/host just for one JSON file.

**Why the engine/effect split continues.** DPoP proof construction and JWT
assembly are pure (given a key and a clock); the token dance (PAR →
authorize → code exchange → refresh) is effects. Mirroring the M1/M2
architecture keeps the pure half TDD-able with RFC test vectors and keeps
the port seam for the future Rust core.

**Why UI surfacing of callability lands here and not in M2.** Callability
only becomes *visibly different* when identity exists — pre-OAuth, every
identity grant shows MayNotPermit and there is nothing to demonstrate. The
parent plan deferred UI surfacing to M3 for exactly this reason.

**What was deliberately kept out of scope.** Multi-account switching,
token revocation UI, `did:web` callers, and using the OAuth session for
*writes* (publishing our own endpoint records from the app — that is its
own milestone). M3 proves identity for *evaluation* only.

## Verified Assumptions

- `CallerContext(provenDid=...)` flips `m3registered`/`m3mutuals` from
  MayNotPermit to Callable — proven by `CallabilityTest` against the same
  record shapes the live fixtures use (croft `f24e622`).
- The AppView reports the two test accounts as mutuals in exactly the shape
  `Evaluate.areMutuals` consumes — probed live 2026-08-17
  (`getRelationships`: both `following` and `followedBy` populated).
- EncryptedSharedPreferences is already in the app and working
  (`IdentityStore` persists the iroh secret key with it).
- connect.croft.ing is GitHub Pages from connect `main`, continuously
  deployed (connect `CLAUDE.md`).
- Everything about the OAuth protocol itself is **deliberately unverified**
  here — that is Phase 0's job. Nothing below Phase 0 may proceed on
  memory of the atproto OAuth spec.

## Documentation Impact

- `croft/plans/2026-08-17-phase11-cap-admission.md` — M3 section gains a
  pointer to this plan now, and a DONE stamp at ship (Phase 4).
- `croft/CHANGELOG.md` — Unreleased entry for M3 (Phase 4, same commit as
  the version bump).
- `croft/ops/RELEASING.md` — Current ledger entry when the candidate is cut
  (Phase 4).
- `connect` repo: new `web/oauth-client-metadata.json` (or equivalent path
  fixed in D4) + a line in connect's docs/README about the file's purpose
  and that croft owns its contents (Phase 1, the phase that creates it —
  cross-repo commit in `connect`, coordinated per its CLAUDE.md).
- `croft/CLAUDE.md` / `README.md` status paragraphs — only at promote, per
  G2 (post-plan, rides with the promote commit).
- Grepped `croft/` and `connect/` for `client-metadata` / `oauth` — no
  existing references to collide with (searched 2026-08-17).

## Concurrency Map

All phases sequential: each phase consumes what the prior produced (D
findings shape Phase 1; the metadata URL from Phase 1 is baked into the
Phase 2 flow; Phase 3a/3b store and surface what Phase 2 obtains; Phase 4
surfaces what 3 stores). No parallel sets declared. Pass 2 considered one
candidate — Phase 1's croft code (Dpop) vs its connect-repo metadata file
have disjoint write-sets across two repos — and kept it sequential: both
halves are small, share the D1 findings as input, and a single-session
executor gains nothing from the split.

## Phases

### Phase 0: Discovery — the OAuth ground truth

**Goal:** Replace every remembered fact about atproto OAuth with a cited
one. No implementation code.

- [ ] **D1: What does the atproto OAuth spec require of a native client?**
  - **Probe:** Read https://atproto.com/specs/oauth (and the linked
    client-implementation guide). Record: required client-metadata fields
    for a native app (`application_type`, `redirect_uris` rules — is a
    custom scheme like `croftcall://oauth` permitted?, `dpop_bound_access_tokens`,
    `grant_types`, `scope` values incl. what grants `transition:generic` vs
    `atproto`), whether PAR is mandatory, whether DPoP is mandatory, token
    lifetimes/refresh rules, and any debug/localhost client provisions.
  - **Success criteria:** A filled-in "client metadata skeleton" in this
    plan with every field's value justified by a spec citation, and a
    yes/no on custom-scheme redirects.
  - **Disposition:** throwaway (notes into Verified Assumptions).
- [ ] **D2: What do the live servers actually serve?**
  - **Probe:** GET the caller account's PDS
    `/.well-known/oauth-protected-resource`, follow to the authorization
    server's `/.well-known/oauth-authorization-server`; record the PAR,
    authorize, and token endpoints plus advertised `dpop_signing_alg_values_supported`
    and `scopes_supported` for a bsky.social-hosted account.
  - **Success criteria:** The endpoint set for `test_user2`'s account
    captured verbatim in this plan.
  - **Disposition:** keep-as-fixture (the JSON responses become test
    fixtures for the flow engine's discovery step).
- [ ] **D3: Can we sign ES256 DPoP proofs with what we already ship?**
  - **Probe:** Confirm `java.security` on Android API 26+ generates P-256
    keypairs and signs SHA256withECDSA; confirm the signature needs
    DER→raw (JOSE) conversion; decide hand-rolled compact JWS vs a JOSE
    dependency by writing a spike that produces one DPoP proof and
    validating its shape against RFC 9449's example structure.
  - **Success criteria:** A spike-produced DPoP JWT whose header/claims
    decode to the RFC 9449 shape, and a named decision (hand-roll vs lib).
  - **Disposition:** promote (the spike signer becomes Phase 1 production
    code under TDD — the spike itself gets no tests).
- [ ] **D4: Where exactly does the client metadata live and does Pages
      serve it right?**
  - **Probe:** Push a draft metadata JSON to a scratch path on connect
    Pages (or verify with an existing JSON file already served); confirm
    content-type and availability at a stable URL; fix the final URL.
  - **Success criteria:** `curl` shows the JSON served 200 from the chosen
    URL (content-type acceptable per D1's spec reading).
  - **Disposition:** promote (the real metadata file ships in Phase 1).

**Done when:** D1–D4 answered with citations/output in this plan; Open
Questions OQ2/OQ3 resolved; later phases adjusted if any answer
contradicts them (recorded in the Review Log).

### Phase 1: DPoP + client metadata (the pure half, and the hosted file)

**Goal:** The cryptographic core exists and is fully tested; the client
metadata document is live at its final URL.
**Changes:**
- [ ] `croft/android/app/src/main/java/ing/croft/call/caps/Dpop.kt` — P-256
  keypair handling + DPoP proof JWS builder (pure: key, url, method, nonce,
  clock in → compact JWS out), shaped per D3's decision.
- [ ] `croft/android/app/src/test/java/ing/croft/call/caps/DpopTest.kt` —
  RED first: header/claims shape per RFC 9449, signature verifies with the
  public key, nonce and htu/htm handling, base64url correctness.
- [ ] `connect/web/oauth-client-metadata.json` (path per D4) — the client
  metadata document, fields per D1; committed in the connect repo with its
  doc line (cross-repo).
**Call chain:** Phase 2's token client → `Dpop.proof(...)`. (Within this
phase the chain terminates in tests; Phase 2 wires it — noted so the
wiring debt is explicit.)
**Wiring test:** Deferred to Phase 2's flow test by design (Dpop is a leaf
library this phase; the Phase 2 wiring test
`OAuthFlowTest.token exchange attaches a DPoP proof` is the wire).
**Depends on:** Phase 0 (D1 fields, D3 decision, D4 URL).
**Read-set:** D-findings in this plan; `Tickets.kt` (hash/base64 idioms).
**Write-set:** the three files above.
**Shared-state contract:** none beyond the write-set (pure code + one
static file; the connect commit touches only the new file + one doc line).
**Risks:** JOSE signature format (DER vs raw) is the classic silent
corruptor — covered by a verify-roundtrip test, not just shape checks.
**Done when:**
1. Behavioral: a DPoP proof built by the engine validates (signature +
   claims) in our own tests, and the metadata URL serves the final JSON.
2. Verification: `./gradlew testDebugUnitTest --tests '*DpopTest'` green;
   `curl` of the metadata URL shows the D1-compliant document.
**Validation:** Narrow-moderate: unit tests + the live curl. No device
needed yet.

### Phase 2: The OAuth flow engine (PAR → authorize → tokens)

**Goal:** Given a handle, the engine can run the full authorization dance
up to holding DPoP-bound tokens — with the browser hop abstracted so tests
cover everything except the human tap.
**Changes:**
- [ ] `caps/OAuthFlow.kt` — server discovery (protected-resource →
  auth-server metadata, D2 fixtures), PAR request, authorize-URL builder,
  code+PKCE exchange, refresh.
- [ ] `caps/HttpForm.kt` — a **separate** POST port (Pass 2 finding: `Http`
  is a single-method `fun interface` and cannot grow a second method; and
  the GET port's body-only return is insufficient here because the DPoP
  server nonce arrives in the **`DPoP-Nonce` response header**, including
  on error responses). Shape: `suspend fun postForm(url, fields, headers):
  FormResponse(status, headers, body)` — status and headers are part of
  the contract, and a non-2xx must NOT throw (the nonce-retry dance reads
  400s).
- [ ] `caps/OAuthFlowTest.kt` — RED first, canned-route fakes from D2
  fixtures: discovery chain, PAR carries the metadata client_id + PKCE,
  authorize URL shape, token exchange attaches a DPoP proof (the Phase 1
  wiring test), DPoP-nonce retry, refresh path, fail-closed on every
  mismatch (issuer, state, missing fields).
- [ ] `net/UrlHttp.kt` — implement `HttpForm` alongside the existing GET
  (returns status+headers without throwing on non-2xx, per the port
  contract above).
**Call chain:** Phase 3's `AuthManager` → `OAuthFlow.*`; browser hop:
`OAuthFlow.authorizeUrl(...)` → Custom Tab (Phase 3) → redirect intent →
`OAuthFlow.exchangeCode(...)`.
**Wiring test:** `OAuthFlowTest` end-to-end fake run: discovery → PAR →
exchange → tokens, one test walking the whole chain over canned routes.
**Depends on:** Phase 1 (Dpop), Phase 0 (D2 endpoints).
**Read-set:** `Dpop.kt`, `Http` port, D2 fixtures.
**Write-set:** the three files above.
**Shared-state contract:** none beyond the write-set (tests use fakes; no
live network in unit tests).
**Risks:** DPoP server nonces (RFC 9449 §8) force a retry dance — modeled
explicitly, not as an afterthought; issuer/audience confusion between PDS
and entryway — pinned by fail-closed tests from D2's real shapes.
**Done when:**
1. Behavioral: the full dance succeeds against canned servers, producing
   DPoP-bound tokens and the session DID.
2. Verification: `./gradlew testDebugUnitTest --tests '*OAuthFlowTest'`
   green, including the whole-chain test.
**Validation:** Moderate: unit tests here; the live-server proof is
Phase 3's on-device sign-in (deliberately not faked there).

### Phase 3a: AuthManager + redirect capture (no UI yet)

**Goal:** The app can run the whole dance headlessly: launch the browser,
capture the redirect, hold tokens and the DID durably.
**Changes:**
- [ ] `identity/AuthManager.kt` — orchestrates OAuthFlow with `UrlHttp`,
  opens the authorize URL via a plain `ACTION_VIEW` intent (Pass 2
  decision: no Custom Tabs dependency — the app has no `androidx.browser`
  dep today and the default browser suffices; upgrade later if UX
  demands), stores tokens + DID in EncryptedSharedPreferences (same
  pattern as `IdentityStore`), exposes `provenDid: StateFlow<String?>`
  and sign-out.
- [ ] `MainActivity.kt` — route the redirect intent to `AuthManager`
  (alongside the existing invite/deep-link routing).
- [ ] `AndroidManifest.xml` — a **new** intent-filter for the redirect URI
  (Pass 2 finding: the existing filter is `croftcall://call` only — a
  `croftcall://oauth` callback needs its own `host` entry; exact value
  per D1/OQ2).
**Call chain:** AuthManager.signIn(handle) → OAuthFlow(UrlHttp) PAR →
ACTION_VIEW browser → user approves → redirect intent → MainActivity →
AuthManager.onRedirect → OAuthFlow.exchangeCode → EncryptedSharedPreferences
→ `provenDid` flow.
**Wiring test:** Robolectric: a synthesized redirect intent reaches
AuthManager and drives the (canned-route) exchange to a stored DID.
**Depends on:** Phases 1–2; the live metadata URL (Phase 1).
**Read-set:** OAuthFlow, Dpop, IdentityStore, manifest.
**Write-set:** `identity/AuthManager.kt`, `MainActivity.kt`,
`AndroidManifest.xml` (+ its Robolectric test file).
**Shared-state contract:** device browser + live PDS at on-device time;
tokens land only in EncryptedSharedPreferences; **never log tokens — log
DIDs only.**
**Risks:** redirect capture with singleTask (the app already runs
singleTask — the good case); clock skew on DPoP `iat`.
**Done when:**
1. Behavioral: on a real phone (driven via adb, no UI), `signIn` for the
   test handle round-trips the browser and `provenDid` is populated and
   survives process restart.
2. Verification: Robolectric routing test green + the on-device headless
   run's logcat showing the DID (never a token).
**Validation:** Broad: on-device live OAuth against the real PDS with
`test_user2` (creds in `CroftC/.env`), kill-and-relaunch included.

### Phase 3b: Sign-in surfaced in the UI

**Goal:** A user can do it with taps: handle in, browser approve, identity
visible, sign-out available.
**Changes:**
- [ ] `MainViewModel.kt` — sign-in/out entry points delegating to
  AuthManager; exposes `provenDid`.
- [ ] `ui/CallScreen.kt` — sign-in row on the This-device card (handle
  field → "Sign in via browser"; signed-in state shows the DID + sign
  out).
**Call chain:** CallScreen tap → MainViewModel → AuthManager (Phase 3a
chain) → `provenDid` flow → CallScreen shows the identity.
**Wiring test:** On-device: sign in as `test_user2` from the UI on the
Pixel; the This-device card shows their DID after the browser round-trip.
**Depends on:** Phase 3a.
**Read-set:** AuthManager, CallScreen.
**Write-set:** `MainViewModel.kt`, `ui/CallScreen.kt`.
**Shared-state contract:** as Phase 3a (device browser + live PDS).
**Risks:** minimal — thin UI over the proven 3a chain.
**Done when:**
1. Behavioral: the tap-through sign-in works on the phone, DID visible.
2. Verification: the on-device UI run + full unit suite green.
**Validation:** Moderate: the on-device tap-through is the check; engine
risk was burned down in 3a.

### Phase 4: The flip — callability surfaced, validated, cut

**Goal:** The proven DID feeds the resolver, callability becomes visible in
the UI, and the fixtures demonstrate MayNotPermit → Callable on-device.
**Changes:**
- [ ] `MainViewModel.kt` — the callee card resolves callability lazily on
  arrival (D1 decision honoured: user action = the link/lookup, cached via
  `CallabilityCache`) with `CallerContext(provenDid = authManager.provenDid)`.
  Pass 2 finding: nothing constructs a `CallabilityCache` yet — the
  ViewModel owns one instance (TTL default **5 min**; identity-keyed
  entries already prevent cross-identity leakage, so sign-in/out needs no
  cache flush — a stale other-identity entry can never be read).
- [ ] `ui/CallScreen.kt` — callability line on the callee card
  ("callable via grant …", "may not permit", "not listed").
- [ ] `CHANGELOG.md` + `ops/RELEASING.md` + parent plan M3 stamp +
  `versionCode 4` / `versionName 0.4.0` (docs ride the phase that makes
  them stale).
**Call chain:** deep link / redeem → MainViewModel → Callability.resolve
(CallabilityCache, CallerContext with real provenDid) → CallScreen line.
**Wiring test:** On-device with the live fixtures: signed **out**, a link
to the callee shows "may not permit"; signed **in** as `test_user2`, the
same lookup shows callable (via `m3registered` or `m3mutuals`) — the flip,
observed on the phone. Robolectric test for the ViewModel wiring.
**Depends on:** Phase 3b (UI sign-in state is what the flip demonstrates
against).
**Read-set:** Callability, CallabilityCache, AuthManager.
**Write-set:** the files above.
**Shared-state contract:** live PDS reads on device; nothing else.
**Risks:** cache keying across sign-in/out (covered by CallabilityCacheTest
identity-keying already); AppView rate limits (lookups are lazy+cached).
**Done when:**
1. Behavioral: the on-device flip described in the wiring test, both
   directions (sign out → MayNotPermit again after TTL/cache clear).
2. Verification: on-device run + full `./gradlew testDebugUnitTest` green;
   then cut `v0.4.0-rc.1` per RELEASING with the validation noted.
**Validation:** Broad: the on-device flip with live records is the
milestone's acceptance gate, exactly like M1's redeem run.

## Open Questions

- [CONFIRMED: BLOCKING — RESOLVED 2026-08-17: straight OAuth, no
  app-password stepping stone (owner).] **OQ1.** *Fallback re-enters only
  if Phase 0 finds OAuth unworkable for a debug-signed native client.*
- [CONFIRMED: PHASE-GATED (Phase 1)] **OQ2 — redirect URI scheme:
  custom scheme (`croftcall://oauth`) vs https App Link.** *Confirmed
  2026-08-17: decided by D1's spec citation before Phase 1; custom scheme
  preferred if permitted (avoids the deferred assetlinks dependency).*
- [CONFIRMED: PHASE-GATED (Phase 1)] **OQ3 — client metadata URL/path on
  connect.croft.ing.** *Confirmed 2026-08-17: D4 fixes it concretely
  before Phase 1; the connect-repo commit is coordinated per its
  CLAUDE.md.*
- [CONFIRMED: ADVISORY] **OQ4 — scope string.** *Confirmed 2026-08-17:
  whether `atproto` alone suffices for identity-only or
  `transition:generic` is needed; D1/D2 will say. Either answer fits the
  same flow code.*

## Review Log

- 2026-08-17 Pass 1: initial plan (this document). All four open questions
  walked through and confirmed with the owner; OQ1 resolved (straight
  OAuth).

### Pass 2: Gap Analysis — 2026-08-17
**Found:**
- `Http` is a single-method `fun interface` — it cannot grow a POST
  method, and its body-only, throw-on-error contract is wrong for the
  token endpoints: the DPoP server nonce arrives in the `DPoP-Nonce`
  **response header**, including on 400 responses the flow must read.
- Phase 3's write-set had five entries — over the 4-file hard rule.
- The manifest's existing filter is `croftcall://call` only; the OAuth
  redirect needs its own host entry (was implied, now explicit).
- No `androidx.browser` (Custom Tabs) dependency exists; decided plain
  `ACTION_VIEW` to the default browser, no new dependency.
- Nothing constructs a `CallabilityCache`; Phase 4 now names the owner
  (ViewModel), the TTL default (5 min), and why sign-in/out needs no
  flush (identity-keyed entries).
**Concurrency:**
- Map re-confirmed sequential after the 3a/3b split; one parallel
  candidate (Phase 1's two-repo halves) considered and declined with
  reasons in the map.
**Changed:**
- Phase 2 gains `caps/HttpForm.kt` (separate POST port returning
  status+headers+body, non-throwing) and `UrlHttp` implements it.
- Phase 3 split into 3a (AuthManager + redirect capture + manifest,
  headless on-device validation) and 3b (UI sign-in row). Phase 4's
  dependency updated to 3b.
**Confirmed:**
- IdentityStore really is EncryptedSharedPreferences (read
  `identity/IdentityStore.kt`) — the AuthManager token-storage pattern
  has a live precedent.
- MainActivity is singleTask with intent routing already centralized in
  `route()` — the redirect capture slots into an existing seam.
- The engine/effect split and fixtures claims in Verified Assumptions
  held against the code as committed (`f24e622`).
