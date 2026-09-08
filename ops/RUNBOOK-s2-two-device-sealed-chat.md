# RUNBOOK — S2 §14: sealed chat between two real devices

**Status: RUN 2026-09-08/09. Rungs 1-6 GREEN. Rung 7 not attempted.**

Written before the run, deliberately — the plan asks for per-rung expected
output up front so a failure names its rung instead of "it didn't work". The
EXPECTED blocks below are the predictions as written; each rung now carries a
**RESULT** underneath it.

**Where prediction and reality parted, stated plainly:** every rung passed, and
the predictions were accurate about what to look for. What they did not
anticipate was how much would be broken on the way there. Six defects surfaced
between rung 3 and rung 6, **none of which any test tier below this one could
have caught** — two of them silent on the device that had the problem. That is
the finding of this run, more than the green ladder is. They are listed after
rung 7, and each one is fixed with a test that now fails without the fix.

The run was driven over adb (bounds read from `uiautomator dump`, taps and text
injected) rather than by fingers on glass. That is honest for a ladder about
transport and state; it is not a judgment about how any of this FEELS to use,
and nothing here should be read as one.

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

**RESULT — GREEN.** Both packages present on both devices. The calling app was
fingerprinted before and after and did not move:

```
                   BEFORE and AFTER, both identical
Samsung  ing.croft.call  0.5.0  lastUpdateTime=2026-08-28 11:28:50
                         codePath=/data/app/~~BrF3OU81_K2cLct-C2zihA==/...
Pixel    ing.croft.call  0.5.0  lastUpdateTime=2026-08-27 22:10:03
                         codePath=/data/app/~~Ojx12Lj6rv9XNPUPARwTxg==/...
```

Re-checked at the END of the whole run: both timestamps still identical. The P7
standing constraint held throughout, as evidence rather than as an assurance.

**Worth knowing before you install:** the APK is **39 MB**, because
`libcroft_ffi.so` is **29.6 MB** once iroh, iroh-gossip and tokio are linked in
(it was 6.3 MB before S2). That is the measured cost of Q2's Rust-side gossip.

**A trap this run fell into.** `make bindings` builds the DESKTOP cdylib;
`make ffi-android` builds the arm64 one. Building only the former and then
assembling produces an APK carrying a stale `.so`, which installs happily and
fails at runtime. Check `unzip -l <apk> | grep croft_ffi` and match the byte
count against what `make ffi-android` reports.

### Rung 2 — each device stands up its own identity

Launch the social app on both. **EXPECTED:** each shows "No groups yet." and a
distinct principal once a group is founded (the short hex in the members
panel). If the two devices show the *same* principal, the device key was
committed or copied — stop, that is a real defect and not a test-rig quirk.

### Rung 3 — A founds a group and seats real MLS

On the Samsung: New group.

**EXPECTED:** `adb logcat -s croft.social` shows a state line with `groups=1`,
and `hasMlsGroup` is true. The group is at MLS epoch 0.

**RESULT — GREEN**, though the rung as written could not be checked at first:
the app logged no epoch and no MLS seating at all, so there was nothing to read.
That is now in every state line, which is what the plan asked for
("epoch transitions ... log with the group and epoch") and what rung 4 below
depends on.

```
state: groups=1 selected=1 timeline=0 members=1 peers=0 mls=true epoch=0 ...
```

Note `selected` and `peers`: founding a group does not select it, and selecting
it is what starts the swarm link. A group that is not selected has no link, no
pairing code, and an Invite button that is correctly disabled.

### Rung 4 — B's key package reaches A, and A invites

Via whatever pairing step got built.

**EXPECTED on A:** the epoch advances (0 → 1) and logcat records
`invited, epoch advanced`. **EXPECTED on B:** seated from the Welcome, its own
epoch now 1.

**If B refuses the Welcome:** check the epoch on both. A Welcome for an epoch B
cannot reach is the shape a lost commit makes.

**RESULT — GREEN, and the pairing step is a TWO-CODE exchange, not one scan.**

```
HOST   (founded)  groups=1 members=1 peers=0 mls=true  epoch=0
JOINER (paired)   groups=0 members=0 peers=1 mls=false epoch=null
HOST   (invited)  groups=1 members=2 peers=1 mls=true  epoch=1   <- 0 -> 1
JOINER (offered)  groups=0            peers=1 mls=true  epoch=1  offered=true
JOINER (accepted) groups=1                     mls=true epoch=1  offered=false
```

**Why two codes.** The gossip topic IS the group id, so a joining device cannot
reach the swarm until it knows one — and the only place a group id appeared was
inside the record, which arrives over that very swarm. The host therefore shows
a code naming its group; the joiner reads it, joins, and answers with a code
carrying its key package. This was found here, on hardware, because the JVM tier
had been handing the group id from one surface to the other directly and no
phone can do that (defect 2 below).

**The judgment is real and it is visible.** Between "offered" and "accepted"
above, the joiner had `mls=true` and `groups=0` — seated in the lockbox and
absent from the record, because nobody had decided anything yet. What it showed:

```
A device is offering you a group.
group c3e86d5c
founded by fd56b52a
3 assertions, seating 2
  fd56b52a  Owner
  19b4d777  Member
It would seat you.
[Accept]  [Decline]
```

Only after Accept did `groups` become 1. Reading a record folds nothing; that is
pinned in the tests and it is what the phone actually does.

### Rung 5 — the sealed exchange

A sends; B reads. Then B sends; A reads.

**EXPECTED:** each message appears on the other device with the sender's short
principal. Both directions, because one-way would pass with a broken receive
path on the quiet side.

**RESULT — GREEN, both directions, and the two-direction rule earned itself.**

```
HOST    cd60c887: bring bread    19474115: and cheese
JOINER  cd60c887: bring bread    19474115: and cheese
```

Identical timelines, each line carrying its true author. The author is not
supplied by the transport: what is sealed is the whole signed ASSERTION
ENVELOPE, so the far device reads the author out of the envelope it just
opened.

**The first attempt was one-way, and that is exactly what the rung warns
about.** Host to joiner worked immediately. Joiner to host was silently refused
by the host's fold — perfectly asymmetric and perfectly silent: the joiner saw
both sides of the conversation, the host saw only itself, and neither device
said a word. Defect 4 below. Had this rung been written as "A sends, B reads",
it would have passed.

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

**RESULT — GREEN. Both devices were force-stopped, not just one.**

```
HOST    pid 3295 -> (killed) -> 6315
        after relaunch: groups=1 members=2 timeline=2 mls=true epoch=1
JOINER  pid 5775 -> (killed) -> 7774
        after relaunch: groups=1 members=2 timeline=2 mls=true epoch=1

restarted HOST seals ->  JOINER OPENS IT
   both: cd60c887: after the kill
```

The check that matters is the one on the far device, and the far device read it.

**Two things this rung found before it could be run at all**, both of which would
have presented here as "B has gone quiet":

- A device seated by a **Welcome** never wrote its group id down, so the joiner
  came back from a restart with its governance record intact and **no MLS
  group**. Every restart test before this one restarted the device that CREATED
  the group, and creators take a different path (defect 3).
- A restarted host **forgot every credential it had ever registered**, because
  the registry is in memory. It then refused everything its own member sent
  (defect 4, second half).

**Expected, and not a defect: the swarm does not survive a restart.** Links are
process state; after a force-stop the pairing code must be exchanged again
before anything crosses. State survives, the conversation survives, the
*connection* does not. Worth knowing before someone reads `peers=0` as a fault.

### Rung 7 — departure and token return, device to device

Per the plan's Done-when. Expected output to be filled in when the departure
path is wired; it is **not** built as of this writing and should not be
attempted before rungs 1–6 are green.

**RESULT — NOT RUN, and not attempted.** The departure path is still not built,
exactly as this rung said. Rungs 1–6 being green does not change that; it only
means the rung is now unblocked. It stays owed, together with the lost-race
scenario carried from S1.

---

## The six defects this run found

Recorded together because the pattern is the point: **not one of them was
reachable from any tier below this one**, and two were silent on the device that
had the problem. Each is fixed with a test that fails without the fix.

| # | What was wrong | Why no lower tier could see it |
|---|---|---|
| 1 | No `INTERNET` permission, so iroh could not bind a socket | A desktop JVM has no permission model |
| 2 | The pairing code did not carry the group id | The JVM test handed the id between surfaces directly |
| 3 | A device seated by a Welcome never persisted its group id | Every restart test restarted the group's CREATOR |
| 4 | The host had no credential for its own invitee, then forgot it again on restart | Needs two independent identities AND a process boundary |
| 5 | The composer sat behind the keyboard that tapping it opens | No soft keyboard on a JVM |
| 6 | Send sat under the gesture navigation bar (6 reachable pixels) | The emulator's default navigation does not reproduce it |

Defects 3 and 4 are the two silent ones. In both, the device with the problem
looks perfectly healthy and the *other* device simply stops hearing from it —
which is precisely the failure shape this rung ladder was built to catch, and
the argument for the device tier existing at all.

**A seventh thing, not a defect but a trap:** `guard` clears its notice on
success, so a refusal followed by any successful guarded call vanishes before
anything reads it. Defect 1 hid behind that for three attempts. `startLink` now
logs its own failure rather than relying on the notice.

## What this run does NOT show

- **Anything about feel.** It was driven over adb. Nobody has used this with
  their thumbs.
- **Anything relay-mediated, NAT-crossed, or offline.** Both phones were on one
  WiFi with `RelayMode::Disabled`; no relay was contacted at any point, by
  construction rather than by care.
- **More than two devices**, and more than one group per device.
- **Rung 7**, the departure and token-return arc.
- **The lost-race scenario** carried from S1 — two concurrent admissions and the
  losing side's rendering. This run could have staged it and did not.

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
