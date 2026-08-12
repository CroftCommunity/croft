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

### Not yet true
- Nothing in this repo runs. No core, no shell, no app.
- `env/{bootstrap,emulator,record-checksums}.sh` are **untested** — no Android
  SDK on the authoring machine.
- Checksums in `env/toolchain.yml` are `UNSET`; `verify.sh` fails while they are.
