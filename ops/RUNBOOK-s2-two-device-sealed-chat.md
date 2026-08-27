# RUNBOOK — S2 §14: sealed chat between two real devices

**Status: NOT RUN. Written before the run, deliberately** — the plan asks for
per-rung expected output up front so a failure names its rung instead of "it
didn't work". Everything below the line marked EXPECTED is a prediction, not a
result. When this runs, record what actually happened underneath each rung and
say plainly where prediction and reality parted.

**Written 2026-08-27** by the session that built S2's JVM half, for a fresh
context to execute. Everything it needs is here or linked; nothing depends on
that session's memory.

---

## What is already true, and how to re-verify it in one command each

| Claim | Command | Expected |
|---|---|---|
| MLS state persists | `cargo test -p keylayer-openmls` | **26** passed |
| Sealed chat at Rust grade | `cargo test -p croft-ffi` | **24** passed (22 session + 2 tracing) |
| **Sealed chat through the bindings** | `make bindings` | 12 PASSED |
| The social app runs on arm64 | `make ffi-android` then install | `LOADED AND RESOLVED` |
| The calling app is untouched | `unzip -l …/app-debug.apk \| grep -c social` | `0` |

These counts were re-verified 2026-08-27 against croft `2c50689`. A count that
is *higher* than stated is fine — someone added tests. A count that is **lower**
means something was removed, and that is worth understanding before going near
a phone.

If any of those is not true, **stop** — the device run cannot tell you anything
useful on top of a broken JVM tier, and the plan's checkpoint is exactly that
ordering.

## What this run is for

The one thing a JVM test cannot prove: that two *separate devices*, each with
its own store and its own process, can hold a sealed conversation — and that it
survives one of them being killed.

What it is **not** for: NAT traversal, offline delivery, or anything about the
relay. Q2 put the transport on **iroh-gossip device-to-device** precisely so
fabric admission (M4's live subject, still baking) and group admission stay
severed by construction. **No relay contact at any point in this run.** If you
find yourself reaching for `relay.croft.ing`, you are in the wrong runbook.

## Before you start

1. **Claim the devices.** `CroftC/.coordination/claims/testbed--samsung.md` and
   `testbed--pixel.md`, per `CroftC/.claude/TESTBED.md`. The M4 track uses the
   same phones and is mid-bake; a collision here is two people flashing one
   device.
2. **Do not disturb the calling app.** `ing.croft.call` on both phones is
   running v0.5.0-rc.1 and is the thing being baked. The social app is
   `ing.croft.social` — a different applicationId, installable alongside. If
   you ever find yourself uninstalling the calling app, stop.
3. Serials and adb gotchas: agent memory (`test-devices-samsung-pixel`) and
   `CroftC/.claude/TESTBED.md`.

## The gap you will hit first

**Transport is not built yet.** S2's JVM tier proves seal/open across two
substrates *in one process*; the Welcome and the sealed messages are passed as
byte arrays by the test itself. Nothing yet carries them between two phones.

So this run needs, in order:

1. **The gossip transport** — iroh-gossip, device-to-device, carrying two
   artifact kinds: the Welcome (once, at invite) and sealed application
   messages (ongoing). The app already carries iroh on both sides of the FFI,
   so this is wiring rather than a new dependency.
2. **A pairing step** — how device B's key package reaches device A. The
   simplest honest thing for a dev app is a QR code or a copy-paste blob; it
   does **not** need to be the calling app's exchange-invite machinery, and
   reusing that would drag the contract in.

Neither is written. Budget for them as real work, not setup.

---

## The rungs

### Rung 1 — both apps installed, neither disturbed

```
make ffi-android                       # builds the .so, refuses on non-arm64
cd android && ./gradlew :social:assembleDebug
adb -s <SAMSUNG> install -r social/build/outputs/apk/debug/social-debug.apk
adb -s <PIXEL>   install -r social/build/outputs/apk/debug/social-debug.apk
```

**EXPECTED:** both install. `adb shell pm list packages | grep croft` shows
**both** `ing.croft.call` and `ing.croft.social` on each device. The calling app
is untouched — same version, same data.

### Rung 2 — each device stands up its own identity

Launch the social app on both. **EXPECTED:** each shows "No groups yet." and a
distinct principal once a group is founded (the short hex in the members
panel). If the two devices show the *same* principal, the device key was
committed or copied — stop, that is a real defect and not a test-rig quirk.

### Rung 3 — A founds a group and seats real MLS

On the Samsung: New group.

**EXPECTED:** `adb logcat -s croft.social` shows a state line with `groups=1`,
and `hasMlsGroup` is true. The group is at MLS epoch 0.

### Rung 4 — B's key package reaches A, and A invites

Via whatever pairing step got built.

**EXPECTED on A:** the epoch advances (0 → 1) and logcat records
`invited, epoch advanced`. **EXPECTED on B:** seated from the Welcome, its own
epoch now 1.

**If B refuses the Welcome:** check the epoch on both. A Welcome for an epoch B
cannot reach is the shape a lost commit makes.

### Rung 5 — the sealed exchange

A sends; B reads. Then B sends; A reads.

**EXPECTED:** each message appears on the other device with the sender's short
principal. Both directions, because one-way would pass with a broken receive
path on the quiet side.

### Rung 6 — the one this whole phase exists for: kill and relaunch

Force-stop the social app on A (`adb shell am force-stop ing.croft.social` —
force-stop, not a clean exit, because a clean exit is the case that already
works). Relaunch. Send from A.

**EXPECTED:** B opens it. This is the rung that fails if MLS state did not
survive, and it fails *on B* with a signature or decryption error while A looks
completely healthy — which is why the check is "can B still read A", never "did
A restart cleanly".

That asymmetry is not hypothetical: the JVM tier caught exactly this bug, where
a restart minted a fresh signature key and every seal afterwards was rejected by
the other member with no local symptom at all.

### Rung 7 — departure and token return, device to device

Per the plan's Done-when. Expected output to be filled in when the departure
path is wired; it is **not** built as of this writing and should not be
attempted before rungs 1–6 are green.

---

## Recording the result

Append the results *under each rung*, in the style of §11/§12 of
`RUNBOOK-two-device-call-test.md`: what was run, what appeared, and anything
that surprised you. A rung that passed for a reason you did not expect is worth
more words than one that passed as predicted.

Then update, in the same commit as the results:
- `plans/2026-08-25-1-plan-product-shell-adoption.md` — S2's close-out
- `CLAUDE.md` and `README.md` — status, but **only what was run**
- `discovery/alpha/ROADMAP_TODO.md` — the E137 row

## Known open questions this run will touch

- **E141** (roadmap): the substrate has no group-title mechanism, so the two
  phones will show **different names for the same group** and nothing will
  complain. Expected, not a bug, and the first time it is visible to a person.
  Do not "fix" it here.
- The **lost-race scenario** carried from S1: two concurrent admissions and the
  losing side's rendering. This run is the first time it can be staged for
  real. It is still owed.
