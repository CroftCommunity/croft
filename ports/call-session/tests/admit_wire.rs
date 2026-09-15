//! The camp mint's wire, mapped onto call-core's `CampOutcome` exactly as
//! `Admit.kt` maps it — so the same rule (`call_core::camp::action`) reads
//! the same facts on both sides.
//!
//! Wire shapes from the server source (croft-stack
//! `croft-relay-admit/src/camp.rs`): the request is `{endpoint, proof:
//! {serviceAuth}}` with deny_unknown_fields; 200 carries `{token, expiresIn}`;
//! a refusal is 403 with `{error: <discriminant>}`; 400 is our defect; anything
//! else is the mint being unavailable, which is not an authorization answer.

use call_core::model::{CampOutcome, CampRefusal};
use call_session::admit::{camp_outcome, camp_request};

#[test]
fn the_request_carries_exactly_the_named_fields() {
    let v = camp_request("ab".repeat(32).as_str(), "the.jwt");
    assert_eq!(v["endpoint"], "ab".repeat(32));
    assert_eq!(v["proof"]["serviceAuth"], "the.jwt");
    assert_eq!(v.as_object().map(serde_json::Map::len), Some(2));
    assert_eq!(v["proof"].as_object().map(serde_json::Map::len), Some(1));
}

#[test]
fn a_200_with_a_token_is_minted_with_its_expiry() {
    assert_eq!(
        camp_outcome(200, r#"{"token":"t.o.k","expiresIn":43200}"#),
        CampOutcome::Minted {
            token: "t.o.k".to_string(),
            expires_in_secs: 43_200
        }
    );
}

#[test]
fn a_200_without_a_token_is_not_a_mint() {
    // Whatever the status said. Admit.kt: "A 200 with no token is not a mint".
    assert_eq!(
        camp_outcome(200, r#"{"expiresIn":43200}"#),
        CampOutcome::Unavailable
    );
    assert_eq!(camp_outcome(200, "not json"), CampOutcome::Unavailable);
}

#[test]
fn every_403_discriminant_maps_and_an_unknown_one_fails_closed() {
    for (wire, want) in [
        ("no_proof", CampRefusal::NoProof),
        ("proof_unsupported", CampRefusal::ProofUnsupported),
        ("jwt_invalid", CampRefusal::JwtInvalid),
        ("replay", CampRefusal::Replay),
        ("unknown_key", CampRefusal::UnknownKey),
        ("endpoint_unbound", CampRefusal::EndpointUnbound),
        ("something_new", CampRefusal::Unknown),
    ] {
        assert_eq!(
            camp_outcome(403, &format!(r#"{{"error":"{wire}"}}"#)),
            CampOutcome::Refused(want),
            "{wire}"
        );
    }
    assert_eq!(
        camp_outcome(403, "garbage"),
        CampOutcome::Refused(CampRefusal::Unknown)
    );
}

#[test]
fn a_400_is_our_defect_and_anything_else_is_the_mint_being_down() {
    assert_eq!(
        camp_outcome(400, r#"{"error":"bad_request"}"#),
        CampOutcome::BadRequest
    );
    for status in [500, 502, 503, 404, 401] {
        assert_eq!(
            camp_outcome(status, ""),
            CampOutcome::Unavailable,
            "{status}"
        );
    }
}
