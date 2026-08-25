//! **The admission-machinery facts (E117 P4): issuance, revocation, and the
//! admission fact — on the governance chain, with C4's measured semantics.**
//!
//! §11.7's machinery becomes chain data: a token's issuance is a governance
//! fact (holding bytes is not holding a fact — the severance S24 arm (d)
//! measured); revocation is a chain fact the policy check consults, needing
//! no key-deletion race; and the admission fact is the R6-shaped acceptance
//! record the merging member deposits — **an event record that opens a
//! membership span, never a slot-competing membership addition** (§11.7's
//! comparator placement).
//!
//! The comparator placement is the load-bearing pin (C4's two-sided
//! boundary): an admission fact racing a BAN folds **silently-but-visibly**
//! — the fact folds without error, the span is on the chain, and the ban
//! governs standing, so the subject stays out with no CONTESTED and no
//! refusal. A readmission *quorum* (MembershipAdd) racing the same ban
//! still hard-stops CONTESTED (pinned in removal_kind.rs). Decision
//! contests decision; an enactment record contests nothing.
//!
//! Wire consequence: `GroupState` must carry the standing-ceiling set
//! (banned lineages) so replay applies an admission at its position without
//! reaching outside the fold — GroupState v3, v2 refused with the rebuild
//! demand (WIRE-REGISTER posture).

mod common;

use common::{add_payload, env, genesis_payload_with, remove_payload, MemStore};
use social_tree_core::admission::{issuance_view, TokenId};
use social_tree_core::model::{
    envelope_hash, AssertionType, ForkStatus, GroupId, Hash, MembershipView, PrincipalId,
};

const GROUP: [u8; 32] = [0xC7; 32];

fn group() -> GroupId {
    GroupId::new(GROUP)
}

fn pid(seed: u8) -> PrincipalId {
    PrincipalId::new([seed; 32])
}

fn issuance_payload(token: u8, lineage: u8) -> Vec<u8> {
    let mut p = [token; 32].to_vec();
    p.extend_from_slice(&[lineage; 32]);
    p
}

fn revocation_payload(token: u8) -> Vec<u8> {
    [token; 32].to_vec()
}

fn admission_payload(event: u8, lineage: u8, token: u8, frontier: u64) -> Vec<u8> {
    let mut p = [event; 32].to_vec();
    p.extend_from_slice(&[lineage; 32]);
    p.extend_from_slice(&[token; 32]);
    p.extend_from_slice(&frontier.to_be_bytes());
    p
}

/// O(0x20, dev 0x10) owner; C(0x23, dev 0x13) admin; D(0x24, dev 0x14) member.
fn boot(store: &mut MemStore) {
    let genesis = env(
        0x10,
        0x20,
        AssertionType::GroupGenesis,
        1,
        vec![],
        genesis_payload_with([1, 1, 1, 1]),
    );
    store.ingest(&genesis).expect("genesis");
    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::MembershipAdd,
            2,
            vec![],
            add_payload(0x23, 1),
        ))
        .expect("add admin C");
    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::MembershipAdd,
            3,
            vec![],
            add_payload(0x24, 2),
        ))
        .expect("add member D");
}

/// **Pin 1 — issuance and revocation are chain facts, and the view derives
/// from the log.**
///
/// The at-join mint: O issues D's re-entry token as a governance fact.
/// `issuance_view` derives exactly the `IssuanceFact` the admission
/// decision consumes — the same struct, so the chain is the context's
/// source and nothing arrives out-of-band. A later revocation flips the
/// derived fact without erasing it.
#[test]
fn issuance_and_revocation_derive_from_the_chain() {
    let mut store = MemStore::default();
    boot(&mut store);

    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::TokenIssuance,
            4,
            vec![],
            issuance_payload(0x77, 0x24),
        ))
        .expect("issuance folds");

    let view = issuance_view(store.log(&group()));
    assert_eq!(view.len(), 1);
    assert_eq!(view[0].token, TokenId::new([0x77; 32]));
    assert_eq!(view[0].lineage, pid(0x24));
    assert!(!view[0].revoked, "issued, not revoked");

    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::TokenRevocation,
            5,
            vec![],
            revocation_payload(0x77),
        ))
        .expect("revocation folds");

    let view = issuance_view(store.log(&group()));
    assert_eq!(view.len(), 1, "revocation is not an erasure");
    assert!(view[0].revoked, "the derived fact is revoked");
}

/// **Pin 2 — revoking a token that was never issued is refused loudly.**
///
/// A revocation names an issuance fact; naming nothing is either a replay
/// artifact or a forged ledger move, and both must surface (the
/// Resolution-names-no-pair posture).
#[test]
fn revoking_an_unissued_token_is_refused() {
    let mut store = MemStore::default();
    boot(&mut store);

    let err = store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::TokenRevocation,
            4,
            vec![],
            revocation_payload(0x99),
        ))
        .expect_err("a revocation of nothing must surface");
    assert!(
        err.contains("issuance"),
        "the refusal names the missing issuance: {err}"
    );
}

/// **Pin 3 — the returner arc: departure, then the admission fact re-opens
/// the span.**
///
/// D migrates out (departure kind — standing intact). Later the acceptor C
/// deposits the admission fact for D's return commit. The fact seats D as
/// a Member again: the span opens at the fact's position, no
/// MembershipAdd quorum involved — this is the external-commit path's
/// governance shadow, and the whole point of the §7.6.4 distinction.
#[test]
fn an_admission_fact_reopens_a_departed_members_span() {
    let mut store = MemStore::default();
    boot(&mut store);

    store
        .ingest(&env(
            0x14,
            0x24,
            AssertionType::MembershipRemove,
            10,
            vec![],
            remove_payload(0x24, 0x00), // departure — migration to cold
        ))
        .expect("D departs");
    assert_eq!(store.state(&group()).membership(&pid(0x24)), MembershipView::NotMember);

    store
        .ingest(&env(
            0x13,
            0x23, // acceptor C, a member
            AssertionType::Admission,
            11,
            vec![],
            admission_payload(0xE1, 0x24, 0x77, 41),
        ))
        .expect("the admission fact folds");

    let state = store.state(&group());
    assert_eq!(
        state.membership(&pid(0x24)),
        MembershipView::Member,
        "the span re-opened: D participates at the current epoch"
    );
    assert!(matches!(state.fork_status, ForkStatus::Clean));
}

/// **Pin 4 — fact-vs-ban: silently-but-visibly, both orders, never
/// CONTESTED (C4's boundary).**
///
/// D is banned. An admission fact for D folds WITHOUT error and WITHOUT a
/// hard-stop — the acceptor was concurrently stale, no fault (§7.5.1) —
/// but the ban governs standing: D stays out. Both arrival orders land on
/// the same roster and the same clean fork status. The record says the
/// window was real; nothing is retroactively unmade; and no one
/// manufactured a verdict.
#[test]
fn an_admission_fact_racing_a_ban_folds_excluded_never_contested() {
    let add_d_hash = envelope_hash(&env(
        0x10,
        0x20,
        AssertionType::MembershipAdd,
        3,
        vec![],
        add_payload(0x24, 2),
    ));

    let c_bans_d = env(
        0x13,
        0x23,
        AssertionType::MembershipRemove,
        12,
        vec![add_d_hash],
        remove_payload(0x24, 0x01), // ban
    );
    let o_admits_d = env(
        0x10,
        0x20,
        AssertionType::Admission,
        12,
        vec![add_d_hash],
        admission_payload(0xE2, 0x24, 0x77, 40),
    );

    let mut ban_first = MemStore::default();
    boot(&mut ban_first);
    ban_first.ingest(&c_bans_d).expect("ban folds");
    ban_first
        .ingest(&o_admits_d)
        .expect("the admission fact folds without error — silently-but-visibly");

    let mut admit_first = MemStore::default();
    boot(&mut admit_first);
    admit_first.ingest(&o_admits_d).expect("admission folds");
    admit_first.ingest(&c_bans_d).expect("ban folds");

    for (name, store) in [("ban-first", &ban_first), ("admit-first", &admit_first)] {
        let state = store.state(&group());
        assert_eq!(
            state.membership(&pid(0x24)),
            MembershipView::NotMember,
            "{name}: the ban governs standing; the span is chain-visible, the seat is not held"
        );
        assert!(
            matches!(state.fork_status, ForkStatus::Clean),
            "{name}: an enactment record contests nothing: {:?}",
            state.fork_status
        );
    }
}

/// **Pin 5 — a readmission QUORUM clears the ceiling; the decision layer
/// stays sovereign.**
///
/// After the ban, a MembershipAdd (the governance decision, threshold met,
/// causally AFTER the ban) re-seats D — and a subsequent admission fact
/// is then unobstructed. The ceiling yields to a decision, never to an
/// enactment.
#[test]
fn a_readmission_quorum_clears_the_ceiling() {
    let mut store = MemStore::default();
    boot(&mut store);

    let ban = env(
        0x13,
        0x23,
        AssertionType::MembershipRemove,
        10,
        vec![],
        remove_payload(0x24, 0x01),
    );
    store.ingest(&ban).expect("ban folds");

    // Sequential (anteceded) re-add: a decision, not a race.
    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::MembershipAdd,
            11,
            vec![envelope_hash(&ban)],
            add_payload(0x24, 2),
        ))
        .expect("the readmission decision folds");

    let state = store.state(&group());
    assert_eq!(
        state.membership(&pid(0x24)),
        MembershipView::Member,
        "the quorum re-seats D"
    );
}

/// **Pin 6 — a non-member deposits no admission fact.**
///
/// The acceptor is the merging MEMBER; the returner mints nothing, and a
/// stranger mints nothing. An admission fact from outside the roster is
/// refused as plain authorization failure.
#[test]
fn an_admission_fact_from_a_non_member_is_refused() {
    let mut store = MemStore::default();
    boot(&mut store);

    let err = store
        .ingest(&env(
            0x19,
            0x29, // not a member
            AssertionType::Admission,
            10,
            vec![],
            admission_payload(0xE3, 0x24, 0x77, 40),
        ))
        .expect_err("a non-member authors no chain fact");
    assert!(err.contains("authorization"), "plain refusal: {err}");
}
