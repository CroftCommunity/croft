# Versioning — three clocks, not one

The trap in a repo like this is a single version number pretending to describe
three things that move at different speeds and carry different obligations.
Croft has **three clocks**, and conflating them is how a breaking change ships
quietly.

## 1. The product — `croft` itself

**SemVer, currently `0.x`.**

Pre-1.0, and the workspace stance applies: backwards compatibility is **not the
default**. No deprecation layers, no migration shims, no re-exports kept alive
out of politeness. If something should change, change it.

This freedom is real and should be used. It is also **exactly what clock 2 does
not get.**

`1.0` means one thing here: *we are willing to promise the shape to people
outside this repo.* Not "it feels finished."

## 2. The contract — the lexicon and the deep link

**Versioned explicitly, and this one is not free to break.**

The calling contract (`ing.croft.iroh.endpoint`, `croftcall://call?…`) is defined
in `CroftCommunity/connect` and consumed by a **deployed web page** that we do not
redeploy in lockstep with this app. Breaking it does not break a build — it breaks
a link a stranger clicks.

So the asymmetry to hold in your head:

| | Breaking is… | Because |
|---|---|---|
| croft internals | **free** | pre-1.0, single consumer, one repo |
| the contract | **expensive** | a shipped page emits it; users hold old app versions |

Rules:

- A contract change is its own commit, in `connect`, with the version stated.
- This repo records which contract version it speaks, in `docs/CONTRACT.md`.
- An app that meets a deep link it does not understand **degrades visibly** —
  never silently no-ops. "Update Croft to accept this call" is a real state.

**Known breaking change already queued:** the croft-relay plan's Phase 10 moves
the lexicon from a single record at rkey `self` (`getRecord`) to **per-device
records** (`listRecords`) plus a request-policy record. That is a contract break
with a deployed consumer, and it is the first real test of these rules.

## 3. The toolchain — `env/toolchain.yml`

**A dated revision, not a number.** The `meta.reviewed` field is the clock.

Bumping any pin is a deliberate commit: change the version, run
`make record-checksums`, run `make verify`, commit all of it together. Never a
drive-by.

A toolchain bump is *not* a product version bump. They are unrelated events that
happen to live in the same repo.

## Android's `versionCode` is not a fourth clock

Google Play requires a monotonically increasing integer. It is **derived, never
decided** — a build-number counter, carrying no meaning. `versionName` is the
product version (clock 1). Do not let `versionCode` acquire semantics; the moment
someone reads meaning into it, it becomes a fourth thing to reason about for no
benefit.

## What gets a CHANGELOG entry

`CHANGELOG.md` tracks **clock 1 and clock 2**. Toolchain revisions get an entry
only when they change what a developer must do — a new SDK package, a bumped
Rust channel — because that is the part a reader needs, not the pin itself.

Infra commands and their reasoning go in `ops/JOURNAL.md`, not the changelog.
Different audience: the changelog is *what changed for a consumer*, the journal
is *what we did to the environment and why*.
