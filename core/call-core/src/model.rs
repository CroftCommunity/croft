//! The state a decision reads and the outcome shapes it consumes.
//!
//! The core knows what answers admission can give — minted, refused with a
//! reason, unavailable, bad request — not how to get one: the HTTP call that
//! produces an outcome stays with the shell (`caps/` on Android, D4). These
//! are the decision-relevant fields only; the deep link's other fields
//! (endpoint id, relay URL, handle, device) are the shell's.

/// A minted camping pass: the opaque token and when the WIRE said it expires
/// (`expiresIn` on the mint response — the token is never parsed, D3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampPass {
    /// The opaque relay token.
    pub token: String,
    /// Absolute expiry in milliseconds, derived from the mint's `expiresIn`.
    pub expires_at_millis: i64,
}

/// Why a camp mint was refused (croft-admit `/campToken` reasons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl CampRefusal {
    /// Every reason, for tests that must cover each one.
    pub const ALL: [CampRefusal; 7] = [
        CampRefusal::NoProof,
        CampRefusal::ProofUnsupported,
        CampRefusal::JwtInvalid,
        CampRefusal::Replay,
        CampRefusal::UnknownKey,
        CampRefusal::EndpointUnbound,
        CampRefusal::Unknown,
    ];
}

/// What a camp mint came back with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CampOutcome {
    /// A pass was minted.
    Minted {
        /// The opaque relay token.
        token: String,
        /// Seconds until the wire says it expires.
        expires_in_secs: i64,
    },
    /// The mint was refused for a named reason.
    Refused(CampRefusal),
    /// The admission service could not be reached.
    Unavailable,
    /// The client built a request the service rejected as malformed.
    BadRequest,
}

/// Why a call mint was refused (croft-admit `/grantCall` reasons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// No capability was presented.
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

impl Refusal {
    /// Every reason, for tests that must cover each one.
    pub const ALL: [Refusal; 8] = [
        Refusal::NoCap,
        Refusal::CapNotFound,
        Refusal::CapRevoked,
        Refusal::CapMismatch,
        Refusal::JwtInvalid,
        Refusal::Replay,
        Refusal::QuotaExhausted,
        Refusal::Unknown,
    ];
}

/// What a call mint came back with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A call token was minted.
    Minted {
        /// The opaque relay token.
        token: String,
    },
    /// The mint was refused for a named reason.
    Refused(Refusal),
    /// The admission service could not be reached.
    Unavailable,
    /// The client built a request the service rejected as malformed.
    BadRequest,
}

/// The decision-relevant part of a callee card (deep link or in-app redeem).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Callee {
    /// The callee's DID, when the card carries one (v2 contract).
    pub did: Option<String>,
    /// The grant rkey, when the card carries one (v2 contract).
    pub grant: Option<String>,
    /// The retained invite secret, when this callee came from an in-app redeem.
    pub ticket_secret: Option<String>,
}

/// How a camp attempt ended without an outcome. The JVM's `Throwable` becomes
/// this enum at the port: a cancellation is a lifecycle event (the app rebound
/// or backgrounded mid-attempt), not a refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The attempt was cancelled by the shell's lifecycle.
    Cancelled,
    /// The attempt failed with a message.
    Error {
        /// What went wrong, in the shell's words.
        message: String,
    },
}
