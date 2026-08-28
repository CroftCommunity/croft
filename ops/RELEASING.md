# Releasing croft — candidate builds and the validation gate

croft has **no CI yet** (gate G7 has not triggered — see `CLAUDE.md`), so releases
are cut **by hand** with `gh`. This file is the process; `docs/VERSIONING.md` is
the policy (the three clocks). A release tags **clock 1 — the product** (SemVer
`0.x`). The contract clock is `connect`'s, not ours.

## The model: candidates, a validation gate, and pruning

An android build is not trustworthy until two real phones have called each other
with it. So we do not publish "releases" that have never connected — we publish
**candidates**, and a build earns promotion by passing the test.

```
build ──► publish as a PRERELEASE candidate  vX.Y.Z-rc.N   (not "Latest")
             │
             ▼
      two-device call test  (ops/RUNBOOK-two-device-call-test.md)
             │
        pass ├──► PROMOTE: cut vX.Y.Z (full release, "Latest") from the same commit
             │
        fail └──► fix, cut vX.Y.Z-rc.(N+1) alongside, test again
```

- **Each candidate is an independent test milestone.** rc.1, rc.2, … each carry a
  build and the record of what it was meant to prove.
- **Prune on next validation.** Keep a candidate published until a **newer**
  candidate passes its two-device test; then delete the superseded prerelease(s).
  Always keep the most-recent-**validated** build (the `Latest` full release) plus
  any in-flight candidate — never a window with no validated build.
- **Assets are versioned:** `croft-call-X.Y.Z-rc.N-debug.apk`, so a phone's
  Downloads folder is not a pile of identically-named files.

## Commands (manual, until CI exists)

Build:

```
cd android && ANDROID_HOME=~/Library/Android/sdk ./gradlew assembleDebug
cp app/build/outputs/apk/debug/app-debug.apk /tmp/croft-call-<ver>-debug.apk
```

Publish a candidate (prerelease):

```
gh release create vX.Y.Z-rc.N -R CroftCommunity/croft \
  --target <commit> --prerelease \
  --title "croft vX.Y.Z-rc.N — <what it's a candidate for>" \
  --notes "…what it is, the validation gate, what a green run does NOT prove…" \
  /tmp/croft-call-X.Y.Z-rc.N-debug.apk
```

Promote a passing candidate to `Latest`, then prune the candidate:

```
gh release create vX.Y.Z -R CroftCommunity/croft --target <same commit> \
  --title "croft vX.Y.Z" --notes "Validated by the two-device call test on <date>, <devices>." \
  /tmp/croft-call-X.Y.Z-debug.apk
gh release delete vX.Y.Z-rc.N -R CroftCommunity/croft --yes --cleanup-tag
```

Prune a superseded, never-validated candidate once a newer one validates:

```
gh release delete vX.Y.Z-rc.M -R CroftCommunity/croft --yes --cleanup-tag
```

## Signing

Candidates are **debug-signed** — installable (allow "install unknown apps"), no
secrets. Debug signing is **not stable for in-place updates** (the key is
per-environment), so treat each candidate as a fresh install; uninstall the old
APK if an update refuses. A release keystore (a shared signing identity across
builds) is the fix when the app has real update-in-place users — the same caveat
`connect/docs/RELEASING.md` records. `versionCode` is monotonic; bump it every
build, never reuse.

## Current

- `v0.5.0` (Latest) — Phase 11 M4: call-time admission, client side
  complete. Promoted 2026-08-28 from `v0.5.0-rc.2` (versionCode 6) on the
  same-day two-device validation with the PUBLISHED APK: both phones
  honestly camped on production, a call born relayed on
  `relay.croft.ing:8443` and upgraded to direct both ways, and the
  endings verbatim — "you ended the call" / "call ended: closed by peer:
  hangup (code 0)". rc.1 is pruned. **Process note, recorded rather than
  tidied:** rc.2 was never published as a prerelease — it was built,
  installed on both phones, validated, and then published directly as
  this release's asset. The gate's substance held (the binary that
  passed IS the binary that shipped, same file) but the ritual differed
  from the diagram above; do it the normal way unless there is a reason
  not to. Why rc.2 exists at all: rc.1's
  E135(a) fix was proven blind on hardware (§13 step 3) — it polled
  `addr().relayUrl()`, which reports the configured relay even while the
  relay refuses every attach — so promoting rc.1 would have shipped a
  build that says "camped" to an unreachable phone. rc.2 carries the
  `Endpoint.online()` signal, device-verified in BOTH states.
- ~~`v0.5.0-rc.1`~~ (pruned; superseded by rc.2) — Phase 11 M4: call-time admission, client side complete
  (mint-at-dial, camp-at-attach with the pass as the cache, the three
  call-endings with words, the refresh-rotation race fix). Cut 2026-08-24
  from the same-day §12 enforce rehearsal: every rung green on hardware —
  signed-out camp refused, self-minted camping pass admitted with
  sponsorship attribution, the first call with both sides holding passes
  on an enforcing relay, the endings' words verbatim on both screens,
  sign-out refused again (runbook §12). On promote → `v0.5.0`. The
  validation gate note ("§12 ran a local debug build; promotion wants
  the published APK through the two-device test") was SATISFIED
  2026-08-26: §13 steps 2+4 ran with THIS published APK on both phones
  against production — first attributed camp mint, first attributed
  call, E129 endings verbatim (runbook §13 results). **Promotion to
  v0.5.0 is unblocked; the promote itself is the owner's call** (§13
  step 1).
- `v0.4.0` — Phase 11 M2+M3: identity proof (atproto OAuth
  sign-in → `provenDid`) and the derived callability line on the callee
  card. Promoted 2026-08-18 from `v0.4.0-rc.1`, which was cut 2026-08-17
  from the same-day on-device validation: live OAuth against the
  bsky.social entryway (PAR + DPoP + PKCE, redirect on
  `ing.croft.connect:/oauth`), DID surviving force-stop, and the flip
  observed both directions with live records (signed in: callable via
  grant m3registered; signed out: may not permit, immediately). The
  candidate is pruned. Next: Phase 11 M4 (call-time `evaluateGrant` +
  relay enforcement, gated on decision D3).
- `v0.3.0` — Phase 11 M1: ticket redemption (invite link →
  public-record resolution → verified secret → callable contact). Promoted
  2026-08-17 from `v0.3.0-rc.1` on the same-day on-device validation
  (redeem → dial → connected via our relay, roles reversed); the candidate
  is pruned. (Ledger note: this entry was stale until the M3 docs pass —
  the promote happened hours before it was recorded here.)
- `v0.2.0` — rung 3: the endpoint camps on `relay.croft.ing:8443`
  (custom RelayMap over the preset) and the call screen/logcat report the live
  connection path. Promoted 2026-08-17 from `v0.2.0-rc.1` on the same-day
  on-device validation (split-network dial ~4.1 s, both sides born relayed on
  our relay, both upgraded to cross-network direct); the candidate is pruned.
  Next: Phase 11 (cap/admission; `RelayConfig.authToken` is the client hook).
- `v0.1.0` — the one-app consolidation + the connect contract-v2 deep link.
  Promoted 2026-08-17 from `v0.1.0-rc.1` after the two-device call test passed
  (rungs 0–2; `ops/RUNBOOK-two-device-call-test.md` §8); the candidate is
  pruned.
