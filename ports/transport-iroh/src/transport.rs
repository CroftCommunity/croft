//! The transport itself: one iroh endpoint, one gossip topic, a sync surface.
//!
//! # Why the surface is synchronous
//!
//! uniffi can generate async Kotlin, and this could have been an async API. It
//! is not, deliberately. An async surface puts the runtime's lifetime in two
//! languages at once — Kotlin decides when to cancel, Rust decides when to
//! drop — and the failure that produces is a hung task with no owner. Here the
//! runtime is started, held, and stopped by this struct, in one language, and
//! the shell sees ordinary blocking calls it can put on whatever thread it
//! likes. `next_artifact` takes a timeout for exactly this reason: a shell
//! loop that can time out is a shell loop that can be told to stop.
//!
//! # Why the relay severance is structural
//!
//! `presets::Minimal` sets a crypto provider and NOTHING else — no address
//! lookup, no relay. `N0DisableRelay` was the tempting choice and is wrong for
//! S2: it disables the relay but still publishes this device's addresses to
//! n0's DNS/pkarr servers, which is external contact during a run whose whole
//! premise is that there is none. Peers are learned out-of-band through
//! [`MemoryLookup`], from a dial card a person carried across — which is the
//! pairing step, and the reason no discovery service is configured at all.
//!
//! Measured, not assumed (`examples/probe_loopback.rs`): with this
//! configuration `endpoint.addr()` carries direct IP addresses immediately and
//! zero relay-shaped ones.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::presets;
use iroh::protocol::Router;
use iroh::{Endpoint, EndpointAddr, RelayMode, SecretKey, TransportAddr};
use iroh_gossip::api::{Event, GossipSender};
use iroh_gossip::net::{Gossip, GOSSIP_ALPN};
use iroh_gossip::proto::TopicId;
use n0_future::StreamExt;
use tokio::runtime::Runtime;

use crate::frame::{decode, encode, ArtifactKind, Frame};
use crate::TransportError;

/// Which conversation a transport is carrying.
///
/// Derived from the group id rather than exchanged, so two devices that agree
/// on the group agree on the topic without another round trip. Hashed rather
/// than used raw: the topic id is visible to anyone within earshot of the
/// swarm, and a raw group id would make the group's own identifier the thing
/// on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopicKey([u8; 32]);

impl TopicKey {
    /// The topic a group's conversation rides on.
    #[must_use]
    pub fn from_group_id(group_id: &[u8; 32]) -> Self {
        let mut hasher = blake3::Hasher::new();
        // Domain-separated: this hash must never collide with any other use of
        // blake3 over a group id elsewhere in the estate.
        hasher.update(b"croft/s2/gossip-topic/v1");
        hasher.update(group_id);
        TopicKey(*hasher.finalize().as_bytes())
    }

    fn as_topic_id(self) -> TopicId {
        TopicId::from_bytes(self.0)
    }
}

/// Everything one device needs to dial another: who, and where.
///
/// This is the transport half of the pairing blob. It is deliberately plain
/// data — an id and a list of address strings — so that carrying it in a QR
/// code, a paste buffer, or a test fixture are all the same thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialCard {
    /// The peer's endpoint id, hex-encoded.
    pub endpoint_id: String,
    /// Direct addresses, as `ip:port`. Never relay URLs — see the module docs.
    pub addrs: Vec<String>,
}

/// One iroh endpoint joined to one gossip topic, with a sync surface.
pub struct GossipTransport {
    runtime: Runtime,
    endpoint: Endpoint,
    router: Router,
    gossip: Gossip,
    lookup: MemoryLookup,
    sender: GossipSender,
    inbox: Receiver<Frame>,
    /// How many direct neighbours the swarm currently has, maintained by the
    /// pump. This is the shell's "is anyone actually there" signal, and the
    /// thing `wait_for_peer` polls.
    neighbours: Arc<AtomicUsize>,
}

impl std::fmt::Debug for GossipTransport {
    /// By hand, and without the endpoint: `Endpoint`'s own `Debug` is large and
    /// the secret key is reachable from it. A transport that prints its key
    /// material into a test failure is a transport that leaks it into logcat.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GossipTransport")
            .field("endpoint_id", &self.endpoint.id().fmt_short().to_string())
            .finish_non_exhaustive()
    }
}

impl GossipTransport {
    /// Bind an endpoint for `secret_key`, join `topic`, and bootstrap from
    /// `peers`.
    ///
    /// `peers` is what a scanned pairing blob supplies. An empty list is the
    /// founding device: it waits to be dialled rather than dialling.
    pub fn start(
        secret_key: &[u8],
        topic: TopicKey,
        peers: &[DialCard],
    ) -> Result<Self, TransportError> {
        let key: [u8; 32] = secret_key
            .try_into()
            .map_err(|_| TransportError::BadKeyLength {
                got: secret_key.len(),
            })?;

        // Multi-thread rather than current-thread: the gossip actor, the
        // endpoint's socket loop and our receive pump all want to make progress
        // while the shell thread is blocked inside `next_artifact`. On a
        // current-thread runtime that call would starve the actor it is waiting
        // on.
        //
        // **Two workers, explicitly.** `new_multi_thread()` defaults to one
        // worker per CPU, which is wrong here in both directions. This work is
        // I/O-bound — a QUIC socket, a gossip actor, a decode pump — so the
        // extra workers buy nothing, and on a phone they are threads and
        // battery spent to sit idle. Worse, the default made the loopback suite
        // fail outright: six tests in parallel, each holding two or three
        // transports, each sizing itself to the whole machine, and the gossip
        // actors starved badly enough that no swarm formed inside twenty
        // seconds. Every test passed alone and the suite failed together, which
        // reads as flakiness and is not — it is oversubscription. Pinning the
        // width fixes it at the cause; `--test-threads=1` would only have
        // hidden it, and would have hidden it on the machine least like a phone.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| TransportError::Runtime {
                reason: e.to_string(),
            })?;

        let (endpoint, router, gossip, lookup, sender, inbox, neighbours) =
            runtime.block_on(async { Self::bring_up(key, topic, peers).await })?;

        Ok(GossipTransport {
            runtime,
            endpoint,
            router,
            gossip,
            lookup,
            sender,
            inbox,
            neighbours,
        })
    }

    #[allow(clippy::type_complexity)]
    async fn bring_up(
        key: [u8; 32],
        topic: TopicKey,
        peers: &[DialCard],
    ) -> Result<
        (
            Endpoint,
            Router,
            Gossip,
            MemoryLookup,
            GossipSender,
            Receiver<Frame>,
            Arc<AtomicUsize>,
        ),
        TransportError,
    > {
        let lookup = MemoryLookup::new();

        let endpoint = Endpoint::builder(presets::Minimal)
            .secret_key(SecretKey::from_bytes(&key))
            .relay_mode(RelayMode::Disabled)
            .address_lookup(lookup.clone())
            .bind()
            .await
            .map_err(|e| TransportError::Bind {
                reason: e.to_string(),
            })?;

        let gossip = Gossip::builder().spawn(endpoint.clone());
        let router = Router::builder(endpoint.clone())
            .accept(GOSSIP_ALPN, gossip.clone())
            .spawn();

        // Teach this endpoint about the peers a person carried across. This is
        // the ONLY way a peer becomes reachable here — there is no discovery
        // service configured, by design.
        let mut bootstrap = Vec::new();
        for card in peers {
            let addr = card.to_endpoint_addr()?;
            bootstrap.push(addr.id);
            lookup.add_endpoint_info(addr);
        }

        let topic_handle = gossip
            .subscribe(topic.as_topic_id(), bootstrap)
            .await
            .map_err(|e| TransportError::Subscribe {
                reason: e.to_string(),
            })?;

        let (sender, mut receiver) = topic_handle.split();

        // The pump: gossip events in, decoded frames out to a plain std
        // channel the sync surface can drain. Undecodable frames are dropped
        // with a WARN rather than killing the pump — one peer on a newer build
        // must not be able to silence the conversation for everyone else, and
        // the refusal already names the version or the kind byte.
        let (tx, inbox): (Sender<Frame>, Receiver<Frame>) = mpsc::channel();
        let neighbours = Arc::new(AtomicUsize::new(0));
        let pump_neighbours = Arc::clone(&neighbours);
        tokio::spawn(async move {
            while let Some(event) = receiver.next().await {
                match event {
                    Ok(Event::Received(msg)) => match decode(&msg.content) {
                        Ok(frame) => {
                            // A closed receiver means the shell dropped the
                            // transport; stop rather than spin.
                            if tx.send(frame).is_err() {
                                break;
                            }
                        }
                        Err(e) => tracing::warn!(
                            error = %e,
                            from = %msg.delivered_from.fmt_short(),
                            "refused an undecodable frame"
                        ),
                    },
                    Ok(Event::Lagged) => {
                        // Backpressure, not loss of correctness — but it IS
                        // loss of messages, so it must never be silent.
                        tracing::warn!("gossip receiver lagged; messages were dropped");
                    }
                    Ok(Event::NeighborUp(id)) => {
                        pump_neighbours.fetch_add(1, Ordering::SeqCst);
                        tracing::debug!(peer = %id.fmt_short(), "neighbor up");
                    }
                    Ok(Event::NeighborDown(id)) => {
                        // Saturating: a NeighborDown without a matching Up
                        // would otherwise wrap to usize::MAX and make the
                        // transport claim a swarm it does not have.
                        let _ = pump_neighbours.fetch_update(
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                            |n| Some(n.saturating_sub(1)),
                        );
                        tracing::debug!(peer = %id.fmt_short(), "neighbor down");
                    }
                    Err(e) => {
                        tracing::warn!(error = ?e, "gossip receive stream ended");
                        break;
                    }
                }
            }
        });

        Ok((endpoint, router, gossip, lookup, sender, inbox, neighbours))
    }

    /// What to put in a pairing blob so another device can dial this one.
    #[must_use]
    pub fn dial_card(&self) -> DialCard {
        let addr = self.endpoint.addr();
        DialCard {
            endpoint_id: hex(addr.id.as_bytes()),
            addrs: addr
                .addrs
                .iter()
                .filter_map(|a| match a {
                    TransportAddr::Ip(sock) => Some(sock.to_string()),
                    // Unreachable with `RelayMode::Disabled`, and filtered
                    // rather than rendered so that a future configuration
                    // change cannot quietly put a relay URL in a dial card.
                    _ => None,
                })
                .collect(),
        }
    }

    /// Teach this transport about a peer learned after start.
    pub fn add_peer(&self, card: &DialCard) -> Result<(), TransportError> {
        let addr = card.to_endpoint_addr()?;
        self.lookup.add_endpoint_info(addr);
        Ok(())
    }

    /// How many direct neighbours this device currently has on the topic.
    #[must_use]
    pub fn neighbour_count(&self) -> usize {
        self.neighbours.load(Ordering::SeqCst)
    }

    /// Block until at least one peer has joined the swarm, or `timeout` passes.
    ///
    /// Returns whether anyone arrived. This is not a test affordance: gossip
    /// broadcasts to the neighbours it has *at the time of the call*, so a send
    /// into an empty swarm is delivered to nobody and reports success. A shell
    /// that sends without knowing whether anyone is there will show a message
    /// as sent that no one can ever receive — which on two phones is
    /// indistinguishable from the sealed-chat bug this phase exists to catch.
    /// The founding device is the normal case for waiting here: it subscribes
    /// with no bootstrap and simply waits to be dialled.
    #[must_use]
    pub fn wait_for_peer(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        // Polling rather than a notifier: the pump owns the only writer, the
        // wait is a human-scale event, and a 25ms poll costs nothing next to
        // the QUIC handshake it is waiting on.
        loop {
            if self.neighbour_count() > 0 {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Broadcast one artifact to everyone on the topic.
    pub fn broadcast(&self, kind: ArtifactKind, payload: &[u8]) -> Result<(), TransportError> {
        let wire = encode(kind, payload)?;
        self.runtime.block_on(async {
            self.sender
                .broadcast(wire.into())
                .await
                .map_err(|e| TransportError::Broadcast {
                    reason: e.to_string(),
                })
        })
    }

    /// The next artifact, or `None` if none arrived within `timeout`.
    ///
    /// `None` is a timeout, not a failure: on a device run the far phone is
    /// often simply not sending yet, and a shell that treats quiet as an error
    /// would show a refusal where there is nothing wrong.
    #[must_use]
    pub fn next_artifact(&self, timeout: Duration) -> Option<Frame> {
        match self.inbox.recv_timeout(timeout) {
            Ok(frame) => Some(frame),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => {
                tracing::warn!("the gossip pump stopped; no further artifacts will arrive");
                None
            }
        }
    }

    /// This device's endpoint id, short form, for a log line or a members pane.
    #[must_use]
    pub fn endpoint_id_short(&self) -> String {
        self.endpoint.id().fmt_short().to_string()
    }

    /// Leave the topic and close the endpoint.
    ///
    /// Consuming, because a transport that has been shut down has no valid
    /// remaining use and the type system may as well say so. The probe showed
    /// iroh logging `Aborting ungracefully` when an endpoint is merely dropped;
    /// this is the path that does not do that.
    pub fn shutdown(self) {
        self.runtime.block_on(async {
            if let Err(e) = self.gossip.shutdown().await {
                tracing::warn!(error = %e, "gossip did not shut down cleanly");
            }
            self.router.shutdown().await.ok();
            self.endpoint.close().await;
        });
    }
}

impl DialCard {
    fn to_endpoint_addr(&self) -> Result<EndpointAddr, TransportError> {
        let raw = unhex(&self.endpoint_id).ok_or_else(|| TransportError::BadDialCard {
            reason: format!("endpoint id is not hex: {:?}", self.endpoint_id),
        })?;
        let bytes: [u8; 32] = raw
            .as_slice()
            .try_into()
            .map_err(|_| TransportError::BadDialCard {
                reason: format!("an endpoint id is 32 bytes, got {}", raw.len()),
            })?;
        let id = iroh::EndpointId::from_bytes(&bytes).map_err(|e| TransportError::BadDialCard {
            reason: format!("not a valid endpoint id: {e}"),
        })?;

        let mut addrs = std::collections::BTreeSet::new();
        for a in &self.addrs {
            let sock: std::net::SocketAddr =
                a.parse().map_err(|_| TransportError::BadDialCard {
                    reason: format!("not an ip:port address: {a:?}"),
                })?;
            addrs.insert(TransportAddr::Ip(sock));
        }

        Ok(EndpointAddr { id, addrs })
    }
}

fn hex(bytes: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(bytes)
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    data_encoding::HEXLOWER.decode(s.as_bytes()).ok()
}
