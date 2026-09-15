# ADR-0004 — The calling transport is a sibling port, and it enforces "a dial never lowers admission"
**Tags:** calling, iroh, ports, admission, relay

**Status:** accepted (2026-09-14 — R2 of
`plans/2026-09-08-plan-call-core-and-apple-shell.md` built it; the §15 defect
was reproduced from a laptop against a port without the rule and did not
reproduce against the port with it).
**Context:** the calling app's endpoint lifecycle lives in Kotlin over
upstream iroh-ffi (`android/app/.../net/CallPeer.kt`), and the one defect
that has cost a two-device run this month (§15.3, 2026-09-08) lived in that
lifecycle: the tokenless dial path re-bound the endpoint to no token over a
live camping pass, and enforce refused the re-attach ~20 times. R1 put the
decision rules in Rust (`core/call-core`, ADR-0002's pond discipline). This
ADR is where the *endpoint* goes, and what it is allowed to do to itself.
Extends ADR-0003 (a port is a realization the shell holds; the core never
calls it).

## The problem

Two questions the plan's D1 asked and a third R2 found.

1. **Where does a relay-attaching endpoint live?** This workspace already has
   an iroh endpoint in Rust: `ports/transport-iroh`, the gossip transport.
   It could grow a calling mode.
2. **What guarantees does that cost?** `transport-iroh` sets
   `RelayMode::Disabled` and configures no discovery service, so it contacts
   no relay **by construction rather than by care** (croft `CLAUDE.md`), and
   P7 S2's severance claim — fabric admission and group admission stay
   separate — rests on that being structural.
3. **Where is R0's rule enforced?** `call_core::dial::rebind` says a dial
   never lowers admission. A rule that exists in a core and is *available* to
   a shell is one the shell can forget to call; the Kotlin's `rebindWithToken`
   applied it, and the defect it fixed was exactly a caller not applying it.

## Decision 1 — a sibling port, `ports/call-transport-iroh`

Not a mode inside `transport-iroh`. Two endpoint kinds in one crate turn the
severance guarantee into a convention: the D1 probe
(`ports/transport-iroh/tests/two_relay_modes.rs`, RUN 2026-09-08) showed the
two *can* share a process without the severed one publishing a relay-shaped
address, so the sibling is a judgment, not a necessity — and the judgment is
that a reader of `transport-iroh` should never have to check whether the
relay is disabled on *this* code path. The shared surface is small (`Endpoint`
construction, `EndpointId`); the rest of `transport-iroh` is gossip and
framing that calling does not want.

The new crate's dev-dependency on `transport-iroh` is for the cross-process
`SwarmLock` only — a test-harness concern — and nothing in `src/` of either
crate names the other.

## Decision 2 — the port owns the endpoint lifecycle, on its own runtime

`CallEndpoint` binds with a persisted key, attaches to OUR relay with the
token the mint issued, dials and accepts over the v0 ALPN, and is a
synchronous surface over a tokio runtime it starts and stops. Same reasoning
as `transport-iroh`: one lifetime, in one language, which is what the Android
FFI will want when D3's switch happens.

The wire format (`wire.rs`) is byte-pinned against `WireFormat.kt`, because
this port dials phones running the released app.

## Decision 3 — R0 is enforced AT the endpoint, not only in the decision layer

`CallEndpoint::rebind(wanted)` consults `call_core::dial::rebind(bound,
wanted)` before touching anything. `Keep` leaves the endpoint exactly as it
is; `Swap` stops it and binds again with the same key, asserting the id
stable. A shell cannot reproduce §15 by asking carelessly, because asking is
all it can do.

**Why the token forces a stop/start, read rather than assumed.** In iroh 1.1
the relay actor reads `auth_token` from the relay map when it *starts* a
relay connection (`start_active_relay`); a live connection keeps the token it
opened with, and `Endpoint::insert_relay` only re-runs address discovery. So
the Kotlin's lifecycle fact — the token belongs to the endpoint — holds on the
Rust side too, and the port models it the same way.

**Measured, both ways, on the staging enforce listener (2026-09-14,
`tests/live_s15_regression.rs`).** Against a port whose `rebind` swapped
unconditionally (the pre-§15.3 shape): `rebind(None)` over a live pass →
`Swapped` → the tokenless re-attach drew ten `denied … reason="no_token"`
lines in ten seconds and the caller was no longer camped. Against the port
with the rule: `Kept`, still camped, the dial connected through the relay,
the hang-up arrived as `ClosedByPeer { code: 0, reason: "hangup" }`, and the
journal carried **zero** lines during the call — §16's device result,
reproduced with no phone.

## Decision 4 — the port emits what the relay cannot see

§15 was diagnosed from the relay's journal because the client said nothing.
This port logs at `debug`, with the short endpoint id on every line: a bind
attempted (token present or not), a rebind decided and why, an attach
confirmed or timed out, a dial placed, a connection made, a hang-up, an
ending. The live test's output can be laid beside the journal by timestamp.

## What this port does NOT decide

- **Whether to mint, or what to mint with.** That is `call-core` (R1) plus a
  shell performing the effect. The port takes a token or none.
- **The words for an ending.** `Ending` is typed data; "you ended the call"
  is a screen's sentence (`CallEnding.kt` today, R4's honesty surface later).
- **Discovery policy.** `Discovery::N0` mirrors the app; `Discovery::None`
  is for tests and rigs whose keys must never be published. Which one a
  production caller uses is R3's call.

## Consequences

- `ports/call-transport-iroh` joins the `ports-and-ffi` CI job (clippy
  DENY-clean and fmt-clean on the commit that adds it, per that job's rule).
- The `:live` regression is `#[ignore]`d and named `live`; the rig recipe is
  `ports/call-transport-iroh/live/README.md`. It reads the relay journal over
  ssh with `sudo`, because without it journalctl prints "No entries" over a
  hint rather than failing — an empty set graded green.
- R1 stops being dead code one rung early: `call_core::dial::rebind` now has
  a production caller. `camp::*` and `dial::plan/action` still wait for R3.
- The Kotlin keeps its own copy of the rule until D3's switch; the matrix
  grades both. Per the two-way-street rule (plan, Reasoning), a defect the
  port surfaces in the Kotlin's lifecycle is fixed on both sides.
