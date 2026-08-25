//! **The invite-path seam pins (E117 P4, ADR-0003 extended).**
//!
//! The token-return path's discipline, applied to the invite path: the MLS
//! Add-commit + Welcome is an ENACTMENT, and the decision it enacts is the
//! folded `MembershipAdd` quorum. So `add_with_welcome` — the port's other
//! membership-mutating operation — demands an [`InviteApproval`] that only
//! the core can mint, and the core mints it only when the fold has already
//! seated the invitee: **MLS seating follows the fold, never precedes it.**
//! (S21's propose → govern → commit → Welcome, with the govern step made
//! unskippable by construction.)
//!
//! Same test posture as keylayer_seam.rs: an honest in-memory adapter, no
//! MLS — the openmls realization arrives behind the same trait and the
//! loopback end-to-end is the phase's done-when.

mod common;

use common::{add_payload, env, genesis_payload_with, MemStore};
use social_tree_core::admission::{authorize_invite_enactment, InviteRefusal};
use social_tree_core::model::{AssertionType, GroupId, PrincipalId};
use social_tree_core::ports::keylayer::{InviteArtifacts, KeyLayer, KeyLayerError, MergedEpoch};

const GROUP: [u8; 32] = [0xC7; 32];

fn group() -> GroupId {
    GroupId::new(GROUP)
}

fn pid(seed: u8) -> PrincipalId {
    PrincipalId::new([seed; 32])
}

/// The invite half of an honest in-memory key layer: mints trivial
/// artifacts, seats only through the approval.
struct MemInviteLayer {
    epoch: u64,
    seated: Vec<PrincipalId>,
}

impl MemInviteLayer {
    fn new() -> Self {
        Self {
            epoch: 3,
            seated: Vec::new(),
        }
    }
}

impl KeyLayer for MemInviteLayer {
    fn stage_commit(
        &mut self,
        _wire: &[u8],
    ) -> Result<social_tree_core::admission::AdmissionClaims, KeyLayerError> {
        Err(KeyLayerError::Parse(
            "this fixture serves the invite path".to_string(),
        ))
    }

    fn merge_admission(
        &mut self,
        _approval: social_tree_core::admission::MergeApproval,
    ) -> Result<MergedEpoch, KeyLayerError> {
        Err(KeyLayerError::Process(
            "this fixture serves the invite path".to_string(),
        ))
    }

    fn add_with_welcome(
        &mut self,
        approval: social_tree_core::admission::InviteApproval,
    ) -> Result<InviteArtifacts, KeyLayerError> {
        let invitee = *approval.invitee();
        self.epoch += 1;
        self.seated.push(invitee);
        Ok(InviteArtifacts {
            commit_wire: invitee.as_bytes().to_vec(),
            welcome: vec![0x77],
        })
    }
}

/// **Pin 1 — no folded decision, no slip: an un-decided invite cannot be
/// enacted.**
#[test]
fn an_unfolded_invite_mints_no_approval() {
    let mut store = MemStore::default();
    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::GroupGenesis,
            1,
            vec![],
            genesis_payload_with([1, 1, 1, 1]),
        ))
        .expect("genesis");

    let refusal = authorize_invite_enactment(&pid(0x24), store.state(&group()))
        .expect_err("the fold has not seated the invitee; the enactment waits");
    assert_eq!(refusal, InviteRefusal::NotDecided { invitee: pid(0x24) });
}

/// **Pin 2 — the arc: fold the decision, mint the slip, enact, and the
/// seat matches the slip.**
#[test]
fn the_folded_decision_enacts_and_the_seat_matches_the_slip() {
    let mut store = MemStore::default();
    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::GroupGenesis,
            1,
            vec![],
            genesis_payload_with([1, 1, 1, 1]),
        ))
        .expect("genesis");
    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::MembershipAdd,
            2,
            vec![],
            add_payload(0x24, 2),
        ))
        .expect("the governance decision folds first");

    let approval = authorize_invite_enactment(&pid(0x24), store.state(&group()))
        .expect("the folded decision mints the enactment slip");

    let mut kl = MemInviteLayer::new();
    let artifacts = kl.add_with_welcome(approval).expect("the enactment runs");
    assert!(!artifacts.welcome.is_empty(), "a Welcome exists for the invitee");
    assert_eq!(
        kl.seated,
        vec![pid(0x24)],
        "the key layer seated exactly the principal the slip names"
    );
    assert_eq!(kl.epoch, 4, "the Add-commit rolled the epoch");
}

/// **Pin 3 — a contested invitee mints no slip: the gate manufactures no
/// verdict on the invite path either.**
#[test]
fn a_contested_invitee_mints_no_approval() {
    use common::remove_payload;
    use social_tree_core::model::envelope_hash;

    let mut store = MemStore::default();
    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::GroupGenesis,
            1,
            vec![],
            genesis_payload_with([1, 1, 1, 1]),
        ))
        .expect("genesis");
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
    let add_d = env(
        0x10,
        0x20,
        AssertionType::MembershipAdd,
        3,
        vec![],
        add_payload(0x24, 2),
    );
    store.ingest(&add_d).expect("add D");

    // Ban racing re-add: D's slot goes CONTESTED (the E108 hard-stop).
    store
        .ingest(&env(
            0x13,
            0x23,
            AssertionType::MembershipRemove,
            12,
            vec![envelope_hash(&add_d)],
            remove_payload(0x24, 0x01),
        ))
        .expect("ban folds");
    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::MembershipAdd,
            12,
            vec![envelope_hash(&add_d)],
            add_payload(0x24, 2),
        ))
        .expect("the hard-stop half folds");

    let refusal = authorize_invite_enactment(&pid(0x24), store.state(&group()))
        .expect_err("no enactment through an open contradiction");
    assert_eq!(refusal, InviteRefusal::Contested { invitee: pid(0x24) });
}
