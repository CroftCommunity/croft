//! Camp posture rows of `docs/ENFORCEMENT-SCENARIOS.md` (M4e — the pass is the
//! cache), pinned on the Rust side. Each test name is a `RUST:` pin in that file;
//! the walker (`enforcement_matrix.rs`) proves the row reaches it. Mirrors
//! `CampAdmissionTest.kt` edge for edge — the boundary is pinned at three points
//! (inside, exactly at, one past) so the rule cannot drift by one comparison
//! operator with both suites green.

use call_core::camp::{self, Action, Plan, REMINT_MARGIN_MILLIS};
use call_core::model::{CampOutcome, CampPass, CampRefusal, Failure};

const NOW: i64 = 1_700_000_000_000;

fn pass(expires_at_millis: i64) -> CampPass {
    CampPass {
        token: "opaque".to_string(),
        expires_at_millis,
    }
}

fn tokenless_note(action: Action) -> Option<String> {
    match action {
        Action::CampTokenless { note } => note,
        other => panic!("expected CampTokenless, got {other:?}"),
    }
}

#[test]
fn signed_out_camps_tokenless_with_no_note() {
    assert_eq!(
        camp::plan(false, None, NOW),
        Plan::CampTokenless { note: None }
    );
}

#[test]
fn signed_in_with_no_cached_pass_mints() {
    assert_eq!(camp::plan(true, None, NOW), Plan::Mint);
}

#[test]
fn a_live_cached_pass_is_reused_the_token_is_the_cache() {
    let p = pass(NOW + 3_600_000);
    assert_eq!(
        camp::plan(true, Some(&p), NOW),
        Plan::UseCached {
            token: "opaque".to_string()
        }
    );
}

#[test]
fn a_pass_inside_the_remint_margin_mints_fresh() {
    let p = pass(NOW + REMINT_MARGIN_MILLIS - 1);
    assert_eq!(camp::plan(true, Some(&p), NOW), Plan::Mint);
}

#[test]
fn a_pass_exactly_at_the_margin_boundary_still_mints() {
    let p = pass(NOW + REMINT_MARGIN_MILLIS);
    assert_eq!(camp::plan(true, Some(&p), NOW), Plan::Mint);
}

#[test]
fn a_pass_one_past_the_margin_boundary_is_reused() {
    let p = pass(NOW + REMINT_MARGIN_MILLIS + 1);
    assert_eq!(
        camp::plan(true, Some(&p), NOW),
        Plan::UseCached {
            token: "opaque".to_string()
        }
    );
}

#[test]
fn signing_out_drops_the_pass() {
    // A cached pass without a session does not camp.
    let p = pass(NOW + 3_600_000);
    assert_eq!(
        camp::plan(false, Some(&p), NOW),
        Plan::CampTokenless { note: None }
    );
}

#[test]
fn a_mint_camps_with_the_token_and_caches_it_by_the_wires_expires_in() {
    let action = camp::action(
        CampOutcome::Minted {
            token: "tok".to_string(),
            expires_in_secs: 600,
        },
        NOW,
    );
    assert_eq!(
        action,
        Action::Camp {
            auth_token: "tok".to_string(),
            pass: CampPass {
                token: "tok".to_string(),
                expires_at_millis: NOW + 600_000
            },
        }
    );
}

#[test]
fn a_refusal_camps_tokenless_with_words() {
    // Reception must not die quietly.
    let note = tokenless_note(camp::action(
        CampOutcome::Refused(CampRefusal::EndpointUnbound),
        NOW,
    ));
    assert_eq!(
        note.as_deref(),
        Some("this device is not published by your account")
    );
}

#[test]
fn each_refusal_reason_has_its_own_words() {
    let expected = [
        (
            CampRefusal::EndpointUnbound,
            "this device is not published by your account",
        ),
        (CampRefusal::JwtInvalid, "identity proof was not accepted"),
        (
            CampRefusal::Replay,
            "identity proof was already used — try again",
        ),
        (
            CampRefusal::ProofUnsupported,
            "the admission service does not accept identity proofs",
        ),
        (CampRefusal::NoProof, "no identity proof was presented"),
        (
            CampRefusal::UnknownKey,
            "this key is not trusted by the admission service",
        ),
        (CampRefusal::Unknown, "the camping pass was refused"),
    ];
    assert_eq!(
        expected.len(),
        CampRefusal::ALL.len(),
        "a refusal reason has no row here"
    );
    for (reason, words) in expected {
        let note = tokenless_note(camp::action(CampOutcome::Refused(reason), NOW));
        assert_eq!(note.as_deref(), Some(words), "{reason:?}");
    }
}

#[test]
fn an_outage_camps_tokenless_with_the_availability_note() {
    let note = tokenless_note(camp::action(CampOutcome::Unavailable, NOW));
    assert_eq!(
        note.as_deref(),
        Some("admission service unreachable — camping without a pass")
    );
}

#[test]
fn a_client_defect_camps_tokenless_and_says_so() {
    let note = tokenless_note(camp::action(CampOutcome::BadRequest, NOW));
    assert_eq!(
        note.as_deref(),
        Some("client error building the camping request")
    );
}

#[test]
fn a_cancelled_camp_is_not_a_failure_and_says_nothing() {
    assert_eq!(camp::failure_note(&Failure::Cancelled), None);
}

#[test]
fn a_real_failure_still_says_what_went_wrong() {
    let f = Failure::Error {
        message: "boom".to_string(),
    };
    assert_eq!(
        camp::failure_note(&f).as_deref(),
        Some("camping pass setup failed: boom")
    );
}
