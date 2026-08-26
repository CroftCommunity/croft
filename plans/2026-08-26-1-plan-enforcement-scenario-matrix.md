# Plan — the enforcement scenario matrix: every MUST-ADMIT and MUST-REFUSE, named and pinned

**Status: SERVER HALF LANDED (croft-stack `444aec4`, 2026-08-26). The canonical
server matrix is `croft-stack/docs/ENFORCEMENT-SCENARIOS.md`, gated by
`tests/enforcement_matrix.bats` in `make check`. Remaining here: the croft
client-posture rows + their harness meta-gate (next phase).**

## Problem Statement

Enforcement has been validated by *runs* — §11, §12, §13 each walked a path and
watched it work (or refuse). But the negative space is covered opportunistically:
we know `endpoint_unbound` refuses because a phone happened to hit it, and we
know signed-out camps are refused because §12 staged that rung. Nobody can
answer, from one artifact, "what is the complete set of things this system must
refuse, and which test proves each one?" Two incidents this week show the cost:
a silent successful mint was briefly read as a failure (§13 results — the
observability of success was never a stated scenario), and the §13 bake
criterion named a journal line that production's log filter cannot emit (an
evidence path nobody had pinned). For hardening — and for the enforce flip —
the refusal surface must be enumerated, not remembered.

## Approach

One matrix, two axes: **surface** (camp-at-attach, mint-at-dial/grantCall,
relay attach, client posture) × **outcome class** (MUST ADMIT / MUST REFUSE /
MUST DEGRADE — the outage postures). Every row names: the scenario, the
expected observable outcome (the *words*, verbatim where the contract states
them), **where it is enforced** (admit / relay / client — refusals must bite at
the relay even if the client misbehaves), and **which test pins it** — or
`GAP`. The matrix lives in this repo (the workflow harness owns most rows);
croft-stack rows cite its tests. A meta-gate makes the matrix load-bearing: a
design test walks the table and fails on any row whose named test does not
exist (the same lesson as tonight's unregistered-bats-file: an unwired
scenario silently reads as covered).

## Reasoning

- Rows come from the **source taxonomies, not from memory** (handoff rule:
  verbatim or marked): admit refusal reasons from `croft-relay-admit`
  (`bad_request, cap_mismatch, cap_not_found, cap_revoked, endpoint_unbound,
  jwt_invalid, no_proof, proof_unsupported, unavailable, unknown_key`), relay
  token verdicts from `croft-relay-bin` (`no_token, invalid_token,
  spent_token`), client postures from `DialAdmission`/`CampAdmission`
  (`Dial / DialTokenless / Refuse`, `Camp / CampTokenless / UseCached`).
- The relay is the gate (D3): every MUST-REFUSE has a relay-or-admit row, never
  only a client row — a client test proves politeness, not enforcement.
- MUST-DEGRADE is its own class because the outage postures are design
  decisions (admit down → dial/camp tokenless WITH words), and a hardening pass
  that only knows admit/refuse would "fix" them into refusals.

## The matrix (v1 — rows compiled 2026-08-26; `PIN:` = existing test, `GAP:` = none found yet)

### A. Camp (callee's pass, `/campToken`)

| # | Scenario | Must | Observable | Enforced at | Pinned by |
|---|----------|------|------------|-------------|-----------|
| A1 | Signed-in, device published by account | ADMIT | silent camp; relay usage attributed | admit+relay | PIN: CampJourneyTest (arc); §13 live |
| A2 | Signed out | DEGRADE (open) / REFUSE (enforce) | camp tokenless, no admit call; enforce: relay refuses `no_token` | client; relay | PIN: CampJourneyTest; camp_enforce_loop (stack); §12 rung |
| A3 | Signed in, device NOT published | DEGRADE | `endpoint_unbound` words on screen; WARN in admit journal | admit | PIN: §13 observed; CampJourneyTest row? (verify) |
| A4 | Pass expired | ADMIT (re-mint) | silent re-mint at expiry | client+admit | PIN: CampJourneyTest expiry row |
| A5 | Session revoked mid-camp | DEGRADE→REFUSE on enforce | re-mint fails; tokenless; enforce: next attach refused | admit; relay | PIN: CampJourneyTest revocation row |
| A6 | Admit outage | DEGRADE | tokenless WITH outage words | client | PIN: CampJourneyTest outage row |
| A7 | Proof for wrong lxm/aud (method binding) | REFUSE | admit `jwt_invalid`/`no_proof` (verify which) | admit | PIN: harness method-binding row (aba4d06); stack unit? (verify) |
| A8 | Sign-out drops the cached pass | REFUSE next enforce-attach | pass cleared; camp tokenless | client; relay | PIN: signOut test; §12 sign-out rung |

### B. Call (caller's token, `/grantCall`)

| # | Scenario | Must | Observable | Enforced at | Pinned by |
|---|----------|------|------------|-------------|-----------|
| B1 | Ticket proof, valid secret | ADMIT | mint + dial; attributed usage | admit+relay | PIN: TicketJourneyTest; §11.1 |
| B2 | Identity proof, registered caller | ADMIT | mint + dial | admit+relay | PIN: IdentityJourneyTest; §11 addendum, §13 call |
| B3 | Grant revoked (cap_revoked ≠ cap_not_found) | REFUSE | "this invite has been revoked", NO dial | admit; client | PIN: §11.3; journey revocation rows |
| B4 | Grant never existed | REFUSE | `cap_not_found` refusal words | admit | GAP: distinct-from-B3 test? (verify) |
| B5 | Proof identity ≠ grant's registered DID | REFUSE | `cap_mismatch` | admit | GAP |
| B6 | Wrong/garbage ticket secret | REFUSE | refusal words, no dial | admit | GAP: journey has disagreement rows (verify coverage) |
| B7 | Admit outage at dial | DEGRADE | dial tokenless WITH note (relay is the gate) | client | PIN: DialCompositionJourneyTest outage row |
| B8 | v1 callee (no grant params) | ADMIT (compat) | dials exactly as v0.4.0 | client | PIN: M4c v1-callee test |

### C. Relay attach (the actual gate; enforce mode)

| # | Scenario | Must | Observable | Enforced at | Pinned by |
|---|----------|------|------------|-------------|-----------|
| C1 | Valid token, right issuer+key | ADMIT | `admitted sponsorship=` (debug!) + attributed usage | relay | PIN: camp_enforce_loop; §12 |
| C2 | No token | REFUSE | `no_token` denial with words to client | relay | PIN: camp_enforce_loop; §12 rung 1 |
| C3 | Expired token | REFUSE | `invalid_token` (verify reason string) | relay | GAP: unit exists in token.rs? (verify) |
| C4 | Token signed by WRONG key (e.g. staging key on prod) | REFUSE | `invalid_token` | relay | GAP — the exact cross-environment hazard we now have two live keys for |
| C5 | Token replayed (single-use?) | REFUSE | `spent_token` | relay | GAP: semantics unverified — read token.rs before writing the row's test |
| C6 | Old-format claims (tier-era v0.1.1 vs D3) | REFUSE loudly | deserializer refusal (the staging rung's first find) | relay | PIN: found live 2026-08-23; unit? (verify) |

### D. Client honesty (posture, not enforcement)

| # | Scenario | Must | Observable | Pinned by |
|---|----------|------|------------|-----------|
| D1 | Refused attach must not read "camped" | HONEST | E130(a): "NOT camped — calls cannot reach this device" | GAP (landed under tests per E130? verify branch state) |
| D2 | Silent success is stated | HONEST | mint success invisible at every layer is DOCUMENTED (§13 results) | PIN: runbook §13; consider a status line instead |
| D3 | Refusals carry words, never blank | HONEST | every REFUSE row above has user-visible words | partial — each journey asserts its own |

## Wiring (the enforcement part — not started)

1. The matrix moves to `docs/ENFORCEMENT-SCENARIOS.md` once rows marked
   `(verify)` are resolved against source; this plan keeps the reasoning.
2. **Meta-gate**: a unit test parses the doc's `Pinned by:` column and fails on
   `GAP` rows above an agreed ratchet date, and on named tests that don't exist
   (grep-able names). Same ratchet philosophy as the repo's commit gates.
3. GAP rows become RED tests first: C4 (wrong-key token) and B5 (cap_mismatch)
   are the highest-value negatives — C4 is the cross-environment mistake we are
   one config edit from making, B5 is the impersonation seam.
4. croft-stack rows (C1–C6) get their tests in `relay/source` (its gate), with
   the doc cross-referencing both repos' test names.

## Review Log

- 2026-08-26 croftc-b4: v1 draft. Rows from source taxonomies; every
  uncertainty marked `(verify)` rather than asserted. Not yet reviewed.

- 2026-08-26 croftc-b4 (landing pass): **v1's gap analysis was wrong, and the
  way it was wrong is the lesson.** C4 (wrong-key) and B5 (cap_mismatch) were
  already pinned (`phase2_token.rs::wrong_issuer_key_denies`,
  `mint.rs::an_unlisted_did_is_refused_even_with_a_valid_proof`) — the draft
  compiled gaps from remembered test names instead of reading the test files.
  Reading them found the deny surface nearly complete (all sixteen refusal
  reasons asserted, plus rows the draft never listed: IdMismatch, no_cap,
  quota_exhausted, replay, burn-on-success). The one real server gap was C6
  (tier-era claims), landed RED-first with the serde-default hazard stated.
  The missing artifact was the MAP + the drift gate, both now in croft-stack.
  Next phase (this repo): the client-posture rows against the workflow
  harness journeys + a matching meta-gate wired into `make gate`, and the
  E130 rows once that lands.
