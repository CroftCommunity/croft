# Phase 11 M3 — identity proof: `provenDid` via atproto OAuth

Plan doc (phase-plan skill — passes 1–3 complete, ready for execution).
Parent plan:
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

### Phase 0: Discovery — the OAuth ground truth — ✅ DONE 2026-08-17

**Findings (all four probes ran live; evidence below, fixtures committed
with this plan update; the connect-side draft is connect `ac9d022`):**

- **D1 (spec, https://atproto.com/specs/oauth):** `client_id` is the URL
  of a hosted metadata JSON. Required for a native public client:
  `application_type: "native"`, `grant_types:
  ["authorization_code","refresh_token"]`, `response_types: ["code"]`,
  `token_endpoint_auth_method: "none"`, `dpop_bound_access_tokens: true`,
  scope must include `atproto`. **PAR is mandatory** ("clients of all
  types must use PAR") and **DPoP is mandatory**, ES256 required of all
  clients/servers. Custom-scheme redirects: **yes, but the scheme must be
  the client_id hostname in reverse-domain order**, followed by a single
  colon, single slash, and a path — for `connect.croft.ing` that is
  `ing.croft.connect:/oauth`. The plan's sketched `croftcall://oauth` is
  not spec-legal; Phase 3a adjusted below. Access tokens < 30 min;
  refresh tokens single-use rotating; public-client session ≤ 2 weeks.
  For identity-only, `atproto` alone suffices (resolves OQ4).
- **D2 (live servers):** test_user2's PDS is
  `https://fibercap.us-west.host.bsky.network`; its
  `/.well-known/oauth-protected-resource` names auth server
  `https://bsky.social`, whose metadata advertises
  `pushed_authorization_request_endpoint: /oauth/par`,
  `authorization_endpoint: /oauth/authorize`, `token_endpoint:
  /oauth/token`, `require_pushed_authorization_requests: true`, ES256 in
  `dpop_signing_alg_values_supported`, S256 PKCE,
  `client_id_metadata_document_supported: true`, scopes incl. `atproto`.
  Responses saved verbatim as fixtures:
  `android/app/src/test/resources/oauth/oauth-protected-resource.json`
  and `.../oauth-authorization-server.json`.
- **D3 (DPoP spike, desktop JVM, scratchpad — throwaway; findings
  promote):** `java.security` alone builds a valid ES256 DPoP JWT —
  P-256 keygen, `SHA256withECDSA`, hand-rolled DER→raw(64) conversion,
  compact JWS with unpadded base64url; signature roundtrip-verifies.
  **Decision: hand-roll, no JOSE dependency.** The left-pad branch is
  empirically live: a 31-byte `r` appeared within 10 random signatures —
  the Pass 3 fixed-vector edge test is not theoretical. EC coordinates
  in the jwk also need 32-byte normalization (BigInteger sign bytes).
- **D4 (hosting):** connect Pages deploys `web/` as the site root
  (`.github/workflows/web.yml`); the draft metadata is live at the final
  URL **`https://connect.croft.ing/oauth-client-metadata.json`**, served
  `200` with `content-type: application/json; charset=utf-8` (resolves
  OQ3). Draft committed as connect `ac9d022`; Phase 1 finalizes the
  fields and adds the doc line.

Original probe specs below, kept for the record.

**Goal:** Replace every remembered fact about atproto OAuth with a cited
one. No implementation code.

- [x] **D1: What does the atproto OAuth spec require of a native client?**
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
- [x] **D2: What do the live servers actually serve?**
  - **Probe:** GET the caller account's PDS
    `/.well-known/oauth-protected-resource`, follow to the authorization
    server's `/.well-known/oauth-authorization-server`; record the PAR,
    authorize, and token endpoints plus advertised `dpop_signing_alg_values_supported`
    and `scopes_supported` for a bsky.social-hosted account.
  - **Success criteria:** The endpoint set for `test_user2`'s account
    captured verbatim in this plan.
  - **Disposition:** keep-as-fixture (the JSON responses become test
    fixtures for the flow engine's discovery step).
- [x] **D3: Can we sign ES256 DPoP proofs with what we already ship?**
  - **Probe:** Confirm `java.security` on Android API 26+ generates P-256
    keypairs and signs SHA256withECDSA; confirm the signature needs
    DER→raw (JOSE) conversion; decide hand-rolled compact JWS vs a JOSE
    dependency by writing a spike that produces one DPoP proof and
    validating its shape against RFC 9449's example structure.
  - **Success criteria:** A spike-produced DPoP JWT whose header/claims
    decode to the RFC 9449 shape, and a named decision (hand-roll vs lib).
  - **Disposition:** promote (the spike signer becomes Phase 1 production
    code under TDD — the spike itself gets no tests).
- [x] **D4: Where exactly does the client metadata live and does Pages
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

### Phase 1: DPoP + client metadata — ✅ SHIPPED (`0e91c1b`; connect `ac9d022`+`5c46fd4`)

**Goal:** The cryptographic core exists and is fully tested; the client
metadata document is live at its final URL.
**Changes:**
- [x] `croft/android/app/src/main/java/ing/croft/call/caps/Dpop.kt` — P-256
  keypair handling + DPoP proof JWS builder (pure: key, url, method, nonce,
  clock in → compact JWS out), shaped per D3's decision.
- [x] `croft/android/app/src/test/java/ing/croft/call/caps/DpopTest.kt` —
  RED first: header/claims shape per RFC 9449, signature verifies with the
  public key, nonce and htu/htm handling, base64url correctness (no
  padding, url-safe alphabet). **Edges (Pass 3):** the DER→raw conversion
  must be tested with a *fixed* keypair/vector whose `r` or `s` needs
  left-padding to 32 bytes — a random-key sign/verify roundtrip only
  exercises that branch ~1 run in 256, so a broken pad survives as a
  flaky pass, the worst kind of mutation survivor.
- [x] `connect/web/oauth-client-metadata.json` (path per D4) — the client
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
2. Verification: full `./gradlew testDebugUnitTest` green (Pass 3: the
   suite is 67 tests and cheap — run all of it so a regression surfaces
   in the phase that caused it, not two phases later); `curl` of the
   metadata URL shows the D1-compliant document.
**Validation:** Narrow-moderate: unit tests + the live curl. No device
needed yet.

### Phase 2: The OAuth flow engine (PAR → authorize → tokens) — ✅ SHIPPED (`5e0eb15`)

**Goal:** Given a handle, the engine can run the full authorization dance
up to holding DPoP-bound tokens — with the browser hop abstracted so tests
cover everything except the human tap.
**Changes:**
- [x] `caps/OAuthFlow.kt` — server discovery (protected-resource →
  auth-server metadata, D2 fixtures), PAR request, authorize-URL builder,
  code+PKCE exchange, refresh.
- [x] `caps/HttpForm.kt` — a **separate** POST port (Pass 2 finding: `Http`
  is a single-method `fun interface` and cannot grow a second method; and
  the GET port's body-only return is insufficient here because the DPoP
  server nonce arrives in the **`DPoP-Nonce` response header**, including
  on error responses). Shape: `suspend fun postForm(url, fields, headers):
  FormResponse(status, headers, body)` — status and headers are part of
  the contract, and a non-2xx must NOT throw (the nonce-retry dance reads
  400s).
- [x] `caps/OAuthFlowTest.kt` — RED first, canned-route fakes from D2
  fixtures: discovery chain, PAR carries the metadata client_id + PKCE,
  authorize URL shape, token exchange attaches a DPoP proof (the Phase 1
  wiring test), DPoP-nonce retry, refresh path, fail-closed on every
  mismatch (issuer, state, missing fields). **Edges (Pass 3):** the nonce
  retry must be *bounded* — exactly one retry per nonce challenge, then
  the error surfaces (a test asserting only "retries on 400+nonce" would
  let a mutated infinite-retry loop pass; assert the second consecutive
  400 is raised, not retried).
- [x] `net/UrlHttp.kt` — implement `HttpForm` alongside the existing GET
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
2. Verification: full `./gradlew testDebugUnitTest` green, including the
   whole-chain OAuthFlowTest (Pass 3: full suite per phase, same reason
   as Phase 1).
**Validation:** Moderate: unit tests here; the live-server proof is
Phase 3's on-device sign-in (deliberately not faked there).

### Phase 3a: AuthManager + redirect capture — ✅ SHIPPED (`2125da5`)

**Delivered:** as specified, with two recorded deviations. (1) The
on-device browser round-trip rides with Phase 3b's validation — the
plan's headless `signIn` drive would have required a dev-only intent
trigger that 3b's UI deletes immediately; one on-device run at the 3b
boundary validates both (taps for sign-in, kill-and-relaunch for
persistence). (2) `MainViewModel.kt` gained the minimal plumbing
(AuthManager ownership + `onOAuthRedirect`) here rather than 3b —
MainActivity cannot route to an AuthManager nobody owns. Prefs are
injected (plain in tests, encrypted in production) because
AndroidKeyStore is absent under Robolectric.

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
- [ ] `AndroidManifest.xml` — a **new** intent-filter for the redirect URI.
  **Phase 0 correction:** the spec ties the custom scheme to the
  client_id hostname in reverse-domain order, so the redirect is
  `ing.croft.connect:/oauth` — scheme `ing.croft.connect`, *not*
  `croftcall`. Note the URI has no authority (`:` + `/`, no `//`), and
  Android path filters only apply when an authority is present — so the
  filter matches on scheme alone and `route()` checks the path.
- [ ] `identity/AuthManagerTest.kt` (Robolectric) — RED first: the wiring
  test below, plus token persistence round-trip (store → new AuthManager
  instance → `provenDid` restored) and sign-out clears it. (Pass 3: this
  file was described in prose but missing from Changes — the test is a
  deliverable, not a footnote. Robolectric 4.14.1 confirmed present in
  `build.gradle.kts`.)
**Logging (Pass 3):** the OAuthFlow engine stays pure (throws carry the
reason; no `android.util.Log` in `caps/`); AuthManager, at the effect
edge, logs each milestone at the existing `CroftCall` TAG — sign-in
started (handle), PAR accepted, browser launched, redirect received,
exchange succeeded (DID only), refresh ran, and every failure at WARN
with the thrown reason. Tokens, codes, and PKCE verifiers never appear
in a log line. This is what makes the on-device Done-when debuggable:
a stalled sign-in shows *which* hop died in `adb logcat -s CroftCall`.
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
- [ ] `MainViewModelCallabilityTest.kt` (Robolectric) — RED first: the
  ViewModel wiring — a callee arrival triggers exactly one resolve over
  canned routes, a repeat inside the TTL hits the cache (no second
  network pass), and the signed-in vs signed-out context produces the
  two different derived states. (Pass 3: named as a Changes checkbox;
  was only prose in the wiring-test field.)
- [ ] `CHANGELOG.md` + `ops/RELEASING.md` + parent plan M3 stamp +
  `versionCode 4` / `versionName 0.4.0` (docs ride the phase that makes
  them stale).
**Logging (Pass 3):** the ViewModel logs each resolution outcome at the
`CroftCall` TAG — principal, derived state, grant rkey when Callable,
and cache hit vs miss — mirroring the existing `redeemInvite` log
lines. This is device-local logcat, not telemetry; it is how the
on-device flip is *read* during validation.
**Call chain:** deep link / redeem → MainViewModel → Callability.resolve
(CallabilityCache, CallerContext with real provenDid) → CallScreen line.
**Wiring test:** On-device with the live fixtures: signed **out**, a link
to the callee shows "may not permit"; signed **in** as `test_user2`, the
same lookup shows callable (via `m3registered` or `m3mutuals`) — the flip,
observed on the phone. Robolectric test for the ViewModel wiring.
**Depends on:** Phase 3b (UI sign-in state is what the flip demonstrates
against).
**Read-set:** Callability, CallabilityCache, AuthManager.
**Write-set:** the files above (code: `MainViewModel.kt`,
`ui/CallScreen.kt`, `MainViewModelCallabilityTest.kt`; the rest are the
doc/version items — within the 4-code-file rule).
**Shared-state contract:** live PDS reads on device; nothing else.
**Risks:** cache keying across sign-in/out (covered by CallabilityCacheTest
identity-keying already); AppView rate limits (lookups are lazy+cached).
**Done when:**
1. Behavioral: the on-device flip described in the wiring test, both
   directions. (Pass 3 correction: sign-out flips back *immediately*, no
   TTL wait — the signed-out lookup uses a different cache key (empty
   provenDid), so it can never read the signed-in entry. The earlier
   "after TTL/cache clear" wording contradicted the plan's own
   identity-keyed cache design.)
2. Verification: on-device run + full `./gradlew testDebugUnitTest` green;
   then cut `v0.4.0-rc.1` per RELEASING with the validation noted.
**Validation:** Broad: the on-device flip with live records is the
milestone's acceptance gate, exactly like M1's redeem run.

## Open Questions

- [CONFIRMED: BLOCKING — RESOLVED 2026-08-17: straight OAuth, no
  app-password stepping stone (owner).] **OQ1.** *Fallback re-enters only
  if Phase 0 finds OAuth unworkable for a debug-signed native client.*
- [RESOLVED 2026-08-17 by D1] **OQ2 — redirect URI scheme.** Custom
  scheme permitted and chosen, but the spec fixes its value: the
  client_id hostname reversed — `ing.croft.connect:/oauth`. No
  assetlinks dependency.
- [RESOLVED 2026-08-17 by D4] **OQ3 — client metadata URL.**
  `https://connect.croft.ing/oauth-client-metadata.json`, live and
  serving `application/json` (draft: connect `ac9d022`).
- [RESOLVED 2026-08-17 by D1] **OQ4 — scope string.** `atproto` alone —
  the spec says identity-only clients request just the base scope;
  `transition:generic` is the app-password-equivalent write scope M3
  does not need.

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

### Pass 3: Quality Gates — 2026-08-17
**TDD ordering:**
- Phases 3a and 4 described Robolectric wiring tests in prose but did not
  carry them as Changes checkboxes; both now name their test file as a
  RED-first deliverable (`AuthManagerTest.kt`, `MainViewModelCallabilityTest.kt`).
- Mutation-resistance edges named where a happy-path assertion would
  survive a one-line break: Phase 1's DER→raw conversion needs a fixed
  vector with a left-pad-required `r`/`s` (random keys hit that branch
  ~1/256 — a broken pad would pass flakily); Phase 2's nonce retry must
  assert the bound (one retry, then the second 400 raises).
- Phase 1's deferred wiring test (the Dpop leaf is wired by Phase 2's
  `token exchange attaches a DPoP proof`) reviewed and kept — it is
  declared, named, and lands one phase later, not never.
**Observability:**
- Positive logging plans added to 3a (AuthManager milestone lines at the
  existing `CroftCall` TAG; failures at WARN; tokens/codes/verifiers
  never logged) and 4 (resolution outcome + cache hit/miss). `caps/`
  stays pure — no `android.util.Log` in the engine; logging lives at the
  effect edge, matching `redeemInvite`'s existing pattern.
**Debugging readiness:**
- Checkpoints are the per-phase commits plus `adb logcat -s CroftCall`;
  each on-device Done-when now has log lines that localize which hop of
  the dance failed. No state file needed — the phases are small and each
  ends green.
**Validation calibration:**
- Phase 1/2 verification widened from targeted `--tests` runs to the full
  unit suite (67 tests, cheap) so regressions surface in the phase that
  caused them.
- Phase 0 dispositions all declared and each `promote` has its named TDD
  follow-up (D3→Phase 1 Dpop, D4→Phase 1 metadata). Considered resolving
  D1 (spec read) during planning; kept in Phase 0 — D1's citations need
  recording alongside D2's live confirmation, Phase 0 is the immediate
  next step anyway, and splitting it saves nothing.
**Concurrency honesty:**
- Map confirmed; sequential plan. Pass 3's additions put test files into
  their own phases' write-sets — no files moved between phases, no new
  parallel candidates.
**Coherence:**
- One self-contradiction fixed: Phase 4's Done-when said the sign-out
  flip appears "after TTL/cache clear", but the identity-keyed cache
  means the signed-out lookup is a different key — the flip back is
  immediate. Wording corrected; no design change.
- Scope unchanged since Pass 2; the plan still answers the Problem
  Statement (flip MayNotPermit→Callable via a proven DID).
**Documentation impact:**
- Section verified complete: every listed file has a phase item, doc
  updates ride the phase that makes them stale, no trailing docs phase,
  no renames. Spot-checked `versionCode` (currently 3) — the Phase 4
  bump to 4/0.4.0 is correct.
**Spot-checks:**
- Robolectric 4.14.1 + androidx.test:core present in `build.gradle.kts`
  (the 3a/4 wiring tests are runnable as planned); `Log.i/w("CroftCall", …)`
  confirmed as the live logging convention in `MainViewModel.kt`.
**Confirmed ready:** yes — all open questions carry user-confirmed
severities from Pass 1 (OQ1 resolved; OQ2/OQ3 phase-gated on Phase 0;
OQ4 advisory). Execution starts with Phase 0.

### Phase 0 close — 2026-08-17
All four probes ran live; findings recorded in the Phase 0 section and
every open question is now resolved. One plan correction: the redirect
scheme is `ing.croft.connect:/oauth` (spec ties custom schemes to the
client_id host reversed) — Phase 3a's manifest item updated; the change
is contained to that one value, no restructuring. Dispositions honored:
D1 notes in-plan (throwaway), D2 responses committed as fixtures under
`android/app/src/test/resources/oauth/` (keep-as-fixture), D3 spike ran
in the session scratchpad and only its findings promote (the Phase 1
`Dpop.kt` is written TDD-first from scratch), D4's draft metadata is
live at the final URL (promote — Phase 1 finalizes fields + doc line).
