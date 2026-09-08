# Plan — one stream: calling and chat are the same client, and the roadmap should say so

**Status:** PROPOSED 2026-09-08. This is a **program document**: it sequences work that
lives in child plans, and it holds the decisions that no single child can settle because
they span both workstreams. It builds nothing itself.

**Children:** `2026-09-08-plan-call-core-and-apple-shell.md` (the enabling work, R1–R4
below) · `2026-08-20-1-plan-m4-call-time-admission.md` (M4, largely delivered) ·
`2026-08-25-1-plan-product-shell-adoption.md` (P7) ·
`ops/RUNBOOK-s2-two-device-sealed-chat.md` and `ops/RUNBOOK-two-device-call-test.md` (the
evidence tiers for both).

## Problem Statement

**Calling and chat have been run as two workstreams, and they are one client.** The
architecture already says so, in this repo's own rules:

> **Calling is a capability, not a pond**, and attaches to the *rendered principal* seam.

P7 S2 (closed 2026-09-08/09) produced this system's first real rendered principals —
sealed chat between two phones, governance folded, a sender's principal rendered from
inside the envelope. M4 (§15, 2026-09-08) produced calling with live admission under
enforcement. The seam where they are specified to meet has never been buildable, for a
reason that is mechanical rather than architectural:

```
CALLING (shipped, v0.5.0)   Kotlin 2,902 LOC ─→ computer.iroh (upstream iroh-ffi) ─→ iroh
SOCIAL  (P7 S2, dev module) Kotlin ─→ our libcroft_ffi.so ─→ our Rust ─→ iroh
```

Two independent iroh integrations in one APK, in two stacks. Nothing can attach calling to
a rendered principal across that gap, so the product moment the architecture describes —
*call the person you are talking to* — has no home.

**Two further problems are shared and are currently being solved twice, or not at all.**

**1. Failures here are asymmetric and silent on the afflicted side.** Four defects across
two independent sessions, one shape:

| Where | Defect | What the broken device showed |
|---|---|---|
| P7 S2 | a Welcome-seated phone forgot its group on restart | healthy |
| P7 S2 | a host had no credential for its own invitee | healthy |
| §15 | dead OAuth refresh token, phone unreachable | `Signed in` |
| §13/§15 | relay refusing every attach | `ready, camped on relay` |

In every case the device with the problem believes it is fine and the peer experiences
only absence. Calling has since derived a rule from this — E135(a): *a claim must come
from a live signal, never from stored state* (`Endpoint.online()` is the truth;
`addr().relayUrl()` is a stored intention that lies under refusal). **The social side has
no such rule**, and its two silent defects are what the absence produces.

**2. The three primitives have grown twice, at different settings:**

```
              CALLING (M4)                        SOCIAL (P7)
directory     ing.croft.iroh.endpoint records     a 669-char code carried by hand;
              on atproto, per-device — the        the NON-local case is UNBUILT,
              LOCAL case is UNBUILT               not refused
rendezvous    our relay, admission-gated          gossip, RelayMode::Disabled,
                                                  no relay by construction
capability    sponsorship/scope passes; minting;  governance tokens (E117);
              live revocation, device-validated   token return on departure — NOT BUILT
```

**The directory row is a mirrored diagonal, and reading it wrong is easy.** Each side built
one case and neither built the other's — chat has local, calling has non-local. An earlier
draft of this plan mistook chat's missing half for a deliberate refusal and asked the owner
to choose a privacy posture; the owner corrected it (2026-09-08): QR-or-code is the
intended *local* baseline and stays, and chat being more restrictive than calling **for the
social tree** is not intended, merely unbuilt. R6 and R8 build the two missing cells. The
`RelayMode::Disabled` guarantee is real and is about **transport**, not about whether a
person can be found — treating it as the latter is a layer-collapse.

The capability row is the sharpest: calling's revocation is built and device-validated,
social's token return (rung 7) is unbuilt, and building it twice is how the two drift.

## Approach

**One stream, in dependency order.** Cores stay separate throughout — "per-pond cores, do
not grow a god-core" is existing law and nothing here relaxes it. What merges is the
shell, the FFI, and the three primitives underneath.

```
R0  fix the §15 dial defect (Kotlin)         ── independent, urgent, blocks nothing
R0b the shared honesty invariant             ── cheap; retires a defect class in both
         │
         ▼
R1  call-core: the decision rules            ─┐
R2  the calling transport port                │  child plan:
R3  headless binary — THE FAST LOOP           │  call-core-and-apple-shell
R4  one FFI + shell/'s first occupant (macOS)─┘
         │
         ▼
R5  the rendered-principal seam: call from a chat member list   ← the product moment
R6  first contact LOCAL — one QR card, chat + calling
R7  capability convergence: rung 7 built once, not twice
R8  first contact NON-LOCAL — reaching someone through the social tree
```

**R0 — fix the dial defect first.** §15: one Connect tap tears down the caller's camped
connection, costing reachability for seconds or minutes, silently. It is shipped, it is
live under enforce, and it must not wait behind an architecture migration.

**R0b — the shared honesty invariant.** Write E135(a)'s rule once, as a rule both cores
are held to: *no surface claims a capability it has not confirmed against a live signal.*
Enforce it the way calling already does — a matrix row with a test. This also settles one
of P7's open UI questions: "you are already in this group" **must** be a distinct outcome,
because an Accept that silently no-ops is the same lie in miniature.

**R1–R4 — the enabling work.** Specified in the child plan. Additive: Android is untouched
until the core is proven by a second shell, and the enforcement matrix grades both
implementations so a row passing in one and failing in the other is the finding.

**R5 — the seam.** Calling reachable from a rendered principal. This is what the merge is
*for*; everything above it is cost.

**R6 — first contact, local: one card, both capabilities.** A desktop shows a QR that a
phone scans — the ergonomic fix for the 669-character paste, and P7's open question about
whether rung 4 should exercise a camera. **And it carries calling contact information too**
(owner, 2026-09-08): exchanging who-to-call in the same in-the-room way should not require
going through atproto.

The gap this stream closes is a mirrored diagonal, not a one-sided hole:

|  | local (QR / carried code) | non-local (social tree, atproto) |
|---|---|---|
| **chat** | **BUILT** — the pairing blob | **MISSING** → R8 (no key-package record exists) |
| **calling** | **MISSING** → this phase | **BUILT** — `ing.croft.iroh.endpoint`, published |

Each side built one case; neither built the other's. What chat already carries locally:

```rust
pub struct PairingBlob {
    pub card: DialCard,        // who to dial, and where — chat's iroh endpoint
    pub key_package: Vec<u8>,  // MLS key package — the bulk of the 669 characters
    pub group_id: Vec<u8>,     // the invitation
}
```

**The two halves cost very different amounts, and the plan should not pretend otherwise.**
Calling's local card is close to already existing: `croftcall://call?endpoint=…&handle=…&grant=…`
is a calling contact card in URL form, it is already handled by `DeepLink`, and §13/§15
drove real calls from it. A QR is an *encoding* of that, plus a scanner — largely UI work,
no new protocol. R8, by contrast, needs a new record type and a lexicon investigation.
Sequence accordingly rather than treating "first contact" as one uniform job.

Open question for R6, not settled here: whether the two capabilities travel as **one card**
carrying both reachabilities, or two artifacts scanned separately. One card is the better
product and makes the shell's job simpler; two is easier to ship incrementally and avoids
coupling chat's pairing format to calling's contract version (`docs/VERSIONING.md` clock 2
governs the second). Decide when R6 is picked up, with the deep-link contract in hand.

**R7 — capability convergence.** Rung 7 (departure and token return) built against the same
primitive as calling's revocation, informed by the half that is already device-validated.

**R8 — first contact, non-local: reaching someone through the social tree.** R6 and R8 are
the two cases of one feature ("how a person joins a group"), and only R6 exists today.

Measured state, 2026-09-08: `ing.croft.iroh.endpoint` is published and public — it is how
calling reaches a device — while an MLS **key package exists only inside the carried
pairing blob**, with no record type at all. That asymmetry is the whole of the gap.

- **What it needs:** a published key-package record, so a person can be invited without
  being handed 669 characters. That is a new `ing.croft.*` type and therefore
  `LEXICONS.md`'s four acts, **investigate first** — search the official lexicons *and*
  `community.lexicon.*` *and* what we already consume, and record what was checked. MLS has
  a Delivery Service concept and a published-key-package shape may already exist; minting
  ours before looking is the failure that document exists to prevent. Validate on the way
  **in**: a real PDS accepted a record missing every required field.
- **What it does NOT need: the ring walker.** Worth stating because the two look alike and
  are not. *Inviting a specific person is a point lookup* — handle → DID → their key
  package — exactly the shape calling already uses to resolve a callee. *Browsing your
  tree* is a graph walk, and that is the ring walker
  (`discovery/alpha/research/ring-walk-sans-relay-2026-09.md`, owner decisions 2026-09-08:
  TypeScript, a package inside croft-pwa, consumed by forage / pdsview / the social-tree
  site). Dragging a browser-side TS package into a native Rust client to invite one person
  would be building a walk where a lookup was wanted. If the native client ever wants to
  *browse*, that is a separate decision with the cross-repo constraint below.
- **Bar:** no more restrictive than calling. Not more open either — calling's reachability
  is already public and R8 should match it, not exceed it.

**How code in this stream is allowed to travel** (`CroftC/.claude/SHARED-CODE.md`, landed
2026-09-08 while this plan was being written). R1–R5 are unaffected: `call-core`,
`chat-core`, the ports, `ffi` and `shell` are members of one Cargo workspace, and rule 1
explicitly permits path dependencies *inside* a repo — the rule is about the repo boundary.
**R3 is the one phase that touches a boundary.** croft-stack already has `attach_probe`,
which mints and attaches against the real relay, and a headless croft binary wanting that
logic must take it as a **git dependency pinned to a commit**, or leave it in croft-stack
and drive it — never a copy. A copy would be debt with a name (rule 4) and would land as a
FLAG on its first audit. The same applies to anything R7 wants from croft-stack's admit.

*Recorded because it is a live false positive, not a rule to work around:* check 47a
currently FLAGs `croft/ffi/Cargo.toml` for `path = "src/bin/uniffi-bindgen.rs"`. That is a
`[[bin]]` target path in uniffi's recommended layout, the file exists in-repo, and it is
not a dependency. Reported to the dimension's author 2026-09-08; croft should read GREEN on
47a. Do not restructure `ffi/` to satisfy it.

## Reasoning

**Why one stream rather than two that coordinate.** The three primitives are already
duplicated and already diverging — one has revocation, the other does not; one has a
directory, the other has a 669-character paste. Two streams that coordinate would keep
producing two answers and reconciling them afterwards. The merge is not an efficiency
argument; it is that *the second answer is usually wrong and nobody notices until a device
run*.

**Why the cores still do not merge.** Per-pond cores is law, and it is right. Calling is a
capability that attaches to a principal, not a pond with its own concerns. Merging
`call-core` into `chat-core` would put calling inside a specific pond — the exact move the
rule says to stop and reverse.

**Why R0b is early and cheap.** It is a rule plus tests, not a build. It retires a defect
class that has already produced four defects across two sessions, and each of those cost a
device run to find. Anything that moves that class below the device tier pays for itself
immediately.

**Why R5 is late and should stay late.** It is the product moment and therefore the
tempting thing to pull forward. It cannot come earlier without either merging the cores
(forbidden) or building the seam twice (the thing this plan exists to stop).

**What this stream does not do, and what still needs two phones and a person.** Stated
plainly because a fast desktop loop is exactly the kind of green that gets over-read — the
same misreading §13 made with attributed `usage` lines:

- NAT traversal across networks, cellular, mobile lifecycle (backgrounding, doze, process
  death) — the runbook ladder's territory, unchanged.
- **The lost-race scenario** (S1, and P7 S2 had two live devices and did not stage it —
  recorded as a miss, not dropped).
- **Feel.** Nobody has used sealed chat with their thumbs; it has only ever been driven
  over adb. A macOS shell does not touch this and must not be claimed to.
- The paste advisory expires **2026-11-29** and needs re-checking then, not silently
  renewing.

## Open decisions

### DS1 — RETIRED as posed. It was a missing feature, not a decision.

**An earlier draft of this plan asked which privacy posture the merged product should
commit to**, having observed that calling publishes `ing.croft.iroh.endpoint` records
publicly while chat refuses even a lookup. **The owner corrected the premise
(2026-09-08):** QR-or-code is the intended *local* baseline and stays; chat being *more
restrictive than calling for the social tree* is **not** intended — that case simply has
not been built.

So there is no posture to choose. There is a second case to build (R8), and the bar it
must meet is *no more restrictive than calling*, which already reaches people through
atproto identity and the graph.

Recording the mistake because the shape recurs: **an unbuilt path read as a deliberate
refusal.** The evidence for "deliberate" was real but partial — `RelayMode::Disabled` and
the 23 MB paid for it are genuine, and they are about *transport*, not about whether a
person can be found. Reading a constraint at one layer as a policy at another is the
layer-collapse this estate keeps writing rules against, and it produced a plan section that
asked the owner to decide something nobody had proposed.

### DS2 — inherited from the child plan, restated because it now spans both

**D1: where the calling endpoint lives.** `ports/transport-iroh` contacts no relay *by
construction*, and P7 S2's separation claim rests on that. A crate holding both endpoint
kinds downgrades a structural guarantee to a conventional one. Recommendation stands at a
separate sibling port, with the coexistence probe run first.

### DS3 — does Android switch onto the core at all?

Unchanged from the child plan and still genuinely open: a defensible outcome is macOS on
the core, Android staying Kotlin, and the matrix grading both — duplication accepted
deliberately rather than by drift. Decide after R3.

## Review Log

*(empty — this plan has not been reviewed)*
