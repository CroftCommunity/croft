# Plan — the apple shell: filling `call-core` so the calling arc can be exercised without two phones

**Status:** PROPOSED 2026-09-08. Not started. D1–D3 are open and are the point of
this document; nothing below D1 should be built until it is settled.

**Motivated by:** `ops/RUNBOOK-two-device-call-test.md` §15 (2026-09-08) — the first
phone-earned admission lines under enforce, and the dial defect found in the same run.

## Problem Statement

**Finding a client defect currently costs two physical phones, a borrowed SIM, and a
forty-second edit-to-observation cycle.** §15 found that a single Connect tap tears down
the caller's camped relay connection, costing reachability for four minutes in the worst
observation. That defect is nine days old — it has been live since the enforce flip — and
nothing below the two-device tier could see it. The tiers that exist all stop short:

| Tier | Can it see the dial dropping the camp? | Why not |
|---|---|---|
| Kotlin unit / journey tests | No | `FixtureExchange` fakes the ports; no real iroh endpoint exists to tear down |
| `cargo test` (our Rust) | No | Calling is not in our Rust at all |
| `attach_probe` (croft-stack) | No | Attaches and holds; it never dials, so it never exercises the path |
| Emulator | Partially, at best | Relay-mediated behaviour is visible, but it is one endpoint and the networking is NAT'd through the host |
| Two physical phones | **Yes** — this is how it was found | The most expensive tier we have |

The reason the cheap tiers cannot reach it is structural, not an oversight in test
coverage. **The calling app reaches iroh through upstream Kotlin bindings, not through our
Rust**, and there are two independent iroh integrations in one APK today:

```
CALLING (shipped, v0.5.0)   Kotlin 2,902 LOC ──→ computer.iroh (upstream iroh-ffi, Maven
                                                  Central, bundles libiroh_ffi.so) ──→ iroh
SOCIAL  (P7 S2, dev module) Kotlin ──→ our libcroft_ffi.so ──→ our Rust
                                        (ports/transport-iroh, RelayMode::Disabled) ──→ iroh
```

The endpoint lifecycle — construct, attach, camp, dial, tear down — lives in
`net/CallPeer.kt` and `MainViewModel.kt`, driving upstream bindings. It is reachable from
Kotlin instrumentation and from a real device, and from nowhere else. So is every future
defect of the same shape.

Meanwhile `core/call-core/` has been an empty `.gitkeep` since 2026-08-11, and
`Cargo.toml` does not list it as a workspace member. `core/feed-core/` and `shell/` are
the same. The architecture named a home for this work and it was never filled.

**What we want:** to exercise the full calling arc — sign in, camp, be refused, mint, be
admitted, dial, hang up — on a laptop, against the real relay, in seconds, without adb and
without borrowing a phone that has someone's SIM in it.

## Approach

Fill `core/call-core` with the calling logic in Rust, and build the macOS client on it.
Do it **additively**: Android is not touched until the core has been proven by a second
shell, which honours P7's standing constraint that every phase stay additive while
croftcall bakes.

**Phase 0 — the decisions and their probes.** D1–D3 below. Each carries a probe that
must run before the decision is taken, because the last three architectural surprises in
this repo (`relayUrl()` under refusal, the sqlite `StorageProvider` on wasm32, upstream
Q2's premise) were all found by probing an assumption that read as obviously true.

**Phase 1 — the decision rules, ported and dual-graded.** Move the pure decision logic —
`CampAdmission`, `DialAdmission`, the callability derivation in `caps/` — into
`core/call-core` as an `update(model, intent) -> (model, Vec<effect>)` core. These are
already pure functions with ports injected and effects at the edges, so this is
translation rather than untangling. **Android is untouched in this phase.** The
enforcement scenario matrix (`docs/ENFORCEMENT-SCENARIOS.md`) becomes the shared oracle:
the same rows must be walked by the Kotlin suite and by a new Rust suite, and a row that
passes in one and fails in the other is the finding.

**Phase 2 — the calling transport port.** The endpoint lifecycle in Rust, behind whatever
D1 decides. This is the phase that buys us the defect class §15 found: attach, camp with a
pass, dial, and — critically — *what happens to the camped connection when a dial starts*,
as something `cargo test` can assert against a real relay.

**Phase 3 — a headless shell, and the fast loop arrives here.** A Rust binary that walks
the whole arc against `relay.croft.ing:8443` under enforce and prints what it observed. No
UI. This is the smallest artifact that solves the stated problem, and it should land
before any pixel is drawn. It also becomes a CI-able instrument in a way two phones never
will be.

**Phase 4 — the macOS UI shell.** `shell/` gets its first occupant.

**Phase 5 — Android switches over.** Its own phase, its own device run against this
runbook, and explicitly *not* a foregone conclusion (D3).

**Sequencing note that is not a phase:** the §15 dial defect should be fixed in Kotlin
**first**, ahead of Phase 0. It is small, it is shipped, and it is costing real
reachability today. It must not wait behind an architecture migration.

## Reasoning

**Why not write the Mac client natively in Swift.** It would be quicker to start and it
would be the wrong thing. Every defect §15 found — the dial tearing down the camp,
`dial failed: null`, a dead refresh token rendering as `Signed in` — lives in the Kotlin
client's own paths. A Swift reimplementation would almost certainly not reproduce them,
and its green would mean "the Mac client works," not "the shipped client works." We would
have built a second thing to trust instead of a cheaper way to check the first. The
enforcement matrix exists precisely because these rules are subtle enough to be got wrong
twice, differently.

**Why the port is more tractable than "port the app" sounds.** The calling app is 2,902
Kotlin LOC total, and the admission surface is six files. It is already hexagonal —
`CampAdmission` and `DialAdmission` are pure decision functions, the ports are injected,
the effects are at the edges, and the journey tests already run over real ports against a
fixture exchange. The shape the core wants is the shape the Kotlin already has.

**Why porting the rules alone would not have caught §15's defect, and why we are doing it
anyway.** Worth stating plainly so nobody expects the wrong payoff. The decision rules and
the endpoint lifecycle are different layers:

```
ports to core cleanly →  given a session, a cached pass, a refusal reason: camp or
                         degrade, dial or refuse, and what words to say
                         (this IS the enforcement matrix — one suite instead of two)

does NOT port by itself →  the iroh endpoint lifecycle, OAuth's browser round-trip,
                           platform HTTP
                           ↑ §15's defect lives HERE
```

Phase 1 buys shared *rules*. Only Phase 2 buys shared *transport behaviour*, which is
where the live bug is. A plan that stopped at Phase 1 would be worth doing and would not
solve the stated problem, so Phase 2 is not optional and Phase 3 is where the goal is
actually met.

**Why additive.** The calling app is the thing baking under enforce right now, with an
open reachability defect. Switching it onto a fresh core in the same motion would mean two
moving things and no way to attribute a regression to either. Running the core as a second
implementation graded by the same matrix costs some duplication for a while and buys the
ability to say which layer broke.

**What this does not solve, and will never solve.** A laptop cannot answer real NAT
traversal between two networks, cellular paths, or mobile lifecycle (backgrounding, doze,
process death). The runbook's ladder is right that two real devices are the proof for
those. This plan lowers the cost of everything *below* that line; it does not move the
line. The failure mode to guard against is a green desktop arc being read as "calling
works" — the same misreading §13 made with attributed `usage` lines, and §15's method note
applies verbatim: a signal is evidence only once you know the case where it shows green
and the property is false.

## Open decisions

### D1 — where does the calling endpoint live? *(the plan hinges on this)*

**Options.** (a) Extend `ports/transport-iroh` with a calling mode. (b) A new sibling
port, e.g. `ports/transport-iroh-call`.

**Recommendation: (b), a separate port.** `ports/transport-iroh` sets
`RelayMode::Disabled` and contacts no relay **by construction rather than by care** —
CLAUDE.md states it in those words, and P7 S2's separation claim rests on it (the calling
APK contains zero gossip classes; the social module contacts no relay). A crate that
contains both a relay-disabled gossip endpoint and a relay-attaching calling endpoint
downgrades that guarantee from structural to conventional, and the guarantee is the
valuable part. The shared surface is genuinely small — `Endpoint` construction and
`EndpointId` handling — and most of `transport-iroh`'s 1,277 LOC is codec (`frame.rs`,
`record.rs`, `pairing.rs`) that calling does not want.

**Probe before deciding:** confirm two iroh `Endpoint`s with different `RelayMode`s can
coexist in one process and one binary without the relay-disabled one acquiring relay
behaviour. If they cannot, (b) is not merely preferable but forced, and the P7 separation
claim needs re-examining too.

### D2 — what is the first macOS artifact?

**Options.** (a) Headless Rust binary walking the arc, no UI. (b) SwiftUI app over uniffi,
matching the FFI surface Android would eventually use.

**Recommendation: (a) first, (b) after.** (a) is Phase 3 and it is what actually removes
adb from the loop. (b) is Phase 4 and is mostly about whether a person can *use* it, which
is a different question from whether we can *test* it. Shipping (a) first also keeps
Phase 4's UI work from blocking the fast loop that motivated the whole plan.

### D3 — does Android switch over at all, and when?

Left genuinely open. The case for: one implementation, one matrix, defects found in
`cargo test`. The case against: the Kotlin client is shipped, released, and now
device-validated under enforce; replacing its transport is a large risk against an app
that is finally working, and the upstream `computer.iroh` bindings are maintained by n0
for free. A defensible outcome is that macOS runs on the core, Android stays on its
Kotlin path, and the matrix grades both — accepting the duplication permanently and
deliberately. **Decide this after Phase 3**, when the core has actually been exercised and
the cost of the switch is measurable rather than estimated.

## What "done" means for each phase

- **P1:** `core/call-core` is a workspace member; the Rust suite walks the same
  `ENFORCEMENT-SCENARIOS.md` rows the Kotlin suite walks; Android untouched and its 172
  tests still green.
- **P2:** a `cargo test` asserts the camped connection's fate across a dial, against a
  real relay. This is the test that would have caught §15 and is the phase's whole
  justification.
- **P3:** one command on a laptop produces `admitted … sponsorship=…` in the production
  journal, and a dial, and a hang-up, with no phone attached.
- **P4:** a person can do the above without reading a terminal.
- **P5:** deferred to D3.

## Review Log

*(empty — this plan has not been reviewed)*
