# croft — agent directives

The Croft client: one shared Rust core, thin per-platform shells. These sit **on
top of** the global coding-agents practices (`~/.claude/coding-agents/`) — TDD,
type safety, fail-loud — and add only what is specific here. Git identity:
chasemp (`chase@owasp.org`, `github-personal`).

**Status: the shared core is real; the android app runs and is released.**
`core/social-tree-core` holds the governance fold and the admission
machinery (E117 P1–P4: CONTESTED, real Ed25519, §7.6.4 removal kinds, the
token/admission facts), `ports/keylayer-openmls` realizes the KeyLayer port
on real openmls — both admission paths green end-to-end at loopback
(`ports/keylayer-openmls/tests/loopback_e2e.rs`); `shell/` is still
skeleton. The **android
app** — the inherited croftcall client — builds, launches, and is published as
**`v0.4.0`** (Latest): camps on **our relay** (`relay.croft.ing:8443`),
reports the live connection path, redeems exchange invite links (Phase 11
M1, contract §6), and proves caller identity via **atproto OAuth**
(Phase 11 M3: sign-in from the This-device card → durable `provenDid` →
a derived callability line on the callee card). Rungs 0–3 of the
two-device ladder and Phase 11 M1–M3 are all validated on real devices,
2026-08-17 (`ops/RUNBOOK-two-device-call-test.md` §8/§5-rung-3;
`plans/2026-08-17-2-plan-m3-identity-proof.md` close-out). **M4 is in
progress** (plan `plans/2026-08-20-1-plan-m4-call-time-admission.md`): D3
is decided, the relay side is built (croft-stack Phase 8 — `/grantCall`
mints sponsorship+scope tokens), and client chunks M4a+M4b are landed
with the **workflow harness** (`android/.../workflow/` — journey tests
over the real ports against `FixtureExchange`; first-class per the plan).
M4c (mint-at-dial) is landed, and **M4d's first device run is done
2026-08-21** (runbook §11): real mint from a phone, minted-token dial
(EndpointId stable), live revocation refused with words, recovery.
O1 was decided AND built 2026-08-21/23 (croft-stack: `/campToken`
behind a pluggable proof seam; the TLS staging listener enforces live
on relay.croft.ing:8444 running the v0.2.0 candidate), and **M4e
(camp-at-attach) landed under tests 2026-08-23**: `CampAdmission` +
`Admit.campToken` + the ViewModel trigger — the pass is the cache
(wire `expiresIn`), refusals camp tokenless with words. **The three
call-endings landed under tests 2026-08-23 (E129)**: the app holds the
live Connection, a closed()-watcher ends the state honestly (before it,
a remote ending left the UI stuck at Connected), Hang up closes with
"you ended the call", and Ended keeps the endpoint bound — still
camped, still callable. **The §12 enforce rehearsal RAN
2026-08-24, all rungs green** (runbook §12 results): refusal on
hardware, the self-minted camping pass admitted with attribution, the
first fully-enforced call (both sides holding passes), the endings'
words verbatim on both screens, and the sign-out negative. The run's
find — a refresh-token race between the foreground refresh and the camp
mint — was reproduced in the harness and fixed the same session
(single-use rotation in the fixture, `freshAccessToken` serialized).
M4's remaining distance is operational: croft-admit activation and the
production enforce flip (owner-gated, croft-stack `TODO.md`), plus the
small closures (optimistic-Ready honesty, caller-side camp posture,
E125–E128). Do not
describe anything else here as working until it has been run.

## Read before writing code

1. `docs/adr/0001-client-architecture.md` — the shape, and why. Adopted from
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
  changelog and this file track what runs and what does not — the shared core does
  not yet; the android app does (published as `v0.4.0`). When that changes,
  the change ships in the same commit as the thing that made it true.
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

## The android app (inheritance complete)

`croft/android` is **the** Croft Call app. The croftcall client inherited from
`CroftCommunity/connect` now lives here and launches (the E100 launch crash was
rooted — see `discovery` ROADMAP_TODO **E100**; `make crash` reads the buffer if
it recurs). `connect/android` is **retired** as a stopgap — its last release is
connect **v0.2.0**, no further development there. The two apps are now one.

Contract **v2** (per-device `listRecords` + the capability model) is landed and
canonical on connect `main`; `DeepLink` here captures its `device`/`grant` params.
The client's Phase 11 work is specified in `CroftCommunity/connect`
`docs/PHASE11-HANDOFF.md`; of its items, ticket redemption (M1, v0.3.0),
the callability resolver (M2), and OAuth identity proof (M3, v0.4.0) are
**shipped and device-validated** — the engine lives in
`android/.../caps/` behind injected `Http`/`HttpForm` ports, effects at
the edges. What remains is call-time `evaluateGrant` as an effect + relay
enforcement (M4, with croft-stack). Build against v2, never the
single-record shape.

## Concurrent sessions (workspace norm)

Multiple agent sessions share the `CroftC/` workspace. Do multi-turn work in a dedicated
worktree — `git -C croft worktree add ../worktrees/croft/<slug> -b claude/<slug>` — never in
this checkout (peer sessions stage with `git add -A`; loose files get swept into unrelated
commits). Contested surfaces here — claim in `CroftC/.coordination/claims/` before
touching: **landing on `main`**, the contract consumption surface (`docs/CONTRACT.md` — connect owns the canonical contract). Full protocol and the reasons behind it: `CroftC/.claude/COORDINATION.md`.
