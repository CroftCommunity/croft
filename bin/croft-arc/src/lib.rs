//! croft-arc: the headless calling arc, as a library the binary drives.
//!
//! Child plan R3 (`plans/2026-09-08-plan-call-core-and-apple-shell.md`). One
//! command walks the arc against production: load or create a stored
//! session, bind the endpoint, publish this device's record, camp with a
//! self-minted pass, then either wait to be dialled or dial a named peer,
//! hold, hang up — printing each step with the endpoint's short id so a run
//! lays beside the relay journal by eye.
//!
//! The steps are `call-session`'s (R4 extracted them so the macOS shell
//! drives the same code); what is here is the command line, the environment
//! credentials, and the exit codes.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

/// The arc itself.
pub mod arc;
/// The command line.
pub mod args;

/// Everything the arc can refuse or fail on, in words.
#[derive(Debug, thiserror::Error)]
pub enum ArcError {
    /// The command line could not be understood.
    #[error("{0}")]
    Args(String),
    /// No stored session and no credentials to make one.
    #[error("no stored session and {0} not set — the first run needs CROFT_ARC_HANDLE and CROFT_ARC_APP_PASSWORD")]
    NoCredentials(&'static str),
    /// The session refused or failed, in its own words.
    #[error(transparent)]
    Session(#[from] call_session::Error),
}

impl ArcError {
    /// The process exit code: 2 for an honest refusal the words explain (a
    /// dead session, not camped, bad arguments), 1 for everything else.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            ArcError::Args(_) | ArcError::NoCredentials(_) => 2,
            ArcError::Session(call_session::Error::SessionDead(_))
            | ArcError::Session(call_session::Error::NotCamped { .. }) => 2,
            ArcError::Session(_) => 1,
        }
    }
}
