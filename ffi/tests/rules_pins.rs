//! D3.2: the camp and dial rules cross the boundary as functions.
//!
//! The rules themselves are `call-core`'s and are graded by the enforcement
//! matrix there; what these pin is the TRANSLATION — that a plan, an outcome,
//! an action and a rebind decision cross with their fields and words intact,
//! so the Kotlin `CampAdmission` / `DialAdmission` objects can delegate here
//! and every `PIN:` row in the matrix keeps passing against the Rust rules.

use croft_ffi::{
    camp_action, camp_failure_note, camp_plan, dial_action, dial_plan, dial_rebind, CampAction,
    CampFailure, CampOutcome, CampPass, CampPlan, CampRefusal, DialAction, DialCallee, DialOutcome,
    DialPlan, DialRefusal, ProofSource, RebindDecision,
};

#[test]
fn a_signed_out_camp_plans_tokenless_with_no_note() {
    assert_eq!(
        camp_plan(false, None, 1_000),
        CampPlan::CampTokenless { note: None }
    );
}

#[test]
fn a_live_cached_pass_is_reused_and_one_inside_the_margin_mints() {
    let pass = CampPass {
        token: "camp-pass".to_string(),
        expires_at_millis: 100_000,
    };
    assert_eq!(
        camp_plan(true, Some(pass.clone()), 100_000 - 60_001),
        CampPlan::UseCached {
            token: "camp-pass".to_string()
        }
    );
    assert_eq!(
        camp_plan(true, Some(pass), 100_000 - 60_000),
        CampPlan::Mint
    );
}

#[test]
fn a_mint_crosses_as_camp_with_the_pass_dated_by_the_wire() {
    let action = camp_action(
        CampOutcome::Minted {
            token: "tok".to_string(),
            expires_in_secs: 30,
        },
        5_000,
    );
    assert_eq!(
        action,
        CampAction::Camp {
            auth_token: "tok".to_string(),
            pass: CampPass {
                token: "tok".to_string(),
                expires_at_millis: 35_000,
            },
        }
    );
}

#[test]
fn a_refusal_crosses_with_its_own_words() {
    let action = camp_action(
        CampOutcome::Refused {
            reason: CampRefusal::EndpointUnbound,
        },
        0,
    );
    assert_eq!(
        action,
        CampAction::CampTokenless {
            note: Some("this device is not published by your account".to_string())
        }
    );
    assert_eq!(
        camp_action(CampOutcome::Unavailable, 0),
        CampAction::CampTokenless {
            note: Some("admission service unreachable — camping without a pass".to_string())
        }
    );
}

#[test]
fn a_cancellation_earns_no_words_and_an_error_does() {
    assert_eq!(camp_failure_note(CampFailure::Cancelled), None);
    assert_eq!(
        camp_failure_note(CampFailure::Error {
            message: "boom".to_string()
        }),
        Some("camping pass setup failed: boom".to_string())
    );
}

#[test]
fn a_grant_with_no_usable_proof_nudges_and_a_ticket_is_the_proof() {
    let callee = DialCallee {
        did: Some("did:plc:callee".to_string()),
        grant: Some("g1".to_string()),
        ticket_secret: None,
    };
    assert_eq!(
        dial_plan(callee.clone(), false),
        DialPlan::DialTokenless {
            note: Some("sign in to present this grant".to_string())
        }
    );
    let with_secret = DialCallee {
        ticket_secret: Some("s3cr3t".to_string()),
        ..callee
    };
    assert_eq!(
        dial_plan(with_secret, false),
        DialPlan::Mint {
            callee_did: "did:plc:callee".to_string(),
            grant: "g1".to_string(),
            proof: ProofSource::Ticket {
                secret: "s3cr3t".to_string()
            },
        }
    );
}

#[test]
fn a_dial_refusal_never_dials_and_an_outage_dials_tokenless_and_says_so() {
    assert_eq!(
        dial_action(DialOutcome::Refused {
            reason: DialRefusal::CapRevoked
        }),
        DialAction::Refuse {
            message: "this invite has been revoked".to_string()
        }
    );
    assert_eq!(
        dial_action(DialOutcome::Unavailable),
        DialAction::Dial {
            auth_token: None,
            note: Some("admission service unreachable — dialing without a token".to_string()),
        }
    );
}

#[test]
fn a_dial_never_lowers_admission_and_the_swap_token_is_never_null() {
    assert_eq!(
        dial_rebind(Some("pass".to_string()), None),
        RebindDecision::Keep
    );
    assert_eq!(
        dial_rebind(Some("pass".to_string()), Some("pass".to_string())),
        RebindDecision::Keep
    );
    assert_eq!(
        dial_rebind(None, Some("call-token".to_string())),
        RebindDecision::Swap {
            token: "call-token".to_string()
        }
    );
}
