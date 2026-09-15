//! The croft calling transport: one relay-attaching iroh endpoint that camps
//! with a pass and dials by endpoint id.
//!
//! Child plan R2 (`plans/2026-09-08-plan-call-core-and-apple-shell.md`). This
//! is the Rust of `android/app/.../net/CallPeer.kt`'s lifecycle — bind with a
//! persisted key, attach to OUR relay with the token the mint issued, dial,
//! accept, hang up — with one rule moved from the decision layer INTO the
//! endpoint: **a dial never lowers admission** (`call_core::dial::rebind`, R0,
//! runbook §15.3). The Kotlin applies that rule in `rebindWithToken`; here it
//! is applied in [`CallEndpoint::rebind`], so no shell can reproduce §15 by
//! asking carelessly.
//!
//! # Why a sibling of `transport-iroh` and not a mode inside it
//!
//! `docs/adr/0004-call-transport-port.md`. `ports/transport-iroh` contacts no
//! relay **by construction** — `RelayMode::Disabled`, no discovery service —
//! and P7 S2's severance claim rests on that being structural. This crate's
//! whole purpose is to attach to a relay. Two endpoint kinds in one crate
//! would turn a guarantee into a convention.
//!
//! # Why the surface is synchronous
//!
//! Same reason as `transport-iroh`: the runtime is started, held, and stopped
//! by [`CallEndpoint`], in one language. The Android FFI (D3: assume the
//! switch) then sees ordinary blocking calls with timeouts, never a future
//! whose cancellation two languages disagree about.
//!
//! # Observability (plan R2, Pass 3)
//!
//! §15 was diagnosed from the *relay's* journal because nothing on the client
//! said anything. This port emits at `debug` what the relay cannot see — a
//! bind attempted with or without a token, a rebind decided and why, an attach
//! confirmed or timed out, a connection closed and by whom — with the short
//! endpoint id on every line, so a run can be laid beside the journal by eye.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

/// A live call: one connection, the hello each way, the ending.
pub mod call;
/// The endpoint: bind, camp, rebind, dial, accept.
pub mod endpoint;
/// The relay this endpoint camps on, and how the token rides with it.
pub mod relay;
/// The v0 wire format, byte-compatible with the released app.
pub mod wire;

pub use call::{Call, Direction, Ending};
pub use endpoint::{BindOptions, CallEndpoint, Discovery, PeerAddr, Rebound};
pub use relay::RelayTarget;

/// Everything the transport can refuse, in its own words.
///
/// Every variant carries a sentence. "dial failed: null" reached a real
/// screen on 2026-09-08 (§15.3) because a fieldless variant crossed the FFI
/// with an empty message; the matrix requires words, and these are them.
#[derive(Debug, thiserror::Error)]
pub enum CallTransportError {
    /// The relay URL could not be parsed.
    #[error("not a relay url: {url:?} ({reason})")]
    BadRelayUrl {
        /// What was supplied.
        url: String,
        /// What the parser said.
        reason: String,
    },
    /// The tokio runtime this endpoint owns could not be built.
    #[error("could not start the calling runtime: {reason}")]
    Runtime {
        /// What the runtime builder said.
        reason: String,
    },
    /// The iroh endpoint could not bind.
    #[error("could not bind the calling endpoint: {reason}")]
    Bind {
        /// What iroh said.
        reason: String,
    },
    /// A token swap re-bound the endpoint and its id changed.
    ///
    /// The mint bound the token to the id, so a drift makes every token
    /// worthless. The persisted key is supposed to make this impossible;
    /// asserting it is cheaper than discovering it from a refusal.
    #[error("the endpoint id changed across a token swap: {before} -> {after}")]
    IdDrift {
        /// The id before the swap.
        before: String,
        /// The id after.
        after: String,
    },
    /// A peer address could not be turned into something dialable.
    #[error("cannot dial {endpoint_id:?}: {reason}")]
    BadPeerAddr {
        /// The endpoint id that was supplied.
        endpoint_id: String,
        /// What was wrong with the address.
        reason: String,
    },
    /// The dial did not connect.
    #[error("dial failed: {reason}")]
    Dial {
        /// What iroh said, or that patience ran out.
        reason: String,
    },
    /// An incoming connection could not be accepted.
    #[error("could not accept the incoming call: {reason}")]
    Accept {
        /// What iroh said.
        reason: String,
    },
    /// The hello exchange failed after the connection came up.
    #[error("the hello exchange failed: {reason}")]
    Hello {
        /// What went wrong, in words.
        reason: String,
    },
    /// A hello body was larger than the u16 length header can carry.
    #[error("a hello is at most {max} bytes, got {got}")]
    HelloTooLarge {
        /// The size that was asked for.
        got: usize,
        /// The largest the frame can carry.
        max: usize,
    },
}
