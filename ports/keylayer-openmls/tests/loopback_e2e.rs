//! **The P4 done-when: both admission paths end-to-end at loopback, on real
//! openmls behind the KeyLayer port.**
//!
//! Per-plane rung, stated honestly (ADR-0003 / the E117 plan): governance
//! per P3 (real Ed25519 on every fact); MLS **Rung A** (real openmls 0.8.1,
//! real key schedule, real AEAD); transport **loopback = Modeled, never
//! Verified** (artifacts cross serialize → re-parse in one process).
//!
//! - **Invite path:** the fold decides (genesis → MembershipAdd → the
//!   at-join TokenIssuance), the core mints the InviteApproval, the port
//!   enacts (real Add-commit + Welcome), the invitee joins and the two
//!   exchange a sealed message at AEAD grade.
//! - **Token-return path:** the member departs (departure-kind removal +
//!   MLS leaf removal), fetches GroupInfo, builds a REAL external commit
//!   carrying the token as a PSK proposal; the incumbent stages it (claims
//!   as data), the core cross-checks against the chain-derived issuance
//!   view and mints the MergeApproval, the port merges, the admission fact
//!   deposits to the fold — and the returner participates again.
//! - **The S16 arm:** cryptographic validity is not admission. A stranger's
//!   externally-valid commit stages fine; the decision refuses it
//!   (NoIssuanceFact — bytes are not facts), and no approval exists to
//!   merge, so the stranger is never seated.

mod fold_harness;

use fold_harness::{add_payload, env, genesis_payload_with, remove_payload, MemStore};
use keylayer_openmls::OpenMlsKeyLayer;
use social_tree_core::admission::{
    authorize_invite_enactment, evaluate_admission, issuance_view, AdmissionContext,
    AdmissionRefusal, SubjectStanding, TokenId,
};
use social_tree_core::model::{AssertionType, GroupId, MembershipView, PrincipalId, Role};
use social_tree_core::ports::keylayer::KeyLayer;
use social_tree_core::project::head_currency::HeadCurrency;

const GROUP: [u8; 32] = [0xC7; 32];

fn group() -> GroupId {
    GroupId::new(GROUP)
}

fn pid(seed: u8) -> PrincipalId {
    PrincipalId::new([seed; 32])
}

fn issuance_payload(token: &TokenId, lineage: u8) -> Vec<u8> {
    let mut p = token.as_bytes().to_vec();
    p.extend_from_slice(&[lineage; 32]);
    p
}

fn admission_payload(event: [u8; 32], lineage: u8, token: &TokenId, frontier: u64) -> Vec<u8> {
    let mut p = event.to_vec();
    p.extend_from_slice(&[lineage; 32]);
    p.extend_from_slice(token.as_bytes());
    p.extend_from_slice(&frontier.to_be_bytes());
    p
}

#[test]
fn both_admission_paths_end_to_end_at_loopback() {
    // ---- The cast: O (owner, 0x20/dev 0x10) and B (member, 0x24/dev 0x14).
    let o = pid(0x20);
    let b = pid(0x24);
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

    let mut o_kl = OpenMlsKeyLayer::new(o);
    o_kl.create_group().expect("O plants the MLS group");
    let mut b_kl = OpenMlsKeyLayer::new(b);

    // =====================================================================
    // INVITE PATH: decide (fold) → mint slip (core) → enact (port) → join.
    // =====================================================================
    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::MembershipAdd,
            2,
            vec![],
            add_payload(0x24, 2),
        ))
        .expect("the governance decision folds FIRST");

    // The at-join token mint (§11.7 issuance setting 1) — a chain fact.
    let token = TokenId::new([0x77; 32]);
    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::TokenIssuance,
            3,
            vec![],
            issuance_payload(&token, 0x24),
        ))
        .expect("the at-join issuance folds");
    // The token's SECRET is group state carried to members (S23): both
    // providers hold it. Possession carries no standing weight — the chain
    // fact above is what admits.
    let secret = [0x5A; 32];
    o_kl.store_token(&token, &secret).expect("O holds the ledger entry");
    b_kl.store_token(&token, &secret).expect("B holds their token");

    let slip = authorize_invite_enactment(&b, store.state(&group()))
        .expect("the folded decision mints the slip");
    let b_kp = b_kl.key_package_bytes().expect("B mints a KeyPackage");
    o_kl.deposit_key_package(b, &b_kp).expect("O holds B's KeyPackage");
    let artifacts = o_kl.add_with_welcome(slip).expect("the enactment runs on real MLS");

    b_kl.join_from_welcome(&artifacts.welcome).expect("B seats from the Welcome");

    // Proof of seating at AEAD grade: B seals, O opens.
    let sealed = b_kl.seal(b"hello from the newly seated B").expect("B seals");
    let opened = o_kl.open(&sealed).expect("O opens");
    assert_eq!(opened, b"hello from the newly seated B");

    // =====================================================================
    // DORMANCY: B departs (standing intact) — fold + MLS enactment.
    // =====================================================================
    store
        .ingest(&env(
            0x14,
            0x24,
            AssertionType::MembershipRemove,
            10,
            vec![],
            remove_payload(0x24, 0x00), // departure — migration, not a ban
        ))
        .expect("B's departure folds");
    o_kl.enact_departure(&b).expect("the MLS leaf is removed");
    assert_eq!(
        store.state(&group()).membership(&b),
        MembershipView::NotMember,
        "B is off the hot roster"
    );

    // =====================================================================
    // TOKEN-RETURN PATH: external commit + PSK → stage → decide → merge →
    // deposit the fact.
    // =====================================================================
    let gi = o_kl.group_info_with_tree().expect("the served pull artifact");
    let commit_wire = b_kl
        .return_via_external_commit(&gi, &token)
        .expect("B builds the REAL external commit carrying the token");

    // The incumbent stages: claims as data, no decision made.
    let claims = o_kl.stage_commit(&commit_wire).expect("the commit stages");
    assert_eq!(claims.joiner_lineage, b, "the leaf credential resolves to B");
    assert_eq!(claims.presented_token, token, "the PSK proposal is countable");

    // The core decides against the CHAIN-DERIVED issuance view.
    let issuance = issuance_view(store.log(&group()));
    let ctx = AdmissionContext {
        issuance: &issuance,
        subject_standing: SubjectStanding::Good, // departure ≠ ban (§7.6.4)
        currency: HeadCurrency::new(),
        freshness: 1, // k for the 1-member hot roster
        member_count: 1,
        acceptor_frontier: store.log(&group()).len() as u64,
    };
    let approval = evaluate_admission(&claims, &ctx).expect("the cross-check admits B");
    let fact = *approval.fact();

    // The port merges ONLY with the approval; the fact deposits to the fold.
    o_kl.merge_admission(approval).expect("the merge seats B's return");
    store
        .ingest(&env(
            0x10,
            0x20, // the acceptor O deposits the fact
            AssertionType::Admission,
            11,
            vec![],
            admission_payload(*fact.event.as_bytes(), 0x24, &token, fact.acceptor_frontier),
        ))
        .expect("the admission fact folds — the span re-opens");
    assert_eq!(
        store.state(&group()).membership(&b),
        MembershipView::Member(Role::Member),
        "the fold shows the re-opened span"
    );

    // The returner participates again at the current epoch, AEAD grade.
    let sealed = b_kl.seal(b"back from dormancy").expect("returned B seals");
    let opened = o_kl.open(&sealed).expect("O reads the returner");
    assert_eq!(opened, b"back from dormancy");

    println!(
        "P4 done-when MEASURED (loopback): invite path (fold decision → InviteApproval → real \
         Add+Welcome → seated, AEAD round-trip) and token-return path (real external commit + \
         PSK → staged claims → chain-derived cross-check → MergeApproval → merged → admission \
         fact folded, AEAD round-trip) both green on openmls 0.8.1. Rungs: governance real-\
         Ed25519; MLS Rung A; transport loopback = Modeled."
    );
}

/// **The S16 arm: validity is not admission, and the port cannot be talked
/// past the decision.**
#[test]
fn a_strangers_valid_commit_stages_but_never_seats() {
    let o = pid(0x20);
    let stranger = pid(0x66);
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

    let mut o_kl = OpenMlsKeyLayer::new(o);
    o_kl.create_group().expect("plant");

    // The stranger somehow holds token BYTES (a leaked ledger — S23 says
    // every departing member exits holding it all) and mints a
    // cryptographically flawless external commit with them.
    let leaked = TokenId::new([0x99; 32]);
    let secret = [0x11; 32];
    o_kl.store_token(&leaked, &secret).expect("incumbent ledger");
    let mut s_kl = OpenMlsKeyLayer::new(stranger);
    s_kl.store_token(&leaked, &secret).expect("leaked bytes");

    let gi = o_kl.group_info_with_tree().expect("served");
    let wire = s_kl
        .return_via_external_commit(&gi, &leaked)
        .expect("the stranger's commit is cryptographically VALID — that is S16's whole point");

    // It stages: the port reports what it says, no opinion.
    let claims = o_kl.stage_commit(&wire).expect("valid bytes stage");
    assert_eq!(claims.joiner_lineage, stranger);

    // The decision refuses: no issuance fact on the chain names this token.
    let issuance = issuance_view(store.log(&group()));
    let ctx = AdmissionContext {
        issuance: &issuance,
        subject_standing: SubjectStanding::Good,
        currency: HeadCurrency::new(),
        freshness: 1,
        member_count: 1,
        acceptor_frontier: 1,
    };
    let refusal = evaluate_admission(&claims, &ctx).expect_err("bytes are not facts");
    assert_eq!(refusal, AdmissionRefusal::NoIssuanceFact);

    // No approval exists, so no merge is expressible: the stranger is not
    // seated, and the group's next message is not readable by them.
    assert_eq!(o_kl.member_count().expect("count"), 1, "still just O");
}
