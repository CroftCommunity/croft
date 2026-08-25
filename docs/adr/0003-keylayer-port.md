# ADR-0003 — The key layer is a port that carries artifacts and never answers "admit?"
**Tags:** mls, keys, ports, admission

**Status:** accepted (2026-08-24 — the P4 build validated the shape; both
admission paths run end-to-end at loopback on the openmls realization).
**Context:** E117 Phase 4 joins MLS to
`social-tree-core`. The plan (discovery
`plans/2026-08-20-1-plan-social-tree-core.md`, Pass-3 Q2) requires this design
beat before any P4 code, and the review integration (2026-08-21) ordered two
invariants recorded here: the A3 decision stays core-side, and the
key-custody/recovery seam is named, not designed. Extends ADR-0001 (ports held
by the shell) and ADR-0002 (foundation vs ponds; the two admissions).

## The problem

The admission machinery — Part 2 §11.7's token cross-check, the merge rule,
the §7.3.8 stall, the admission fact — must appear on the core's surface,
with MLS supplying the cryptographic artifacts. MLS lives in openmls (=0.8.1,
the meer-queue lineage); its group state is ratchet state, mutable and not
derivable from the governance log. The core is pure: no I/O, no clock,
replayable from the log. Something has to sit between them, and the shape of
that something is where S16's failure class waits: bare MLS treats
cryptographic validity as admission. A port shaped so the adapter answers
"admit?" would rebuild S16 one layer up.

## Decision 1 — an artifact carrier with parsed claims, in `core::ports`

`KeyLayer` is a port trait in `social-tree-core::ports`, beside
`Signer`/`Verifier` (the precedent: core defines the contract, adapters
realize it, the corpus pins behavior against mocks). Its methods mint and
consume **opaque artifacts** — KeyPackage, Welcome, GroupInfo + tree,
external commit, sealed application messages — and return **parsed claims as
data**: the lineage the joiner's leaf credential resolves to, the presented
PSK ids, the commit's epoch and content address. Claims are inputs to a
decision; they are never the decision. No method on the trait returns an
admission answer, a membership view, or standing.

## Decision 2 — the admission decision is a pure core function, and the port cannot act without its token

`admission::evaluate(claims, ctx) -> Result<MergeApproval, Refusal>` lives in
the core. It consumes the KeyLayer's parsed claims plus governance data the
fold already owns — issuance facts from the log, standing evaluated **at the
commit's causal position** (S26's discipline, already the fold's), the
charter, and the §7.4 freshness state (`head_currency`'s
`admits_membership_origination`, on the core surface since P3). The refusal
set is S24's, plus the stall:

- `NoIssuanceFact` — PSK bytes resolve but no chain fact exists; holding
  bytes is not holding a fact (S24 arm d).
- `Revoked` — the issuance fact exists and is revoked at head.
- `LineageMismatch` — the token was issued to a different lineage than the
  joiner's credential resolves to (§11.7's cross-check).
- `NotCorroboratedFresh` — the §7.3.8 finality gate: a member that cannot
  corroborate the subject's standing as fresh stalls the merge, fail closed.

**The typestate is the enforcement.** `MergeApproval` has private fields and
is constructible only by `admission::evaluate`. The port's one
membership-mutating operation takes it:

```rust
fn merge_admission(&mut self, approval: MergeApproval) -> Result<MergedEpoch, KeyLayerError>;
```

So the two ordered invariants are not conventions but type errors waiting to
refuse violations: an adapter cannot answer "admit?" because nothing it can
construct expresses the answer (A3 stays core-side), and a merge that would
not deposit its admission fact cannot be expressed, because `MergeApproval`
carries the minted `AdmissionFact` inside it (§11.7's merge-rule clause: the
fact rides the approval, and the shell persists both or neither).

The orchestration is the shell's, per ADR-0001: parse (port) → decide (core)
→ enact (port) → deposit the fact (adapter persists it to the log). The core
never holds a `&mut dyn KeyLayer`; it only ever sees claims and emits
approvals.

**Generalized during the build (2026-08-24): every membership-mutating port
operation demands a core-minted slip.** The token-return path's
`merge_admission` takes the `MergeApproval` above; the invite path's
`add_with_welcome` takes an `InviteApproval` that
`authorize_invite_enactment` mints only when the fold has already seated
the invitee — the MLS Add-commit + Welcome is the *enactment* of the folded
`MembershipAdd` decision (S21's propose → govern → commit → Welcome), so
MLS seating follows the fold, never precedes it, on both paths.

## Decision 3 — MLS state lives adapter-side, entirely

The core holds governance-derived data only: issuance facts, membership
spans, standing — things it can fold and replay from the log. `MlsGroup`
state, the PSK store, provider storage: all adapter. Two reasons, one
structural, one measured. Structural: ratchet state is not replayable from
the log, so it can never be an input to a pure fold — a core that held it
would have state its own replay cannot reconstruct. Measured: S23's
token-ledger-as-group-state splits exactly here — the **issuance fact** is
chain data (core), the **PSK bytes** are key material (adapter's provider),
and severing the two is what S24 arm (d) proved matters: possession of bytes
carries no standing weight.

## Decision 4 — the native adapter is meer-queue's openmls code, adapted not rebuilt

openmls =0.8.1 (with `openmls_rust_crypto`, the pins `w1_mls_roundtrip`
guards). The S23–S26-measured shapes — seal/open with named errors,
GroupInfo policy, the external-commit path, the admission evaluation the
`admission.rs` module modeled — move behind the port as its first
realization. openmls-on-wasm is reported upstream but unverified here: the
probe is a P4 build item and the claim stays `[confirm]` until it runs. Until
then the wasm32 CI arm proves what it proves today — the core compiles and
its fold is wasm-clean — with `KeyLayer` feature-gated like `ed25519`, and
the browser shell claims nothing.

## Decision 5 — the custody seam: named, not designed

No `KeyLayer` method exposes key material; artifacts are opaque, and secrets
stay inside the adapter's provider. Backup, export, and recovery — §7.3.9's
pluggable targets (air-gapped QR, file export, blind PDS vault) and the
lineage-rotation recovery direction — are a **separate future port**
(working name `KeyCustody`), designed with the recovery-anchor prototype,
not here. The one thing this ADR fixes so P4 cannot foreclose it: recovery
per §7.3.9 **extends the lineage forward** rather than exporting live group
state, so `KeyLayer` owes no export surface and must not grow one as a
convenience — a state-export method added casually would become the
recovery path by default, unbounded by any of §7.3.9's invariants.

## Restated at this port (from ADR-0002)

The two admissions stay severed: no fabric-admission signal — relay token,
camp token, sponsorship claim — is an input to `admission::evaluate`. The
relay admits traffic, never members.

## Consequences

- The invite path (Add-commit + Welcome, the at-join token mint point) and
  the token-return path (external commit + PSK) land on the core surface as
  intents/effects; P4 is done when both run end-to-end at loopback with the
  per-plane rung stated — governance per P3, MLS Rung A, transport loopback
  = Modeled, never Verified.
- `admission::evaluate` is RED-first like everything else: the S24 refusal
  set and the S26 position-discipline become core pins before the port has
  a realization.
- The adapter and its openmls dependency live outside the core crate (the
  core's no-default-features arm must not pull openmls); workspace placement
  is a P4 build decision, not fixed here.
- Mutation posture per MUTATION.md: the new module joins the standing
  corpus-side burn-down; commit green before any hand-run mutant.
- One membership-mutating operation remains OUTSIDE the slip discipline,
  deliberately and visibly: the openmls adapter's inherent
  `enact_departure` (loopback plumbing for P4's dormancy step). The removal
  enactment joins the slip discipline when the eviction machinery lands —
  an `authorize_removal_enactment` gated on the folded `MembershipRemove`,
  the mirror of the invite slip. Until then it lives only on the concrete
  adapter, never on the trait.
- The wasm arm's honest rung: `keylayer-openmls` COMPILES for
  wasm32-unknown-unknown with the `js` features on getrandom and openmls;
  browser runtime behavior is unverified until a shell exercises it. The
  core's lean arms still never pull openmls.
