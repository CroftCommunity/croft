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

| Artefact | Shape (contract v2 — connect v0.2.0, 2026-08-16) |
|---|---|
| Endpoint | `ing.croft.iroh.endpoint`, **per-device rkeys** (`self` = primary), enumerated via `listRecords`; `endpointId` required, plus `homeRelay` / `label` / `createdAt` |
| Grant | `ing.croft.call.grant` — a `matcher` (`ticket` \| `mutuals` \| `registeredCallers`), `devices`, `policyRef` |
| Policy | `ing.croft.call.policy` — composable rules (`expires` \| `maxUses` \| `burnOnSuccess`) |
| Deep link | `croftcall://call?endpoint=&relay=&handle=&did=&device=&grant=` — `endpoint` required |

**v2 LANDED (2026-08-16).** The Phase 10 change is no longer in flux — it shipped
on connect `main` as **v0.2.0**: per-device `listRecords`, the capability model
(grants / matchers / policies), the invite → redeem path, and the call-time
evaluation engine. **Implement against v2**, not the single-record shape.

**Resolved (was Open): `connect` remains canonical.** It moved deliberately — the
owner made the break and released it; this file keeps pointing. A future contract
change stays coordinated (connect bumps `Contract version`, and this file + the
croft-relay plan update in the same change). **Handoff + the vocabulary bridge:**
`CroftCommunity/connect` `docs/PHASE11-HANDOFF.md`.

> **Vocabulary note.** v2 is richer than the `anyone | mutuals | nobody` request
> policy below. Request policy is **derived from the set of grant records** — which
> also cover `registeredCallers` (explicit DID lists), `ticket` (handed-out
> invites), and revocation rules — not a single enum on the endpoint. Model
> callability as `evaluateGrant` (does any grant admit me, and do its rules still
> hold), per the handoff's bridge table. The three callability states below still
> hold; their *definitions* now route through `evaluateGrant`.

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
