# Runbook — the two-device call test

**Status: run, green.** Date attempted: 2026-08-17

The first test of whether two Croft Call endpoints actually connect. Everything
proven so far is single-node; this is the first time the accept/connect path runs
against a real peer.

---

## 1. Why this is next

What is already proven, on a headless emulator:

- the client initialises and survives launch
- it publishes a real EndpointId
- it reaches a relay — status line reads **"ready, camped on relay"**
- it accepts a `croftcall://` deep link and populates the callee card

What is **not** proven, and cannot be proven on the emulator:

- two endpoints finding each other
- the ALPN handshake completing
- the `croft-call/0` hello exchange
- whether the path is direct or relayed

The emulator NATs through the host, so holepunching is unrepresentative, and
emulator-to-emulator on one machine is a topology that exists nowhere in the
wild. Two real devices are the acceptance surface — this is stated in
`env/avd.yml` as a known limit, not discovered here.

There is a second reason this is next: the iroh Kotlin `accept`/`connect`/stream
method names were **inferred** from a "the API maps 1:1 to Rust" promise, then
resolved against n0's reference app and unit-tested (`connect@5fa0258`,
`7433238`). They have **never run against another endpoint.** Unit tests do not
exercise a handshake.

## 2. What this test does NOT prove

Read this before drawing conclusions from a green run.

- **It does not test `relay.croft.ing`.** The app is wired to `presetN0()` —
  n0's public relays (`CallPeer.kt:58`). Pointing at our own relay is isolated to
  one function but not enabled, pending a check of the Kotlin surface for custom
  relay maps. A pass validates *the client and iroh connectivity*, not our relay.
- **It does not report direct-vs-relayed in the UI.** `via …` on the callee card
  shows the relay URL **from the deep link** — what the link claimed, not the path
  iroh chose. `direction` is only `incoming`/`outgoing`. Whether the connection
  type is obtainable at all from the Kotlin binding is **unverified**; we look in
  logcat and take what we find. Do not record a holepunch rate from this run
  unless the log actually says so.
- **It does not test background behaviour.** The client is foreground-only by
  design; staying reachable while suspended needs a foreground service and
  push-to-wake, which is its own phase.

## 3. The package

**Was under test: candidate `v0.1.0-rc.1`** — validated 2026-08-17 and promoted
to **`v0.1.0`** (Latest); the candidate is pruned, so the rc.1 download URL is
gone. The released artifact is byte-identical (sha256 `c3fbc013…843a987`).

Download the release, or rebuild locally — same artifact:

```
# published release (versioned asset):
https://github.com/CroftCommunity/croft/releases/download/v0.1.0/croft-call-0.1.0-debug.apk

# or local build:
croft/android/app/build/outputs/apk/debug/app-debug.apk
  ~42 MB · package ing.croft.call · minSdk 26 (Android 8.0+)
  native: lib/arm64-v8a/libiroh_ffi.so   (arm64 ONLY)
```

Rebuild with `cd android && ./gradlew assembleDebug`. This is `croft/android`, the
**one** Croft Call app — `connect/android` is retired at connect v0.2.0. It
captures the connect contract-v2 deep link (`device`/`grant`) and dials by
`endpointId`.

**ABI note:** the APK carries **arm64-v8a only**. Any recent Pixel or Samsung is
arm64, so this is fine. A 32-bit-only device would install and then die with
`UnsatisfiedLinkError: dlopen failed: library "libiroh_ffi.so" not found` — the
exact crash this build just fixed. That is the signature to recognise, not a
regression.

## 4. Phone setup (~2 min each)

### Enable developer options

- **Pixel:** Settings → About phone → tap **Build number** 7×.
  Then Settings → System → Developer options → **USB debugging** ON.
- **Samsung:** Settings → About phone → **Software information** → tap
  **Build number** 7×. Then Settings → Developer options → **USB debugging** ON.

### Connect

Plug **both** phones into the Mac if you can — `adb` handles multiple devices, so
both halves can be driven and logged simultaneously, which is far better evidence
than one side plus a description of the other.

Each phone shows *"Allow USB debugging?"* — tick **Always allow from this
computer**.

- [ ] `adb devices` lists **two** devices, both as `device` (not `unauthorized`)

**Samsung gotcha:** if it does not appear, pull down the notification shade and
change the USB mode from *Charging* to **File Transfer**. adb frequently will not
see a Samsung in charging mode.

### Install

```
adb -s <SERIAL> install -r android/app/build/outputs/apk/debug/app-debug.apk
```

- [ ] Pixel: `Success`
- [ ] Samsung: `Success`

**Samsung gotcha:** Knox / Play Protect may show *"Blocked by Play Protect"* for a
debug-signed app. Choose **Install anyway**.

## 5. The rungs

Each rung isolates one thing. If a rung is red, everything below it is noise —
stop and diagnose there.

### Rung 0 — each phone alone

Launch the app on both.

- [ ] Pixel reaches **"ready, camped on relay"** and shows a 64-char EndpointId
- [ ] Samsung reaches **"ready, camped on relay"** and shows a *different* EndpointId

A phone stuck at **"binding endpoint…"** is an init or networking failure. Stop;
that is not a two-device problem.

### Rung 1 — same WiFi

Both phones on the same network. This should connect directly over the LAN with
minimal NAT involvement, so a failure here is **the app or the ALPN wiring**, not
networking — which is why it comes first.

Read A's EndpointId, then fire it into B as a deep link so neither of us types 64
hex characters:

```
adb -s <B_SERIAL> shell am start -a android.intent.action.VIEW \
  -d "'croftcall://call?endpoint=<A_ENDPOINT_ID>&handle=phone-a'"
```

**The quotes are load-bearing.** Unquoted, the device shell treats `&` as a
background operator and silently truncates the URL — the intent arrives with only
`endpoint`, the card renders "(unnamed peer)", and it looks exactly like a parser
bug. That cost a false bug report on 2026-08-12.

- [ ] B shows the callee card with A's EndpointId
- [ ] **Tap Connect on B**
- [ ] B: `dialing…` → `connected (outgoing)`
- [ ] A: `connected (incoming)`
- [ ] each shows the other's hello

### Rung 2 — split networks

Turn WiFi **off** on one phone so it is on cellular. Repeat rung 1.

This is the real NAT-traversal test. Rung 1 can pass on a LAN without exercising
traversal at all, so a rung-1 pass says nothing about this.

- [ ] connects across networks
- [ ] note how long the dial takes (a long pause before success suggests a
      holepunch attempt timing out into a relayed fallback)

### Rung 3 — our relay (separate change, not this run)

Repoint at `relay.croft.ing` and repeat rung 2. Requires enabling the custom
relay map in `CallPeer.kt` first. Out of scope here; listed so the ladder is
visible.

**RUN 2026-08-17, PASS** (same devices, on the 0.2.0 build with the path
instrument): rung 0 both camped on our relay; same-WiFi dial connected with
the callee's first path `relayed https://relay.croft.ing:8443/` before
upgrading to direct on the LAN; split-network dial (Samsung WiFi caller →
Pixel LTE callee) connected in ~4.1 s with **both** sides' first path
`relayed https://relay.croft.ing:8443/`, then **both** upgraded to a
cross-network direct path (WiFi↔LTE holepunch confirmed). Our relay carried
the call end to end; direct-vs-relayed is no longer unknowable — the path
line said all of this itself.

## 6. During the run

- **Both apps must stay in the foreground.** Do not switch away or let a screen
  sleep mid-dial. Consider bumping screen timeout first.
- Keep both phones plugged in so logcat is captured from both sides.

## 7. Evidence to capture

- `adb -s <SERIAL> logcat -c` before each attempt, `logcat -d` after
- screenshots of both screens at `connected`
- anything in the logs naming a path, relay, or connection type
- for rung 2: wall-clock time from tapping Connect to `connected`

## 8. Results

*(fill in as run)*

| Rung | Result | Notes |
|---|---|---|
| 0 — each alone | **PASS** | Pixel 9 Pro (`631277dd…98f044`) and Samsung SM-S947U1 (`14af214d…c9c5ab`) both reached "ready, camped on relay" with distinct 64-char EndpointIds. Pixel also re-reached "ready" on cellular-only before rung 2. |
| 1 — same WiFi | **PASS** | Both on the same WiFi. Deep link (quoted) populated the callee card with handle `@pixel-9-pro` intact. Samsung `connected (outgoing) {"hello":"callee"}`, Pixel `connected (incoming) {"hello":"croftcall-android"}`. Connected in a few seconds. |
| 2 — split networks | **PASS** | Samsung on WiFi, Pixel on LTE (Samsung has no SIM, so roles were WiFi-caller → cellular-callee). Clean restart of both apps first. Tap-to-`connected` ≈ **4 s** (`dialing…` at ~2 s poll, `connected` by ~4.2 s) — no long pause suggesting a holepunch timeout into relay fallback, but see below. Same hello exchange both sides; Pixel status bar shows LTE, no WiFi. |

**Direct or relayed?** **unknown** — nothing in logcat from either side names a
path, relay, or connection type (the iroh binding logs nothing to logcat).
Devices/date: Pixel 9 Pro + Samsung SM-S947U1, 2026-08-17.

**Run notes (setup friction, for next time):**
- Samsung One UI **Auto Blocker** silently blocks USB debugging even with the
  Developer options toggle on — Settings → Security and privacy → Auto Blocker
  → off, then the authorize prompt appears. This is upstream of the
  charging-mode gotcha already listed.
- First two USB cables were charge-only; phones enumerate on the Mac's USB bus
  only for MTP, and adb sees nothing (not even `unauthorized`) until debugging
  is truly on.
- EndpointIds persisted across force-stop + relaunch (same keys in rung 2 as
  rung 1).

**Candidate under test:** `v0.1.0-rc.1`.
**Promote and prune: DONE 2026-08-17** (per `ops/RELEASING.md`) — `v0.1.0`
(Latest) cut from the rc.1 commit `aa89fa4`, rc.1 deleted with its tag:

```
gh release create v0.1.0 -R CroftCommunity/croft --target aa89fa4 \
  --title "croft v0.1.0" \
  --notes "Validated by the two-device call test on 2026-08-17, Pixel 9 Pro + Samsung SM-S947U1." \
  croft-call-0.1.0-debug.apk
gh release delete v0.1.0-rc.1 -R CroftCommunity/croft --yes --cleanup-tag
```

**Follow-ups raised:**
- The app logs nothing that names the chosen path (direct vs relayed) — rung 3
  will want that observable before we claim anything about our relay.
  *(Landed same day: `PathSummary` + a 2 s poll over `Connection.paths()` now
  put `direct <addr>` / `relayed <url>` in the footer and logcat, tag
  `CroftCall`. On-device it watched a callee upgrade relayed → direct.)*
- Test-device standing arrangement: the **Samsung SM-S947U1 is the dedicated
  test device** (developer mode on, stays ready; it has no SIM, so it is always
  the WiFi side). The Pixel 9 Pro is a personal phone borrowed for two-device
  runs — plan future tests around asking for it, and prefer the Samsung for
  anything single-device.

## 9. If it fails

Likely causes, roughly in order:

1. **ALPN / accept-loop wiring** — the least-exercised code in the app. Unit
   tests cover parsing, not a handshake.
2. **Foreground/lifecycle** — an app backgrounded mid-dial tears down its
   endpoint.
3. **Network** — carrier-grade NAT on cellular is the classic rung-2 failure, and
   is exactly what a relay exists to paper over.
4. **The relay preset** — if n0's relays are unreachable from your network, the
   status line would not have reached "camped on relay" at rung 0, so this should
   already be excluded.

Capture logcat from **both** sides before changing anything. The failure is more
informative than the fix.

## 10. After a green run — where this leads

Proving a call connects is the foundation, not the finish. On pass (and after the
promote-and-prune in §8), the staged follow-up is already thought through:

1. **Rung 3 — our own relay.** ✅ **Done 2026-08-17, shipped as `v0.2.0`**
   (see §5-rung-3): the endpoint camps on `relay.croft.ing:8443` via a
   custom RelayMap, and the call screen/logcat report the live connection
   path per side.
2. **Phase 11 — the cap/admission layer.** The contract for *who may call* is
   built and canonical on `connect` (contract v2); the client-side work is
   specified in **`CroftCommunity/connect` `docs/PHASE11-HANDOFF.md`**.
   Status: **M1 ticket redemption shipped as `v0.3.0`** (invite link →
   public-record resolution → verified secret → callable contact, validated
   on-device), and **M2+M3 shipped as `v0.4.0`** (callability resolver +
   atproto OAuth identity proof — the flip from may-not-permit to callable
   observed live both directions;
   `plans/2026-08-17-2-plan-m3-identity-proof.md`). What remains is **M4**:
   **`evaluateGrant`** as an effect at call time plus relay-side
   enforcement, gated on decision D3 (relay token semantics, designed with
   croft-stack). Each milestone was published and validated the same way
   (candidate → on-device behaviour test → promote).

So the ladder does not stop at "a call connected": it climbed to our relay, then
into admission. This runbook validates the bottom rungs; the handoff carries the
rest.

## §11 — M4 call-time admission, first device run (2026-08-21)

Rig: **local croft-admit** on the workstation (memory store, `[mint]`
against production atproto; `--keygen` throwaway keypair) + **production
relay** (open mode). Debug builds carry `-PcroftAdmitBase=http://<LAN-IP>:8401`
(new BuildConfig overrides; debug-only cleartext). Pixel = caller,
Samsung = callee (endpoint id matched the published record exactly).

What validated, in order, all driven over adb with the live test repo:

1. **The real mint from a phone**: redeem `m1ticket` → tap Connect →
   local admit logs `minted cap=m1ticket budget=Bytes(262144)` — real plc
   + PDS reads, the real invite secret, sub-second.
2. **The minted-token dial**: mint → `rebindWithToken` (EndpointId stable)
   → dial → `connected (outgoing) … direct` with the callee's hello —
   the M4c pipeline end to end on hardware.
3. **Revocation, live**: grant deleted from the real repo (record backed
   up first) → next Connect → app shows **"this invite has been
   revoked"** and does NOT dial; admit WARNs `cap_revoked` (not
   `cap_not_found` — the seen-grants memory held).
4. **Recovery**: grant restored via putRecord → next Connect minted and
   connected again.

**Finding — the local-relay rig needs TLS**: with BOTH endpoints pointed
at a plain-HTTP relay on the LAN, phones never complete a relay attach
and even LAN-direct dials fail (`dial failed: null`) — the discovery
records carry the http relay URL and iroh-ffi chokes. The rust
`iroh_relay::client` attaches to the same relay fine (see croft-stack
`examples/attach_probe.rs`), so this is endpoint/ffi-side. Consequence:
the on-device ENFORCE loop needs a TLS relay — either the staging
listener on the production box (real certs, separate port) or admit
activation itself. Deferred with O1 (the callee's camping token), which
the enforce loop would hit immediately anyway.

Not yet observed on-device: server-side attribution (`admitted
sponsorship=…` needs a relay `[token]` pointed at a real mint key —
arrives with croft-admit activation), the identity-proof mint (needs the
re-sign-in under the new scope), and the three call-endings.

**§11 addendum, same night — the identity-proof mint on-device.** Fresh
sign-in on the Pixel as the caller account under the NEW scope
(`atproto transition:generic`): PAR accepted, password via Playwright
over the browser's DevTools socket (the workspace rule; the default
browser turned out to be Brave — same `chrome_devtools_remote` socket),
consent authorized, `signed in as did:plc:l5xig…`. Then a deep link
carrying `grant=m3registered` (no secret): callability flipped to
`Callable via m3registered`, and Connect ran the identity path —
`freshAccessToken` → `getServiceAuth` at the caller's PDS (the O2 scope
working LIVE) → the admit resolved BOTH identities, verified the real
ES256K proof against the caller's DID document, admitted via
`registeredCallers`, and `minted cap=m3registered` → connected direct.
Both proof paths (possession and identity) are now device-validated.

## §12 — the ENFORCE rehearsal against the staging listener (RUN 2026-08-24 — ALL RUNGS GREEN; results at the end)

Everything below was staged 2026-08-23; the run record follows the recipe.

**What exists already:** `croft-relay-staging` is LIVE on the production
box — `https://relay.croft.ing:8444`, `admission = "enforce"`, real
certs (same certsync), running the croft-relay **v0.2.0 candidate**
(v0.1.1's tier-era claims refuse today's tokens — found by this rung).
Its `[token]` verifies the STAGING mint key; the private half is in
`CroftC/.env` as `CROFT_STAGING_MINT_KEY` (never on the box, never in
logs). Host-side the whole loop is proven (tiered-admission Review Log
2026-08-23): token-less refused with words → `/campToken` mint →
attached + pong + `admitted sponsorship=…` in the journal.

**Client is ready (M4e):** camp-at-attach is landed under tests — when
Ready meets a signed-in session the app mints its camping pass
(service-auth, `lxm ing.croft.relay.campToken`) and binds it; refusals
camp tokenless with words on screen; the pass re-mints at expiry margin.

**The run, sketched:**
1. LAN admit on the workstation, as §11, but `[mint] signing_key_env`
   pointed at `CROFT_STAGING_MINT_KEY` and `issuer` unchanged — the
   staging relay then honors its mints. Allow it in the macOS firewall.
2. Samsung (callee): debug build with
   `-PcroftRelayUrl=https://relay.croft.ing:8444`
   `-PcroftAdmitBase=http://<LAN-IP>:<port>`; **sign in as the callee
   account** (the camp proof needs the session; Playwright-over-DevTools
   recipe in §5). Expect: camp REFUSED tokenless at first attach (words
   in logcat/journal), then the camp mint fires and the re-attach camps
   — `admitted sponsorship=…` for the callee's endpoint in the staging
   journal.
3. Pixel (caller): same relay override; redeem/dial as §11 — the dial
   mint now needs the staging-keyed admit too. Expect the §11 story
   under enforcement: refusals refuse, admits carry the call.
4. Negative rungs: sign the callee out → next attach camps tokenless →
   staging refuses the camp (reception dies WITH words on screen);
   unpublish the callee's endpoint record → next camp mint refuses
   `endpoint_unbound` with its words.
5. Point both phones back at production 8443 before ending the session
   (polluted discovery records — the §11 lesson).


### §12 results — 2026-08-24, both phones, all rungs green

The rehearsal ran exactly as sketched (LAN admit on 8401 with the
staging key, both phones on `-PcroftRelayUrl=https://relay.croft.ing:8444`):

1. **The refusal, on hardware**: the Samsung signed-out camp was denied
   at the staging relay — `denied endpoint_id=14af214d8c reason="no_token"`
   on every auto-retry. The first real phone ever refused by our
   enforcement. (Finding: the app's line status still said "ready,
   camped on relay" — the optimistic-Ready honesty gap, filed below.)
2. **The recovery**: sign-in as the callee (Playwright over DevTools,
   §5 recipe; consent authorized), and the camp-mint chain fired
   unprompted — admit `camp minted budget=Bytes(262144)` → relay
   `admitted endpoint_id=14af214d8c sponsorship=BudgetBytes(262144)`.
   The first phone to camp on an enforcing Croft relay with its own
   self-minted pass; "ready, camped on relay" became TRUE.
3. **The enforced call**: the Pixel (caller, signed in) redeemed the
   live `m1ticket` link, and Connect ran mint-at-dial: the Pixel's own
   earlier token-less attach had been `denied … no_token`, then
   `minted cap=m1ticket` at the admit → `admitted endpoint_id=631277dda5`
   → **connected** — the first call carried with BOTH sides holding
   passes on an enforcing relay.
4. **The endings, on hardware** (E129's first device outing): Hang up on
   the Pixel → "you ended the call — ready, camped on relay"; the
   Samsung → "call ended: closed by peer: hangup (code 0) — ready,
   camped on relay" — the pass-through-the-transport's-words design,
   verbatim. Both sides returned to camped and callable; no force-stop
   anywhere in the session.
5. **The sign-out negative**: Sign out on the Samsung + relaunch → the
   relay refuses its camp again (`denied … no_token`). Reachability dies
   at the relay when the identity goes away, exactly the O1 model.
   (The unpublish-endpoint negative was NOT run on-device — it needs
   live record surgery on the test account; its refusal path is
   journey-covered (`endpoint_unbound`) and was left for a future run.)
6. Both phones rebuilt onto production defaults; LAN admit stopped.

**The run's find — a real concurrency bug, fixed the same session**: two
coroutines raced `freshAccessToken()` on the Pixel (the foreground
best-effort refresh vs the camp mint's) and rotated the SINGLE-USE
refresh token concurrently — the entryway answered
`400 invalid_grant "refresh token rotated concurrently"` and the camp
mint failed (the session survived; the winner's pair persisted).
`FixtureExchange`'s token endpoint now enforces single-use rotation like
the real entryway, the session journey reproduces the race verbatim
(RED observed), and `AuthManager.freshAccessToken` is serialized behind
a Mutex with a double-check — the loser rides the winner's fresh pair.

**Still open after this run**: the optimistic-Ready honesty gap (the
app cannot see its own relay-attach refusal; related to the silent
native logging, E128); the caller-side camp under enforce (the caller
account publishes no endpoint record for the Pixel, so its camp mint
would refuse `endpoint_unbound` by design — masked this run by the
refresh race, worth observing cleanly next time); the identity-proof
CAMP on a device where the account HAS published the endpoint is proven
(the Samsung); production enforce flip remains owner-gated on the
croft-admit activation prerequisites.

## §13 — the next phone session: v0.5.0 on devices + the production BAKE (prepared 2026-08-24)

Production is already baking (croft-stack `ec267f3` converged): the
relay runs the **v0.2.0 candidate** in OPEN mode with the PRODUCTION
admit key; croft-admit + ciss-admit are ACTIVE on the box (loopback);
`admit.croft.ing` DNS is the one owner-console step left (A
15.204.81.133 / AAAA 2604:2dc0:222::431 — currently parked at
Porkbun; the admit vhost's cert issues itself once it resolves).

1. **Owner: promote `v0.5.0-rc.1` → v0.5.0 Latest** (ops/RELEASING.md),
   then install on BOTH phones (production defaults — no gradle
   overrides this time; the client's ADMIT_BASE needs `admit.croft.ing`
   resolving first).
2. **The bake validation (M4d(c), at last)**: sign the Samsung in as the
   callee; expect the camp mint against the PRODUCTION admit and
   `admitted … sponsorship=…` in the PRODUCTION relay journal — calls
   work exactly as v0.4.0, now attributed. Dial from the Pixel: dial
   mint + attribution for the caller too.
3. **E130(a) device verification**: the honest camped line is polled
   from `endpoint.addr().relayUrl()` (landed under tests; the poll
   pattern is proven but the REFUSED-attach semantics of relayUrl()
   are javap-verified only). Point one phone at staging 8444 signed-out:
   the line must read "ready — NOT camped on relay; calls cannot reach
   this device" while staging refuses, and flip to camped after sign-in.
4. **E130(b) while there**: observe the caller-side camp refusal
   cleanly (the race no longer masks it): a signed-in account with NO
   published endpoint record for the device should show the
   `endpoint_unbound` words from the camp mint and still dial fine.
5. After a few days of clean bake journal, the flip is one word —
   owner-gated, out of this runbook.

### §13 results — 2026-08-26 ~02:20–02:35Z (steps 2+4 RAN; 1 and 3 remain)

Driven over adb + Playwright-over-DevTools (owner present; both phones on
USB). Deviation from step 1: the phones run **v0.5.0-rc.1 in-place** — the
promote to v0.5.0 Latest has NOT happened yet, owner's call pending.

- **The callee's production camp mint (step 2): DONE.** Samsung signed in
  as the callee account (Chrome this time — same `chrome_devtools_remote`
  socket), and the camp mint against the PRODUCTION admit succeeded
  **silently** — no client log, no admit journal line; the proof is the
  relay's attributed close: `usage endpoint_id=14af214d8c …`. The §12
  endpoint record at rkey `self` still matched the device exactly.
- **Step 4 observed for free**: the Pixel kept its §12 caller session
  through the in-place install, camped `endpoint_unbound` with the words
  on screen AND in the admit journal (02:23:14Z `WARN camp refused
  reason="endpoint_unbound"`), and still dialed fine.
- **The first attributed production call**: deep link
  (`grant=m3registered`, no secret) → "callable via grant m3registered"
  resolved from live records → Connect → mint-at-dial (silent success) →
  connected, upgraded to LAN-direct; Hang up landed "call ended: closed
  by peer: hangup (code 0) — ready, camped on relay" verbatim on the
  callee. Caller attribution flushed at connection close:
  `usage endpoint_id=631277dda5` (81 s, ~13 KB relayed).
- **Finding — the bake is journal-invisible at the current filter**: the
  relay's `admitted sponsorship=…` line is `debug!`
  (croft-relay-bin/src/main.rs:135) and production runs
  `relay_log_filter: "info,usage=debug"`, so §13's stated evidence line
  never appears in the production journal. The bake reads from
  **attributed `usage` lines** instead (they carry the endpoint id), or
  the filter gains the admission module at debug (croft-stack
  group_vars). Corollary: a SUCCESSFUL mint is silent at every layer —
  client, admit, relay-until-close. Do not read silence as failure.

Both phones left camped: Samsung holding its pass, Pixel tokenless with
the honest words (the designed caller posture). Remaining: step 1 (the
promote), step 3 (E130(a) against staging 8444), and the bake days.

### §13 step 3 + the Pixel's record — 2026-08-28 (~02:38–02:47Z, both phones on USB)

Two results, one of them a defect the earlier tests could not have caught.

**The caller became reachable (E135(b)'s empirical half).** The Pixel camped
`endpoint_unbound` against production for one reason: its account
(`bobzmudacroft.bsky.social`) published NO `ing.croft.iroh.endpoint` record —
verified by `listRecords` before touching anything, and the admit's rule read
from source first (`camp.rs`: `list_records` → `parse_endpoints` → any record
whose `endpointId` equals the connecting hex; only that field is load-bearing).
Published `rkey=self`, `label=pixel-test`, the device's endpoint id, same shape
as the callee's record. The phone then minted its pass **silently** and the
relay attributed its next connection close (`usage endpoint_id=631277dda5`).
One transient `endpoint_unbound` refusal was logged between the write and the
successful mint (02:40:20Z) — read-after-write lag on the PDS read path, gone
by the retry. **Consequence for the flip: the Pixel is now reachable under
enforce. It was not before, and nothing on screen said so.**

**E135(a) IS NOT FIXED ON DEVICE — the honest line still lies.** A debug build
pointed at the staging enforce listener (`-PcroftRelayUrl=…:8444`, which
verifies the STAGING key and so must refuse a production-minted pass) was
installed on the Pixel. Staging refused every attach, repeatedly:

    denied endpoint_id=631277dda5 reason="invalid_token" detail=SignatureOrMalformed
    error accepting upgraded connection: The relay denied our authentication

…and the phone's screen read **"ready, camped on relay"** the entire time,
with logcat reporting `home relay: https://relay.croft.ing:8444/`. The E135(a)
fix polls `endpoint.addr().relayUrl()`, and **iroh-ffi keeps returning the
CONFIGURED home relay after a refused attach** — the poll's input never goes
false, so the mapping (which its unit test pins correctly) is never reached.
The javap-verified assumption behind that fix — that `relayUrl()` reflects a
successful attach — is now measured false. A reachability signal has to come
from somewhere else (a probe, an ffi connection-state hook, or attach failure
surfaced through E128's missing native logging).

**Live confirmation of matrix row C4.** The same run is the wrong-key refusal
observed on hardware with two real keys, `SignatureOrMalformed` at the relay —
the row pinned in `phase2_token.rs::wrong_issuer_key_denies` yesterday.

Rig restored: the Pixel is back on the published `v0.5.0-rc.1` APK, camping on
production with its pass and attributing. Both phones now hold passes.

**Flip implication, stated plainly:** under enforce, any phone the relay
refuses — signed out, session expired, endpoint unpublished — will show
"ready, camped on relay" while being unreachable. E135(a) should be treated as
OPEN, and fixing it before the flip is the honest sequencing.

### §13 CORRECTION — 2026-08-28: the attributed lines never meant admission

Everything recorded above under §13 happened, but **the evidence was read
wrong**, and the correction matters more than the run did.

The §13 results cite attributed `usage` lines — a line naming an `endpoint_id`
rather than `unattributed` — as proof that a camping pass was minted and
accepted. It proves only that a token was **presented**. The relay completes
the token→connection join *before* acting on the verdict, on purpose ("a denied
handshake still has an authenticated endpoint worth attributing",
`croft-relay-bin/src/main.rs`), so a REFUSED pass attributes exactly like an
accepted one.

When the production log filter was widened (E148, 2026-08-28) the verdicts
became visible for the first time, and they say:

```
denied  endpoint_id=14af214d8c reason="invalid_token" detail=SignatureOrMalformed
admitted without token endpoint_id=14af214d8c mode="open"
```

Both phones, every rebind carrying a pass. **No `admitted sponsorship=` line
has ever appeared on production.** So:

- "the callee's first PRODUCTION camp mint" — the mint succeeded (the admit
  really does issue a valid pass; one minted by hand verifies against the
  relay's configured key). **The relay never accepted it.**
- "the first attributed production call" — the call connected, over a relay in
  OPEN mode that admits tokenless clients. It was not an admitted-by-pass call.
- The §12 rehearsal is unaffected: staging already ran `croft_relay=debug`, so
  its `admitted sponsorship=` lines were real. What travelled wrongly was the
  word "device-validated" moving from staging to production.

**Under enforce, both phones would be refused today.** Tracked as E150
(croft-stack `TODO.md`, full trail in
`croft-stack/sessions/2026-08-28-e150-tokens-never-verified.md`). The flip is
blocked until a real phone earns an `admitted sponsorship=` line on production.

**Method note for future rungs:** a signal is evidence only once you know the
case where it shows green and the property is false. For attribution, that case
was one `grep` away in the code that emits it.
