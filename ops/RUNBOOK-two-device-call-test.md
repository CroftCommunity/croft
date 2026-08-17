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

1. **Rung 3 — our own relay.** Repoint from `presetN0()` to `relay.croft.ing` and
   repeat rung 2 (needs the custom relay map enabled in `CallPeer.kt`). This is
   what proves the Membership/relay side end to end. Cut as its own candidate.
2. **Phase 11 — the cap/admission layer.** The contract for *who may call* is
   built and canonical on `connect` (contract v2). The client-side work is
   specified in **`CroftCommunity/connect` `docs/PHASE11-HANDOFF.md`**: a
   **callability resolver** (the rendered-principal seam), **OAuth identity proof**
   to obtain `provenDid`, and **`evaluateGrant`** as an effect at call time. Each
   is a milestone of its own, published and validated the same way (candidate →
   two-device/behaviour test → promote).

So the ladder does not stop at "a call connected": it climbs to our relay, then to
admission. This runbook validates the bottom rung; the handoff carries the rest.
