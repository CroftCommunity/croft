//! The calling endpoint — the Rust of the endpoint half of `CallPeer.kt`.
//!
//! Bind with a persisted key so the id is stable (the mint binds a pass to
//! the id); attach to OUR relay with whatever token the mint issued; dial and
//! accept over the v0 ALPN. The token belongs to the endpoint, so changing it
//! is a stop/start — which is exactly why [`CallEndpoint::rebind`] consults
//! `call_core::dial::rebind` before doing anything at all.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use call_core::dial::Rebind;
use iroh::endpoint::{presets, RecvStream};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, SecretKey, TransportAddr, Watcher as _};
use tokio::runtime::Runtime;

use crate::call::{Call, Direction};
use crate::relay::RelayTarget;
use crate::wire::{self, ALPN};
use crate::CallTransportError;

/// How peers are found when a dial names only an endpoint id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Discovery {
    /// n0's DNS and pkarr services — what the released app uses
    /// (`presetN0()` with our relay layered on top), so a phone that
    /// published its address can be dialled by id alone.
    N0,
    /// No discovery service at all. A dial then needs a [`PeerAddr`] with a
    /// relay URL or direct addresses. Hermetic tests and rigs whose keys must
    /// never be published use this.
    None,
}

/// Everything a bind needs.
#[derive(Debug, Clone)]
pub struct BindOptions {
    /// The endpoint's persisted secret key. Same key, same endpoint id.
    pub secret_key: [u8; 32],
    /// The relay to camp on.
    pub relay: RelayTarget,
    /// The admission token to present at attach, if any. `None` is the
    /// tokenless posture an enforcing relay refuses.
    pub token: Option<String>,
    /// How peers are found.
    pub discovery: Discovery,
}

/// Where a peer can be dialled: its id, and whatever is known of its address.
///
/// The app learns the relay from the peer's published endpoint record; a
/// hermetic test hands over direct addresses. With [`Discovery::N0`] an id
/// alone can be enough.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerAddr {
    /// The peer's endpoint id, 32 bytes of lowercase hex.
    pub endpoint_id: String,
    /// The relay the peer is camped on, if known.
    pub relay_url: Option<String>,
    /// Direct `ip:port` addresses, if known.
    pub addrs: Vec<String>,
}

/// What [`CallEndpoint::rebind`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum Rebound {
    /// The endpoint was left alone: the wanted token was already bound, or
    /// binding it would have lowered admission.
    Kept,
    /// The endpoint stopped and re-attached with the wanted token.
    Swapped,
}

/// The endpoint: one iroh `Endpoint` on a runtime this struct owns.
pub struct CallEndpoint {
    runtime: Arc<Runtime>,
    endpoint: Endpoint,
    secret: SecretKey,
    relay: RelayTarget,
    discovery: Discovery,
    token: Option<String>,
    /// How many times an endpoint has been bound for this key by this
    /// struct. A swap increments it; a keep does not. It is the observable
    /// that separates "kept" from "torn down and rebuilt" in a test.
    generation: u32,
}

impl std::fmt::Debug for CallEndpoint {
    /// By hand: the secret key is reachable from `Endpoint`'s own `Debug`,
    /// and the token is a bearer credential. Neither belongs in a log line.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallEndpoint")
            .field("endpoint_id", &self.endpoint.id().fmt_short().to_string())
            .field("relay", &self.relay.url())
            .field("token_bound", &self.token.is_some())
            .field("generation", &self.generation)
            .finish()
    }
}

impl CallEndpoint {
    /// Bind an endpoint for `opts.secret_key` and start attaching to the relay.
    ///
    /// Returns as soon as the local socket is bound; whether the relay
    /// admitted the attach is [`Self::attached_relay`]'s question.
    pub fn bind(opts: BindOptions) -> Result<Self, CallTransportError> {
        let runtime = new_runtime()?;
        let secret = SecretKey::from_bytes(&opts.secret_key);
        let endpoint = runtime.block_on(bind_endpoint(
            &secret,
            &opts.relay,
            opts.token.as_deref(),
            opts.discovery,
        ))?;
        Ok(CallEndpoint {
            runtime: Arc::new(runtime),
            endpoint,
            secret,
            relay: opts.relay,
            discovery: opts.discovery,
            token: opts.token,
            generation: 1,
        })
    }

    /// This endpoint's id, 32 bytes of lowercase hex — what the endpoint
    /// record publishes and what a caller dials.
    #[must_use]
    pub fn endpoint_id(&self) -> String {
        self.endpoint.id().to_string()
    }

    /// The first ten hex characters — what the relay journal prints as
    /// `endpoint_id=…`.
    #[must_use]
    pub fn endpoint_id_short(&self) -> String {
        self.endpoint.id().fmt_short().to_string()
    }

    /// The endpoint id a secret key implies, without binding anything.
    #[must_use]
    pub fn endpoint_id_for(secret_key: &[u8; 32]) -> String {
        SecretKey::from_bytes(secret_key).public().to_string()
    }

    /// The secret key, for the shell to persist.
    #[must_use]
    pub fn secret_key_bytes(&self) -> [u8; 32] {
        self.secret.to_bytes()
    }

    /// The token the current endpoint presented at attach, if any.
    #[must_use]
    pub fn bound_token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// How many endpoints have been bound for this key by this struct.
    #[must_use]
    pub fn endpoint_generation(&self) -> u32 {
        self.generation
    }

    /// Re-bind with `wanted` — or do nothing, when doing something would
    /// lower admission.
    ///
    /// The decision is `call_core::dial::rebind` (R0, §15.3): same token, or
    /// tokenless over a live pass, keeps the endpoint exactly as it is; a
    /// different token stops the endpoint and binds again with the same key,
    /// and the id is asserted stable across the swap.
    pub fn rebind(&mut self, wanted: Option<&str>) -> Result<Rebound, CallTransportError> {
        let short = self.endpoint_id_short();
        // The rule is call-core's; this is the first place it is ENFORCED
        // rather than merely available. Watched to fail without it (Review
        // Log, 2026-09-14): an unconditional swap here reproduced §15 from a
        // laptop — ten `denied reason="no_token"` in ten seconds on the
        // staging journal, the caller no longer camped.
        match call_core::dial::rebind(self.token.as_deref(), wanted) {
            Rebind::Keep => {
                tracing::debug!(
                    endpoint = %short,
                    wanted = wanted.is_some(),
                    bound = self.token.is_some(),
                    "rebind: keep — already bound, or the want would lower admission"
                );
                Ok(Rebound::Kept)
            }
            Rebind::Swap { token } => {
                tracing::debug!(endpoint = %short, "rebind: swap — stopping and re-attaching");
                let before = self.endpoint_id();
                self.token = Some(token);
                self.runtime.block_on(self.endpoint.close());
                // A NEW runtime for the new endpoint. Measured against
                // production (R3's arc, 2026-09-15): when the old endpoint was
                // closed mid-attach, iroh logged `relay_recv_channel closed`
                // and the new endpoint, bound on the SAME runtime, never
                // reached the relay in 20 s — a task the close left behind
                // outlived its endpoint. Dropping the old runtime cancels
                // everything that ran on it; a `Call` still holding it keeps
                // it alive until that call is gone.
                let runtime = new_runtime()?;
                self.endpoint = runtime.block_on(bind_endpoint(
                    &self.secret,
                    &self.relay,
                    self.token.as_deref(),
                    self.discovery,
                ))?;
                self.runtime = Arc::new(runtime);
                self.generation += 1;
                let after = self.endpoint_id();
                if before != after {
                    return Err(CallTransportError::IdDrift { before, after });
                }
                Ok(Rebound::Swapped)
            }
        }
    }

    /// The relay this endpoint is ACTUALLY attached to, or `None` if it is
    /// attached to none within `patience`.
    ///
    /// E135(a): the configured relay is not the attached relay. `online()` is
    /// the endpoint's own reachability answer — prompt when attached, blocked
    /// while an enforcing relay refuses — so the URL only becomes an
    /// attachment when it agrees, and patience is what turns "still blocked"
    /// into an answer.
    #[must_use]
    pub fn attached_relay(&self, patience: Duration) -> Option<String> {
        let short = self.endpoint_id_short();
        self.runtime.block_on(async {
            if tokio::time::timeout(patience, self.endpoint.online())
                .await
                .is_err()
            {
                tracing::debug!(endpoint = %short, ?patience, "NOT attached to any relay within patience");
                return None;
            }
            let attached = self
                .endpoint
                .home_relay_status()
                .get()
                .into_iter()
                .find(|status| status.is_connected())
                .map(|status| status.url().to_string());
            tracing::debug!(endpoint = %short, relay = ?attached, "attached");
            attached
        })
    }

    /// This endpoint's direct `ip:port` addresses, as iroh currently knows
    /// them. Empty until iroh has looked at the local interfaces.
    #[must_use]
    pub fn local_addrs(&self) -> Vec<String> {
        self.endpoint
            .addr()
            .ip_addrs()
            .map(ToString::to_string)
            .collect()
    }

    /// Dial `peer`, send our hello, read theirs.
    ///
    /// The whole exchange must complete within `patience`; a peer nobody is
    /// running is refused with iroh's words or with "no answer", never
    /// silently.
    pub fn dial(
        &self,
        peer: &PeerAddr,
        hello_from: &str,
        patience: Duration,
    ) -> Result<Call, CallTransportError> {
        let addr = peer.to_endpoint_addr()?;
        let hello = wire::encode_hello(hello_from)?;
        let short = self.endpoint_id_short();
        tracing::debug!(endpoint = %short, peer = %addr.id.fmt_short(), "dialling");
        self.runtime.block_on(async {
            let conn = tokio::time::timeout(patience, self.endpoint.connect(addr, ALPN))
                .await
                .map_err(|_| CallTransportError::Dial {
                    reason: format!("no answer within {patience:?}"),
                })?
                .map_err(|e| CallTransportError::Dial {
                    reason: e.to_string(),
                })?;
            let (mut send, mut recv) =
                conn.open_bi()
                    .await
                    .map_err(|e| CallTransportError::Hello {
                        reason: format!("could not open the hello stream: {e}"),
                    })?;
            send.write_all(&hello)
                .await
                .map_err(|e| CallTransportError::Hello {
                    reason: format!("could not send our hello: {e}"),
                })?;
            let _ = send.finish();
            let peer_hello = read_hello(&mut recv, patience).await;
            Ok(Call::new(
                Arc::clone(&self.runtime),
                conn,
                Direction::Outgoing,
                peer_hello,
            ))
        })
    }

    /// Wait up to `patience` for an incoming call, read the caller's hello,
    /// answer with ours.
    ///
    /// `Ok(None)` is patience running out, or the endpoint being closed —
    /// quiet is not an error.
    pub fn accept_next(&self, patience: Duration) -> Result<Option<Call>, CallTransportError> {
        let short = self.endpoint_id_short();
        self.runtime.block_on(async {
            let Ok(incoming) = tokio::time::timeout(patience, self.endpoint.accept()).await else {
                return Ok(None);
            };
            let Some(incoming) = incoming else {
                tracing::debug!(endpoint = %short, "accept: the endpoint is closed");
                return Ok(None);
            };
            let conn = incoming
                .accept()
                .map_err(|e| CallTransportError::Accept {
                    reason: e.to_string(),
                })?
                .await
                .map_err(|e| CallTransportError::Accept {
                    reason: e.to_string(),
                })?;
            let (mut send, mut recv) = tokio::time::timeout(patience, conn.accept_bi())
                .await
                .map_err(|_| CallTransportError::Hello {
                    reason: format!("the caller opened no hello stream within {patience:?}"),
                })?
                .map_err(|e| CallTransportError::Hello {
                    reason: format!("could not accept the hello stream: {e}"),
                })?;
            let peer_hello = read_hello(&mut recv, patience).await;
            send.write_all(&wire::encode_hello("callee")?)
                .await
                .map_err(|e| CallTransportError::Hello {
                    reason: format!("could not send our hello: {e}"),
                })?;
            let _ = send.finish();
            Ok(Some(Call::new(
                Arc::clone(&self.runtime),
                conn,
                Direction::Incoming,
                peer_hello,
            )))
        })
    }

    /// Close the endpoint. Consuming: a closed endpoint has no remaining use.
    pub fn shutdown(self) {
        tracing::debug!(endpoint = %self.endpoint_id_short(), "shutting down");
        self.runtime.block_on(self.endpoint.close());
    }
}

/// The runtime one bound endpoint lives on.
///
/// Two workers, as transport-iroh: I/O-bound work — a QUIC socket, the relay
/// actor — and on a phone the per-CPU default is threads and battery spent
/// idle.
fn new_runtime() -> Result<Runtime, CallTransportError> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| CallTransportError::Runtime {
            reason: e.to_string(),
        })
}

async fn bind_endpoint(
    secret: &SecretKey,
    relay: &RelayTarget,
    token: Option<&str>,
    discovery: Discovery,
) -> Result<Endpoint, CallTransportError> {
    let short = secret.public().fmt_short().to_string();
    tracing::debug!(
        endpoint = %short,
        relay = relay.url(),
        token = token.is_some(),
        ?discovery,
        "binding"
    );
    let map = relay.relay_map(token)?;
    // The preset is the baseline and the relay mode is layered on top, the
    // same order iroh-ffi's `Endpoint::bind` applies for the app. `Minimal`
    // installs the crypto provider and nothing else.
    let builder = match discovery {
        Discovery::N0 => Endpoint::builder(presets::N0),
        Discovery::None => Endpoint::builder(presets::Minimal),
    };
    builder
        .secret_key(secret.clone())
        .alpns(vec![ALPN.to_vec()])
        .relay_mode(RelayMode::Custom(map))
        .bind()
        .await
        .map_err(|e| CallTransportError::Bind {
            reason: e.to_string(),
        })
}

/// Read one v0 hello frame off `recv`, or `None` if the peer sent something
/// else or nothing within `patience`.
async fn read_hello(recv: &mut RecvStream, patience: Duration) -> Option<String> {
    tokio::time::timeout(patience, async {
        let mut header = [0u8; 2];
        recv.read_exact(&mut header).await.ok()?;
        let mut body = vec![0u8; wire::frame_length(header)];
        recv.read_exact(&mut body).await.ok()?;
        wire::decode_hello(&body)
    })
    .await
    .ok()
    .flatten()
}

impl PeerAddr {
    fn to_endpoint_addr(&self) -> Result<EndpointAddr, CallTransportError> {
        let bad = |reason: String| CallTransportError::BadPeerAddr {
            endpoint_id: self.endpoint_id.clone(),
            reason,
        };
        let raw = data_encoding::HEXLOWER
            .decode(self.endpoint_id.as_bytes())
            .map_err(|_| bad("an endpoint id is lowercase hex".to_string()))?;
        let bytes: [u8; 32] = raw
            .as_slice()
            .try_into()
            .map_err(|_| bad(format!("an endpoint id is 32 bytes, got {}", raw.len())))?;
        let id = EndpointId::from_bytes(&bytes)
            .map_err(|e| bad(format!("not a valid endpoint id: {e}")))?;

        let mut addrs = Vec::new();
        if let Some(url) = &self.relay_url {
            let url = url.parse().map_err(|e: iroh::RelayUrlParseError| {
                bad(format!("not a relay url: {url:?} ({e})"))
            })?;
            addrs.push(TransportAddr::Relay(url));
        }
        for a in &self.addrs {
            let sock: SocketAddr = a
                .parse()
                .map_err(|_| bad(format!("not an ip:port address: {a:?}")))?;
            addrs.push(TransportAddr::Ip(sock));
        }
        Ok(EndpointAddr::from_parts(id, addrs))
    }
}
