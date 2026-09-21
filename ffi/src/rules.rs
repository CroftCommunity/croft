//! The camp and dial rules at the boundary (D3.2).
//!
//! `call-core` decides; these are its decisions as uniffi functions, with
//! boundary types that mirror the core's rather than re-exporting them —
//! the same reason the chat surface mirrors `chat_core::model::Intent`. The
//! Kotlin `CampAdmission` and `DialAdmission` objects keep their shape and
//! delegate here, so every `PIN:` row of the enforcement matrix keeps
//! grading the shipped app while the rules run in Rust. Words cross
//! verbatim from the core: a shell that re-words a refusal is a shell that
//! can flatter the record.

use call_core::{camp, dial, model};

/// A minted camping pass: the opaque token and when the WIRE said it
/// expires (the token is never parsed).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CampPass {
    /// The opaque relay token.
    pub token: String,
    /// Absolute expiry in milliseconds, derived from the mint's `expiresIn`.
    pub expires_at_millis: i64,
}

impl From<CampPass> for model::CampPass {
    fn from(p: CampPass) -> Self {
        model::CampPass {
            token: p.token,
            expires_at_millis: p.expires_at_millis,
        }
    }
}

impl From<model::CampPass> for CampPass {
    fn from(p: model::CampPass) -> Self {
        CampPass {
            token: p.token,
            expires_at_millis: p.expires_at_millis,
        }
    }
}

/// What this attach should do about a camping pass.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CampPlan {
    /// Camp with the cached pass; no network.
    UseCached {
        /// The cached token.
        token: String,
    },
    /// Call the camp mint, then act on the outcome.
    Mint,
    /// Camp with no token; `note` surfaces when there is one.
    CampTokenless {
        /// Why, in words, when there is anything honest to say.
        note: Option<String>,
    },
}

/// Why a camp mint was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CampRefusal {
    /// No identity proof was presented.
    NoProof,
    /// The admission service does not accept identity proofs.
    ProofUnsupported,
    /// The identity proof was not accepted.
    JwtInvalid,
    /// The identity proof was already used.
    Replay,
    /// The key is not trusted by the admission service.
    UnknownKey,
    /// This device is not published by the account.
    EndpointUnbound,
    /// A refusal the client does not know by name.
    Unknown,
}

impl From<CampRefusal> for model::CampRefusal {
    fn from(r: CampRefusal) -> Self {
        match r {
            CampRefusal::NoProof => model::CampRefusal::NoProof,
            CampRefusal::ProofUnsupported => model::CampRefusal::ProofUnsupported,
            CampRefusal::JwtInvalid => model::CampRefusal::JwtInvalid,
            CampRefusal::Replay => model::CampRefusal::Replay,
            CampRefusal::UnknownKey => model::CampRefusal::UnknownKey,
            CampRefusal::EndpointUnbound => model::CampRefusal::EndpointUnbound,
            CampRefusal::Unknown => model::CampRefusal::Unknown,
        }
    }
}

/// Every way a camp mint call ends.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CampOutcome {
    /// The relay token, opaque, and the wire's expiry.
    Minted {
        /// The token.
        token: String,
        /// `expiresIn` from the mint response.
        expires_in_secs: i64,
    },
    /// The mint said no, and why.
    Refused {
        /// The server's discriminant.
        reason: CampRefusal,
    },
    /// The mint is down — try again, conclude nothing about permission.
    Unavailable,
    /// We sent something malformed — a defect here, not a policy fact.
    BadRequest,
}

impl From<CampOutcome> for model::CampOutcome {
    fn from(o: CampOutcome) -> Self {
        match o {
            CampOutcome::Minted {
                token,
                expires_in_secs,
            } => model::CampOutcome::Minted {
                token,
                expires_in_secs,
            },
            CampOutcome::Refused { reason } => model::CampOutcome::Refused(reason.into()),
            CampOutcome::Unavailable => model::CampOutcome::Unavailable,
            CampOutcome::BadRequest => model::CampOutcome::BadRequest,
        }
    }
}

/// What a camp-mint outcome does to the camp.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CampAction {
    /// Camp with the pass, and cache it for the next attach.
    Camp {
        /// The token to bind.
        auth_token: String,
        /// The pass to cache.
        pass: CampPass,
    },
    /// Camp with no token; `note` surfaces when there is one.
    CampTokenless {
        /// Why, in words, when there is anything honest to say.
        note: Option<String>,
    },
}

/// How a camp attempt ended without an outcome.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CampFailure {
    /// The app rebound or backgrounded mid-attempt — a lifecycle event, not
    /// a refusal, and not worth words on a screen.
    Cancelled,
    /// A real failure, in its own words.
    Error {
        /// What went wrong.
        message: String,
    },
}

/// Whether this attach presents a camping pass.
#[uniffi::export]
#[must_use]
pub fn camp_plan(signed_in: bool, cached: Option<CampPass>, now_ms: i64) -> CampPlan {
    let cached = cached.map(model::CampPass::from);
    match camp::plan(signed_in, cached.as_ref(), now_ms) {
        camp::Plan::UseCached { token } => CampPlan::UseCached { token },
        camp::Plan::Mint => CampPlan::Mint,
        camp::Plan::CampTokenless { note } => CampPlan::CampTokenless { note },
    }
}

/// What a camp-mint outcome does to the camp.
#[uniffi::export]
#[must_use]
pub fn camp_action(outcome: CampOutcome, now_ms: i64) -> CampAction {
    match camp::action(outcome.into(), now_ms) {
        camp::Action::Camp { auth_token, pass } => CampAction::Camp {
            auth_token,
            pass: pass.into(),
        },
        camp::Action::CampTokenless { note } => CampAction::CampTokenless { note },
    }
}

/// Words for a camp attempt that threw, or nothing when there is nothing
/// honest to say.
#[uniffi::export]
#[must_use]
pub fn camp_failure_note(failure: CampFailure) -> Option<String> {
    let failure = match failure {
        CampFailure::Cancelled => model::Failure::Cancelled,
        CampFailure::Error { message } => model::Failure::Error { message },
    };
    camp::failure_note(&failure)
}

/// The callee as the card carries it.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct DialCallee {
    /// The callee's DID, when the card carries one (v2 contract).
    pub did: Option<String>,
    /// The grant rkey, when the card carries one (v2 contract).
    pub grant: Option<String>,
    /// The retained invite secret, when this callee came from a redeem.
    pub ticket_secret: Option<String>,
}

/// Where the mint's proof comes from.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ProofSource {
    /// The retained invite secret.
    Ticket {
        /// The secret.
        secret: String,
    },
    /// A service-auth JWT from the caller's session (fetched at dial).
    ServiceAuth,
}

/// What this dial should do about admission.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum DialPlan {
    /// Call the mint with this proof, then act on the outcome.
    Mint {
        /// The callee's DID.
        callee_did: String,
        /// The grant.
        grant: String,
        /// The proof to present.
        proof: ProofSource,
    },
    /// Dial with no token; `note` surfaces when there is one.
    DialTokenless {
        /// Why, in words, when there is anything honest to say.
        note: Option<String>,
    },
}

/// The dial mint's refusal discriminants, plus a fail-closed unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DialRefusal {
    /// No invite was presented.
    NoCap,
    /// The invite no longer exists.
    CapNotFound,
    /// The invite has been revoked.
    CapRevoked,
    /// The invite does not admit this caller.
    CapMismatch,
    /// The identity proof was not accepted.
    JwtInvalid,
    /// The identity proof was already used.
    Replay,
    /// The sponsoring account is out of quota.
    QuotaExhausted,
    /// A refusal the client does not know by name.
    Unknown,
}

impl From<DialRefusal> for model::Refusal {
    fn from(r: DialRefusal) -> Self {
        match r {
            DialRefusal::NoCap => model::Refusal::NoCap,
            DialRefusal::CapNotFound => model::Refusal::CapNotFound,
            DialRefusal::CapRevoked => model::Refusal::CapRevoked,
            DialRefusal::CapMismatch => model::Refusal::CapMismatch,
            DialRefusal::JwtInvalid => model::Refusal::JwtInvalid,
            DialRefusal::Replay => model::Refusal::Replay,
            DialRefusal::QuotaExhausted => model::Refusal::QuotaExhausted,
            DialRefusal::Unknown => model::Refusal::Unknown,
        }
    }
}

/// Every way a dial mint call ends.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum DialOutcome {
    /// The relay token, opaque.
    Minted {
        /// The token.
        token: String,
    },
    /// The mint said no, and why.
    Refused {
        /// The server's discriminant.
        reason: DialRefusal,
    },
    /// The mint is down — try again, conclude nothing about permission.
    Unavailable,
    /// We sent something malformed — a defect here, not a policy fact.
    BadRequest,
}

/// What a dial-mint outcome does to the dial.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum DialAction {
    /// Proceed, with the token when there is one.
    Dial {
        /// The token, when minted.
        auth_token: Option<String>,
        /// A note for the screen, when there is one.
        note: Option<String>,
    },
    /// Do not dial; show why.
    Refuse {
        /// The refusal in words.
        message: String,
    },
}

/// What a dial must do to the endpoint's currently bound relay token.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RebindDecision {
    /// Leave the endpoint alone: the wanted token is already bound, or
    /// binding it would lower admission.
    Keep,
    /// Bind `token`; the endpoint stops and re-attaches. Never null — the
    /// only downgrade available is giving up a token, and that is a Keep.
    Swap {
        /// The token to bind.
        token: String,
    },
}

/// Which proof this dial presents.
#[uniffi::export]
#[must_use]
pub fn dial_plan(callee: DialCallee, signed_in: bool) -> DialPlan {
    let callee = model::Callee {
        did: callee.did,
        grant: callee.grant,
        ticket_secret: callee.ticket_secret,
    };
    match dial::plan(&callee, signed_in) {
        dial::Plan::Mint {
            callee_did,
            grant,
            proof,
        } => DialPlan::Mint {
            callee_did,
            grant,
            proof: match proof {
                dial::ProofSource::Ticket { secret } => ProofSource::Ticket { secret },
                dial::ProofSource::ServiceAuth => ProofSource::ServiceAuth,
            },
        },
        dial::Plan::DialTokenless { note } => DialPlan::DialTokenless { note },
    }
}

/// What a dial-mint outcome does to the dial.
#[uniffi::export]
#[must_use]
pub fn dial_action(outcome: DialOutcome) -> DialAction {
    let outcome = match outcome {
        DialOutcome::Minted { token } => model::Outcome::Minted { token },
        DialOutcome::Refused { reason } => model::Outcome::Refused(reason.into()),
        DialOutcome::Unavailable => model::Outcome::Unavailable,
        DialOutcome::BadRequest => model::Outcome::BadRequest,
    };
    match dial::action(outcome) {
        dial::Action::Dial { auth_token, note } => DialAction::Dial { auth_token, note },
        dial::Action::Refuse { message } => DialAction::Refuse { message },
    }
}

/// A dial never lowers the endpoint's admission (R0).
#[uniffi::export]
#[must_use]
pub fn dial_rebind(current: Option<String>, wanted: Option<String>) -> RebindDecision {
    match dial::rebind(current.as_deref(), wanted.as_deref()) {
        dial::Rebind::Keep => RebindDecision::Keep,
        dial::Rebind::Swap { token } => RebindDecision::Swap { token },
    }
}
