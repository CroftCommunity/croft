//! The pure half of camp-at-attach (Phase 11 M4e, O1): whether this attach
//! presents a camping pass, and what each camp-mint outcome does to the camp.
//! Decisions only — no I/O, no clock reads; the shell performs the plan and
//! feeds the outcome back, exactly [`crate::dial`]'s shape.
//!
//! Posture (plan M4e), deliberately unlike dial:
//! - **the token is the cache** (O1 point 3): a minted pass is reused across
//!   reconnects until [`REMINT_MARGIN_MILLIS`] before its expiry — the identity
//!   round-trip happens once per pass lifetime, not per connectivity flap;
//! - a camp REFUSAL still camps tokenless WITH the worded reason — in open
//!   mode reception must keep working, and under enforce the relay's refusal
//!   is the visible gate;
//! - an OUTAGE camps tokenless with the M4c-style note;
//! - signed-out has no camping identity under O1: tokenless, silently —
//!   exactly v0.4.0 in open mode.

use crate::model::{CampOutcome, CampPass, CampRefusal, Failure};

/// Re-mint when the pass is within this margin of its expiry.
pub const REMINT_MARGIN_MILLIS: i64 = 60_000;

/// What this attach should do about a camping pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Camp with the cached pass; no network.
    UseCached {
        /// The cached token.
        token: String,
    },
    /// Call the camp mint, then act on the outcome.
    Mint,
    /// Camp with no token; `note` surfaces when there is one.
    CampTokenless {
        /// Words for the screen, when there are any.
        note: Option<String>,
    },
}

/// Decide what this attach does about a camping pass.
///
/// `now_ms` is a parameter, never read: "is my pass still good?" is the one
/// decision that needs the time, and the shell supplies it (D4).
#[must_use]
pub fn plan(signed_in: bool, cached: Option<&CampPass>, now_ms: i64) -> Plan {
    if !signed_in {
        return Plan::CampTokenless { note: None };
    }
    match cached {
        Some(pass) if now_ms + REMINT_MARGIN_MILLIS < pass.expires_at_millis => Plan::UseCached {
            token: pass.token.clone(),
        },
        _ => Plan::Mint,
    }
}

/// What a camp-mint outcome does to the camp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Camp with the pass, and cache it for the next attach.
    Camp {
        /// The token to bind on the endpoint.
        auth_token: String,
        /// The pass to cache, with its wire-derived expiry.
        pass: CampPass,
    },
    /// Camp with no token; `note` surfaces when there is one.
    CampTokenless {
        /// Words for the screen, when there are any.
        note: Option<String>,
    },
}

/// The words for each camp refusal. Each reason has its own; a refusal that
/// dies quietly is a defect even when reception works.
fn refusal_words(reason: CampRefusal) -> &'static str {
    match reason {
        CampRefusal::EndpointUnbound => "this device is not published by your account",
        CampRefusal::JwtInvalid => "identity proof was not accepted",
        CampRefusal::Replay => "identity proof was already used — try again",
        CampRefusal::ProofUnsupported => "the admission service does not accept identity proofs",
        CampRefusal::NoProof => "no identity proof was presented",
        CampRefusal::UnknownKey => "this key is not trusted by the admission service",
        CampRefusal::Unknown => "the camping pass was refused",
    }
}

/// Decide what a camp-mint outcome does to the camp.
#[must_use]
pub fn action(outcome: CampOutcome, now_ms: i64) -> Action {
    match outcome {
        CampOutcome::Minted {
            token,
            expires_in_secs,
        } => Action::Camp {
            pass: CampPass {
                token: token.clone(),
                expires_at_millis: now_ms + expires_in_secs * 1000,
            },
            auth_token: token,
        },
        CampOutcome::Refused(reason) => Action::CampTokenless {
            note: Some(refusal_words(reason).to_string()),
        },
        CampOutcome::Unavailable => Action::CampTokenless {
            note: Some("admission service unreachable — camping without a pass".to_string()),
        },
        CampOutcome::BadRequest => Action::CampTokenless {
            note: Some("client error building the camping request".to_string()),
        },
    }
}

/// Words for a camp attempt that failed without an outcome, or `None` when
/// there is nothing honest to say: a cancellation means the app rebound or
/// backgrounded mid-attempt — a lifecycle event, not a refusal — and rendering
/// it as "camping pass setup failed" put a scary, unactionable line on screen
/// during device runs (2026-08-28).
#[must_use]
pub fn failure_note(failure: &Failure) -> Option<String> {
    match failure {
        Failure::Cancelled => None,
        Failure::Error { message } => Some(format!("camping pass setup failed: {message}")),
    }
}
