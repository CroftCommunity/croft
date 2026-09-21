//! The calling endpoint at the boundary (D3.3): the port's lifecycle as the
//! object the phone's `CallPeer.kt` holds instead of upstream iroh-ffi's.
//!
//! Thin over `call_transport_iroh::CallEndpoint`, which owns the runtime, the
//! iroh endpoint, the persisted key and the rules that live at the endpoint:
//! R0 at `rebind` (a dial never lowers admission) and the id stable across a
//! token swap. What is here is locking and translation. A `RwLock`, not a
//! `Mutex`: `accept_next`, `dial` and `attached_relay` block for their
//! patience and may run concurrently (readers); `rebind` and `shutdown`
//! change the endpoint (writers) and wait for the readers' slices to end —
//! which is why a shell waits in short slices.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use call_transport_iroh::relay::RelayTarget;
use call_transport_iroh::{BindOptions, CallEndpoint as Port, Discovery, Rebound};

use crate::call::{ActiveCall, CallError, PeerAddress};

/// Everything a bind needs.
#[derive(Debug, Clone, uniffi::Record)]
pub struct EndpointOptions {
    /// The persisted 32-byte secret key, or none to generate one — read it
    /// back with `secret_key()` and persist it: the key IS the identity the
    /// account publishes.
    pub secret_key: Option<Vec<u8>>,
    /// The relay to camp on.
    pub relay_url: String,
    /// The relay's QUIC port (croft's is nonstandard).
    pub quic_port: u16,
    /// The admission token to present, if a pass is already held.
    pub token: Option<String>,
    /// Whether to use n0's discovery (production) or none (hermetic).
    pub discovery_n0: bool,
}

/// One calling endpoint: bound once, rebound on a token change, shut down
/// on background.
#[derive(Debug, uniffi::Object)]
pub struct CallEndpoint {
    inner: RwLock<Option<Port>>,
    /// Fixed at bind: the port asserts the id survives every swap, so a
    /// shell reads it without a lock and after a shutdown.
    id: String,
    secret: Vec<u8>,
}

#[uniffi::export]
impl CallEndpoint {
    /// Bind, and start attaching to the relay.
    #[uniffi::constructor]
    pub fn bind(opts: EndpointOptions) -> Result<Self, CallError> {
        let secret_key: [u8; 32] = match opts.secret_key {
            Some(bytes) => bytes.as_slice().try_into().map_err(|_| CallError::State {
                reason: format!("an endpoint key is 32 bytes, got {}", bytes.len()),
            })?,
            None => iroh::SecretKey::generate().to_bytes(),
        };
        let port = Port::bind(BindOptions {
            secret_key,
            relay: RelayTarget::new(&opts.relay_url).with_quic_port(opts.quic_port),
            token: opts.token,
            discovery: if opts.discovery_n0 {
                Discovery::N0
            } else {
                Discovery::None
            },
        })?;
        Ok(CallEndpoint {
            id: port.endpoint_id(),
            secret: port.secret_key_bytes().to_vec(),
            inner: RwLock::new(Some(port)),
        })
    }

    /// This endpoint's id, 64 hex.
    pub fn endpoint_id(&self) -> String {
        self.id.clone()
    }

    /// The secret key behind the id, 32 bytes — persist it.
    pub fn secret_key(&self) -> Vec<u8> {
        self.secret.clone()
    }

    /// Bind `token` if the rules allow: `true` when the endpoint stopped and
    /// re-attached with it, `false` when it was kept — already bound, or
    /// binding it would lower admission (R0). The id is the same after.
    pub fn rebind(&self, token: Option<String>) -> Result<bool, CallError> {
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let port = guard.as_mut().ok_or_else(CallError::closed)?;
        Ok(matches!(port.rebind(token.as_deref())?, Rebound::Swapped))
    }

    /// The relay this endpoint is attached to, as the endpoint itself
    /// answers within `patience_secs`; none is NOT camped.
    pub fn attached_relay(&self, patience_secs: u64) -> Option<String> {
        self.read(|p| p.attached_relay(Duration::from_secs(patience_secs)))
            .ok()
            .flatten()
    }

    /// Direct `ip:port` addresses, as iroh currently knows them.
    pub fn local_addrs(&self) -> Vec<String> {
        self.read(|p| p.local_addrs()).unwrap_or_default()
    }

    /// Dial `peer`, sending `hello`; connects within `patience_secs` or is
    /// refused with words.
    pub fn dial(
        &self,
        peer: PeerAddress,
        hello: String,
        patience_secs: u64,
    ) -> Result<Arc<ActiveCall>, CallError> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let port = guard.as_ref().ok_or_else(CallError::closed)?;
        let call = port.dial(&peer.into(), &hello, Duration::from_secs(patience_secs))?;
        Ok(ActiveCall::wrap(call))
    }

    /// Wait up to `patience_secs` to be dialled. `None` means nobody did.
    pub fn accept_next(&self, patience_secs: u64) -> Result<Option<Arc<ActiveCall>>, CallError> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let port = guard.as_ref().ok_or_else(CallError::closed)?;
        Ok(port
            .accept_next(Duration::from_secs(patience_secs))?
            .map(ActiveCall::wrap))
    }

    /// Close the endpoint. Every later step is refused as closed.
    pub fn shutdown(&self) {
        let taken = self.inner.write().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(port) = taken {
            port.shutdown();
        }
    }
}

impl CallEndpoint {
    fn read<T>(&self, f: impl FnOnce(&Port) -> T) -> Result<T, CallError> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let port = guard.as_ref().ok_or_else(CallError::closed)?;
        Ok(f(port))
    }
}
