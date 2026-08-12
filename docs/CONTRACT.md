# Contracts this client honours

**This file does not define the calling contract. It points at the definition and
records what this repo owes it.**

That distinction matters: two files claiming to define the same deep link is how a
contract silently forks. `croft` is a *consumer* of the calling contract, not its
owner.

## Canonical: `CroftCommunity/connect` `docs/contract.md`

`connect` is the directory — handle → DID → PDS → endpoint — and it stands on its
own as a web property. Calling in this app is **integration with it**, not a copy
of it. Its `docs/contract.md` is cited as ground truth by
`discovery/alpha/plans/2026-08-07-1-plan-croft-relay-tiered-admission.md`, and
that remains true.

Two artefacts live there:

| Artefact | Shape (as of 2026-08-11) |
|---|---|
| Lexicon | `ing.croft.iroh.endpoint`, `endpointId` required, plus `homeRelay` / `createdAt` |
| Deep link | `croftcall://call?endpoint=&relay=&handle=&did=` — `endpoint` required |

**Both are in flux.** The croft-relay tiered-admission plan's **Phase 10** moves
the lexicon from a single record at rkey `self` read via `getRecord` to
**per-device records via `listRecords`**, and adds a request-policy record. That
phase is green-lit (`listRecords` verified from lexicon source). **Do not
implement against the single-record shape.**

**Open:** whether `connect` remains the canonical home once this client is the
primary consumer. Do not resolve it by drift — either it stays there and this file
keeps pointing, or it moves deliberately and `connect` plus the croft-relay plan
are updated in the same change.

## What this repo does own

### The rendered-principal seam

The one interface calling needs from the rest of the app. Where a view model
carries a principal, the shell can attach a call affordance. `core/call-core`
answers the question; it never reaches into another pond's core.

### Callability is not a boolean

Drawing a call icon requires knowing whether a principal is reachable, and that
has **at least three states**, because it is the relay's admission model, not a
presence check:

| State | Meaning | UI obligation |
|---|---|---|
| `callable` | endpoint record present, policy admits you | offer the call |
| `not-listed` | no endpoint record — **the normal case** | do not present as an error |
| `may-not-permit` | record present, request policy is `mutuals` / `nobody` | do not promise a call we cannot place |

Caps are opaque-id records in the **callee's** repo, per-device, with a
user-chosen request policy (`anyone | mutuals | nobody`). The icon must not
promise a call the admission gate will refuse.

### Resolution cost and the metadata leak — decide before building

An always-visible call icon means resolving callability for **every principal on
screen**. Scrolling a feed is dozens of PDS lookups, and those lookups **reveal
who you are looking at**, not merely who you call.

This is cheap to decide now and expensive to retrofit. Candidates:

- **Lazy on tap** — zero leak, zero cost; the icon cannot show state until pressed.
- **Cached with TTL** — leak proportional to unique principals seen, not to scrolling.
- **Batched** — fewer requests, same leak, more complexity.

**Undecided.** Whichever is chosen, it is a stated privacy property of the
product, not an implementation detail.
