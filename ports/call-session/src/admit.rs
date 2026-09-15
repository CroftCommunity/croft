//! The camp mint's wire (`POST /campToken`), mapped onto call-core's
//! `CampOutcome` exactly as `Admit.kt` maps it.
//!
//! Wire shapes from the server source (croft-stack
//! `croft-relay-admit/src/camp.rs`), never inferred: the request is
//! `{endpoint, proof: {serviceAuth}}` (deny_unknown_fields); 200 carries
//! `{token, expiresIn}`; 403 carries `{error: <discriminant>}`. Never log
//! the proof or a minted token.

use call_core::model::{CampOutcome, CampRefusal};

/// croft-admit, the mint.
pub const ADMIT_BASE: &str = "https://admit.croft.ing";
/// The proof audience: croft-admit's DID.
pub const ADMIT_AUD: &str = "did:web:admit.croft.ing";
/// The camp mint's method binding.
pub const ADMIT_CAMP_LXM: &str = "ing.croft.relay.campToken";

/// The request body.
#[must_use]
pub fn camp_request(endpoint_id: &str, service_auth_jwt: &str) -> serde_json::Value {
    serde_json::json!({
        "endpoint": endpoint_id,
        "proof": { "serviceAuth": service_auth_jwt },
    })
}

/// The mint's answer as a fact call-core can act on.
#[must_use]
pub fn camp_outcome(status: u16, body: &str) -> CampOutcome {
    let json: Option<serde_json::Value> = serde_json::from_str(body).ok();
    match status {
        200 => {
            let token = json
                .as_ref()
                .and_then(|j| j.get("token"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if token.is_empty() {
                // A 200 with no token is not a mint, whatever the status said.
                return CampOutcome::Unavailable;
            }
            let expires_in_secs = json
                .as_ref()
                .and_then(|j| j.get("expiresIn"))
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            CampOutcome::Minted {
                token: token.to_string(),
                expires_in_secs,
            }
        }
        403 => {
            let wire = json
                .as_ref()
                .and_then(|j| j.get("error"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            CampOutcome::Refused(match wire {
                "no_proof" => CampRefusal::NoProof,
                "proof_unsupported" => CampRefusal::ProofUnsupported,
                "jwt_invalid" => CampRefusal::JwtInvalid,
                "replay" => CampRefusal::Replay,
                "unknown_key" => CampRefusal::UnknownKey,
                "endpoint_unbound" => CampRefusal::EndpointUnbound,
                _ => CampRefusal::Unknown,
            })
        }
        400 => CampOutcome::BadRequest,
        _ => CampOutcome::Unavailable,
    }
}
