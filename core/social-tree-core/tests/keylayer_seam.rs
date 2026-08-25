//! **The KeyLayer seam pin (E117 P4, ADR-0003 decision 1).**
//!
//! The port is an artifact carrier: it stages a commit's wire bytes into
//! parsed claims (data, never a decision) and it merges only when handed the
//! [`MergeApproval`] that `evaluate_admission` minted. The test plays the
//! shell — the orchestration is parse (port) → decide (core) → enact (port)
//! — against an honest in-memory adapter, and pins the composition contract:
//!
//! - the claims the port stages are exactly what the decision consumes;
//! - the fact the adapter ends up holding is byte-for-byte the fact the
//!   approval carried (deposit-what-was-minted, §11.7's merge-rule clause);
//! - a refused decision leaves the adapter unmerged — there is no approval
//!   to give it, so the refused path is not a code path at all.
//!
//! What this deliberately does NOT test: MLS. The mock stages claims from a
//! trivial encoding; the real openmls adapter arrives behind the same trait
//! and the loopback end-to-end (both admission paths) is the phase's
//! done-when, not this pin's.

use social_tree_core::admission::{
    evaluate_admission, AdmissionClaims, AdmissionContext, AdmissionFact, AdmissionRefusal,
    IssuanceFact, SubjectStanding, TokenId,
};
use social_tree_core::model::{Hash, PrincipalId};
use social_tree_core::ports::keylayer::{KeyLayer, KeyLayerError, MergedEpoch};
use social_tree_core::project::head_currency::HeadCurrency;

/// An honest in-memory key layer: stages claims from a fixture encoding,
/// merges only via the approval, records the fact it was handed. No MLS —
/// the trait's contract is what is under test.
struct MemKeyLayer {
    staged: Vec<AdmissionClaims>,
    epoch: u64,
    deposited: Vec<AdmissionFact>,
}

impl MemKeyLayer {
    fn new() -> Self {
        Self {
            staged: Vec::new(),
            epoch: 7,
            deposited: Vec::new(),
        }
    }
}

impl KeyLayer for MemKeyLayer {
    fn stage_commit(&mut self, wire: &[u8]) -> Result<AdmissionClaims, KeyLayerError> {
        // Fixture encoding: joiner(32) ‖ token(32). Anything else is not a
        // commit — the loud refusal the real adapter gives to non-MLS bytes.
        if wire.len() != 64 {
            return Err(KeyLayerError::Parse("not a commit".to_string()));
        }
        let mut joiner = [0u8; 32];
        joiner.copy_from_slice(&wire[..32]);
        let mut tok = [0u8; 32];
        tok.copy_from_slice(&wire[32..]);
        let claims = AdmissionClaims {
            joiner_lineage: PrincipalId::new(joiner),
            presented_token: TokenId::new(tok),
            commit_content_address: Hash::new(*blake3::hash(wire).as_bytes()),
            commit_position: self.epoch,
        };
        self.staged.push(claims);
        Ok(claims)
    }

    fn merge_admission(
        &mut self,
        approval: social_tree_core::admission::MergeApproval,
    ) -> Result<MergedEpoch, KeyLayerError> {
        let fact = *approval.fact();
        if !self
            .staged
            .iter()
            .any(|c| c.commit_content_address == fact.event)
        {
            return Err(KeyLayerError::UnknownCommit(fact.event));
        }
        self.epoch += 1;
        self.deposited.push(fact);
        Ok(MergedEpoch { epoch: self.epoch })
    }

    fn add_with_welcome(
        &mut self,
        _approval: social_tree_core::admission::InviteApproval,
    ) -> Result<social_tree_core::ports::keylayer::InviteArtifacts, KeyLayerError> {
        Err(KeyLayerError::Process(
            "this fixture serves the token-return path".to_string(),
        ))
    }
}

fn wire_for(joiner: PrincipalId, token: TokenId) -> Vec<u8> {
    let mut w = Vec::with_capacity(64);
    w.extend_from_slice(joiner.as_bytes());
    w.extend_from_slice(token.as_bytes());
    w
}

/// **Pin 1 — the graceful arc: parse → decide → enact, and the deposited
/// fact is the minted fact.**
#[test]
fn the_seam_deposits_exactly_the_fact_the_decision_minted() {
    let joiner = PrincipalId::new([0xA1; 32]);
    let token = TokenId::new([0x11; 32]);
    let mut kl = MemKeyLayer::new();

    // The shell stages the wire bytes; the port returns claims as data.
    let claims = kl
        .stage_commit(&wire_for(joiner, token))
        .expect("well-formed commit stages");
    assert_eq!(
        claims.joiner_lineage, joiner,
        "claims are data from the wire"
    );

    // The core decides; the port had no say.
    let issuance = [IssuanceFact {
        token,
        lineage: joiner,
        revoked: false,
    }];
    let ctx = AdmissionContext {
        issuance: &issuance,
        subject_standing: SubjectStanding::Good,
        currency: HeadCurrency::new(),
        freshness: 3,
        member_count: 5,
        acceptor_frontier: 41,
    };
    let approval = evaluate_admission(&claims, &ctx).expect("clean cross-check admits");
    let minted = *approval.fact();

    // The port enacts only with the approval, and the deposit is the mint.
    let merged = kl.merge_admission(approval).expect("approved merge lands");
    assert_eq!(merged.epoch, 8, "the merge rolled the epoch");
    assert_eq!(
        kl.deposited,
        vec![minted],
        "deposit-what-was-minted: the fact the adapter holds is the fact the decision made"
    );
}

/// **Pin 2 — a refused decision leaves the key layer untouched: there is no
/// approval to give it.**
#[test]
fn a_refusal_leaves_the_key_layer_unmerged() {
    let joiner = PrincipalId::new([0xA1; 32]);
    let token = TokenId::new([0x11; 32]);
    let mut kl = MemKeyLayer::new();
    let claims = kl.stage_commit(&wire_for(joiner, token)).unwrap();

    // No issuance fact: the decision refuses.
    let ctx = AdmissionContext {
        issuance: &[],
        subject_standing: SubjectStanding::Good,
        currency: HeadCurrency::new(),
        freshness: 3,
        member_count: 5,
        acceptor_frontier: 41,
    };
    let refusal = evaluate_admission(&claims, &ctx).expect_err("no fact, no admission");
    assert_eq!(refusal, AdmissionRefusal::NoIssuanceFact);

    // Nothing to hand the port: the epoch never rolled, nothing deposited.
    assert_eq!(kl.epoch, 7, "a refusal cannot reach the key layer");
    assert!(kl.deposited.is_empty());
}

/// **Pin 3 — the port refuses bytes that are not a commit, loudly and typed.**
#[test]
fn non_commit_bytes_are_refused_at_the_stage() {
    let mut kl = MemKeyLayer::new();
    let err = kl
        .stage_commit(b"not a commit at all")
        .expect_err("garbage does not stage");
    assert!(matches!(err, KeyLayerError::Parse(_)));
}

/// **Pin 4 — an approval for a commit this adapter never staged is refused:
/// the approval authorizes, it does not conjure.**
#[test]
fn an_approval_for_an_unstaged_commit_does_not_merge() {
    let joiner = PrincipalId::new([0xA1; 32]);
    let token = TokenId::new([0x11; 32]);

    // Stage on one adapter, decide, then try to merge on ANOTHER adapter
    // that never saw the commit.
    let mut staging_kl = MemKeyLayer::new();
    let claims = staging_kl.stage_commit(&wire_for(joiner, token)).unwrap();
    let issuance = [IssuanceFact {
        token,
        lineage: joiner,
        revoked: false,
    }];
    let ctx = AdmissionContext {
        issuance: &issuance,
        subject_standing: SubjectStanding::Good,
        currency: HeadCurrency::new(),
        freshness: 3,
        member_count: 5,
        acceptor_frontier: 41,
    };
    let approval = evaluate_admission(&claims, &ctx).unwrap();

    let mut other_kl = MemKeyLayer::new();
    let err = other_kl.merge_admission(approval).expect_err(
        "an approval names an event; without the staged commit there is nothing to merge",
    );
    assert!(matches!(err, KeyLayerError::UnknownCommit(_)));
    assert!(other_kl.deposited.is_empty());
}
