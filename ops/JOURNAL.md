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
