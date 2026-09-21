//! The calling session at the boundary (R4).
//!
//! Thin, like the chat surface: the boundary types, the `#[uniffi::export]`
//! annotations, and the translation onto `call_session::steps`, which holds
//! the machinery in ordinary Rust where it is tested without a binding.
//!
//! Two objects cross. [`CallSession`] is what a shell holds for the life of
//! the window — sign in, camp, wait or dial — and [`ActiveCall`] is the one
//! call it may have, separate so a shell can hang up from one thread while
//! another waits on the ending. Every line a step reports is kept in the
//! session's log and crosses with the view, so a screen shows what happened
//! in the words the product committed to; a shell that composes its own
//! words here is a shell that can flatter the record.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use call_session::report::ending_line;
use call_session::steps::{self, Options, Presence};
use call_transport_iroh::{Call, Direction, Discovery, Ending, PeerAddr};

/// The production relay, as the port names it — so a shell's default is the
/// product's commitment and not a string it typed.
#[uniffi::export]
#[must_use]
pub fn croft_relay_url() -> String {
    call_transport_iroh::relay::CROFT_RELAY_URL.to_string()
}

/// What a session is opened with.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CallOptions {
    /// Where the key, session and pass live; `None` is the default under
    /// the user's state directory.
    pub state_dir: Option<String>,
    /// The relay to camp on.
    pub relay: String,
    /// This device's rkey and label.
    pub label: String,
    /// Whether to use n0's discovery (production) or none (hermetic).
    pub discovery_n0: bool,
    /// How long an attach may take before it is called a refusal.
    pub attach_patience_secs: u64,
}

/// What a screen renders. Words only.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CallView {
    /// Whether there is a session at all (stored or proven).
    pub signed_in: bool,
    /// The session line.
    pub session: String,
    /// Whether the endpoint reported itself attached to a relay just now.
    pub camped: bool,
    /// The presence line, from the endpoint's own answer.
    pub presence: String,
    /// The bound endpoint id, 64 hex, once camp has bound one.
    pub endpoint_id: Option<String>,
    /// The short form the relay journal prints.
    pub endpoint_id_short: Option<String>,
    /// Every line the steps reported, oldest first.
    pub log: Vec<String>,
}

/// A peer whose address is already known.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PeerAddress {
    /// The endpoint id, 64 hex.
    pub endpoint_id: String,
    /// The relay to reach it through, if known.
    pub relay_url: Option<String>,
    /// Direct `ip:port` addresses, if known.
    pub addrs: Vec<String>,
}

impl From<PeerAddress> for PeerAddr {
    fn from(p: PeerAddress) -> Self {
        PeerAddr {
            endpoint_id: p.endpoint_id,
            relay_url: p.relay_url,
            addrs: p.addrs,
        }
    }
}

/// A refusal, as the foreign side sees it. Every variant carries `reason`
/// for the same cause as `FfiError`: uniffi builds the exception's message
/// from the fields, and a typed refusal with no sentence is a blank screen.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CallError {
    /// A step needed a signed-in account and there is none.
    #[error("{reason}")]
    NotSignedIn {
        /// The refusal in words.
        reason: String,
    },
    /// The PDS refused to refresh the session.
    #[error("{reason}")]
    SessionDead {
        /// The refusal in words.
        reason: String,
    },
    /// A step needed a bound endpoint and camp has not bound one.
    #[error("{reason}")]
    NotBound {
        /// The refusal in words.
        reason: String,
    },
    /// The callee has no record for the device asked for.
    #[error("{reason}")]
    NoSuchDevice {
        /// The refusal in words.
        reason: String,
    },
    /// The session was closed and refuses further steps.
    #[error("{reason}")]
    Closed {
        /// The refusal in words.
        reason: String,
    },
    /// The PDS, the admit, or the network, in their own words.
    #[error("{reason}")]
    Network {
        /// What happened.
        reason: String,
    },
    /// The state directory could not be read or written.
    #[error("{reason}")]
    State {
        /// What the OS said.
        reason: String,
    },
    /// The transport refused.
    #[error("{reason}")]
    Transport {
        /// What the transport said.
        reason: String,
    },
}

impl CallError {
    /// The refusal in words — never empty, whichever variant this is.
    #[must_use]
    pub fn reason(&self) -> &str {
        match self {
            CallError::NotSignedIn { reason }
            | CallError::SessionDead { reason }
            | CallError::NotBound { reason }
            | CallError::NoSuchDevice { reason }
            | CallError::Closed { reason }
            | CallError::Network { reason }
            | CallError::State { reason }
            | CallError::Transport { reason } => reason,
        }
    }

    pub(crate) fn closed() -> Self {
        CallError::Closed {
            reason: "this session was closed; open another".to_string(),
        }
    }
}

impl From<call_session::Error> for CallError {
    fn from(e: call_session::Error) -> Self {
        use call_session::Error as E;
        let reason = e.to_string();
        match e {
            E::NotSignedIn => CallError::NotSignedIn { reason },
            E::SessionDead(_) => CallError::SessionDead { reason },
            E::NotBound => CallError::NotBound { reason },
            E::NoSuchDevice { .. } => CallError::NoSuchDevice { reason },
            E::Http { .. } | E::Pds { .. } | E::Resolve { .. } | E::NotCamped { .. } => {
                CallError::Network { reason }
            }
            E::State { .. } => CallError::State { reason },
            E::Transport(_) => CallError::Transport { reason },
        }
    }
}

impl From<call_transport_iroh::CallTransportError> for CallError {
    fn from(e: call_transport_iroh::CallTransportError) -> Self {
        CallError::Transport {
            reason: e.to_string(),
        }
    }
}

/// One calling session: the state directory, the account, the endpoint.
///
/// Behind a `Mutex` for the same reason `ChatSession` is: one session, one
/// step at a time, and a shell's threads are the platform's. A step that
/// waits (`wait_for_call`, `dial`) holds the lock while it waits, so a shell
/// waits in short slices and takes the view between them.
#[derive(Debug, uniffi::Object)]
pub struct CallSession {
    inner: Mutex<Option<steps::CallSession>>,
    log: Mutex<Vec<String>>,
}

#[uniffi::export]
impl CallSession {
    /// Open the state directory and read what it holds. Offline.
    #[uniffi::constructor]
    pub fn open(opts: CallOptions) -> Result<Self, CallError> {
        let session = steps::CallSession::open(Options {
            state_dir: opts.state_dir.map(PathBuf::from),
            relay: opts.relay,
            label: opts.label,
            discovery: if opts.discovery_n0 {
                Discovery::N0
            } else {
                Discovery::None
            },
            attach_patience: Duration::from_secs(opts.attach_patience_secs),
        })?;
        Ok(CallSession {
            inner: Mutex::new(Some(session)),
            log: Mutex::new(Vec::new()),
        })
    }

    /// The screen, now. Re-asks the endpoint whether it is attached.
    pub fn view(&self) -> CallView {
        let inner = self.lock();
        let log = self.log_lines();
        match inner.as_ref() {
            Some(s) => {
                let v = s.view();
                CallView {
                    signed_in: v.signed_in,
                    session: v.session,
                    camped: v.camped,
                    presence: v.presence,
                    endpoint_id: v.endpoint_id,
                    endpoint_id_short: v.endpoint_id_short,
                    log,
                }
            }
            None => CallView {
                signed_in: false,
                session: CallError::closed().reason().to_string(),
                camped: false,
                presence: call_session::report::camped_line(None),
                endpoint_id: None,
                endpoint_id_short: None,
                log,
            },
        }
    }

    /// The bound endpoint's direct `ip:port` addresses, empty until bound.
    pub fn local_addrs(&self) -> Vec<String> {
        self.lock()
            .as_ref()
            .map(steps::CallSession::local_addrs)
            .unwrap_or_default()
    }

    /// Sign in with an app password.
    pub fn sign_in(&self, handle: String, app_password: String) -> Result<(), CallError> {
        let mut inner = self.lock();
        let s = inner.as_mut().ok_or_else(CallError::closed)?;
        let mut out = self.sink();
        s.sign_in(&handle, &app_password, &mut out)?;
        Ok(())
    }

    /// Bind, publish this device's record, camp as `call-core` decides.
    /// `true` when the endpoint reported itself attached; `false` is NOT
    /// camped, with the endpoint left bound so the screen can be retried.
    pub fn camp(&self) -> Result<bool, CallError> {
        let mut inner = self.lock();
        let s = inner.as_mut().ok_or_else(CallError::closed)?;
        let mut out = self.sink();
        Ok(matches!(s.camp(&mut out)?, Presence::Camped { .. }))
    }

    /// Wait up to `patience_secs` to be dialled. `None` means nobody did.
    pub fn wait_for_call(&self, patience_secs: u64) -> Result<Option<Arc<ActiveCall>>, CallError> {
        let mut inner = self.lock();
        let s = inner.as_mut().ok_or_else(CallError::closed)?;
        let mut out = self.sink();
        Ok(
            s.wait_for_call(Duration::from_secs(patience_secs), &mut out)?
                .map(ActiveCall::wrap),
        )
    }

    /// Dial `who`'s device `device`.
    pub fn dial(&self, who: String, device: String) -> Result<Arc<ActiveCall>, CallError> {
        let mut inner = self.lock();
        let s = inner.as_mut().ok_or_else(CallError::closed)?;
        let mut out = self.sink();
        Ok(ActiveCall::wrap(s.dial(&who, &device, &mut out)?))
    }

    /// Dial a peer whose address is already known.
    pub fn dial_endpoint(&self, peer: PeerAddress) -> Result<Arc<ActiveCall>, CallError> {
        let mut inner = self.lock();
        let s = inner.as_mut().ok_or_else(CallError::closed)?;
        let mut out = self.sink();
        Ok(ActiveCall::wrap(s.dial_endpoint(&peer.into(), &mut out)?))
    }

    /// Close the endpoint. Every later step is refused as closed. Named
    /// apart from `close` because uniffi's generated Kotlin object already
    /// has one (its `AutoCloseable`), and two `close`es are a compile error.
    pub fn shut_down(&self) {
        if let Some(s) = self.lock().take() {
            s.close();
        }
    }
}

impl CallSession {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<steps::CallSession>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn log_lines(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// A sink for a step's lines: append to the log. Separate lock from the
    /// session's, so the view can read the log while a step holds the session.
    fn sink(&self) -> impl FnMut(&str) + '_ {
        move |l: &str| {
            self.log
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(l.to_string())
        }
    }
}

/// The one call a session may have.
#[derive(Debug, uniffi::Object)]
pub struct ActiveCall {
    call: Call,
}

impl ActiveCall {
    pub(crate) fn wrap(call: Call) -> Arc<Self> {
        Arc::new(ActiveCall { call })
    }
}

/// How a call ended (E129), as observed on this side — typed, so a shell
/// chooses its own words for a local hang-up and passes the peer's reason
/// through verbatim, never guessing remote-end from an error.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CallEnding {
    /// This side hung up.
    LocalHangUp,
    /// The peer closed with an application code and reason.
    ClosedByPeer {
        /// The application close code.
        code: u64,
        /// The reason bytes, as UTF-8 (lossy).
        reason: String,
    },
    /// The transport lost the connection.
    Lost {
        /// What iroh said.
        reason: String,
    },
}

#[uniffi::export]
impl ActiveCall {
    /// The peer's endpoint id, 64 hex.
    pub fn peer_endpoint_id(&self) -> String {
        self.call.peer_endpoint_id().to_string()
    }

    /// The name the peer's hello carried, if it sent one.
    pub fn peer_hello(&self) -> Option<String> {
        self.call.peer_hello().map(ToString::to_string)
    }

    /// Whether this side placed the call.
    pub fn outgoing(&self) -> bool {
        self.call.direction() == Direction::Outgoing
    }

    /// Hang up (E129): close with code 0 and reason `hangup`.
    pub fn hang_up(&self) {
        self.call.hang_up();
    }

    /// Wait up to `patience_secs` for the call to end, and say how it did,
    /// in the words the product committed to. `None` means it is still up.
    pub fn ended(&self, patience_secs: u64) -> Option<String> {
        self.call
            .ended(Duration::from_secs(patience_secs))
            .as_ref()
            .map(ending_line)
    }

    /// The same wait, typed (D3.3): for a shell that words its own endings.
    pub fn ending(&self, patience_secs: u64) -> Option<CallEnding> {
        self.call
            .ended(Duration::from_secs(patience_secs))
            .map(|e| match e {
                Ending::LocalHangUp => CallEnding::LocalHangUp,
                Ending::ClosedByPeer { code, reason } => CallEnding::ClosedByPeer { code, reason },
                Ending::Lost { reason } => CallEnding::Lost { reason },
            })
    }

    /// Which path the call is using right now, in the screen's words:
    /// `direct <addr>`, `relayed <addr>`, or `path unknown`. A snapshot —
    /// re-ask while the call is up, iroh migrates paths after connect.
    pub fn path(&self) -> String {
        self.call.path_summary()
    }
}
