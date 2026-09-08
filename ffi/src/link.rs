//! The gossip link, as the shell sees it.
//!
//! A second uniffi object beside [`crate::ChatSession`], deliberately separate.
//! The session owns MLS state and the store; the link owns a socket and a
//! swarm. Fusing them would mean the shell could not hold a conversation
//! without a network, and rung 6 of the S2 runbook — force-stop, relaunch,
//! reload the group — is exactly a moment when there is state and no swarm.
//!
//! The two are joined in the shell, by a loop two branches wide: a Welcome goes
//! to `acceptInvite`, a sealed message goes to `openSealed`. That branch is
//! deliberately in Kotlin rather than hidden here, because it is the one piece
//! of S2's wiring the JVM test tier can drive end to end.

use std::sync::Mutex;

use transport_iroh::frame::ArtifactKind;
use transport_iroh::pairing::{decode_blob, encode_blob, PairingBlob};
use transport_iroh::{DialCard as CoreDialCard, GossipTransport, TopicKey, TransportError};

use crate::FfiError;

/// Where to reach one device, as the shell carries it.
///
/// A mirror of the transport's own type rather than a re-export: uniffi records
/// must be declared in the crate that exports them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DialCard {
    /// The peer's endpoint id, hex.
    pub endpoint_id: String,
    /// Direct `ip:port` addresses. Never a relay URL.
    pub addrs: Vec<String>,
}

/// A pairing code, once read: who to dial and the key package to invite them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PairingCode {
    /// Who to dial, and where.
    pub card: DialCard,
    /// The MLS key package the code carried.
    pub key_package: Vec<u8>,
    /// The group whose swarm to join, or empty in a joiner's code.
    ///
    /// A joining device cannot reach a group's swarm without this: the topic IS
    /// the group id. Found on hardware, because the JVM tier had handed the id
    /// between surfaces directly and two phones cannot.
    pub group_id: Vec<u8>,
}

/// Which of the two artifact kinds arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ArtifactBox {
    /// An MLS Welcome — hand this to `acceptInvite`.
    Welcome,
    /// A sealed message — hand this to `openSealed`.
    Sealed,
    /// A group's record — hand this to `readRecord`, show a person what it
    /// claims, and only fold it if they accept.
    Record,
}

/// One artifact off the wire.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Artifact {
    /// Which kind arrived, and therefore which method it is for.
    pub kind: ArtifactBox,
    /// The artifact's bytes, framing already stripped.
    pub payload: Vec<u8>,
}

/// One device's link to a group's swarm.
///
/// `Debug` is implemented by hand below rather than derived: the transport
/// holds a signing key, and a derived form would print it into any test
/// failure or log line that touched a link.
#[derive(uniffi::Object)]
pub struct GossipLink {
    /// `None` once shut down. uniffi objects are shared by reference and the
    /// foreign side may well call a method after `shutdown`; taking the
    /// transport out rather than consuming the object lets that be a refusal
    /// with words instead of a use-after-free.
    inner: Mutex<Option<GossipTransport>>,
}

#[uniffi::export]
impl GossipLink {
    /// Join the swarm for `topic` as `signing_key`, bootstrapping from `peers`.
    ///
    /// `peers` empty is the founding device — it waits to be dialled. The topic
    /// is the 32-byte group id; the transport derives the wire topic from it.
    #[uniffi::constructor]
    pub fn start(
        signing_key: Vec<u8>,
        group_id: Vec<u8>,
        peers: Vec<DialCard>,
    ) -> Result<Self, FfiError> {
        let group: [u8; 32] =
            group_id
                .as_slice()
                .try_into()
                .map_err(|_| FfiError::BadGroupIdLength {
                    got: group_id.len() as u32,
                    reason: format!("a group id is 32 bytes, got {}", group_id.len()),
                })?;

        let cards: Vec<CoreDialCard> = peers.into_iter().map(Into::into).collect();

        let transport =
            GossipTransport::start(&signing_key, TopicKey::from_group_id(&group), &cards)?;

        Ok(GossipLink {
            inner: Mutex::new(Some(transport)),
        })
    }

    /// What to show the other device so it can dial this one.
    #[must_use]
    pub fn dial_card(&self) -> DialCard {
        match self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            Some(t) => t.dial_card().into(),
            // A shut-down link has no address. Empty rather than a refusal
            // because a shell asking "where am I" after teardown wants an
            // answer it can render, not an exception on a dead path.
            None => DialCard {
                endpoint_id: String::new(),
                addrs: Vec::new(),
            },
        }
    }

    /// Teach this link about a peer learned after it started.
    ///
    /// What reading a pairing code does. Takes the card by value because uniffi
    /// records cross by value; the transport's own method borrows.
    pub fn add_peer(&self, card: DialCard) -> Result<(), FfiError> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let transport = guard.as_ref().ok_or_else(Self::gone)?;
        Ok(transport.add_peer(&card.into())?)
    }

    /// The full pairing code: this device's dial card, key package, and group.
    ///
    /// `group_id` empty means "I have no group to offer" — a joiner's code.
    pub fn pairing_code(
        &self,
        key_package: Vec<u8>,
        group_id: Vec<u8>,
    ) -> Result<String, FfiError> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let transport = guard.as_ref().ok_or_else(Self::gone)?;
        Ok(encode_blob(&PairingBlob {
            card: transport.dial_card(),
            key_package,
            group_id,
        })?)
    }

    /// How many devices are currently on this topic.
    #[must_use]
    pub fn neighbour_count(&self) -> u32 {
        match self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            Some(t) => t.neighbour_count() as u32,
            None => 0,
        }
    }

    /// Block until someone joins, or `timeout_ms` passes. Returns whether
    /// anyone arrived.
    ///
    /// The shell needs this before it sends: gossip delivers to the neighbours
    /// it has at the time of the call, so a send into an empty swarm succeeds
    /// and reaches nobody.
    #[must_use]
    pub fn wait_for_peer(&self, timeout_ms: u64) -> bool {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            Some(t) => t.wait_for_peer(std::time::Duration::from_millis(timeout_ms)),
            None => false,
        }
    }

    /// Send a Welcome to the swarm.
    pub fn broadcast_welcome(&self, welcome: Vec<u8>) -> Result<(), FfiError> {
        self.broadcast(ArtifactKind::Welcome, &welcome)
    }

    /// Send a sealed message to the swarm.
    pub fn broadcast_sealed(&self, sealed: Vec<u8>) -> Result<(), FfiError> {
        self.broadcast(ArtifactKind::Sealed, &sealed)
    }

    /// Offer a group's record to whoever is joining.
    ///
    /// Sent alongside the Welcome at invite time. The Welcome seats the joiner
    /// in the lockbox; this is what lets them fold the group at all.
    pub fn broadcast_record(&self, offer: Vec<u8>) -> Result<(), FfiError> {
        self.broadcast(ArtifactKind::Record, &offer)
    }

    /// The next artifact, or `None` if none arrived within `timeout_ms`.
    ///
    /// `None` is quiet, not failure — the far phone simply has not sent yet,
    /// which is the normal state for most of a device run.
    #[must_use]
    pub fn next_artifact(&self, timeout_ms: u64) -> Option<Artifact> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let transport = guard.as_ref()?;
        transport
            .next_artifact(std::time::Duration::from_millis(timeout_ms))
            .map(|f| Artifact {
                kind: match f.kind {
                    ArtifactKind::Welcome => ArtifactBox::Welcome,
                    ArtifactKind::Sealed => ArtifactBox::Sealed,
                    ArtifactKind::Record => ArtifactBox::Record,
                },
                payload: f.payload,
            })
    }

    /// Leave the swarm and close the socket.
    ///
    /// Idempotent: a shell whose lifecycle fires twice should not be punished
    /// for it, and on Android that lifecycle fires twice routinely.
    pub fn shutdown(&self) {
        let taken = self.inner.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(t) = taken {
            t.shutdown();
        }
    }
}

impl std::fmt::Debug for GossipLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f.debug_struct("GossipLink")
            .field("live", &guard.is_some())
            .finish_non_exhaustive()
    }
}

impl GossipLink {
    fn broadcast(&self, kind: ArtifactKind, payload: &[u8]) -> Result<(), FfiError> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let transport = guard.as_ref().ok_or_else(Self::gone)?;
        Ok(transport.broadcast(kind, payload)?)
    }

    fn gone() -> FfiError {
        FfiError::Transport {
            reason: "this link has been shut down".to_string(),
        }
    }
}

/// Read a pairing code shown by another device.
///
/// A free function rather than a method: the reading device has no link for
/// that group yet — reading the code is how it learns what to join.
#[uniffi::export]
pub fn read_pairing_code(code: &str) -> Result<PairingCode, FfiError> {
    let blob = decode_blob(code)?;
    Ok(PairingCode {
        card: blob.card.into(),
        key_package: blob.key_package,
        group_id: blob.group_id,
    })
}

impl PairingCode {
    /// Read a pairing code — the Rust-side name, so pins read as prose.
    pub fn read(code: &str) -> Result<PairingCode, FfiError> {
        read_pairing_code(code)
    }
}

impl From<CoreDialCard> for DialCard {
    fn from(c: CoreDialCard) -> Self {
        DialCard {
            endpoint_id: c.endpoint_id,
            addrs: c.addrs,
        }
    }
}

impl From<DialCard> for CoreDialCard {
    fn from(c: DialCard) -> Self {
        CoreDialCard {
            endpoint_id: c.endpoint_id,
            addrs: c.addrs,
        }
    }
}

impl From<TransportError> for FfiError {
    /// The transport's refusals, mapped so their words survive the crossing.
    ///
    /// Every arm carries `reason`, because uniffi builds a generated
    /// exception's `message` from the variant's FIELDS — the S1 finding. The
    /// pairing arms are kept distinct from the rest: their words are read by a
    /// person standing between two phones, and flattening them into a generic
    /// transport error would put a stack-trace sentence in front of someone
    /// trying to type a code.
    fn from(e: TransportError) -> Self {
        let reason = e.to_string();
        match e {
            TransportError::BadKeyLength { got } => FfiError::BadKeyLength {
                got: got as u32,
                reason,
            },
            TransportError::BadPairingCode { .. }
            | TransportError::BlobVersion { .. }
            | TransportError::BadDialCard { .. } => FfiError::BadPairingCode { reason },
            _ => FfiError::Transport { reason },
        }
    }
}
