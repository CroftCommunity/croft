//! call-session: the calling session a shell holds.
//!
//! Child plan R3 built this as the headless arc's library; R4 extracted it
//! when the macOS shell needed the same steps behind buttons. One
//! [`CallSession`] walks the arc a step at a time — sign in, camp, wait or
//! dial, hang up — and reports what it observed in the words the product
//! committed to, so a shell renders the words and never composes them.
//!
//! It is glue, not a client. Every admission decision is `call-core`'s
//! (`camp::plan`/`camp::action`, `dial::plan`); every endpoint act is
//! `call-transport-iroh`'s. The pure pieces — the session policy, record
//! reconciliation, the admit wire mapping, the report lines — are separate
//! modules with hermetic tests; the network glue is exercised by the `:live`
//! arc (`bin/croft-arc/tests/live_arc.rs`) and the shell's live run.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

/// The camp mint's wire, onto call-core's outcome.
pub mod admit;
/// The PDS and admit HTTP client.
pub mod atproto;
/// This device's endpoint record.
pub mod records;
/// The words reported.
pub mod report;
/// The session policy.
pub mod session;
/// The state directory: key, session, pass.
pub mod state;
/// The session a shell holds, a step at a time.
pub mod steps;

/// Everything the session can refuse or fail on, in words.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A step needed a signed-in account and there is none.
    #[error("not signed in")]
    NotSignedIn,
    /// A step needed a bound endpoint and camp has not bound one.
    #[error("no endpoint is bound yet — camp first")]
    NotBound,
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
    /// The PDS (or the admit) answered with a status the session cannot use.
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
