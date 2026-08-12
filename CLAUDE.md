# croft — agent directives

The Croft client: one shared Rust core, thin per-platform shells. These sit **on
top of** the global coding-agents practices (`~/.claude/coding-agents/`) — TDD,
type safety, fail-loud — and add only what is specific here. Git identity:
chasemp (`chase@owasp.org`, `github-personal`).

**Status: skeleton.** Structure and toolchain contract exist; no implementation.
Do not describe anything here as working until it has been run.

## Read before writing code

1. `docs/ADR-0001-client-architecture.md` — the shape, and why. Adopted from
   `discovery`, where it was Accepted 2026-06-22 and demonstrated in code first.
2. `docs/PLATFORM-POSTURE.md` — **before writing any user-facing claim about
   background, offline, or P2P.** The constraints there are platform policy, not
   gaps to engineer around.
3. `docs/CONTRACT.md` — the calling contract lives in `CroftCommunity/connect`,
   not here. This repo consumes it.

## The rules that are load-bearing

- **The core stays pure.** No I/O, no async, no clock, WASM-clean. One `async fn`
  in a core destroys the property that makes the whole architecture testable, and
  a clock read is the classic slow rot. If a core needs the time, it is an
  effect.
- **Effects are data.** `update` is `(model, intent) -> (model, Vec<effect>)`. A
  core emits a request; the shell performs it and feeds the result back as a new
  intent. An awaited port call cannot satisfy that signature — that is the point.
- **Ports are held by the shell, never called by a core.**
- **Per-pond cores.** Do not grow a god-core. A pond's concerns live in its own
  core, never smeared across a shared one.
- **Calling is a capability, not a pond**, and attaches to the *rendered
  principal* seam. If you find yourself integrating calling into a specific
  pond's core, stop — that is the wrong seam and it will multiply.

## Commit gates — a ratchet, not a wall

Gates here **tighten as the repo gains the capability to enforce them**. Writing a
gate you cannot yet run is theatre; skipping the ratchet is how a repo arrives at
1.0 with no gates at all. So each is recorded with the trigger that turns it on.

**Enforced now:**

- **G1 — the commit says why.** Descriptive prose in the house voice, naming the
  reasoning and anything that turned out false. Not Conventional Commits; the
  estate's history reads as prose and consistency beats convention here.
- **G2 — no claim that something runs until it has been run.** The README, the
  changelog and this file all currently say the repo does not run. When that
  changes, the change is part of the same commit as the thing that made it true.
- **G3 — checksums never regress.** Once a value replaces `UNSET`, a commit that
  reintroduces `UNSET` is a defect, not a rollback.
- **G4 — environment changes are journalled.** `ops/JOURNAL.md`, with the reason
  and the outcome including failures.

**Turns on when there is code to run it against:**

- **G5 — `make verify` green.** Trigger: `make bootstrap` has been run
  successfully once, by anyone. Until then a fresh clone cannot pass it and it
  would only train people to skip gates.
- **G6 — `make gate` green** (verify + `cargo test` + unit tests). Trigger: the
  first core lands.
- **G7 — CI runs the same `make gate`.** Trigger: G6. The workspace rule from
  `.claude/CI-PATTERN.md` applies — a workflow without a `pull_request` trigger is
  a notification, not a gate, and a gate nobody has watched fail is
  indistinguishable from one that is not wired. **Watch it fail before trusting
  it.**
- **G8 — contract compatibility.** Trigger: the first shipped app that speaks the
  deep link. A contract change then requires a stated version and a visible
  degrade path (`docs/VERSIONING.md` clock 2).

If you find yourself wanting to skip a gate, the honest move is to move it back
down the ratchet with a recorded reason — not to bypass it quietly.

## The environment refuses, it does not warn

`env/` declares the toolchain; `env/verify.sh` **exits non-zero** on drift. That
is deliberate: `fun` shipped `.nvmrc` *and* an `engines` field, both of which only
warned, and a developer ran the wrong Node for a full day while CI stayed green.

- `make bootstrap` — zero → working, idempotent
- `make verify` — refuses on drift; run it when anything is strange
- `make emulator` / `make emulator-nuke` — the AVD is a definition, not a pet

**Never type a checksum from memory.** `make record-checksums` fetches them from
the publisher. A guessed checksum looks like verification and performs none —
that is strictly worse than the honest `UNSET` that fails the gate.

**Resolve Rust through `rustup which`, never bare `cargo`.** Homebrew's cargo
shadows rustup on PATH and has no wasm std. `rust-toolchain.toml` pins the
channel; it exists because `verify.sh` caught this repo resolving to `stable` on
its very first run.

## The emulator: what it can and cannot tell you

`arm64-v8a` is **required, not preferred**. An x86_64 image can mask a native
library packaging bug that arm64 reproduces — precisely the class of bug in the
inherited croftcall client. A wrong ABI makes the emulator lie.

It **can** answer: startup and linkage failures, UI, deep-link intent handling,
anything relay-mediated. It **cannot** answer whether a real call connects —
emulator networking is NAT'd through the host, so direct holepunching is
unrepresentative and emulator-to-emulator is a topology that exists nowhere.
**Emulator is the iteration surface; two real devices are the proof.**

`make screenshot` captures the screen from a *headless* emulator, so UI can be
inspected without a window and without a human. Reserve human eyes for judgment —
feel, layout, whether it is right — not for confirming something drew.

## Known inheritance

`android/` will take the croftcall client from `CroftCommunity/connect`. It
**crashes immediately on launch** (owner-reported 2026-08-09) and the cause is
**not** established — see `discovery` ROADMAP_TODO **E100**, which ranks two
candidates and asserts neither. Do not record a cause that has not been observed;
read the crash buffer first (`make crash`).
