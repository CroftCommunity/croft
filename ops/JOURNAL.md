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
