//! The croft device-to-device transport: iroh-gossip.
//!
//! P7 S2, Q2. Two devices, one topic, two artifact kinds — the MLS Welcome
//! once at invite, and sealed application messages ongoing. **No relay
//! contact**: that is not a runtime posture here but a construction, so that
//! fabric admission (M4's live subject) and group admission stay severed by
//! something stronger than discipline.
//!
//! What this crate is NOT for, and the runbook says the same: NAT traversal,
//! offline delivery, or anything relay-mediated. Two phones on one WiFi is the
//! whole topology S2 claims.
//!
//! The framing lives in [`frame`] and has no iroh in it, so the part most
//! worth testing exhaustively is testable without a network.

#![warn(missing_docs)]

/// The wire frame and its two artifact kinds.
pub mod frame;
/// The pairing blob a person carries between two phones.
pub mod pairing;
/// The transport: one endpoint, one topic, a sync surface.
pub mod transport;

pub use transport::{DialCard, GossipTransport, TopicKey};

/// Everything the transport can refuse, in its own words.
///
/// Framing refusals name the number that was wrong — the version, the kind
/// byte, the length. During a device run logcat is the only instrument, so a
/// refusal that does not carry its number costs a rebuild to diagnose.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// A frame carried a wire version this build does not know.
    #[error("this build speaks frame version {expected}, got {got}")]
    WireVersion {
        /// The version that arrived.
        got: u8,
        /// The version this build speaks.
        expected: u8,
    },
    /// A frame named an artifact kind this build does not know.
    #[error("unknown artifact kind byte {got}")]
    UnknownKind {
        /// The kind byte that arrived.
        got: u8,
    },
    /// A frame was too short to carry even its header.
    #[error("a frame is at least 2 bytes, got {got}")]
    ShortFrame {
        /// How many bytes actually arrived.
        got: usize,
    },
    /// A frame carried a header and no artifact.
    #[error("an empty {kind} carries nothing, so it is refused")]
    EmptyPayload {
        /// Which kind was empty.
        kind: &'static str,
    },
    /// A signing key was not 32 bytes.
    #[error("a transport key is 32 bytes, got {got}")]
    BadKeyLength {
        /// The length actually supplied.
        got: usize,
    },
    /// The tokio runtime this transport owns could not be built.
    #[error("could not start the transport runtime: {reason}")]
    Runtime {
        /// What the runtime builder said.
        reason: String,
    },
    /// The iroh endpoint could not bind.
    #[error("could not bind an endpoint: {reason}")]
    Bind {
        /// What iroh said.
        reason: String,
    },
    /// The gossip topic could not be joined.
    #[error("could not join the topic: {reason}")]
    Subscribe {
        /// What gossip said.
        reason: String,
    },
    /// A broadcast could not be sent.
    #[error("could not broadcast: {reason}")]
    Broadcast {
        /// What gossip said.
        reason: String,
    },
    /// A pairing blob carried a version this build does not know.
    #[error(
        "this pairing code was made by a newer build (version {got}, this one speaks {expected})"
    )]
    BlobVersion {
        /// The version that arrived.
        got: u8,
        /// The version this build speaks.
        expected: u8,
    },
    /// A pairing code could not be read.
    ///
    /// Its words are addressed to a person standing between two phones, not to
    /// a developer reading a stack trace — a mistyped code is the expected
    /// case here, not an exceptional one.
    #[error("{reason}")]
    BadPairingCode {
        /// What was wrong, in words for the person pairing.
        reason: String,
    },
    /// A dial card could not be turned into something dialable.
    ///
    /// This is the refusal a mistyped or truncated pairing blob produces, so
    /// its words are read by a person mid-pairing rather than by a developer.
    #[error("this pairing code is not usable: {reason}")]
    BadDialCard {
        /// What was wrong with it.
        reason: String,
    },
}
