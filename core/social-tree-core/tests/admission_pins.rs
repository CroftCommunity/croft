//! **The P4 admission pins (E117 P4, RED-first per ADR-0003).**
//!
//! The admission *decision* is a pure core function — the A3 invariant made
//! code: the KeyLayer port carries artifacts and parsed claims, and this
//! function is the only thing that can answer "admit?". Its `Ok` is a
//! [`MergeApproval`] whose private constructor makes it the sole key that
//! turns the port's merge, and the approval carries the minted
//! [`AdmissionFact`] inside it — §11.7's merge-rule clause (a merge that
//! would not deposit its fact MUST be refused) enforced by construction.
//!
//! The refusal set is S24's, measured in meer-queue and pinned here on the
//! core surface:
//! - no issuance fact — holding PSK *bytes* is not holding a *fact* (arm d);
//! - issuance revoked at the evaluated position;
//! - lineage mismatch — the token is persona-bound by cross-check, never by
//!   bearer possession (§11.7);
//! - subject standing excluded (banned) or contested (the gate manufactures
//!   no verdict);
//! - the §7.3.8 stall — an evaluator that cannot corroborate freshness
//!   refuses to merge, fail closed, and admits exactly at k (C3's gate).
//!
//! Fidelity: pure decision logic over adapter-assembled context data — the
//! FoldContext pattern. Position discipline (S26: evaluate at the commit's
//! causal position, never the evaluator's head) is the *adapter's* obligation
//! when it assembles `subject_standing`; the pin here is that the context is
//! position-denominated by name, and the integration arm rides P4's loopback
//! end-to-end.

use social_tree_core::admission::{
    evaluate_admission, AdmissionClaims, AdmissionContext, AdmissionRefusal, IssuanceFact,
    SubjectStanding, TokenId,
};
use social_tree_core::model::{Hash, PrincipalId};
use social_tree_core::project::head_currency::HeadCurrency;

fn lineage(seed: u8) -> PrincipalId {
    PrincipalId::new([seed; 32])
}

fn token(seed: u8) -> TokenId {
    TokenId::new([seed; 32])
}

/// Claims as the KeyLayer would parse them from a `NewMemberCommit`: data,
/// never a decision.
fn claims_for(joiner: PrincipalId, presented: TokenId) -> AdmissionClaims {
    AdmissionClaims {
        joiner_lineage: joiner,
        presented_token: presented,
        commit_content_address: Hash::new([0xC0; 32]),
        commit_position: 7,
    }
}

/// A context where everything is in order: the issuance fact exists for the
/// joiner's lineage, standing is good, and the evaluator is corroborated
/// fresh at k for a five-member group.
fn clean_context(issuance: &[IssuanceFact]) -> AdmissionContext<'_> {
    AdmissionContext {
        issuance,
        subject_standing: SubjectStanding::Good,
        currency: HeadCurrency::new(),
        freshness: 3, // k for member_count = 5
        member_count: 5,
        acceptor_frontier: 41,
    }
}

/// **Pin 1 — the graceful path mints the approval, and the fact rides inside it.**
///
/// The approval is the only value the KeyLayer's merge accepts, and it carries
/// the admission fact: event = the commit's content address (per-acceptor
/// facts corroborate one event, §7.3.4), the merged lineage, the redeemed
/// token, and the acceptor's frontier commitment (§7.5.1 classifies a stale
/// acceptor by it). A fact-less merge is therefore unwritable.
#[test]
fn clean_cross_check_mints_the_approval_carrying_the_fact() {
    let joiner = lineage(0xA1);
    let t = token(0x11);
    let issuance = [IssuanceFact { token: t, lineage: joiner, revoked: false }];
    let ctx = clean_context(&issuance);

    let approval = evaluate_admission(&claims_for(joiner, t), &ctx)
        .expect("a clean cross-check admits");

    let fact = approval.fact();
    assert_eq!(fact.event, Hash::new([0xC0; 32]), "the event IS the commit's content address");
    assert_eq!(fact.merged_lineage, joiner);
    assert_eq!(fact.redeemed_token, t);
    assert_eq!(fact.acceptor_frontier, 41, "the frontier commitment rides the fact");
}

/// **Pin 2 — holding bytes is not holding a fact (S24 arm d).**
///
/// The presented token resolves as key material (the adapter got this far)
/// but no issuance fact exists on the chain — a forged or out-of-band ledger
/// entry. Refused: the admission fact cannot cite an issuance that does not
/// exist.
#[test]
fn a_token_with_no_issuance_fact_is_refused() {
    let joiner = lineage(0xA1);
    let issuance: [IssuanceFact; 0] = [];
    let ctx = clean_context(&issuance);

    let refusal = evaluate_admission(&claims_for(joiner, token(0x11)), &ctx)
        .expect_err("bytes without a chain fact never admit");
    assert_eq!(refusal, AdmissionRefusal::NoIssuanceFact);
}

/// **Pin 3 — a revoked issuance no longer admits, and needs no key-deletion race.**
///
/// The fact still exists on the chain (revocation is a chain fact, not an
/// erasure); it just no longer admits.
#[test]
fn a_revoked_issuance_is_refused() {
    let joiner = lineage(0xA1);
    let t = token(0x11);
    let issuance = [IssuanceFact { token: t, lineage: joiner, revoked: true }];
    let ctx = clean_context(&issuance);

    let refusal = evaluate_admission(&claims_for(joiner, t), &ctx)
        .expect_err("a revoked issuance must not admit");
    assert_eq!(refusal, AdmissionRefusal::IssuanceRevoked);
}

/// **Pin 4 — the token is persona-bound by cross-check, never bearer possession.**
///
/// Every incumbent (and every former member) holds every token's bytes
/// (§11.7's ledger consequence), so possession must carry no standing weight:
/// a bearer whose credential resolves to a different lineage than the
/// issuance names is refused.
#[test]
fn anothers_token_is_refused_as_lineage_mismatch() {
    let issued_to = lineage(0xA1);
    let bearer = lineage(0xB2);
    let t = token(0x11);
    let issuance = [IssuanceFact { token: t, lineage: issued_to, revoked: false }];
    let ctx = clean_context(&issuance);

    let refusal = evaluate_admission(&claims_for(bearer, t), &ctx)
        .expect_err("possession is not personhood");
    assert_eq!(refusal, AdmissionRefusal::LineageMismatch { issued_to });
}

/// **Pin 5 — a banned lineage's own genuine token does not admit.**
///
/// Both proofs are honest (real token, real credential); standing at the
/// evaluated position is what refuses. This is the merge-side enforcement the
/// serve gate never was (S25: the serve is a roster shield, the merge is the
/// membership gate).
#[test]
fn a_banned_subject_is_refused_on_standing() {
    let joiner = lineage(0xA1);
    let t = token(0x11);
    let issuance = [IssuanceFact { token: t, lineage: joiner, revoked: false }];
    let ctx = AdmissionContext {
        subject_standing: SubjectStanding::Excluded,
        ..clean_context(&issuance)
    };

    let refusal = evaluate_admission(&claims_for(joiner, t), &ctx)
        .expect_err("banned at the evaluated position never admits");
    assert_eq!(refusal, AdmissionRefusal::StandingExcluded);
}

/// **Pin 6 — a contested subject stalls the gate; the gate manufactures no verdict.**
///
/// E108's rule reaches admission: while the subject's standing slot is
/// CONTESTED, admitting would be the view layer inventing a resolution.
/// Refused as its own named state — distinct from Excluded, because the UI
/// renders them differently (§7.6) and because resolving the pair is
/// governance's job, never this function's.
#[test]
fn a_contested_subject_is_refused_without_a_verdict() {
    let joiner = lineage(0xA1);
    let t = token(0x11);
    let issuance = [IssuanceFact { token: t, lineage: joiner, revoked: false }];
    let ctx = AdmissionContext {
        subject_standing: SubjectStanding::Contested,
        ..clean_context(&issuance)
    };

    let refusal = evaluate_admission(&claims_for(joiner, t), &ctx)
        .expect_err("no admission through an open contradiction");
    assert_eq!(refusal, AdmissionRefusal::StandingContested);
}

/// **Pin 7 — the §7.3.8 stall: fail closed below k, admit exactly at k.**
///
/// Merging is irreversible, so an evaluator that cannot corroborate the
/// subject's standing as fresh refuses — and the same context flips to admit
/// exactly at k distinct-lineage vouchers (C3's HeadAck gate, consumed here
/// as the admission gate's freshness source).
#[test]
fn an_uncorroborated_evaluator_stalls_and_admits_exactly_at_k() {
    let joiner = lineage(0xA1);
    let t = token(0x11);
    let issuance = [IssuanceFact { token: t, lineage: joiner, revoked: false }];

    // member_count = 5 → k = 3. At freshness 2: stalled, fail closed.
    let stalled_ctx = AdmissionContext {
        freshness: 2,
        ..clean_context(&issuance)
    };
    let refusal = evaluate_admission(&claims_for(joiner, t), &stalled_ctx)
        .expect_err("below k the merge stalls, fail closed");
    assert!(
        matches!(refusal, AdmissionRefusal::Stalled(_)),
        "the refusal names the stall: {refusal:?}"
    );

    // The identical context at freshness 3 = k: admits.
    let fresh_ctx = AdmissionContext {
        freshness: 3,
        ..clean_context(&issuance)
    };
    assert!(
        evaluate_admission(&claims_for(joiner, t), &fresh_ctx).is_ok(),
        "the stall lifts exactly at k"
    );
}
