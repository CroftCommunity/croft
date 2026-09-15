//! A live call — the Rust of the connection half of `CallPeer.kt`.
//!
//! One connection, the v0 hello each way, and the three E129 endings as typed
//! data. The WORDS for an ending ("you ended the call", "connection lost",
//! "call ended: …") are a screen's — `CallEnding.kt` today, R4's honesty
//! surface tomorrow — so this type reports what happened and who did it, and
//! never guesses remote-end from an error string.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use iroh::endpoint::{Connection, ConnectionError, VarInt};
use tokio::runtime::Runtime;

/// Which side placed the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// This endpoint dialled.
    Outgoing,
    /// This endpoint accepted.
    Incoming,
}

/// How a call ended, as observed on this side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending {
    /// This side hung up ([`Call::hang_up`]).
    LocalHangUp,
    /// The peer closed the connection with an application code and reason,
    /// carried verbatim.
    ClosedByPeer {
        /// The peer's application close code.
        code: u64,
        /// The peer's reason bytes, as text.
        reason: String,
    },
    /// The transport ended it — timeout, reset, a closed endpoint — with
    /// iroh's words for why.
    Lost {
        /// What iroh said.
        reason: String,
    },
}

/// The live call's connection, held so hang-up can close it and the ending
/// can be observed honestly. One call at a time per endpoint is the shell's
/// rule, not this type's.
#[derive(Debug)]
pub struct Call {
    runtime: Arc<Runtime>,
    conn: Connection,
    direction: Direction,
    peer: String,
    peer_hello: Option<String>,
    /// Set by [`Call::hang_up`] before closing, so a local ending never
    /// masquerades as the transport's.
    local_hang_up: AtomicBool,
}

impl Call {
    pub(crate) fn new(
        runtime: Arc<Runtime>,
        conn: Connection,
        direction: Direction,
        peer_hello: Option<String>,
    ) -> Self {
        let peer = conn.remote_id().to_string();
        tracing::debug!(
            peer = %conn.remote_id().fmt_short(),
            ?direction,
            hello = ?peer_hello,
            "connected"
        );
        Call {
            runtime,
            conn,
            direction,
            peer,
            peer_hello,
            local_hang_up: AtomicBool::new(false),
        }
    }

    /// The peer's endpoint id, 32 bytes of lowercase hex.
    #[must_use]
    pub fn peer_endpoint_id(&self) -> &str {
        &self.peer
    }

    /// Which side placed the call.
    #[must_use]
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// The name the peer's hello carried, if it sent a v0 hello.
    #[must_use]
    pub fn peer_hello(&self) -> Option<&str> {
        self.peer_hello.as_deref()
    }

    /// Hang up (E129): close with application code 0 and reason `hangup`,
    /// exactly as the app does, so a phone on the other end renders the same
    /// "closed by peer: hangup (code 0)" it renders today.
    pub fn hang_up(&self) {
        self.local_hang_up.store(true, Ordering::SeqCst);
        tracing::debug!(peer = %self.conn.remote_id().fmt_short(), "hanging up");
        self.conn.close(VarInt::from_u32(0), b"hangup");
    }

    /// Wait up to `patience` for the call to end, and say how it did.
    ///
    /// `None` means it is still up. A call that ended is reported the same
    /// way however many times this is asked.
    #[must_use]
    pub fn ended(&self, patience: Duration) -> Option<Ending> {
        // The timeout is built INSIDE the runtime: `tokio::time::timeout`
        // registers with the reactor when constructed, and constructing it on
        // the calling thread panics with "there is no reactor running".
        // Watched to fail exactly that way in `tests/loopback_call.rs`.
        let err = self
            .runtime
            .block_on(async { tokio::time::timeout(patience, self.conn.closed()).await })
            .ok()?;
        let ending = match err {
            ConnectionError::LocallyClosed if self.local_hang_up.load(Ordering::SeqCst) => {
                Ending::LocalHangUp
            }
            ConnectionError::ApplicationClosed(close) => Ending::ClosedByPeer {
                code: close.error_code.into_inner(),
                reason: String::from_utf8_lossy(&close.reason).into_owned(),
            },
            other => Ending::Lost {
                reason: other.to_string(),
            },
        };
        tracing::debug!(peer = %self.conn.remote_id().fmt_short(), ?ending, "call ended");
        Some(ending)
    }
}
