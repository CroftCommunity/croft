//! **The 2026-08-25 owner decisions, pinned (E133/E136 build half).**
//!
//! Three governance-weight rules out of the spec-filing walk:
//! - **A contested subject signs but never counts** (§7.3.2): their
//!   signature on their own pair's resolution is legible consent, and it
//!   contributes zero toward the resolution threshold.
//! - **The readmission threshold is its own dial** (§11.8): re-adding a
//!   lineage under the standing ceiling is gated by `readmission_threshold`
//!   (minted default 1 — easy group, easy mercy), not by the invite dial;
//!   never-banned invitees stay on the add threshold.
//! - **Issue like adding, revoke like removing** (§11.7): a TokenIssuance
//!   carries the add threshold's weight, a TokenRevocation the remove
//!   threshold's.

mod common;

use common::{add_payload, approval_payload, env, genesis_payload_with, remove_payload, MemStore};
use social_tree_core::model::{
    envelope_hash, AssertionType, ForkStatus, GroupId, MembershipView, PrincipalId, Role,
};

const GROUP: [u8; 32] = [0xC7; 32];

fn group() -> GroupId {
    GroupId::new(GROUP)
}
fn pid(seed: u8) -> PrincipalId {
    PrincipalId::new([seed; 32])
}

/// O(0x20, dev 0x10) owner; A(0x21, dev 0x11) admin; C(0x23, dev 0x13) admin;
/// D(0x24, dev 0x14) member.
fn boot(store: &mut MemStore, thresholds: [u32; 4]) {
    store
        .ingest(&env(0x10, 0x20, AssertionType::GroupGenesis, 1, vec![], genesis_payload_with(thresholds)))
        .expect("genesis");
    store
        .ingest(&env(0x10, 0x20, AssertionType::MembershipAdd, 2, vec![], add_payload(0x21, 1)))
        .expect("add admin A");
    store
        .ingest(&env(0x10, 0x20, AssertionType::MembershipAdd, 3, vec![], add_payload(0x23, 1)))
        .expect("add admin C");
    store
        .ingest(&env(0x10, 0x20, AssertionType::MembershipAdd, 4, vec![], add_payload(0x24, 2)))
        .expect("add member D");
}

/// Open a contested pair on D: C bans D racing O's re-add.
fn contest_d(store: &mut MemStore) {
    let add_d = env(0x10, 0x20, AssertionType::MembershipAdd, 4, vec![], add_payload(0x24, 2));
    let ban = env(0x13, 0x23, AssertionType::MembershipRemove, 12, vec![envelope_hash(&add_d)], remove_payload(0x24, 0x01));
    let readd = env(0x10, 0x20, AssertionType::MembershipAdd, 12, vec![envelope_hash(&add_d)], add_payload(0x24, 2));
    store.ingest(&ban).expect("ban folds");
    store.ingest(&readd).expect("hard-stop half folds");
    assert!(matches!(store.state(&group()).fork_status, ForkStatus::Contested(_)));
}

fn resolution_payload(store: &MemStore) -> Vec<u8> {
    let ForkStatus::Contested(entries) = &store.state(&group()).fork_status else {
        panic!("expected contested");
    };
    let (a, b) = entries[0].pair;
    let mut p = a.as_bytes().to_vec();
    p.extend_from_slice(b.as_bytes());
    p
}

fn resolution_subject(payload: &[u8]) -> PrincipalId {
    PrincipalId::new(social_tree_core::update::rule_change_approval_subject(payload))
}

/// **Pin 1 — the contested subject's signature never counts.** Resolution
/// threshold 2; D (the contested subject) approves; O authors. O + D = one
/// COUNTING signature — refused. O + A = two — applies.
#[test]
fn a_contested_subjects_approval_does_not_count() {
    let mut store = MemStore::default();
    boot(&mut store, [1, 1, 1, 1]); // resolution minted at 2
    contest_d(&mut store);
    let payload = resolution_payload(&store);
    let subject = resolution_subject(&payload);

    // D approves its own pair's resolution: the signature EXISTS (folds as
    // an Approval fact) but must not carry the quorum.
    let d_approval = env(
        0x14,
        0x24,
        AssertionType::Approval,
        20,
        vec![],
        approval_payload(AssertionType::Resolution, subject),
    );
    store.ingest(&d_approval).expect("consent may be recorded");

    let refused = env(
        0x10,
        0x20,
        AssertionType::Resolution,
        21,
        vec![envelope_hash(&d_approval)],
        payload.clone(),
    );
    let err = store
        .ingest(&refused)
        .expect_err("O + D is one counting signature, not two");
    assert!(err.contains("threshold"), "refused on the threshold: {err}");

    // O + A (a persona the pair does not name) carries it.
    let a_approval = env(
        0x11,
        0x21,
        AssertionType::Approval,
        22,
        vec![],
        approval_payload(AssertionType::Resolution, subject),
    );
    store.ingest(&a_approval).expect("A approves");
    let resolved = env(
        0x10,
        0x20,
        AssertionType::Resolution,
        23,
        vec![envelope_hash(&a_approval)],
        payload,
    );
    store.ingest(&resolved).expect("O + A resolves");
    assert!(matches!(store.state(&group()).fork_status, ForkStatus::Clean));
}

/// **Pin 2 — the readmission dial gates un-banning, not inviting.** Dial
/// minted at 1: a single-author re-add of a banned lineage seats (easy
/// mercy). Dialed to 2 (rule_key 5): the solo re-add is refused; with a
/// second persona's approval it seats and the ceiling clears. A
/// never-banned stranger stays on the add threshold throughout.
#[test]
fn the_readmission_dial_gates_the_ceiling_not_the_invite() {
    let mut store = MemStore::default();
    boot(&mut store, [1, 2, 1, 1]);

    // Ban D (threshold 2: O + C via approval).
    let subject_d = pid(0x24);
    let c_approval = env(
        0x13,
        0x23,
        AssertionType::Approval,
        10,
        vec![],
        approval_payload(AssertionType::MembershipRemove, subject_d),
    );
    store.ingest(&c_approval).expect("C approves the ban");
    store
        .ingest(&env(0x10, 0x20, AssertionType::MembershipRemove, 11, vec![envelope_hash(&c_approval)], remove_payload(0x24, 0x01)))
        .expect("ban at quorum");
    assert_eq!(store.state(&group()).membership(&subject_d), MembershipView::NotMember);

    // Minted default 1: the solo re-add seats — permissive by design.
    store
        .ingest(&env(0x13, 0x23, AssertionType::MembershipAdd, 12, vec![], add_payload(0x24, 2)))
        .expect("easy mercy at the minted default");
    assert_eq!(
        store.state(&group()).membership(&subject_d),
        MembershipView::Member(Role::Member)
    );

    // Re-ban, then dial readmission to 2 (rule_key 5).
    let c_approval2 = env(
        0x13,
        0x23,
        AssertionType::Approval,
        13,
        vec![],
        approval_payload(AssertionType::MembershipRemove, subject_d),
    );
    store.ingest(&c_approval2).expect("C approves ban 2");
    store
        .ingest(&env(0x10, 0x20, AssertionType::MembershipRemove, 14, vec![envelope_hash(&c_approval2)], remove_payload(0x24, 0x01)))
        .expect("ban 2");
    let mut dial = vec![5u8]; // RuleKey::Readmission
    dial.extend_from_slice(&2u32.to_be_bytes());
    store
        .ingest(&env(0x10, 0x20, AssertionType::RuleChange, 15, vec![], dial))
        .expect("dial readmission to 2");

    // Solo re-add now refused on the READMISSION threshold.
    let err = store
        .ingest(&env(0x13, 0x23, AssertionType::MembershipAdd, 16, vec![], add_payload(0x24, 2)))
        .expect_err("two to un-ban now");
    assert!(err.contains("threshold"), "{err}");

    // A never-banned stranger still enters on the ADD threshold (1).
    store
        .ingest(&env(0x13, 0x23, AssertionType::MembershipAdd, 17, vec![], add_payload(0x66, 2)))
        .expect("inviting strangers is unchanged");

    // With A's approval, the readmission carries.
    let a_approval = env(
        0x11,
        0x21,
        AssertionType::Approval,
        18,
        vec![],
        approval_payload(AssertionType::MembershipAdd, subject_d),
    );
    store.ingest(&a_approval).expect("A approves readmission");
    store
        .ingest(&env(0x13, 0x23, AssertionType::MembershipAdd, 19, vec![envelope_hash(&a_approval)], add_payload(0x24, 2)))
        .expect("two to un-ban satisfied");
    assert_eq!(
        store.state(&group()).membership(&subject_d),
        MembershipView::Member(Role::Member),
        "seated and the ceiling cleared"
    );
}

/// **Pin 3 — issue like adding, revoke like removing.** Add threshold 2:
/// a solo issuance is refused, an approved one folds. Remove threshold 2:
/// a solo revocation is refused, an approved one folds.
#[test]
fn token_weights_follow_their_acts() {
    let mut store = MemStore::default();
    boot(&mut store, [2, 2, 1, 1]); // add 2, remove 2

    let mut issuance = [0x77u8; 32].to_vec();
    issuance.extend_from_slice(pid(0x24).as_bytes());

    // Solo issuance: refused at the ADD weight.
    let err = store
        .ingest(&env(0x10, 0x20, AssertionType::TokenIssuance, 10, vec![], issuance.clone()))
        .expect_err("issuance carries the add weight");
    assert!(err.contains("threshold"), "{err}");

    // With A's approval (subject = the lineage), it folds.
    let a_ok = env(
        0x11,
        0x21,
        AssertionType::Approval,
        11,
        vec![],
        approval_payload(AssertionType::TokenIssuance, pid(0x24)),
    );
    store.ingest(&a_ok).expect("A approves issuance");
    store
        .ingest(&env(0x10, 0x20, AssertionType::TokenIssuance, 12, vec![envelope_hash(&a_ok)], issuance))
        .expect("issuance at quorum");

    // Solo revocation: refused at the REMOVE weight.
    let err = store
        .ingest(&env(0x10, 0x20, AssertionType::TokenRevocation, 13, vec![], [0x77u8; 32].to_vec()))
        .expect_err("revocation carries the remove weight");
    assert!(err.contains("threshold"), "{err}");

    // With A's approval (subject = the token id bytes), it folds.
    let a_ok2 = env(
        0x11,
        0x21,
        AssertionType::Approval,
        14,
        vec![],
        approval_payload(AssertionType::TokenRevocation, pid(0x77)),
    );
    store.ingest(&a_ok2).expect("A approves revocation");
    store
        .ingest(&env(0x10, 0x20, AssertionType::TokenRevocation, 15, vec![envelope_hash(&a_ok2)], [0x77u8; 32].to_vec()))
        .expect("revocation at quorum");
}
