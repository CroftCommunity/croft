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

- `v0.1.0-rc.1` — the first candidate: the one-app consolidation + the connect
  contract-v2 deep link. Awaiting the two-device call test. On pass → `v0.1.0`.
