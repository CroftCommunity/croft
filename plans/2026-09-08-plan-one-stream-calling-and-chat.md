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
directory     ing.croft.iroh.endpoint records     a 669-char code carried by hand
              on atproto, per-device              (no discovery service, by design)
rendezvous    our relay, admission-gated          gossip, RelayMode::Disabled,
                                                  no relay by construction
capability    sponsorship/scope passes; minting;  governance tokens (E117);
              live revocation, device-validated   token return on departure — NOT BUILT
```

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
R6  QR over desktop for first contact
R7  capability convergence: rung 7 built once, not twice
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

**R6 — first contact.** A desktop can show a QR that a phone scans, which is the ergonomic
fix for the 669-character paste and answers P7's open question about whether rung 4 should
exercise a camera. Note what the blob actually carries:

```rust
pub struct PairingBlob {
    pub card: DialCard,        // who to dial, and where  ← calling already publishes this
    pub key_package: Vec<u8>,  // MLS key package         ← the bulk of the 669 characters
    pub group_id: Vec<u8>,     // the invitation
}
```

**R7 — capability convergence.** Rung 7 (departure and token return) built against the same
primitive as calling's revocation, informed by the half that is already device-validated.

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

### DS1 — where does first contact's directory live? *(spans both; nobody can settle it alone)*

**Options.** (a) Keep the carried code — status quo, no lookup, nothing leaks. (b) Publish
key packages as atproto records, as calling already publishes endpoint records. (c)
**Group-as-directory**: carried code for first contact only; afterwards the sealed group
*is* the channel, and calling endpoint information travels inside it — no lookup, no code.

**No recommendation is recorded here, deliberately.** (b) is the obvious-looking win and
carries a real cost: contacting a PDS to resolve a key package leaks *"A is interested in
B"* to that PDS, which is exactly the metadata the carried-code design refuses to leak, and
that refusal was a deliberate choice (Q2, honoured in Rust 2026-08-27 at a measured cost of
`libcroft_ffi.so` 6.3 MB → 29.6 MB). Trading it away silently, for ergonomics, would be the
kind of decision this estate writes plans to avoid.

(c) looks like it dissolves most of the pain without the leak, but it is unprobed. **Probe
before deciding:** whether relay admission can be satisfied for a peer known only through
a group — calling's admit currently requires a *published* `ing.croft.iroh.endpoint`
record (this is what `endpoint_unbound` means), so (c) may reduce to (b) for the calling
half regardless of what chat does. If it does, say so rather than shipping a design that
quietly re-introduces the lookup.

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
