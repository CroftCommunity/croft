# Plan — the session state: proven, stored, unconfirmed, dead (E135(b) + E113)

**Status:** PROPOSED 2026-09-28 — awaiting owner review. No code before the decisions in
§ *Decisions for the owner* are answered; two of them change what Phase 1 builds.

**Source rows:** triage queue row 5 (`discovery/alpha/plans/2026-09-14-plan-workspace-triage-queue.md`,
E135(b) as reframed 2026-09-28); croft `TODO.md` "A dead OAuth refresh token still reads
`Signed in`" and "Schedule the OAuth refresh so an idle phone keeps its session (E113)";
roadmap E135 (its (b) half was annotated as a wording decision; the 09-28 reframe
supersedes that). Ordered **before the v0.6.0 cut** (triage row 2), so the release carries it.

**Naming note:** no `-N` ordinal, per `CroftC/.claude/TRACKING.md`.

## Problem Statement

**The Android app has one bit of session state and renders it as a claim.**
`AuthManager.provenDid` is non-null whenever credentials are *stored*, and `CallScreen.kt`
renders any non-null DID as `Signed in`. Nothing ever clears it except the user tapping
Sign out. A refresh the provider refuses is logged and dropped
(`MainViewModel.onForeground`: `runCatching { … }.onFailure { Log.w(…) }`), and the same
refusal at camp time becomes a camp note, not a session fact.

Observed on hardware three times (§15.2 at ~11 days idle, §16 at 6 days, and the §17
control alive at 7): the account card read `Signed in` with the right DID while the camp
line read "NOT camped … calls cannot reach this device", and logcat carried
`invalid_grant "Invalid refresh token"`. Both lines were individually true, and together
they told the user nothing actionable. Re-sign-in became step 0 of every device run because
**the screen cannot tell a live session from a dead one**.

Two further gaps sit under the visible one:

1. **A failure is never classified.** An outage, a client defect, a lost single-use rotation
   race, and a genuine provider refusal all arrive as one `Throwable` with a message.
   Anything that renders "dead" from that without classifying it would lie the other way,
   because an outage is not an authorization answer.
2. **The session cannot outlive two weeks, whatever the client does** (see *Findings*). E113
   assumed a scheduled refresh would keep an idle phone signed in. For a public client the
   provider's source says it cannot.

## Findings (read before designing — each cited, none inferred)

### F1 — the provider's lifetimes for OUR client (read from source 2026-09-28)

Source: `bluesky-social/atproto` main at `b3d7c33`, `packages/oauth/oauth-provider/src/`.

- **Our client is public.** `connect.croft.ing/oauth-client-metadata.json` says
  `"token_endpoint_auth_method": "none"`, and `client.ts` defines
  `isConfidential = token_endpoint_auth_method !== 'none'`.
- **Public-client lifetimes** (`oauth-constants.ts`): `SESSION_LIFETIME = 2 * WEEK` and
  `REFRESH_LIFETIME = 2 * WEEK`. A confidential client (or a trusted first party) gets
  `SESSION_LIFETIME_EXTENDED = 2 * YEAR` and `REFRESH_LIFETIME_EXTENDED = 3 * MONTH`.
- **Both limits are enforced at refresh** (`oauth-provider.ts` `validateRefreshGrant`):
  `sessionAge > client.sessionLifetime` gives `invalid_grant "Session expired"`, and
  `refreshAge > client.refreshLifetime` gives `invalid_grant "Refresh token expired"`.
  `sessionAge` is measured from the session's `createdAt`, which is the sign-in.
- **Consequence:** for a public client the refresh limit can never trip before the session
  limit, because the time since the last refresh is always ≤ the time since sign-in and
  both limits are 14 days. **A scheduled refresh on the phone buys nothing against these
  rules.** The ceiling is 14 days from sign-in, however often we refresh.
- **The AS metadata advertises none of this.** `bsky.social/.well-known/oauth-authorization-server`
  has no lifetime field (read 2026-09-28). The numbers exist only in source.

### F2 — our recorded deaths are NOT explained by F1 (a cause that fits is not the evidence)

Every recorded death (runbook §15.2, §16) reads `"error_description":"Invalid refresh token"`.
In current source that text comes from `TokenManager.consumeRefreshToken` when **no token
record matches**, not from the lifetime checks, which say `Session expired` / `Refresh token
expired`. So the record was already gone when we asked. Candidates, none established:

- **A replay deleted it.** `token-manager.ts:367`: presenting a non-current refresh token
  deletes the whole token record (`Refresh token replayed`). Every later attempt then reads
  `Invalid refresh token`. The function's own comment says so: "any future refresh attempts
  from outdated tokens will cause the entire session to be invalidated". One way we could replay: `AuthManager.freshAccessToken()`
  persists the rotated pair with `SharedPreferences.apply()`, which is **asynchronous to
  disk**. Its doc comment claims the pair is "persisted before this returns", and that
  holds only in memory. A process death inside that window would leave the spent token
  stored, so the next refresh is a replay.
- **The provider purged an expired record** before we asked, or `bsky.social` runs a
  different provider version than main.

Phase 1 records `signed_in_at` for other reasons (the horizon, D3). That gives the next
death its age for free, and Phase 4 switches the rotation write to `commit()`. The soak
the TODO row asked for becomes unnecessary: F1 bounds the lifetime from source, and the
next observed death, with its age and description, tests F2.

### F3 — the login broker exists, is live, and is the answer to F1 (owner, 2026-09-28)

croft-stack `broker/` (`croft-broker`) is a **confidential** atproto OAuth client
(`private_key_jwt`, ES256) at `https://account.croft.ing`. It was probed 2026-09-28:
`/healthz` returned `ok`, and `/client-metadata.json` was served with
`token_endpoint_auth_method: private_key_jwt`. It refreshes server-side, and the tokens
never leave it: a client holds only an opaque **ticket**. Under F1 a confidential client
gets the extended lifetimes, which solves the 2-week ceiling. That is what it was built for
(`croft-stack/docs/auth-helper.md`).

What it does NOT yet do for this app, read from its source rather than assumed:

- **No service-auth endpoint.** Its API is `/login`, `/callback` and `/api/whoami`. Every
  camp and dial mint needs a `getServiceAuth` JWT (`aud=did:web:admit.croft.ing`, the
  `campToken`/`grantCall` lxm). The broker has the generic `pds_authed_get` internally but
  exposes nothing that returns one.
- **Its errors cannot be classified.** A refused upstream refresh surfaces as `Error::Jwk`
  and becomes HTTP **502**, the same status as an upstream outage (`server.rs` `err_status`).
  An unknown ticket is 400. A client could not tell "your session is dead" from "the
  provider is down", which is exactly the distinction this plan exists to make.
- **The return path is web-shaped.** The callback is `https://account.croft.ing/callback`
  with an allowlisted return-URL prefix. A native app needs a return it can catch (an app
  link, or the `ing.croft.connect:` scheme), and whether the allowlist admits a custom
  scheme is unverified.
- **Hardening H1–H8 is open**, and the doc says it is "required before real users are behind
  the broker". **H8 — "no pad may require the broker"** makes the public client a permanent
  floor. So the dead-session handling in this plan is required with the broker, not replaced
  by it (owner, 2026-09-28: "the problem should still be handled gracefully").

This is an **undeclared edge** (`CroftC/.claude/ARCHITECTURE.md`): croft would depend on
a croft-stack service its card does not name. It is surfaced here, not built across. See D3.

### F4 — the rules half-exist already

- `ports/call-session/src/session.rs` has `decide(stored, now) -> Step {SignIn, Use, Refresh}`,
  `RefreshFailure {Dead, Unavailable}`, and `refusal_words` ("a dead session … never says
  'signed in'; an outage … does not say 'dead'").
- `ports/call-session/src/steps.rs` has a **private** `SessionState {None, Stored, Proven, Dead}`
  with words. That is the state machine this plan names, trapped in a port where Android
  cannot reach it.
- `call-session/src/atproto.rs` classifies **every** 400/401/403 as `Dead`. That is too broad:
  a malformed request is our defect, not the provider's verdict.
- The FFI already carries `call-core`'s camp and dial rules to Kotlin (`ffi/src/rules.rs`,
  D3.2), and the Kotlin objects delegate. That pattern is how the new rules reach Android.

## Approach

Lift the session rules into a pure core, complete the state machine, and make every shell
render it.

```
            ┌──────────── core (pure: no I/O, clock, async; wasm32-clean) ───────────┐
 intents ──▶│ session::update(state, intent, now_ms) -> (state, Vec<Effect>)          │
            │ session::classify(RefreshAnswer, attempted_gen, stored_gen) -> Verdict  │
            │ session::view(state) -> SessionView { line, show_sign_in, may_prove }   │
            └───────────┬───────────────────────────────┬─────────────────────────────┘
                        │ effects as data               │ projection
          ┌─────────────▼─────────┐       ┌─────────────▼──────────────────────┐
          │ call-session (arc,    │       │ croft-ffi rules → Kotlin SessionState│
          │ macOS) performs them  │       │ AuthManager performs; CallScreen    │
          └───────────────────────┘       │ renders; camp::plan reads may_prove │
                                          └─────────────────────────────────────┘
```

**The states** (words are D4's):

| State | Meaning | Screen | May present identity (camp/dial mint)? |
|---|---|---|---|
| `SignedOut` | nothing stored | sign-in field | no |
| `Stored` | credentials on disk; the provider has accepted nothing this run | "checking your sign-in" | yes, and a mint's own refresh is the proof |
| `Proven` | the provider accepted a refresh or exchange this run, at `at` | signed in as DID | yes |
| `Unconfirmed` | refresh attempts could not reach an answer (outage), retries exhausted | "could not reach your atmo provider" | yes, the next use retries (an outage is not a refusal) |
| `Dead` | the provider refused (`invalid_grant`, `ExpiredToken`, …) | **sign-in field returns**, with the reason | **no**: credentials and camp pass dropped |

**The transitions** (every row becomes a pinned test, as in `camp_rows.rs`):

| From | Intent | To | Effects |
|---|---|---|---|
| any | `Loaded(None)` | SignedOut | — |
| any | `Loaded(Some)` | Stored | `Refresh` if the access token is inside the margin |
| any | `SignedIn{at}` | Proven | persist pair, record `signed_in_at` |
| Stored/Proven/Unconfirmed | `UseRequested` (mint) | unchanged | `Refresh` if the access token is stale |
| any live | `RefreshAnswered(Accepted)` | Proven | persist pair (synchronously) |
| any live | `RefreshAnswered(Refused)` + same generation | **Dead** | drop credentials, drop camp pass |
| any live | `RefreshAnswered(Refused)` + generation moved | unchanged | re-read (lost rotation race: the winner's pair is the truth, see below) |
| any live | `RefreshAnswered(Unavailable)`, attempt < N | unchanged | `ScheduleRetry{after_ms}` per D2 |
| any live | `RefreshAnswered(Unavailable)`, attempt = N | Unconfirmed | — |
| any live | `RefreshAnswered(ClientDefect)` | Unconfirmed, words say it is ours | — (no retry loop against our own bug) |
| Unconfirmed | `Foreground` / `UseRequested` | unchanged | `Refresh` (a fresh episode, attempts reset) |
| Stored/Proven | `HorizonTick` (D3, public floor only) | unchanged | — (the view grows "sign-in ends in N days") |
| any | `SignOut` | SignedOut | drop credentials, drop camp pass |
| Dead | `UseRequested` | Dead | — (never refreshes a dead token again, so no replay storm) |

**Classification is pure.** The shell parses HTTP into a `RefreshAnswer`
(`Accepted{…} | Refused{error, description} | Unavailable{reason} | ClientDefect{reason}`),
and the core decides:

- `invalid_grant` (OAuth) or `ExpiredToken` / `InvalidToken` (app-password), whatever the
  description, is **Refused**. It is **Dead** only if the stored refresh-token generation is the
  one attempted.
- A transport error, 5xx, or 429 is **Unavailable**, and gets retried.
- Any other 4xx (`invalid_request`, `invalid_client`, a DPoP proof still refused after the nonce
  retry) is **ClientDefect**: not dead, not an outage, and it says it is ours.

**The single-use race.** F2 shows a replay *deletes* the session at the provider, so the
Kotlin `Mutex` stays and is the real protection. The generation check covers the one case
the mutex cannot: a refusal for a token we had already rotated away from. That case is not
evidence about the *current* token. The core re-reads, and the next use either proves the
current token or earns a same-generation refusal, which is then honestly Dead.

**A dead session is signed out everywhere.** `view(Dead).may_prove = false` feeds
`camp::plan(signed_in = may_prove, …)` and `dial::plan`. So camping goes tokenless (the
existing signed-out row), the pass is dropped (`DropCampPass`), a grant dial gets the
existing sign-in nudge, and the sign-in field returns with the reason. No surface says
`Signed in`.

## Reasoning

**Why a core and not Kotlin.** The Android defect is a missing state machine, and the only
existing one is private to a port Android cannot reach (F4). The D3.2 pattern (core decides,
FFI carries, Kotlin delegates, one matrix grades both) is already proven for camp and dial.
Writing it again in Kotlin would make three copies, and the arc and the macOS app would keep
the app-password over-classification.

**Why the time is a parameter.** ADR-0001: a clock read inside a core is how the architecture
rots. `now_ms`, `signed_in_at` and the retry delays are all data. The shell's scheduler (a
coroutine delay on Android, a loop in the arc) performs `ScheduleRetry`.

**Why no WorkManager refresh (E113 as written).** F1: against a public client it cannot extend
the session a single day. Against the broker (F3) refresh is the broker's job, server-side,
and the phone holds a ticket. A phone-side periodic job would add a battery cost and a
background-execution surface for no survival. What E113 was reaching for, *an idle phone
stays reachable*, is delivered by the broker path (D3). What the floor can honestly do is
**say when it will end** (the horizon) and **say when it has ended** (Dead). E113 is
therefore answered by D3 and closed as "refuted by source for the public client". It is not
built as specified. Owner call.

**Why Unconfirmed may still present identity.** An outage is not an authorization answer
(`refusal_words`' own rule). Refusing to mint during a provider outage would turn a network
blip into a self-inflicted sign-out. The mint's own refresh either proves the session or
gets the refusal that makes it Dead.

**Why Dead drops the credentials instead of keeping them "just in case".** A refused token
can never be accepted again, and F2 shows that presenting a non-current token destroys a
session at the provider. Keeping a dead token invites a replay against whatever session
replaces it. The handle stays prefilled, since it is not a credential.

## Decisions for the owner

### D1 — which core owns the session rules?

- **(a) `call-core::session`.** Smallest step. ADR-0002 already says call-core holds "session
  state" (it meant the call's, but the pond's only consumer today is the calling app). Cost:
  when chat or feed need an atproto session, they depend on the call pond for it, which
  inverts ADR-0001's "per-pond concerns live inside that pond, never smeared across a shared
  one".
- **(b) a new `core/account-core`** (plain noun: *the account session: signed in, proven, or
  not*). It is a sibling capability core like calling: pure, wasm32-clean, joining the
  `core-purity` job. call-core does not depend on it, and the shell projects `may_prove` into
  `camp::plan`, which is ADR-0002 rule 2 (ponds hold projections). Cost: one more crate and
  an ADR-0005.

**Recommendation: (b).** The session is not the call experience. It is whether this device may
act as an account at all, and the next pond to speak atproto should not import calling to
find out. ADR-0005 records it.

### D2 — the retry policy before Unconfirmed

Proposed: 3 retries at **5 s, 30 s, 2 min** within one episode, then Unconfirmed. A new
episode starts on foreground or on the next mint. 429 is Unavailable, and `Retry-After`,
when present, replaces the next delay (the shell parses it; the core takes it as data).
Nothing retries a Refused or a ClientDefect.

### D3 — E113: route through the broker, and in what order relative to v0.6.0?

- **(a) This plan ships the state machine on the public-client floor (Phases 1–5) for v0.6.0,
  plus a horizon line** ("sign-in ends in about N days — sign in again to stay reachable",
  from `signed_in_at + 14 d`, labelled as the provider's public-client rule). The broker path
  becomes a **child plan** once croft-stack takes the edge below. v0.6.0 stops lying and
  says when it will end. It does not yet stay signed in.
- **(b) Hold v0.6.0 for the broker path.** The release would carry long sessions, but it
  waits on croft-stack work plus H1–H8 hardening, which that repo says gates real users.
- **(c) As (a), without the horizon line.** It is simpler, but a 14-day death arrives as a
  surprise instead of a warning.

**Recommendation: (a).** The asks for croft-stack, to be picked up by a session there and not
built from here: (1) a service-auth endpoint (ticket → `getServiceAuth` JWT for a named
`aud`/`lxm`, allowlisted to admit's DID and our two lxms); (2) an error contract that tells
**session refused** (for example 401 `session_dead`) apart from **upstream unavailable** (503)
and **unknown ticket**; (3) a native return path; (4) the H-items the owner requires first.
The core in Phase 1 already classifies broker answers the same way, so the child plan adds
an adapter, not a state.

### D4 — the words for each state

Proposed, in plain product nouns (DESIGN.md: the noun is **atmo provider**):

| State | Line |
|---|---|
| Stored | "Checking your sign-in with your atmo provider…" |
| Proven | "Signed in" + DID (unchanged; now earned) |
| Unconfirmed (outage) | "Could not reach your atmo provider — your sign-in is not confirmed yet. Trying again." |
| Unconfirmed (ours) | "Could not check your sign-in — a problem in this app, not your account." |
| Dead | sign-in field, above it: "Your atmo provider ended this sign-in. Sign in again to be reachable." |
| Horizon (D3a) | "Sign-in ends in about N days — sign in again before then to stay reachable." |

## Phases

Each phase is TDD. Each row test is watched failing first, then made green. Each phase ends
at `make gate` green.

### Phase 1 — the rules, in the core D1 names

- The state enum, the intent and effect enums, `update`, `classify`, `view`, and the retry
  policy constants. Every row of both tables above is a named test in
  `tests/session_rows.rs`, with boundaries pinned at three points (retry count N−1/N/N+1;
  access-token margin inside/at/past; horizon day 13/14/15).
- `classify` table test: each error discriminant × generation same/moved; every status class.
- ADR-0005 if D1(b). Joins `core-purity` (wasm32 build, clippy disallowed-methods).
- **Mutation audit** after green: `cargo mutants` on the module in a detached scratch
  worktree, with no `CARGO_TARGET_DIR` override. Each survivor is triaged as equivalent or a
  real gap in the Review Log.

### Phase 2 — `call-session` consumes it

- `steps.rs`'s private `SessionState` is replaced by the core's. `session.rs` keeps the
  app-password I/O and loses its policy (`decide`, `RefreshFailure`, `refusal_words` move or
  become the core's).
- `atproto.rs` stops classifying every 400/401/403 as Dead: it produces a `RefreshAnswer` and
  the core classifies it. Test: a malformed-request 400 is ClientDefect, not Dead.
- The arc's and the macOS app's existing tests (`session_policy.rs`, `session_steps.rs`,
  `report_words.rs`, the Swift `ControllerTests`) stay green or are re-pinned with the words
  D4 settles.

### Phase 3 — the FFI

- `ffi/src/rules.rs` (or a sibling `account.rs`) mirrors the boundary types, the D3.2 way.
  **Every error variant carries real words** (the P7 S1 / §15.3 finding: a fieldless variant
  crosses uniffi with an empty `message`).
- `ffi/tests/account_pins.rs` pins the boundary. (`session_pins.rs` is the chat session's
  and is not touched.)

### Phase 4 — Android renders it and stops dropping failures

- `AuthManager` produces `RefreshAnswer`s (HTTP → answer only; no policy) and exposes a
  `StateFlow<SessionView>` driven through the FFI's `update`. `provenDid` is derived from
  it, and a Dead session makes it null.
- The rotation write becomes `commit()` (F2). A test asserts the stored refresh token after
  `freshAccessToken` returns is the rotated one, on a prefs double that models deferred
  `apply()`.
- `MainViewModel.onForeground` sends `Foreground` and performs the effects, and the
  `runCatching … Log.w` is gone. `ScheduleRetry` becomes a coroutine delay in
  `viewModelScope`. The camp trigger reads `may_prove`, and `DropCampPass` clears `campPass`.
- `CallScreen` renders the view's line. The sign-in field returns on Dead, with the reason
  above it and the handle prefilled.
- **Enforcement matrix** (`docs/ENFORCEMENT-SCENARIOS.md` § Screen honesty, `PIN:` + `RUST:`
  each, walked by `EnforcementMatrixTest` and `enforcement_matrix.rs`), in the same commit as
  their tests:

  | Scenario | Outcome |
  |---|---|
  | Provider refuses the refresh | MUST SAY signed out, sign-in field returns, MUST DROP THE PASS, camps tokenless |
  | Provider unreachable during refresh | MUST NOT SAY dead; retries per policy; then Unconfirmed |
  | A refusal for an already-rotated token | MUST NOT SAY dead; re-reads |
  | A malformed refresh request (ours) | MUST SAY it is ours, not dead |
  | A dead session dials a grant | the existing "grant but no usable proof" nudge, via `may_prove` |
  | Stored, not yet proven | MUST NOT SAY `Signed in` |

  The "Open defects" prose bullet for this defect is struck through, pointing at the rows.

### Phase 5 — device verification (runbook §18)

Needs `testbed--samsung` and `testbed--pixel` claims and a seat in `DEVICE-QUEUE.md`. Step 0
is re-sign-in. Do not `adb install -r` over the released APK, because it clears identity
(TESTBED.md § Devices).

- **A genuinely dead session.** The provider must refuse, not a simulated failure. A
  **debug-build-only** "revoke at provider" action calls the AS `revocation_endpoint`
  (advertised in the metadata, F1) with the stored refresh token. The next foreground must
  read signed out with the reason, the pass dropped, the relay journal showing the tokenless
  attach, and no `Signed in` anywhere.
- **An outage.** Block the provider host only; the relay stays reachable. The screen must
  say "could not reach your atmo provider" and never "ended". Unblock, foreground, and the
  session reads Signed in without a re-sign-in.
- Both phones, and both results recorded verbatim in §18 beside the relay journal lines.

## Documentation Impact

croft `CHANGELOG.md` `[Unreleased]` entry on the code branch (`session: …`).
`docs/ENFORCEMENT-SCENARIOS.md` (rows above). Runbook §18. ADR-0005 (if D1b). `TODO.md`
(both rows ticked with date + PR; E113's disposition per D3). Triage row 5 ticked, and row 2
(v0.6.0) announced unblocked. The broker edge goes in `CroftC/.claude/ARCHITECTURE.md` only
once croft-stack accepts it.

## Concurrency Map

Phases 1–4 touch only croft, in this worktree (`worktrees/session-state/croft`,
`claude/session-state`). Phase 5 contends for both phones and waits for their claims (held by
`croftc-ac` for the relay-flap reading at the time of writing). The broker child plan is
croft-stack's surface and is not claimed from here.

## Out of scope

The v0.6.0 cut itself. The relay flap. E125–E128. R5–R8. openmls 0.9. A foreground service
or push-to-wake (if the broker path turns out to need one, stop and ask). Any croft-stack
edit, including the broker asks in D3, which are surfaced rather than built.

## Review Log

- **2026-09-28 — drafted** (session croftc-be). F1 read from provider source; F2 recorded as
  unexplained rather than assigned a cause; F3 added on the owner's pointer to the broker
  mid-draft, with its gaps read from `croft-stack/broker/src` and a live probe.
