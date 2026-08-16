# Changelog

All notable changes to `croft`. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning follows `docs/VERSIONING.md` — **three clocks**, of which this file
tracks the product and the contract.

Environment commands and their reasoning live in `ops/JOURNAL.md`, not here. The
changelog answers *what changed for a consumer*; the journal answers *what we did
to the environment and why*.

## [Unreleased]

### Added
- Repo skeleton: shared-core/per-platform-shell layout (`core/`, `shell/`,
  `design/`, `ports/`, `ffi/`, `web/`, `android/`, `apple/`).
- `docs/ADR-0001-client-architecture.md` — the architecture, restated from the
  ADR Accepted in `discovery` 2026-06-22 and demonstrated in code before this
  repo existed.
- `docs/PLATFORM-POSTURE.md` — the per-platform promise. T14 answered: **best
  effort, stated plainly, and not forever**, with three revisit triggers.
- `docs/CONTRACT.md` — points at `CroftCommunity/connect` as canonical rather
  than restating the lexicon, so the contract cannot silently fork.
- `docs/VERSIONING.md` — the three clocks and their different obligations.
- `env/` — the toolchain as code: pinned versions and checksums, an idempotent
  `bootstrap`, a `verify` that **exits non-zero** on drift, and the emulator as a
  definition rather than a pet.
- `rust-toolchain.toml` — added because `env/verify.sh` caught this repo
  resolving to `stable` instead of the declared 1.97.1 on its first run.
- `ops/JOURNAL.md` — the environment command log.

### Decided
- **Calling is a capability, not a pond**, attaching to the shell's *rendered
  principal* seam — one seam rather than one integration per pond.
- **Callability is three states**, not a boolean (`callable` / `not-listed` /
  `may-not-permit`), because it is the relay's admission model and "not listed"
  is the normal case.
- **`arm64-v8a` is required**, not preferred: an x86_64 emulator image can mask
  the native-packaging bug class already suspected in the inherited croftcall
  client.
- **Android first**, Apple second — the code exists there, and it is the more
  forgiving background story.

### Open
- Callability resolution strategy (lazy / cached / batched) — undecided, because
  an always-visible call icon leaks *who you are looking at*, not just who you
  call.
- The push dependency (APNs/FCM in the delivery path of a no-central-operator
  project) — a values question, deliberately not optimised away.
- Whether contract ownership eventually moves here from `connect`.

### Fixed
- `env/bootstrap.sh` — three defects found by its first real run: it assumed a
  usable JDK (the machine has only 8, and `sdkmanager` requires 17+); it gated on
  `/usr/libexec/java_home -v 17`, which **returns the wrong JDK and exits 0** when
  17 is absent; and it could not converge when a JDK cask is left half-installed
  (brew records it, the disk does not have it, `brew install` then no-ops).
- `env/verify.sh` — now searches Homebrew's SDK root, where the cask actually
  installs, not only the Android Studio default.

### Fixed (app)
- **The launch crash is fixed.** `computer.iroh:iroh` is a Kotlin/JVM artifact
  with no Android ABI directory, so `libiroh_ffi.so` was absent and
  `MainViewModel` died on `UnsatisfiedLinkError`. `env/build-iroh-android.sh` now
  cross-compiles it from pinned iroh-ffi source into `jniLibs/arm64-v8a/`. The app
  launches, publishes a real EndpointId, and reports "ready, camped on relay".
- **`env/verify.sh` had a hole and no longer does.** It asserted the rustc
  *version*, which Homebrew's rust matches exactly while shipping a different
  sysroot with zero cross-compile targets — so it passed on a machine where every
  Android build failed. It now asserts the sysroot and, per target, that the
  compiler can actually emit for it (`--print target-libdir`).

### Verified working
- `make bootstrap` completes and is idempotent (a second run no-ops every SDK
  package). It installs Temurin 17 when absent, and stops with an exact command
  when a JDK cask is left half-installed by an unanswered sudo prompt.
- `make verify` — 9 failures before bootstrap, 1 after. The remaining failure (no
  Gradle wrapper) is correct: it arrives with the `android/` shell.
- `make record-checksums` — Gradle 8.13 checksum taken from the publisher's
  `.sha256`, so a corrupted download cannot launder itself into the pin.
- `make emulator` — AVD `croft-dev` boots headless on **arm64-v8a**, API 35,
  `sys.boot_completed=1`, `adb` attached. Confirmed through `adb` rather than
  trusting the script's own "ready" message.
- `make screenshot` — real 1080x2400 capture from a *headless* emulator, so UI is
  inspectable with no window and no human.

### Runs now
- The **android app** (the inherited croftcall client, `ing.croft.call`) builds
  and launches; single-node behaviour — EndpointId, relay-camp, deep-link — is
  emulator-verified per `ops/RUNBOOK-two-device-call-test.md`. Published as
  two-device-test candidate **`v0.1.0-rc.1`**. The Gradle wrapper and `android/`
  shell have landed; the E100 launch crash is rooted.
- **One-app consolidation** (2026-08-16): `connect/android` retired at connect
  v0.2.0; `croft/android` is the sole Croft Call app. `DeepLink` captures the
  connect contract-v2 params (`device`/`grant`). Release process:
  `ops/RELEASING.md`.

### Not yet true
- The **shared core** does not run yet — `core/`, `shell/`, `ports/` are still
  skeleton, and the android app is not rebuilt on them. `verify` still fails on
  the parts that have not landed.
