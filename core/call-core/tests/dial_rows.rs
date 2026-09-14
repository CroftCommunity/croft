//! Dial posture rows of `docs/ENFORCEMENT-SCENARIOS.md` (M4c — refusals never
//! dial; outages dial tokenless), pinned on the Rust side. Mirrors
//! `DialAdmissionTest.kt`; every refusal reason is asserted against its own words.

use call_core::dial::{self, Action, Plan, ProofSource};
use call_core::model::{Callee, Outcome, Refusal};

fn callee(did: Option<&str>, grant: Option<&str>, secret: Option<&str>) -> Callee {
    Callee {
        did: did.map(str::to_string),
        grant: grant.map(str::to_string),
        ticket_secret: secret.map(str::to_string),
    }
}

fn full() -> Callee {
    callee(Some("did:plc:callee"), Some("grant-rkey"), None)
}

#[test]
fn a_ticket_secret_is_the_proof_whenever_the_card_carries_one() {
    let c = callee(Some("did:plc:callee"), Some("grant-rkey"), Some("s3cr3t"));
    // Even signed in: possession proof wins.
    assert_eq!(
        dial::plan(&c, true),
        Plan::Mint {
            callee_did: "did:plc:callee".to_string(),
            grant: "grant-rkey".to_string(),
            proof: ProofSource::Ticket {
                secret: "s3cr3t".to_string()
            },
        }
    );
}

#[test]
fn a_signed_in_caller_proves_identity_when_there_is_no_secret() {
    assert_eq!(
        dial::plan(&full(), true),
        Plan::Mint {
            callee_did: "did:plc:callee".to_string(),
            grant: "grant-rkey".to_string(),
            proof: ProofSource::ServiceAuth,
        }
    );
}

#[test]
fn no_grant_means_the_v1_tokenless_dial_silently() {
    assert_eq!(
        dial::plan(&callee(Some("did:plc:callee"), None, None), true),
        Plan::DialTokenless { note: None }
    );
    assert_eq!(
        dial::plan(&callee(None, Some("grant-rkey"), None), true),
        Plan::DialTokenless { note: None }
    );
}

#[test]
fn a_grant_with_no_usable_proof_dials_tokenless_with_a_sign_in_nudge() {
    assert_eq!(
        dial::plan(&full(), false),
        Plan::DialTokenless {
            note: Some("sign in to present this grant".to_string())
        }
    );
}

#[test]
fn a_minted_token_dials_with_it() {
    assert_eq!(
        dial::action(Outcome::Minted {
            token: "tok".to_string()
        }),
        Action::Dial {
            auth_token: Some("tok".to_string()),
            note: None
        }
    );
}

#[test]
fn every_refusal_blocks_the_dial_with_its_own_honest_message() {
    let expected = [
        (Refusal::CapRevoked, "this invite has been revoked"),
        (Refusal::CapNotFound, "this invite no longer exists"),
        (Refusal::CapMismatch, "this invite does not admit you"),
        (Refusal::JwtInvalid, "identity proof was not accepted"),
        (
            Refusal::Replay,
            "identity proof was already used — try again",
        ),
        (
            Refusal::QuotaExhausted,
            "the sponsoring account is out of quota",
        ),
        (Refusal::NoCap, "no invite was presented"),
        (Refusal::Unknown, "the call was not permitted"),
    ];
    assert_eq!(
        expected.len(),
        Refusal::ALL.len(),
        "a refusal reason has no row here"
    );
    for (reason, message) in expected {
        assert_eq!(
            dial::action(Outcome::Refused(reason)),
            Action::Refuse {
                message: message.to_string()
            },
            "{reason:?}"
        );
    }
}

#[test]
fn an_admit_outage_dials_tokenless_and_says_so() {
    assert_eq!(
        dial::action(Outcome::Unavailable),
        Action::Dial {
            auth_token: None,
            note: Some("admission service unreachable — dialing without a token".to_string()),
        }
    );
}

#[test]
fn a_bad_request_is_a_client_defect_and_blocks_the_dial() {
    assert_eq!(
        dial::action(Outcome::BadRequest),
        Action::Refuse {
            message: "client error building the admission request".to_string()
        }
    );
}
