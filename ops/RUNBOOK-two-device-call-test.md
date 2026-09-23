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

**Under enforce, both phones would be refused today.** Tracked as E153
(croft-stack `TODO.md`, full trail in
`croft-stack/sessions/2026-08-28-e153-tokens-never-verified.md`). The flip is
blocked until a real phone earns an `admitted sponsorship=` line on production.
*(Superseded by §14: the cause was found and fixed, the line was earned, and
the flip landed 2026-08-30.)*

**Method note for future rungs:** a signal is evidence only once you know the
case where it shows green and the property is false. For attribution, that case
was one `grep` away in the code that emits it.

## §14 — E153 resolved and THE ENFORCE FLIP (2026-08-30; owner-authorized)

The §13-finding's cause was infrastructural, not the client: production had
silently run relay **v0.1.1** since the 2026-08-25 "promotion" (the v0.2.0
tarball was fetched, checksum-verified, and never unpacked — an ansible
`creates:` guard keyed on the binary; croft-stack
`docs/ANSIBLE-HYGIENE.md`). With the genuine v0.2.0 converged
(`8e287cb7…`), passes verified on production for the first time, and the
flip followed the same night with the evidence gate met, not waived:

- **Pre-flip (open):** a production-minted pass admitted via the rust
  `attach_probe`; then a REAL CLIENT — this app, **0.5.0-rc.2** on an
  emulator, OAuth-signed-in as the second test account, endpoint record
  bound — walked the M4e arc live: `denied reason="no_token"` →
  **`admitted endpoint_id=b729d675… sponsorship=BudgetBytes(262144)`**.
- **The flip:** `admission = "enforce"` committed + converged
  (croft-stack `sessions/2026-08-30-enforce-flip.md`).
- **Post-flip, all verified live:** tokenless probe refused with words;
  the minted pass admitted; the signed-in app killed + relaunched
  re-minted and was admitted **under enforcement**; signed out, the app
  was refused and the screen said the true thing — "ready — NOT camped on
  relay; calls cannot reach this device" (E135(a) honesty, now proven
  against a real refusal).

**Remaining for the next phone session:** the two physical phones have not
yet camped under enforce — expected to re-mint on next launch (v0.5.0
camp-at-attach; the emulator proved the arc), but per this runbook's rule
that is a prediction, not a result, until a phone earns its
`admitted sponsorship=` line on the enforcing relay. Clients ≤v0.4.0 are
refused by design.

## §15 — both phones camped under ENFORCE, and the dial that breaks the camp (RUN 2026-09-08)

Both physical phones earned their `admitted … sponsorship=` line on the
enforcing production relay — the result §14 left as a prediction. The call
that was supposed to follow **failed**, and failed in a way no tier below
two devices could reach: **dialling tears down the caller's camping
connection.** The callee is unaffected. Recorded here in the order it was
found, including the two wrong turns.

Rig: Samsung `R5GL712H75Y` (callee, `14af214d8c…`, `ngvalidation2112`) and
Pixel `51021FDAP000RF` (caller, `631277dda5…`, `bobzmudacroft`), both on the
released v0.5.0 (versionCode 6, verified by `dumpsys`, not by the changelog).
Relay artifact verified before anything else: `/opt/iroh-relay/current/croft-relay`
sha256 `8e287cb7020bc83bea25d1dc5dc41778ab97b05c47a426a7fcc8af814df9e2c7`,
marker `.installed-0.2.0`, tarball `ba935f32…` matching the declared value,
unit up since the 2026-08-30 flip converge with no restart since.

### 1. The callee — the line, earned (DEVICE-VERIFIED)

Force-stop, relaunch, watch the relay:

```
20:35:26  (app launched)
20:35:28  denied   endpoint_id=14af214d8c reason="no_token"
20:35:28  denied   endpoint_id=14af214d8c reason="no_token"
20:35:32  admitted endpoint_id=14af214d8c sponsorship=BudgetBytes(262144)
```

Screen agrees: the same endpoint id, `Signed in`, **"ready, camped on relay"**.
The M4e arc — attach tokenless, be refused, mint, be admitted — in six seconds
against a relay that is genuinely refusing. **The one connection then stayed open
for the next fourteen minutes with zero further verdicts.** That stability is the
control for §15.3 below.

### 2. The caller — a dead OAuth session that the screen called "Signed in"

The Pixel came up **"ready — NOT camped on relay; calls cannot reach this
device"** while its account card read `Signed in` with the right DID. The relay
saw it attaching tokenless. The client said why:

```
access token stale; refreshing
foreground token refresh failed: HTTP 400 from https://bsky.social/oauth/token
    after nonce retry: {"error":"invalid_grant","error_description":"Invalid refresh token"}
camp setup failed:          HTTP 400 … {"error":"invalid_grant","error_description":"Invalid refresh token"}
```

Idle since 2026-08-28, the refresh token was dead. Camp setup never ran, so the
attach went tokenless and enforce refused it — correctly.

**Two findings here, not one.**

- The camp line was **honest** (E135(a) working: "NOT camped … calls cannot
  reach this device"), but the account card above it said `Signed in`. Both
  lines are true about different things and the pair reads as a contradiction.
  A dead refresh token is indistinguishable on screen from a live session.
- **The prepared step-0 check is wrong.** "If a phone shows the handle field
  instead of 'Signed in', re-sign-in is step 0" cannot see this state — the
  phone showed `Signed in` throughout. The detectable signal is the camp line
  or `camp setup failed` in logcat, never the account card.

Re-signed in via Playwright over the phone's DevTools socket (handle prefilled,
password from `CroftC/.env`, consent authorized, "Login complete"). The phone
then minted and was admitted:

```
20:40:21  denied   endpoint_id=631277dda5 reason="no_token"
20:40:25  admitted endpoint_id=631277dda5 sponsorship=BudgetBytes(262144)
```

**Both phones have now earned the line under enforcement.**

### 3. The call — FAILED, and the dial is what breaks the camp

Deep link (`grant=m3registered`, quoted) populated the callee card correctly
(`@samsung-callee`, the right endpoint). Tap Connect → `dialing…` →
**`dial failed: null`**. The Samsung never rang.

Worse than the failure: **the dial dropped the caller's camp and it did not come
back for four minutes.** From the tap at 20:41:39, the relay refused the Pixel
~20 times with `no_token`, backing off to ~30 s intervals, until a full app
restart at 20:45:45 restored it. For those four minutes the phone was
unreachable.

Reproduced deliberately, one tap, with the relay watched from both sides:

```
20:48:54  screen: ready, camped on relay
20:48:57  <- ONE Connect tap
20:48:58  relay: actor errored "Stream terminated, exiting" (connection_id=19145)
20:48:58  relay: usage endpoint_id=631277dda5 … duration_ms=76701   <- the camp closed
20:49:00  screen: ready — NOT camped on relay; calls cannot reach this device
20:49:02  relay: admitted endpoint_id=631277dda5 sponsorship=…      <- re-attach
20:49:05  screen: ready, camped on relay
```

The dial tears down the camped relay connection rather than reusing it. The
recovery is **not reliable**: here it re-attached with its pass in 4 s; on the
first dial the re-attach went **tokenless** and stayed refused for four minutes.

**Isolation is clean.** The Samsung — same build, same relay, same window, never
dialled — held one connection for fourteen minutes with no denials. The caller
lost its camp on every dial. This is the dial path, not the network and not the
relay.

Three defects, ranked:

1. **A dial drops the caller's camp** (above). Under enforce this costs
   reachability for seconds at best, minutes at worst.
2. **`dial failed: null`** — a refusal with no words. The matrix requires
   "MUST REFUSE — never dials, words on screen"; `null` is not words. Same shape
   as the P7 S1 uniffi finding (a fieldless variant crossing with an empty
   message), and worth checking for that cause first.
3. **The call never connected at all**, so rungs below it are unproven this run.

### 4. E135(a) — both states now seen on hardware, without the staging repoint

The prepared step 4 (repoint a phone at the staging enforce listener on a debug
build) was **not needed and was not run.** Production refusals supplied the
negative state for free: the Pixel read "NOT camped … calls cannot reach this
device" while genuinely refused, and "camped" while genuinely admitted, on the
released APK against the real relay. That is a better test than the staging
repoint — same evidence, no debug build, no risk of leaving a phone pointed at
the wrong listener. **Prefer this shape.**

### 5. What this says about the flip

The flip is sound for **receiving**: a signed-in phone with a published record
camps and stays camped. It is **not** sound for **calling** — every dial risks
the caller's own reachability, and the failure is silent (`null`). Finding 1
should gate any claim that v0.5.0 is complete under enforcement.

Not run this session: the two-sided call, the E129 endings (blocked by 3), and
E135(b)'s wording decision (its case is now concrete — see the `Signed in` /
"NOT camped" contradiction in §15.2).

## §16 — the dial fix, DEVICE-VERIFIED; and the first connected call of this arc (RUN 2026-09-14)

§15 left the dial fix unit-green and **DEVICE-OPEN**, with the closing condition written
down: *a phone must dial without losing its camp, with no `no_token` in the relay journal
after a Connect tap.* It was met, and three things beyond it came with the run.

**The rig.** Samsung `R5GL712H75Y` as callee on the **released v0.5.0** (versionCode 6,
unmodified) — deliberately, so the callee stays the shipped artifact and the fix is isolated
to one side. Pixel `51021FDAP000RF` as caller on a **debug build of main `983c955`**, whose
APK was verified to contain the fix (`Rebind` present in `classes5.dex`/`classes6.dex`)
rather than assumed from the source tree.

### The result

```
19:48:05   ← ONE Connect tap
(nothing)    no "Stream terminated", no usage close, no denial, no re-admit
19:48:08   screen: "Hang up" — CONNECTED
19:49:08   ← Hang up
```

**The relay journal carried ONE line in total from the tap through the call and the
hang-up** — no verdicts, no `usage` closes. Both camps held. Set against §15.3's identical
action:

| | §15 (broken) | §16 (fixed) |
|---|---|---|
| 1 s after the tap | `actor errored "Stream terminated"` + `usage` close | nothing |
| the camp | died, re-attach needed | never disturbed |
| the screen | `NOT camped … calls cannot reach this device` | `Hang up` |
| the dial | `dial failed: null` | connected |

**The dial now reuses the camped connection instead of tearing it down.** The camp row and
the three Dial posture rows move to DEVICE-VERIFIED.

### The E129 endings, verbatim, both screens

Blocked behind the dial defect since 2026-08-23. Read off the devices:

```
caller:  you ended the call — ready, camped on relay
callee:  call ended: closed by peer: hangup (code 0) — ready, camped on relay
```

Both sides stayed camped after the call, which is E129's other requirement — the endpoint
stays bound and the device stays callable.

### Three findings from getting there, none of them the fix

**1. The OAuth sessions did not survive six days idle.** Both phones came up `Signed in`
with `NOT camped on relay`, and logcat gave the same reason on each:

```
foreground token refresh failed: HTTP 400 … {"error":"invalid_grant","error_description":"Invalid refresh token"}
camp setup failed:               HTTP 400 … {"error":"invalid_grant","error_description":"Invalid refresh token"}
```

§15.2 measured this at ~11 days on the Pixel; this run measured **6 days on the Samsung**.
The practical consequence for every future device run: **re-sign-in is step 0, always** —
not a contingency. The `Signed in` / `NOT camped` pair is the tell, and the account card is
never the signal (§15.2's refutation of the "shows the handle field" check still stands).

**2. Installing a build over the released APK resets the device's endpoint identity.**
`adb install -r` of the debug build reported `Success`, and the Pixel came up **signed out**
with a **new endpoint id** — app data was cleared, taking the persisted iroh secret key with
it. The published record still named the old identity, so the camp mint refused with
`this device is not published by your account` (`endpoint_unbound`), correctly and in words.

```
Pixel endpoint after install:  873f3ddc15e58b2888edc8c51db9f91d2bffbafeb554cd62ce0c997fdb44c926
ing.croft.iroh.endpoint/self:  631277dda58cc960db03cb1521f49fae2fab75c4afdf4c234f63aff08c98f044  (stale)
```

Repaired by re-publishing `rkey=self` with the new id, after which the arc completed —
`denied no_token` 19:47:20 → `admitted … sponsorship=BudgetBytes(262144)` 19:47:21.

**This is a rig hazard with teeth under enforce:** a reinstall silently makes a phone
unreachable, and nothing but the camp line says so. **The caller's published record now
names the DEBUG build's identity** — anyone who reinstalls that phone must re-publish it
again.

**3. A piped build command reported success for a failed build.** `./gradlew assembleDebug
2>&1 | tail -15` exited 0 because the pipeline exits with `tail`'s status; the build had
failed on `:social:compileDebugKotlin` (that module needs generated FFI bindings a fresh
worktree does not have). Caught only because the APK was missing when the install was
attempted. `CroftC/.claude/VERIFICATION.md` names this exact shape. The run used
`:app:assembleDebug` to a log file with the exit code read directly afterwards.

### What this run does NOT claim

Same WiFi, same room, one call. NAT traversal across networks, cellular, and mobile
lifecycle (backgrounding, doze, process death) are untouched — §5's rungs 2 and 3 remain
the tier for those. The call's path was not instrumented here beyond connection; a
relayed-vs-direct reading was not taken.

### §16 rig state as left — and a deliberate departure from "restore the released APK"

The standing rule is that a rig build is restored to the released v0.5.0 when done. **It was
not, on the Pixel, and that is the owner's call (2026-09-14)** rather than an oversight:

- **Samsung (callee)** — released **v0.5.0**, untouched, signed in, camped.
- **Pixel (caller)** — a **debug build of main `983c955`**, kept. Restoring v0.5.0 would put
  back a defect this very run proved fixed, and would wipe app data a second time. Under
  "alpha, forward over historical" that trade is not worth making twice.

**The hazard the departure leaves, stated so the next session does not rediscover it.** The
Pixel's published endpoint record now names the **debug build's** identity:

```
ing.croft.iroh.endpoint/self  ->  873f3ddc15e58b…   (the 631277dda5… of §13–§15 is dead)
```

Reinstalling the calling app on that phone — any build, including the release — wipes app
data, mints a **new** iroh secret key, and leaves the published record naming a device that
no longer exists. Under enforce the phone is then silently unreachable, and the only surface
that says so is the camp line. Re-publish `rkey=self` with the new id and relaunch; the
repair takes a minute once you know, and cost twenty when we did not.

## §17 — the calling app over OUR port, DEVICE-VERIFIED both directions (RUN 2026-09-21)

**What was being tested.** D3.3 of the call-core-and-apple-shell plan: `CallPeer` holds
`uniffi.croft_ffi.CallEndpoint` over `ports/call-transport-iroh` instead of upstream
iroh-ffi's `Endpoint`, and the camp/dial decisions are `core/call-core`'s through the same
bindings (D3.2). The condition, written in advance from §16: *the phone must camp under
enforce, place a call with one Connect tap and no relay verdict after its admit, receive a
call the same way, and show the E129 endings in the words §16 recorded.*

**The rig.** Pixel `51021FDAP000RF` (`bobzmudacroft`, endpoint `873f3ddc15…`) on a **debug
build of `claude/d3-android-core` at D3.3** (`c9f63c8`), installed IN PLACE over the §16
debug build — same debug keystore, so `adb install -r` kept app data and the key: the
endpoint id on screen matched the published `self` record with no repair. The other party
was **`croft-arc` on the laptop** (test account 1, label `croft-arc-callee`, endpoint
`298f91b375…`), because the **Samsung was pattern-locked** and only its owner can clear
that (see "Left undone"). The phone's screen was read with `uiautomator dump`, the taps
placed from a fresh dump's bounds each time, logcat filtered to `CroftCall`, the relay
journal read with `sudo -n journalctl -u iroh-relay`.

### The result

```
21:07:19   app launched; libcroft_ffi.so loaded (nativeloader); "access token stale;
           refreshing" → "session refreshed" — the OAuth session survived 7 days idle
21:07:20–23  relay: denied endpoint_id=873f3ddc15 reason="no_token"  ×5   (tokenless bind
           while the mint ran — the phones' known shape, §15)
21:07:26   logcat: "rebound with a token; re-attaching"
21:07:27   relay: admitted endpoint_id=873f3ddc15 sponsorship=BudgetBytes(262144)
21:07:30   logcat: "home relay: https://relay.croft.ing:8443/"; screen: "ready, camped on relay"
21:09:51   deep link → card "@croft-arc-callee … via https://relay.croft.ing:8443"
21:09:58   ← ONE Connect tap
21:10:03   screen: "connected (outgoing, direct ip:192.168.50.235:64146)  callee"; "Hang up"
           arc: "incoming from 873f3ddc15 (hello "croftcall-android") — connected"
21:10:14   ← Hang up
           screen: "you ended the call — ready, camped on relay"
           arc: "call ended: closed by peer: hangup (code 0)"
21:10:45   arc dials bobzmudacroft.bsky.social --device self (its record names 873f3ddc15)
21:10:56   screen: "connected (incoming, direct ip:192.168.50.235:54819)  croft-arc-callee"
21:11:00   logcat: "path change (incoming) …: direct ip:192.168.50.1:54819"  (iroh migrated)
21:11:04   arc hangs up (--hold 12); screen: "call ended: closed by peer: hangup (code 0)
           — ready, camped on relay"
```

**The relay journal carried NO line for the phone after its admit** — through both calls
and both hang-ups: no `Stream terminated`, no `usage` close, no denial, no re-admit. §16's
condition, met again on a different endpoint implementation. (The two `usage` + `Stream
terminated` pairs on `298f91b375` are the arc's own process exits at `done`, R3's recorded
shape.) Both calls went **direct** on the LAN after connecting; the path line updated live
when iroh migrated, which §15's runbook could only record as "unknown".

**What this proves and what it does not.** It proves the calling app's endpoint over our
port camps under enforce, dials, accepts, hangs up, and renders the E129 endings verbatim,
on a real phone, on production, in both directions — the two-device tier's question for
D3.3. The other party was the arc, not a second phone: **phone-to-phone over our port on
both sides is not yet run**, and neither is a relayed (off-LAN) call over our port.

### Left undone, and why

- **The Samsung.** Pattern-locked at run time; a session cannot draw the owner's pattern.
  It also carries the CI-signed v0.5.0, so installing this build there is a fresh install
  (wipes the key → re-publish `rkey=self`, §16's hazard) plus a browser sign-in — both the
  owner's acts. The two-phone run over our port is owed: `[device: android x2]`.
- **Off-LAN.** Both phones on one Wi-Fi go direct; a relayed call over our port needs one
  on cellular — the Pixel is the only phone with data: `[device: android=pixel]`.

### D3.4 on the same rig, an hour later

The build with `computer.iroh` and `libiroh_ffi.so` removed (D3.4) was installed in place
and took an incoming call from the arc the same way:

```
21:20:29–31  relay: denied endpoint_id=873f3ddc15 reason="no_token"  ×6
21:20:36   relay: admitted endpoint_id=873f3ddc15 sponsorship=BudgetBytes(262144)
21:20:39   screen: "ready, camped on relay"
21:20:52   logcat: "connected (incoming) 298f…: relayed relay:https://relay.croft.ing:8443/"
21:20:54   logcat: "path change (incoming) 298f…: direct ip:192.168.50.235:63620"   ← the
           relayed-first, direct-after-holepunch migration §16 could not see
21:21:03   screen: "call ended: closed by peer: hangup (code 0) — ready, camped on relay"
```

**One crash on the way, fixed the same hour.** The first D3.4 install's launch started
`MainActivity` twice a second apart (two `START u0` in logcat — the launch after a
reinstall), two `MainViewModel`s called the JNI DNS hook twice, and ndk-context's
initializer `assert!`ed on the second: SIGABRT, the app gone from the screen. The hook is
idempotent now (`ffi/src/android.rs`); D3.3's run had simply not hit the double start.
Recorded in `ops/JOURNAL.md`.

### The Samsung, an hour after that — phone-to-phone over our port on BOTH sides (RUN)

The owner unlocked the Samsung. Fresh install of the D3.4 build over the CI-signed v0.5.0
(`adb uninstall` then `install`: the key is wiped, as §16's hazard says), which came up
signed out with a new id and the honest line — *"ready — NOT camped on relay; calls cannot
reach this device"*. Repair as the hazard prescribes: account 1's `self` record
re-published naming `1a0c160324…` (putRecord 200, label kept); the handle typed and the
browser sign-in opened over adb; the owner's password entered on the PDS page (from
`CroftC/.env`, owner-authorized, never echoed); Authorize tapped. The redirect landed,
the mint ran, and the relay said so:

```
22:06:38–39  relay: denied endpoint_id=1a0c160324 reason="no_token"  ×3
22:06:43   relay: admitted endpoint_id=1a0c160324 sponsorship=BudgetBytes(262144)
22:06:44   samsung: "ready, camped on relay"
```

Then the two calls, one Connect tap each, from a fresh `uiautomator` dump every time:

```
22:07:20   samsung → pixel: Connect …  "dial failed: reason=dial failed: no answer within 20s"
           — the PIXEL had been backgrounded ~45 min; its endpoint shuts down on background
           (by design, no foreground service), so nobody was listening. Its screen read
           "ready — NOT camped on relay; calls cannot reach this device" — honest.
22:07:57   pixel foregrounded by the next deep link: relay: admitted endpoint_id=873f3ddc15 …
           (no no_token first: CallPeer re-binds WITH the remembered pass)
22:07:58   pixel → samsung: ONE Connect tap
           pixel:   "connected (outgoing, direct ip:192.168.50.139:50815)  callee"
           samsung: "connected (incoming, direct ip:192.168.50.15:36956)  croftcall-android"
22:08:17   pixel hangs up → "you ended the call — ready, camped on relay"
           samsung: "call ended: closed by peer: hangup (code 0) — ready, camped on relay"
22:09:27   samsung → pixel (pixel foregrounded): ONE Connect tap
           samsung: "connected (outgoing, direct ip:192.168.50.15:36956)  callee"
           pixel:   "connected (incoming, direct ip:192.168.50.139:50815)  croftcall-android"
22:09:46   samsung hangs up → "you ended the call — ready, camped on relay"
           pixel: "call ended: closed by peer: hangup (code 0) — ready, camped on relay"
```

**The relay journal carried NO line for either phone from 22:07:58 through both calls and
both hang-ups.** Both camps held; both directions connected direct on the LAN; the endings
were the same words on both phones, whichever side hung up. Phone-to-phone over our port
on both sides is DONE — `[device done 2026-09-21: samsung↔pixel over call-transport-iroh,
production, both directions]`.

Two things the run surfaced, neither a port defect: **a backgrounded phone is not
callable** (the §16-era policy, now visible because the other phone dialled it — a
foreground service is the fix, a later phase), and the Samsung's failed dial rendered
*"dial failed: reason=dial failed: no answer within 20s"* — uniffi builds a generated
exception's message from the variant's fields (the P7 S1 finding again, in a new coat) and
`CallPeer` prefixed it a second time. Fixed after the run (`CallRefusal.kt`, pinned).

### Two days later — the relayed call over our port, Pixel on LTE (RUN 2026-09-23)

The last owed item: a party off the LAN. The Pixel's Wi-Fi was turned off over adb
(`svc wifi disable`; LTE stayed up), both apps relaunched, the Samsung on the house Wi-Fi.

**The Pixel on LTE took ~65 s to camp** — the tokenless bind was refused `no_token` five
times over 30 s while the mint ran over cellular (IPv6), then *"rebound with a token"* and
*"home relay: https://relay.croft.ing:8443/"* — and **its camp then FLAPPED between calls**:
`home relay: NOT ATTACHED` at 16:33:42, attached 16:33:49, NOT ATTACHED 16:34:00, attached
16:34:16, with the relay journal showing the matching `usage` closes and fresh
`admitted … sponsorship=` lines (the remembered pass re-attaches, never tokenless). The
screen tracked it honestly. Cause not established — a carrier-side idle timeout or the
IPv6 path is the shape of it, and it did not happen on Wi-Fi in any run; recorded as
observed, not explained.

The calls, one Connect tap each, the path line read at +10 s and +25 s:

```
21:32:58   samsung(wifi) → pixel(LTE): Connect
21:33:08   pixel:   "connected (incoming) …: relayed relay:https://relay.croft.ing:8443/"
           samsung: "connected (outgoing, relayed relay:https://relay.croft.ing:8443/)  callee"
21:33:13   both: path change → direct  (samsung sees 174.210.164.32:12660 — the Pixel's LTE
           public address; the pixel sees 136.35.97.150 — the house's)
21:33:32   samsung hangs up → "you ended the call — ready, camped on relay";
           pixel: "call ended: closed by peer: hangup (code 0)"
21:35:39   pixel(LTE) → samsung(wifi): Connect
21:35:48   pixel: "connected (outgoing) …: relayed relay:…"; 21:35:50 path change → direct
           samsung: "connected (incoming, direct ip:174.210.164.32:12660)  croftcall-android"
21:36:14   pixel hangs up → "you ended the call — ready, camped on relay";
           samsung: "call ended: closed by peer: hangup (code 0) — ready, camped on relay"
```

**Both calls connected THROUGH THE RELAY first** — the `relayed relay:https://relay.croft.ing:8443/`
snapshot on both sides — and holepunched to direct across the carrier NAT within ~5 s;
the relay journal carried no line for either phone during the second call. So the
relayed path over our port is exercised and reported on screen, and a call that STAYS
relayed was not observed — iroh's holepunch succeeded across LTE↔Wi-Fi both ways. Owed
item closed: `[device done 2026-09-23: pixel on LTE ↔ samsung on Wi-Fi over
call-transport-iroh, both directions, relayed then direct]`.

One rig trap, again: the Pixel had returned to landscape between runs (`accelerometer_rotation`
back on), which put Connect and the footer below `uiautomator`'s dump — a call that "had no
Connect button". Lock portrait immediately before every Pixel run, not once per session.

### Rig state as left

- **Pixel** — the D3.4 debug build (no upstream iroh in the APK), signed in, camped; same
  key and record as §16. Wi-Fi back on, auto-rotate back on.
- **Samsung** — the D3.4 debug build (fresh install; this machine's debug keystore, so the
  next `adb install -r` from here keeps its data), signed in as `ngvalidation2112`, camped;
  its `self` record names the new id `1a0c160324…`. The CI-signed v0.5.0 is gone from it.
- The arc's `croft-arc-callee` record on account 1 was deleted after the run (getRecord
  400); the phones' `self` records were only read.
