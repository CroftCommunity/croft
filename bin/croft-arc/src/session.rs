//! The session policy, pure — and the words a dead session is allowed.
//!
//! An app-password session rather than OAuth, deliberately: this is an
//! instrument on a laptop, the OAuth scope the app uses is
//! `transition:generic` (app-password equivalence, `AuthManager.SCOPE`),
//! and the proof the mint verifies — `getServiceAuth`'s JWT — is the PDS's
//! either way. The §15.2 honesty case is the same shape on both: a stored
//! token is not a session, and a refresh the PDS refuses is a refusal.

use serde::{Deserialize, Serialize};

/// Refresh this long before nominal expiry — a token that dies mid-mint
/// helps nobody (the Kotlin's `EXPIRY_MARGIN_MS`, same reasoning).
pub const REFRESH_MARGIN_SECS: u64 = 60;

/// What the state directory remembers between runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredSession {
    /// The account's DID.
    pub did: String,
    /// The account's handle as the PDS reported it.
    pub handle: String,
    /// The PDS base URL the session was made on.
    pub pds: String,
    /// The access token, presented as `Bearer`.
    pub access_jwt: String,
    /// The refresh token.
    pub refresh_jwt: String,
    /// When the access token expires (unix seconds), read from its `exp`.
    pub access_expires_at_secs: u64,
}

/// What to do with what is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Nothing stored: make a session from credentials.
    SignIn,
    /// The access token is fresh enough to present.
    Use,
    /// The access token is inside the margin: refresh before presenting.
    Refresh,
}

/// The decision.
#[must_use]
pub fn decide(stored: Option<&StoredSession>, now_secs: u64) -> Step {
    match stored {
        None => Step::SignIn,
        Some(s) if now_secs + REFRESH_MARGIN_SECS < s.access_expires_at_secs => Step::Use,
        Some(_) => Step::Refresh,
    }
}

/// Why a refresh did not produce a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshFailure {
    /// The PDS refused the refresh token: the session is dead.
    Dead {
        /// The PDS's error discriminant (`ExpiredToken`, `invalid_grant`, …).
        error: String,
    },
    /// The PDS could not be reached or answered with something else:
    /// nothing is known about the session.
    Unavailable {
        /// What happened.
        reason: String,
    },
}

/// The words for a refresh that did not produce a session.
///
/// A dead session says so and says what to do; it never says "signed in".
/// An outage is not an authorization answer and does not say "dead".
#[must_use]
pub fn refusal_words(failure: &RefreshFailure) -> String {
    match failure {
        RefreshFailure::Dead { error } => format!(
            "session dead ({error}): the PDS refused the refresh token — sign in again \
             (CROFT_ARC_HANDLE + CROFT_ARC_APP_PASSWORD); this endpoint is NOT camped and \
             calls cannot reach it"
        ),
        RefreshFailure::Unavailable { reason } => format!(
            "session unknown: the PDS could not be asked ({reason}) — not a refusal, try again"
        ),
    }
}

/// The `exp` claim of a JWT, read WITHOUT verification and used only to
/// schedule a refresh. Nothing else in the token is believed.
#[must_use]
pub fn access_expiry_secs(jwt: &str) -> Option<u64> {
    let mut parts = jwt.split('.');
    let (Some(_header), Some(claims), Some(_sig), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    let raw = data_encoding::BASE64URL_NOPAD
        .decode(claims.as_bytes())
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    value.get("exp")?.as_u64()
}
