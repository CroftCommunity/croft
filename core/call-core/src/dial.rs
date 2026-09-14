//! The pure half of mint-at-dial (Phase 11 M4c): which proof this dial
//! presents, what each mint outcome does to the dial, and what a dial may do
//! to the endpoint's bound relay token. Decisions only — no I/O; the shell
//! performs the plan and feeds the outcome back.
//!
//! Posture (plan M4c):
//! - a REFUSAL never dials — "not permitted" must never look like a network
//!   flake;
//! - an OUTAGE dials tokenless with a visible note: the relay is the actual
//!   gate (under enforce it refuses the attach), and calling must not die
//!   because croft-admit is down;
//! - a v1 callee (no grant) dials tokenless silently, exactly as v0.4.0;
//! - a grant with no usable proof dials tokenless with a sign-in nudge —
//!   open-mode compatible; the enforce-mode refusal comes from the relay.

use crate::model::{Callee, Outcome, Refusal};

/// Where the mint's proof comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofSource {
    /// The retained invite secret (possession proof).
    Ticket {
        /// The secret from the redeemed ticket.
        secret: String,
    },
    /// A service-auth JWT from the caller's session, fetched at dial.
    ServiceAuth,
}

/// What this dial should do about admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Call the mint with this proof, then act on the outcome.
    Mint {
        /// The callee's DID.
        callee_did: String,
        /// The grant rkey to redeem.
        grant: String,
        /// The proof to present.
        proof: ProofSource,
    },
    /// Dial with no token; `note` surfaces when there is one.
    DialTokenless {
        /// Words for the screen, when there are any.
        note: Option<String>,
    },
}

/// Decide which proof this dial presents.
#[must_use]
pub fn plan(callee: &Callee, signed_in: bool) -> Plan {
    let (Some(did), Some(grant)) = (&callee.did, &callee.grant) else {
        return Plan::DialTokenless { note: None };
    };
    if let Some(secret) = &callee.ticket_secret {
        return Plan::Mint {
            callee_did: did.clone(),
            grant: grant.clone(),
            proof: ProofSource::Ticket {
                secret: secret.clone(),
            },
        };
    }
    if signed_in {
        return Plan::Mint {
            callee_did: did.clone(),
            grant: grant.clone(),
            proof: ProofSource::ServiceAuth,
        };
    }
    Plan::DialTokenless {
        note: Some("sign in to present this grant".to_string()),
    }
}

/// What a mint outcome does to the dial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Proceed, with the token when there is one.
    Dial {
        /// The token to bind, when the mint produced one.
        auth_token: Option<String>,
        /// Words for the screen, when there are any.
        note: Option<String>,
    },
    /// Do not dial; show why.
    Refuse {
        /// The honest reason, in the caller's words.
        message: String,
    },
}

/// The words for each call refusal — each reason its own.
fn refusal_words(reason: Refusal) -> &'static str {
    match reason {
        Refusal::CapRevoked => "this invite has been revoked",
        Refusal::CapNotFound => "this invite no longer exists",
        Refusal::CapMismatch => "this invite does not admit you",
        Refusal::JwtInvalid => "identity proof was not accepted",
        Refusal::Replay => "identity proof was already used — try again",
        Refusal::QuotaExhausted => "the sponsoring account is out of quota",
        Refusal::NoCap => "no invite was presented",
        Refusal::Unknown => "the call was not permitted",
    }
}

/// Decide what a mint outcome does to the dial.
#[must_use]
pub fn action(outcome: Outcome) -> Action {
    match outcome {
        Outcome::Minted { token } => Action::Dial {
            auth_token: Some(token),
            note: None,
        },
        Outcome::Refused(reason) => Action::Refuse {
            message: refusal_words(reason).to_string(),
        },
        Outcome::Unavailable => Action::Dial {
            auth_token: None,
            note: Some("admission service unreachable — dialing without a token".to_string()),
        },
        Outcome::BadRequest => Action::Refuse {
            message: "client error building the admission request".to_string(),
        },
    }
}

/// What a dial must do to the endpoint's currently bound relay token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rebind {
    /// Leave the endpoint alone. Either the wanted token is already bound, or
    /// binding the wanted token would *lower* our admission.
    Keep,
    /// Bind `token`; the endpoint stops and re-attaches, and the camp gaps.
    /// (The Kotlin's `Swap(token: String?)` can never carry null after the
    /// rule below; the port makes that a type.)
    Swap {
        /// The admitting token to bind.
        token: String,
    },
}

/// A dial never lowers the endpoint's admission (§15.3).
///
/// The relay auth token belongs to the *endpoint*, so changing it costs a
/// `stop()`/`start()` — the camped connection dies and re-attaches. Swapping
/// one admitting token for another is the designed M4c path and pays that
/// cost deliberately. Dropping to tokenless while holding a pass pays it for
/// nothing: a camped endpoint is already admitted, and under enforce the
/// tokenless re-attach is refused by definition.
///
/// Measured before this existed (runbook §15.3, a physical phone on the
/// enforcing relay): the tokenless path rebound to null over a live camping
/// pass, the re-attach was refused ~20 times with `no_token`, and the phone
/// stayed unreachable for four minutes until it was restarted. Open mode had
/// hidden it completely — a tokenless re-attach was admitted anyway.
#[must_use]
pub fn rebind(current: Option<&str>, wanted: Option<&str>) -> Rebind {
    match wanted {
        // Already bound — nothing to do.
        w if w == current => Rebind::Keep,
        // The only downgrade available: giving up a token we already hold.
        None => Rebind::Keep,
        Some(token) => Rebind::Swap {
            token: token.to_string(),
        },
    }
}
