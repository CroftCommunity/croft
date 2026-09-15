//! croft-arc: the headless calling arc, as a library the binary drives.
//!
//! Child plan R3 (`plans/2026-09-08-plan-call-core-and-apple-shell.md`). One
//! command walks the arc against production: load or create a stored
//! session, bind the endpoint, publish this device's record, camp with a
//! self-minted pass, then either wait to be dialled or dial a named peer,
//! hold, hang up — printing each step with the endpoint's short id so a run
//! lays beside the relay journal by eye.
//!
//! It is an instrument, not a client. Every admission decision is
//! `call-core`'s (`camp::plan`/`camp::action`, `dial::plan`); every endpoint
//! act is `call-transport-iroh`'s. What is here is the glue: the PDS calls,
//! the files, the words. The pure pieces — argument parsing, the session
//! policy, record reconciliation, the admit wire mapping, the report lines —
//! are separate modules with hermetic tests; the network glue is exercised
//! only by the `:live` arc (`tests/live_arc.rs`).

#![warn(missing_docs)]
#![forbid(unsafe_code)]

/// The camp mint's wire, onto call-core's outcome.
pub mod admit;
/// The arc itself.
pub mod arc;
/// The command line.
pub mod args;
/// The PDS and admit HTTP client.
pub mod atproto;
/// This device's endpoint record.
pub mod records;
/// The words printed.
pub mod report;
/// The session policy.
pub mod session;
/// The state directory: key, session, pass.
pub mod state;

/// Everything the arc can refuse or fail on, in words.
#[derive(Debug, thiserror::Error)]
pub enum ArcError {
    /// The command line could not be understood.
    #[error("{0}")]
    Args(String),
    /// No stored session and no credentials to make one.
    #[error("no stored session and {0} not set — the first run needs CROFT_ARC_HANDLE and CROFT_ARC_APP_PASSWORD")]
    NoCredentials(&'static str),
    /// The PDS refused to refresh the session: it is dead, not merely stale.
    #[error("{0}")]
    SessionDead(String),
    /// An HTTP request could not be made at all.
    #[error("{what}: {reason}")]
    Http {
        /// Which request.
        what: &'static str,
        /// What reqwest said.
        reason: String,
    },
    /// The PDS (or the admit) answered with a status the arc cannot use.
    #[error("{what}: HTTP {status}: {body}")]
    Pds {
        /// Which request.
        what: &'static str,
        /// The status.
        status: u16,
        /// The body, for the person reading.
        body: String,
    },
    /// A DID or handle could not be resolved.
    #[error("cannot resolve {who}: {reason}")]
    Resolve {
        /// Who.
        who: String,
        /// Why.
        reason: String,
    },
    /// The callee has no record for the device asked for.
    #[error("{who} publishes no ing.croft.iroh.endpoint record named {device:?}")]
    NoSuchDevice {
        /// The callee.
        who: String,
        /// The rkey asked for.
        device: String,
    },
    /// The relay did not admit this endpoint.
    #[error("NOT camped on {relay}; calls cannot reach this endpoint")]
    NotCamped {
        /// The relay asked for.
        relay: String,
    },
    /// The state directory could not be read or written.
    #[error("state {path}: {reason}")]
    State {
        /// The file or directory.
        path: String,
        /// What the OS said.
        reason: String,
    },
    /// The transport refused.
    #[error(transparent)]
    Transport(#[from] call_transport_iroh::CallTransportError),
}

impl ArcError {
    /// The process exit code: 2 for an honest refusal the words explain (a
    /// dead session, not camped), 1 for everything else.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            ArcError::SessionDead(_) | ArcError::NotCamped { .. } | ArcError::Args(_) => 2,
            _ => 1,
        }
    }
}
