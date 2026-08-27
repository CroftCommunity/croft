# Environment journal

Append-only log of commands run **against the environment**, with the reason and
the outcome. Newest at the bottom.

## Why this exists

Infrastructure work is a long tail of one-off commands that vanish into shell
history. Six weeks later nobody can tell which were deliberate, which were
flailing, and which are load-bearing. That gap is where "it works on mine" is
born.

This is a lab notebook, not a transcript. The rule that makes it useful:

> **Record the reason, not just the command.** A command without its why is
> archaeology. A reason without its command is a story.

## What belongs here

- Anything that changes the machine or the toolchain: installs, SDK packages,
  emulator creation, checksum recording, pin bumps.
- Anything you tried that **did not work**, and what it told you. Failures are
  the highest-value entries — they are the ones nobody else can reconstruct, and
  the ones most likely to be repeated.
- One-off diagnostics whose *result* mattered (a version check that revealed
  drift).

## What does not

- Ordinary development commands (`cargo test`, `git commit`). Those are the
  build's job to record, not this file's.
- Consumer-facing changes — those go in `CHANGELOG.md`.

## Format

```
### YYYY-MM-DD — short title
**Why:** what question or need prompted it.
**Ran:** the command(s), verbatim.
**Outcome:** what actually happened, including failures.
**Consequence:** what changed as a result — a file, a pin, a decision, or nothing.
```

---

### 2026-08-11 — recon before proposing an Android loop
**Why:** the croftcall client crashes on launch and the round trip through a
physical phone is too slow to iterate on. Wanted to know what the machine already
had before proposing an emulator.

**Ran:**
```
which adb ; ls ~/Library/Android ; echo $ANDROID_HOME
java -version ; /usr/libexec/java_home -V
gradle --version
ls -a <croftcall scaffold>/
```

**Outcome:** No `adb`, no Android SDK, `ANDROID_HOME` unset. Default JDK is
**Temurin 8** while the project declares `JavaVersion.VERSION_17`. Homebrew
`gradle` is **9.4.1 on JDK 26**. And the croftcall scaffold ships **no Gradle
wrapper at all** — no `gradlew`, no `gradle/wrapper/`.

**Consequence:** The missing wrapper became the headline finding rather than the
missing SDK. With no wrapper there is no pinned Gradle, so two machines building
that APK are not building the same thing — the exact "green locally, red in CI"
class `.claude/CI-PATTERN.md` warns about, except pinned in *neither* place. It is
why `env/verify.sh` treats a missing wrapper as a hard failure and says what the
PATH gradle actually is.

---

### 2026-08-11 — first run of env/verify.sh, before any bootstrap
**Why:** a gate nobody has watched fail is indistinguishable from one that is not
wired. Wanted to see it refuse before trusting it.

**Ran:**
```
./env/verify.sh ; echo "EXIT=$?"
```

**Outcome:** Exited **1**, correctly, listing UNSET checksums, a missing SDK, no
arm64 system image, no adb, and no Gradle wrapper. It also reported
`rust: want 1.97.1, got none` — which was **wrong**: 1.97.1 is installed.

Two separate defects behind one line:
1. **Real:** this repo had no `rust-toolchain.toml`, so `rustup which rustc`
   resolved to the `stable` default rather than the declared channel. A genuine
   drift, on day one, of exactly the kind the manifest exists to prevent.
2. **Mine:** the detection chain (`xargs -I{} {}/rustc`) exited 127 and reported
   "none" rather than failing loudly.

**Consequence:** Added `rust-toolchain.toml` pinning 1.97.1 with the three
targets. Rewrote the check to ask the toolchain rustup resolves *in this repo* —
which is what a build will actually use — and to check targets **per toolchain**,
since a target installed for `stable` is not installed for the pinned channel.
Re-ran: `ok rust 1.97.1`. Watched it fail, watched it pass.

---

### 2026-08-11 — first `make bootstrap`: three failures, each a real finding
**Why:** the env scripts were written but untested (no Android SDK on this
machine). Running them was the test.

**Ran:**
```
./env/bootstrap.sh
```

**Outcome — failure 1: sdkmanager needs JDK 17+.**
```
==> installing declared SDK packages
    platform-tools
This tool requires JDK 17 or later. Your version was detected as 1.8.0_482.
```
The script assumed a usable JDK existed. The machine's only JVM is Temurin 8.
Gradle provisions its own JDK via toolchains, but **the SDK tooling does not** —
so a real 17 must be on disk before anything else works.

**Outcome — failure 2: `/usr/libexec/java_home -v 17` lies.**
Added a JDK check, and it passed while still using JDK 8. Verified directly:

```
$ /usr/libexec/java_home -v 17
/Library/Java/JavaVirtualMachines/temurin-8.jdk/Contents/Home
$ echo $?
0
```

**`java_home -v <N>` does not fail when N is absent — it returns whatever JVM it
has and exits 0.** Any version gate built on its exit status silently accepts the
wrong JDK. Replaced with a check that reads the actual major version out of the
JVM (`java -version`, normalising `1.8.0_482` → 8 and `17.0.9` → 17).

**Outcome — failure 3: the JDK cask needs sudo, and abandoning it leaves a split
state.** `brew install --cask temurin@17` runs a `.pkg` under `sudo` and prompts
for a password. Non-interactively it hangs and is killed. The damaging part is
what it leaves behind:

```
$ brew list --cask | grep temurin     →  temurin@17   (brew: installed)
$ ls /Library/Java/JavaVirtualMachines →  temurin-8.jdk only   (disk: absent)
```

**Homebrew records the cask as installed while the JDK is not on disk**, so a
subsequent `brew install` no-ops and bootstrap can never converge — an idempotent
script looping forever on a state it cannot fix.

**Consequence:** three fixes in `env/bootstrap.sh` — install Temurin 17 when
absent; detect JDK major version properly rather than trusting `java_home`; and
detect the half-installed split state explicitly, then **stop with the exact
command a human must run** (`brew reinstall --cask temurin@17`, since `install`
no-ops). `env/verify.sh` also now searches Homebrew's SDK root
(`/opt/homebrew/share/android-commandlinetools`), which is where the cask puts it
— not the Android Studio default it assumed.

**Blocked on a human:** the sudo prompt. Bootstrap cannot proceed past it.

**Reflection:** every one of these was invisible to inspection and obvious on
first run. The scripts were marked UNTESTED for exactly this reason, and the
marking was worth more than a guess at correctness would have been.

---

### 2026-08-12 — bootstrap completes; the emulator is up and inspectable
**Why:** finish the first bootstrap after the JDK 17 blocker was cleared by hand,
and test the last two untested scripts (`record-checksums`, `emulator`).

**Ran:**
```
brew reinstall --cask temurin@17     # by the owner — sudo prompt, cannot be automated
./env/bootstrap.sh
./env/verify.sh
./env/record-checksums.sh
./env/emulator.sh start --headless
adb exec-out screencap -p > /tmp/croft-screen.png
```

**Outcome — one more bootstrap bug.** `rustup toolchain install X --component
clippy rustfmt` fails: `--component` takes **one value per flag**, so `rustfmt`
was parsed as a second toolchain name. Fixed to `--component clippy --component
rustfmt`. Everything before it (SDK, build-tools, emulator, the arm64 system
image) had already installed, so the re-run was a no-op through that whole
section — **the idempotence claim got tested for free by the failure**.

**Outcome — verify went from 9 failures to 1.** Remaining: no Gradle wrapper,
which is correct and expected — it arrives with the `android/` shell, and until
then a build really would use whatever gradle is on PATH.

**Outcome — checksum recorded from the publisher**, not from a local hash:
`gradle 8.13 → 20f1b1176237254a6fc204d8434196fa11a4cfb387567519c61556e8710aed78`.
Taking Gradle's published `.sha256` rather than hashing a download means a
corrupted fetch cannot launder itself into the pin.

**Outcome — the emulator works, with one benign warning.** `avdmanager` printed
`Error: Could not load devices from .../system-images/.../devices.xml` while
creating the AVD, which looks fatal and is not — the profile applied anyway:

```
hw.device.name  = pixel_6
hw.lcd.density  = 420
ro.product.cpu.abi = arm64-v8a      ← the requirement that actually matters
ro.build.version.sdk = 35
sys.boot_completed = 1
adb devices → emulator-5554  device
```

**Verified rather than trusted:** the script prints "ready", so the state was
confirmed independently through `adb` rather than believing the message. Then
`adb exec-out screencap` from the **headless** emulator produced a real 1080x2400
home screen — so UI can be inspected with no window and no human present.

**Consequence:** the agent-driven loop is live. Human eyes are now needed only for
judgment (feel, layout, whether a thing is *right*), not for confirming that
something drew. `env/toolchain.yml` carries a real checksum; the `UNSET` gate is
satisfied.

**Note for a fresh machine:** the SDK lands at
`/opt/homebrew/share/android-commandlinetools`, not the Android Studio default.
`verify.sh` searches both, so nothing breaks without `ANDROID_SDK_ROOT`, but
exporting it (as bootstrap now prints) keeps every tool agreeing.

---

### 2026-08-12 — the croftcall crash, reproduced and rooted
**Why:** E100 (discovery) recorded the crash with two candidate causes and
asserted neither, because the crash log had never been read. The emulator now
exists, so read it.

**First, a correction to E100's ranking.** The leading suspicion — unresolved
`VERIFY` markers on inferred iroh Kotlin API names — is **refuted**. `connect`
resolved them in `5fa0258` (against n0's reference app) with a follow-up fix in
`7433238` (`readExact` takes `UInt`, not `ULong`, CI-confirmed). The single
remaining grep hit reads `VERIFIED 2026-08-02 (was TO-VERIFY)` — a record of
resolution, not an open marker. The *second* candidate, the packaging class, is
the real one.

**Ran:**
```
./gradlew assembleDebug --no-daemon        # BUILD SUCCESSFUL in 1m 6s
adb install -r app-debug.apk               # Success
adb shell am start -n ing.croft.call/.MainActivity
adb logcat -d
```

**Outcome — reproduced immediately:**
```
java.lang.RuntimeException: Unable to start activity … MainActivity
Caused by: java.lang.RuntimeException: Cannot create an instance of class MainViewModel
Caused by: java.lang.UnsatisfiedLinkError: dlopen failed: library "libiroh_ffi.so" not found
    at ing.croft.call.MainViewModel.<init>(MainViewModel.kt:22)
```

**Root cause — `computer.iroh:iroh:1.0.0` is a desktop JVM artifact, not an
Android one.** Contents of the jar:

```
darwin-aarch64/libiroh_ffi.dylib     14.9 MB
linux-aarch64/libiroh_ffi.so         18.9 MB
linux-x86-64/libiroh_ffi.so          19.7 MB
win32-x86-64/iroh_ffi.dll            16.4 MB
```

**No Android ABI directories at all.** Android's loader looks in
`lib/<abi>/`; every `.so` that reached the APK belongs to JNA or AndroidX:

```
arm64-v8a/libjnidispatch.so, arm64-v8a/libandroidx.graphics.path.so, … (and x86, mips, …)
```

The only `libiroh_ffi` in the APK is `darwin-aarch64/libiroh_ffi.dylib` — a
**macOS** library, swept in verbatim at a path Android will never search.

The assumption is stated in `android/app/build.gradle.kts` itself:

> `// Per n0's reference Android app, this artifact bundles libiroh_ffi.so for
> every Android ABI (no NDK).`

That is **false**, and the same comment already wrote the fallback: *"fall back
to building iroh-ffi from source."* The scaffold predicted its own failure mode
and the prediction was not checked before shipping.

**Honest note on the arm64 argument.** I insisted on an `arm64-v8a` image partly
to avoid masking native-packaging bugs. For *this* bug that was not the deciding
factor — **no** Android ABI is present, so any image would have reproduced it.
The reasoning stands for the general class; it did not earn the credit here.

**Consequence:** E100's cause is established. The fix is to obtain real Android
`.so` files — build `iroh-ffi` per ABI with `cargo-ndk` and package as `jniLibs`,
or use a genuine Android AAR if one is published. Not attempted in this entry;
the finding is the deliverable.

---

### 2026-08-12 — the crash is FIXED; five toolchain traps on the way
**Why:** E100's cause was known (no Android `libiroh_ffi.so`). Build one.

**No AAR exists.** `iroh-ffi`'s `README.kotlin.md` lists Maven Central under
**"Kotlin / JVM"** and documents Android as a *separate build-it-yourself* path.
So the scaffold's claim — "this artifact bundles libiroh_ffi.so for every Android
ABI (no NDK)" — was contradicted by the library's own README all along. Delta
Chat (owner's pointer) confirms the shape: `scripts/ndk-make.sh` sets the NDK
clang linkers per target, builds per ABI, drops each `.so` into `jni/<abi>/`.

**Five traps, each found only by running, each exposing the next:**

1. **Linker configured, C compiler not.** `.cargo/config.toml`'s `linker` covers
   Rust only. `ring` (transitive, via TLS) has a C build script, and `cc` looks
   for an unversioned `aarch64-linux-android-clang` that does not exist in the
   NDK. Needs `CC_`/`AR_`.
2. **API level from a README example.** iroh-ffi's sample uses 29; the app's
   `minSdk` is 26. A lib built against 29 fails on API 26–28 — and an API-35
   emulator would never show it. Caught before it bit.
3. **Toolchain resolved per-directory.** The iroh-ffi checkout has no
   `rust-toolchain.toml`, so it fell back to the rustup default (`stable`), which
   has no Android target → `error[E0463]: can't find crate for 'core'`.
4. **`cargo` resolved from PATH.** Homebrew's cargo precedes the rustup shims and
   ignores `RUSTUP_TOOLCHAIN`. The error *helpfully* says
   `consider downloading the target with rustup target add` — i.e. install
   something already installed, on a different toolchain.
5. **The one that beat my own gate.** Even via `rustup run`, `rustc` resolved to
   `/opt/homebrew/bin/rustc` — **which is the SAME version, 1.97.1**, with a
   different sysroot carrying **zero** Android targets:

   ```
   ~/.rustup/toolchains/1.97.1-aarch64-apple-darwin  android stds: 2
   /opt/homebrew/Cellar/rust/1.97.1                  android stds: 0
   ```

   The workspace already documents "Homebrew shadows rustup". What it had not hit
   is the case where **the version numbers match**, so every version assertion —
   including the one I wrote in `verify.sh` this morning — passes on a machine
   where every cross-compile fails.

**Consequence — the gate was wrong and is fixed.** `verify.sh` now asserts the
**sysroot** is rustup's, notes when bare `rustc` on PATH differs, and replaces
"is the target installed?" (rustup's bookkeeping) with **"can this compiler emit
for the target?"** via `--print target-libdir` + a directory check. Proven to
discriminate: an uninstalled target and Homebrew's same-version rustc both fail
it; the real toolchain passes.

**Result:**
```
libiroh_ffi.so  18,505,096 bytes  ELF 64-bit LSB shared object, ARM aarch64
APK now contains lib/arm64-v8a/libiroh_ffi.so   (was: only darwin-aarch64/*.dylib)
```
App launches, **no fatals**, and the UI shows a real EndpointId
`b729d675…a3a5033` with status **"ready, camped on relay"** — so iroh initialised
and reached a relay, not merely "did not crash".

**Deep link verified**, and a trap worth recording: `adb shell am start -d` with an
unquoted URL lets the shell eat everything after the first `&`, so the intent
arrives truncated and the app renders "(unnamed peer)" — indistinguishable from a
parser bug. Quote the URL; `@alice.bsky.social` then renders correctly. The app
was right and my invocation was wrong.

**Made reproducible:** `env/build-iroh-android.sh` clones iroh-ffi at a pinned tag
(`iroh_ffi_tag` in toolchain.yml), resolves rustc absolutely, refuses a non-rustup
rustc, and installs into `jniLibs/`. The five traps are documented in its header
so the next person does not rediscover them one failure at a time.

### 2026-08-17 — foojay resolver + JDK 21 unit-test launcher

**Why:** the first unit test that constructs `computer.iroh` types
(`PathSummaryTest`, the path-observability instrument) died with
`UnsupportedClassVersionError`: the iroh 1.0.0 jar ships Java-21 bytecode
(class major 65) and the tests ran on the machine's Temurin 17. The APK was
never affected — D8 dexes 21-bytecode fine — only JVM-side tests. Investigating
also showed `toolchain.yml`'s `provisioned_by: gradle-toolchains` claim was
aspirational: no foojay resolver was configured anywhere, so builds silently
ran on whatever JAVA_HOME held.

**Ran:** no machine-level install. Added
`org.gradle.toolchains.foojay-resolver-convention` 0.8.0 to
`android/settings.gradle.kts` and pinned a JDK 21 `javaLauncher` on
`tasks.withType<Test>` in `android/app/build.gradle.kts`; Gradle then
provisioned Temurin 21 itself on the next `./gradlew testDebugUnitTest`.

**Outcome:** all 20 unit tests green (DeepLink 9, WireFormat 5, PathSummary 6);
`assembleDebug` unchanged (compile stays 17). The toolchain claim in
`toolchain.yml` is now true rather than lucky, and the test-launcher exception
is recorded there as `test_launcher_version: "21"`.

**Also learned, on-device:** `Connection.watchPaths()` is unusable from the
Kotlin binding — it fails at runtime with "there is no reactor running, must be
called from the context of a Tokio 1.x runtime" (both devices, iroh-ffi 1.0.0).
`Connection.paths()` works from any thread, so the path summary polls every 2 s
while connected instead. The poll caught a real migration on its first run: the
callee connected `relayed https://use1-1.relay.n0.iroh.link./` and upgraded to
`direct 192.168.50.139:33660` two seconds later.

## 2026-08-21 — the M4d device rig (local admit + phones)

**What:** stood up the first call-time-admission device rig: a local
`croft-relay-admit` on the workstation (memory store, `[mint]` against
production atproto, throwaway keypair from the new `--keygen`), phones on
debug builds carrying `-PcroftAdmitBase=http://<LAN-IP>:8401` (the new
BuildConfig overrides; debug-only cleartext manifest). macOS prompted the
application firewall for both binaries — allow both, or the phone's mint
POST times out while raw TCP still connects (a confusing half-open state
we hit before realizing the first failure was transient).

**Outcome:** the full lifecycle on hardware — real mint (sub-second,
against production plc+PDS), minted-token dial (rebind, EndpointId
stable, connected direct), live revocation ("this invite has been
revoked", no dial, `cap_revoked` at the admit), restore → connected
again. Run record: RUNBOOK §11.

**Learned, the expensive way:** a plain-HTTP local `croft-relay` cannot
host phones. iroh-ffi endpoints never complete the attach against http
(the rust `iroh_relay::client` attaches to the same binary fine —
croft-stack `examples/attach_probe.rs`), and once each phone's discovery
record carries the http relay URL, even LAN-direct dials fail with
`dial failed: null`. Also `computer.iroh.setLogLevel(DEBUG)` produced no
logcat output — native iroh logging on Android is still an open question.
Point phones back at the production relay (TLS) to clear the polluted
records; the on-device enforce rung waits for a TLS staging relay or
admit activation.

## 2026-08-23 — the TLS staging enforce listener goes live (E124)

**What:** deployed `croft-relay-staging` on the production box — a second
croft-relay unit in the same relayns netns, `[::]:8444` DNAT'd next to
8443, same Caddy-cert-via-certsync tmpfs, `admission = "enforce"` with a
fresh STAGING mint keypair (`--keygen`; private half in `CroftC/.env` as
`CROFT_STAGING_MINT_KEY` for the rig's local admit — never on the box).
Production 8443 stayed `admission = "open"` and was not restarted.
Rollback is `systemctl disable --now croft-relay-staging` (ROLLBACK.md).

**Outcome:** enforce is real over TLS: a token-less `attach_probe` is
refused with words ("no admission token"), and the refusal lands in the
journal as `denied … reason="no_token"` — the first enforce refusal ever
produced by this box.

**Learned, immediately:** the deployed v0.1.1 artifact predates the D3
token-claims rework — a camping token minted by today's admit
(sponsorship+scope) fails its `tier`-era deserializer as
`SignatureOrMalformed` even though the signature verifies against the
configured pubkey. Exactly the mismatch class a staging rung exists to
catch before a production enforce flip. Fixed the same night:
croft-relay v0.2.0 released as the CANDIDATE (two CI runs died ENOSPC
first — both workflows now free ~25 GB of preinstalled toolchains before
building) and the staging unit repointed at its own artifact dir
(`/opt/iroh-relay/staging`), so the two units stop sharing a binary the
moment their token formats diverge.

**Loop closed on v0.2.0 (same night):** token-less `attach_probe` →
`denied reason="no_token"`; the camping token from the rig's local admit
(`/campToken`, local-keypair proof, staging mint key) → `ATTACHED` +
`PONG`, and the journal line `admitted endpoint_id=62a611b472
sponsorship=Unlimited` — the FIRST `admitted sponsorship=…` attribution
ever observed (it was on M4d's not-yet-seen list). Production 8443
re-probed token-less: still admitted, shipped clients unaffected. What
remains on this rung is the on-device rehearsal: phones pointed at
`-PcroftRelayUrl=https://relay.croft.ing:8444` with a LAN admit holding
`CROFT_STAGING_MINT_KEY` — and the app's camp-mint flow (the client does
not yet call `/campToken` at attach).

## 2026-08-24 — §12 enforce rehearsal: all rungs green on hardware

**What:** ran runbook §12 end to end — LAN admit (staging mint key,
0.0.0.0:8401), both phones on the staging enforce listener (8444).

**Outcome:** refusal → sign-in → self-minted camping pass → admitted
with attribution → enforced call (both sides carrying passes) →
hang-up with honest words both sides → sign-out refused again. Full
results in RUNBOOK §12. Phones returned to production defaults; LAN
admit stopped; staging listener left running (it is the standing
rehearsal rung).

**Find, fixed same session:** the foreground token refresh and the camp
mint's refresh raced the single-use refresh token
(`invalid_grant: refresh token rotated concurrently`, live entryway).
Fixture upgraded to single-use rotation, race reproduced RED in the
session journey, `freshAccessToken` serialized behind a Mutex.

## 2026-08-26 — `make emulator` had never run on a machine with an SDK

**What:** P7 Phase 0 (probe D3) needed the arm64 emulator. `make emulator`
failed with `env/emulator.sh: line 30: emulator: command not found`.

**Cause:** `env/emulator.sh` resolved the SDK as
`${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}` — the Android Studio default
only. But `env/bootstrap.sh` installs the Homebrew cask to
`/opt/homebrew/share/android-commandlinetools`, and `env/verify.sh` already
probed *both* candidates plus `ANDROID_HOME`. So verify passed and bootstrap
succeeded while the emulator target was broken — the failure mode this repo
names elsewhere: two sources of truth, one of them silently wrong. This machine
also carries a stub `~/Library/Android/sdk` holding platform-tools and no SDK
proper, which is why a bare directory check would not have been enough; the
`cmdline-tools` marker is what distinguishes them.

**Outcome:** `emulator.sh` now uses verify.sh's discovery rule verbatim.
Verified by running it with `ANDROID_SDK_ROOT` and `ANDROID_HOME` both unset —
the case that previously failed — and it reached `==> croft-dev ready`. The
script's header no longer claims to be untested, because it has now been run.

**Also true, and worth recording:** the NDK (`29.0.14206865`), the emulator, the
`arm64-v8a` system image and the `croft-dev` AVD were all present the whole
time. An earlier planning pass concluded the NDK was absent because it inspected
`~/Library/Android/sdk` — the stub. Resolve the path, do not assume it.

## 2026-08-26 — croft-ffi cross-compiles and the emulator loads it (P7 S0)

**What:** S0 needed the android half of its Done-when: the ffi cdylib built for
`aarch64-linux-android` and *loaded* on the arm64 emulator. New script,
`env/build-croft-ffi-android.sh`.

**Cause:** none — this is new capability, not a fix. Recorded because it adds an
`env/` entry point and installs an artifact into the android tree, and G4 says
environment changes are journalled whether or not anything went wrong.

**Outcome:** `libcroft_ffi.so` builds for arm64-v8a (2,146,224 bytes, `ELF
64-bit LSB shared object, ARM aarch64`) and the `croft-dev` emulator both
`dlopen`s it and resolves `uniffi_croft_ffi_fn_constructor_chatsession_open`.
Run, not inferred: `==> LOADED AND RESOLVED`.

**Two things the script does that the iroh one does not, both deliberate.** It
refuses when the attached device is not `arm64-v8a`, because an x86_64 image can
mask exactly the packaging bug the emulator exists to catch. And it refuses when
no device is attached rather than exiting green on a successful build —
producing a `.so` proves the compiler was happy, and only a load proves the
device is. `--no-emulator` says out loud that you are only building.

The build recipe itself is `build-iroh-android.sh`'s, unchanged: direct NDK
clang via `CC_*`/`AR_*`/linker env vars, `rustup which` for both binaries, and
`export RUSTC` because Homebrew's rustc shadows rustup with a sysroot carrying
no Android targets. That script's header lists the five things that must line up
and notes each was found by failing; all five applied here identically, which is
the useful finding — the recipe generalizes with no new surprises.

The `.so` is gitignored for now. Nothing in the app calls it yet, so committing
it would add 2MB to the APK to be carried and never invoked; it lands in git
when S1 links the shell against it.

## 2026-08-26 — `make gate` was not reproducible, and could pass without running

**What:** Running the full gate in a P7 S0 worktree failed at
`:app:testDebugUnitTest` with `SDK location not found`, and the log showed the
FFI wiring test reporting `> Task :test UP-TO-DATE` — skipped, while the build
said SUCCESSFUL. Two separate defects, both found by watching one gate run.

**Cause, defect one.** `android/local.properties` is gitignored — correctly, it
is a per-machine path — but nothing generated it. It had been written by hand
once, in the main checkout, and said `sdk.dir=$HOME/Library/Android/sdk`. Two
consequences. A **worktree has no copy at all**, and this repo's own norm is
that multi-turn work happens in a worktree, so the gate was green in one
checkout and red in another for a reason unrelated to the code. And the
hand-written value pointed at the **stub** — that path holds platform-tools and
nothing else on this machine; the real SDK is the Homebrew cask. That is the
third place the Android-Studio default was assumed, after `emulator.sh` and a
planning pass that concluded the NDK was missing.

**Cause, defect two.** Gradle's up-to-date check for the FFI wiring test knew
about the Kotlin sources and nothing else. The cdylib and the generated
bindings — the two things that actually change between runs, and the two the
test exists to exercise — were invisible to it. So a rebuilt Rust library with
unchanged test code looked like "nothing happened", the task was skipped, and
the build reported success. **A gate that passes without running is worse than
one that fails**, because nothing in the output says so.

**Outcome:** `env/android-local-properties.sh` writes the file from
`verify.sh`'s candidate probe verbatim — the fourth place would have been a
fourth answer — and `make gate` calls it, with `make android-local-properties`
for running it alone. Verified in the worktree that previously failed:
`:app:testDebugUnitTest` BUILD SUCCESSFUL. The gradle test task now declares
the cdylib and the generated bindings as inputs; verified by watching the same
invocation go from `> Task :test UP-TO-DATE` to `> Task :test` with all seven
tests PASSED.

**The pattern, since this is the second time this week:** a target that works
only because of local state somebody set up by hand is not a target, it is a
habit. `emulator.sh` had it, `local.properties` had it. `env/` is supposed to
refuse rather than warn; it cannot refuse over state it never creates.

## 2026-08-27 — the new CI arm went red on its first real run, and could not be reproduced locally

**What:** `ports-and-ffi`, added by S0, failed on the merge push with
`clippy::double_ended_iterator_last` at four call sites in the promoted
store-redb code — while the identical command on this machine passed.

**Cause of the code failure:** four `.range(..).last()` calls on redb ranges.
The range is double-ended, so `next_back()` reaches the same element in one
step where `last()` walks every key in between. Wrong answer, no; wrong cost,
yes, and the cost grows with how much a device has ever written. Real defect,
inherited with the promotion, fixed.

**Cause of the local/CI divergence: not established, and that matters more than
the fix.** Ruled out by testing rather than by reasoning:

- Not the PATH shadow this repo already documents. `/opt/homebrew/bin/cargo-clippy`
  does precede rustup's, but forcing `PATH=$HOME/.cargo/bin:$PATH` changed
  nothing.
- Not a version skew. Both report `clippy 0.1.97 (8bab26f4f6 2026-07-14)`; CI's
  log confirms it synced `1.97.1` from the same `rust-toolchain.toml`.
- Not a stale cache. Reproduced after `touch` and again after
  `cargo clean -p store-redb`.

The remaining difference is the host: CI is `x86_64-unknown-linux-gnu`, this
machine is `aarch64-apple-darwin`. Whether the lint's firing depends on a
platform-conditional `DoubleEndedIterator` impl in redb was not chased — three
attempts to reproduce was the point to stop and let the authority answer.

**Outcome:** **CI is the authority on clippy for this repo, and a local pass is
necessary but not sufficient.** That is a real weakening of "one gate command,
identical locally and in CI" (CI-PATTERN rule 6) and it should be treated as an
open question rather than a settled arrangement. The honest operational rule
until it is settled: do not describe a clippy arm as clean on the strength of a
local run — say it passed locally and wait for CI.

**The wider point.** This is the second lint-shaped surprise this week and the
first one *caught by CI rather than by us*, which is the arm doing exactly the
job it was added for on its very first run. The uncomfortable part is that S0's
own commit message claimed both new crates were "deny-clean today rather than
aspirationally so" — true of this machine, and not true of the gate. Claims
about a gate belong to the gate.

## 2026-08-27 — the social surface runs on the emulator (P7 S1)

**What:** `:social`, a new dev-only android module, installed and driven on the
arm64 emulator: found a group, selected it, typed, sent, and read the message
back off the timeline — all through the real uniffi bindings over a real redb
store on the device.

**Observed, not inferred** (`adb logcat -s croft.social`):

```
state: groups=1 selected=0 timeline=0 members=0 draft=''
state: groups=1 selected=1 timeline=0 members=1 draft=''
state: groups=1 selected=1 timeline=0 members=1 draft='hello from a real device'
state: groups=1 selected=1 timeline=1 members=1 draft=''
```

and on screen: the group row bold, `e8b36870  owner` in the membership panel
with **no** standing label (seated carries no words), and
`e8b36870: hello from a real device` in the timeline.

**The absence proof, which is the point of the module.** The calling app's APK
contains **zero** entries matching `ing/croft/social` or `libcroft_ffi`, and its
native libraries are exactly what they were before this phase
(`libiroh_ffi.so` plus JNA's dispatch libs). The social code is not in `:app`'s
dependency graph, so no configuration mistake can put it there. 164 calling-app
unit tests green and untouched; 15 social tests green; nothing skipped.

**One `env/` change, and it moves an artifact out of the calling app.**
`build-croft-ffi-android.sh` now installs `libcroft_ffi.so` into
`android/social/src/main/jniLibs/` rather than `android/app/src/main/jniLibs/`.
S0 put it in the calling app because that was the only android module; leaving
it there would have quietly undone S1's whole guarantee by shipping 2MB of a
library nothing in the calling app calls. `libiroh_ffi.so` stays where it is,
because that one is genuinely called.

**Half an hour lost to a wrong hypothesis, recorded because the lesson is
cheap.** Tapping a group row appeared to do nothing — no state change, no
refusal, no log. I formed three theories about Compose recomposition and
`ByteArray` equality before instrumenting. The instrumentation showed the
handler was never invoked at all: `uiautomator dump` gave the row's real bounds
as `[32,65][1048,191]` and my taps at y=97 and y=127 had been landing outside
it. The code was correct the whole time. **Dump the bounds before theorising
about the code** — `adb shell uiautomator dump` is one command and would have
answered it immediately.
