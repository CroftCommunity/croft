//! EXP-C3 (E112) — the HeadAck head-currency primitive.
//!
//! A `HeadAck` is a **§7.3.4 sign-the-state object**: its identity is the *state attested*
//! (`group_id`, `head`, `generation`), so two acks of the same head from different signers are one
//! object with two vouchers — they **union**, they never rival. Freshness for the §7.4 gate is the
//! number of **distinct lineages** (never clients/devices, §5.7) attesting a node's current head.
//!
//! Discipline enforced here:
//!   * **No wall-clock, anywhere.** The horizon is a `generation` (epoch counter). Local elapsed
//!     time, if used at all, is a *private* freshness input — never on the wire (§7.4).
//!   * **A forged ack fails the signature.** Verification is a typestate gate: only a
//!     [`VerifiedHeadAck`] can be recorded, so an unverified/forged ack cannot reach the count.
//!   * **An ack naming an unknown head is a detected GAP, not an authorization input** (§7.4.3):
//!     it means "converge before trusting," and is never counted toward freshness.
//!
//! Fidelity: **Modeled / loopback grade.** Real signature binding (via the `Signer`/`Verifier`
//! traits), a real wire crossing (serialize → re-parse), and distinct-lineage union. The transport
//! is modeled in-process; running the same acks over the FANOUT-M1 `IrohGossipBus` (which carries
//! opaque frames) is a mechanical upgrade, not run in this pass.

use std::collections::{BTreeSet, HashMap};

use crate::model::{compute_hash, DeviceId, GroupId, Hash, PrincipalId};
use crate::ports::{CredentialResolver, Signer, Verifier};

const HEAD_ACK_TAG: &[u8] = b"croft/head-ack/v1";

/// A signed attestation that, at `generation`, the signer's `group_id` head was `head`.
///
/// The attesting **lineage** (`signer_lineage`, a persona/principal root) is what freshness counts;
/// the attesting **device** (`signer_device`) is what the signature binds to. Two devices of one
/// lineage attesting one head are still a single voucher (§5.7 — corroboration counts personae,
/// never clients).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadAck {
    pub group_id: GroupId,
    pub head: Hash,
    pub generation: u64,
    pub signer_lineage: PrincipalId,
    pub signer_device: DeviceId,
    pub sig: Vec<u8>,
}

/// The 32-byte digest a HeadAck signs / is identified by — a hash over the *state*
/// (`group_id`, `head`, `generation`), never a timestamp. Signing a digest (rather than the raw
/// concatenation) binds *every* field, so tampering any of them invalidates the signature.
fn signed_digest(group_id: &GroupId, head: &Hash, generation: u64) -> [u8; 32] {
    let mut b = Vec::with_capacity(HEAD_ACK_TAG.len() + 32 + 32 + 8);
    b.extend_from_slice(HEAD_ACK_TAG);
    b.extend_from_slice(group_id.as_bytes());
    b.extend_from_slice(head.as_bytes());
    b.extend_from_slice(&generation.to_be_bytes());
    *compute_hash(&b).as_bytes()
}

/// What went wrong verifying or parsing a HeadAck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadAckError {
    /// The signature did not verify for the claimed device (forged / tampered / wrong signer).
    BadSignature,
    /// The signing device is not a valid credential for the claimed lineage.
    LineageMismatch,
    /// The wire bytes did not parse as a HeadAck.
    Malformed,
}

impl HeadAck {
    /// Mint a HeadAck for `head` at `generation`, signed by `device` on behalf of `lineage`.
    #[must_use]
    pub fn mint<S: Signer>(
        signer: &S,
        lineage: PrincipalId,
        group_id: GroupId,
        head: Hash,
        generation: u64,
    ) -> Self {
        let sig = signer.sign(&signed_digest(&group_id, &head, generation));
        Self {
            group_id,
            head,
            generation,
            signer_lineage: lineage,
            signer_device: DeviceId::new(signer.device_id().0),
            sig,
        }
    }

    /// Serialize to wire bytes (fixed layout: group(32) head(32) gen(8) lineage(32) device(32)
    /// sig_len(4) sig). No timestamp appears anywhere.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(32 + 32 + 8 + 32 + 32 + 4 + self.sig.len());
        b.extend_from_slice(self.group_id.as_bytes());
        b.extend_from_slice(self.head.as_bytes());
        b.extend_from_slice(&self.generation.to_be_bytes());
        b.extend_from_slice(self.signer_lineage.as_bytes());
        b.extend_from_slice(self.signer_device.as_bytes());
        b.extend_from_slice(&(self.sig.len() as u32).to_be_bytes());
        b.extend_from_slice(&self.sig);
        b
    }

    /// Parse wire bytes back into a HeadAck.
    ///
    /// # Errors
    /// [`HeadAckError::Malformed`] if the bytes are the wrong shape.
    pub fn from_bytes(b: &[u8]) -> Result<Self, HeadAckError> {
        if b.len() < 140 {
            return Err(HeadAckError::Malformed);
        }
        let a32 = |off: usize| {
            let mut x = [0u8; 32];
            x.copy_from_slice(&b[off..off + 32]);
            x
        };
        let group_id = GroupId::new(a32(0));
        let head = Hash::new(a32(32));
        let generation =
            u64::from_be_bytes(b[64..72].try_into().map_err(|_| HeadAckError::Malformed)?);
        let signer_lineage = PrincipalId::new(a32(72));
        let signer_device = DeviceId::new(a32(104));
        let sig_len = u32::from_be_bytes(
            b[136..140]
                .try_into()
                .map_err(|_| HeadAckError::Malformed)?,
        ) as usize;
        if b.len() != 140 + sig_len {
            return Err(HeadAckError::Malformed);
        }
        Ok(Self {
            group_id,
            head,
            generation,
            signer_lineage,
            signer_device,
            sig: b[140..].to_vec(),
        })
    }

    /// Verify the signature (and that the signing device is a credential of the claimed lineage).
    /// Only a verified ack can be recorded — this is the typestate gate that keeps a forged ack out
    /// of the freshness count.
    ///
    /// # Errors
    /// [`HeadAckError::BadSignature`] on a bad/forged signature; [`HeadAckError::LineageMismatch`]
    /// if the device does not resolve to the claimed lineage.
    pub fn verify<V: Verifier, C: CredentialResolver>(
        self,
        verifier: &V,
        cred: &C,
    ) -> Result<VerifiedHeadAck, HeadAckError> {
        use crate::ports::{DeviceId as TDeviceId, PrincipalId as TPrincipalId};
        let dev = TDeviceId(*self.signer_device.as_bytes());
        let lin = TPrincipalId(*self.signer_lineage.as_bytes());
        if cred.resolve(&dev, &lin).is_err() {
            return Err(HeadAckError::LineageMismatch);
        }
        let digest = signed_digest(&self.group_id, &self.head, self.generation);
        verifier
            .verify(&dev, &digest, &self.sig)
            .map_err(|_| HeadAckError::BadSignature)?;
        Ok(VerifiedHeadAck(self))
    }
}

/// A HeadAck whose signature and lineage have been checked. Constructible only via
/// [`HeadAck::verify`], so [`FreshnessTracker::record`] cannot be handed a forged ack.
#[derive(Debug, Clone)]
pub struct VerifiedHeadAck(HeadAck);

impl VerifiedHeadAck {
    #[must_use]
    pub fn head(&self) -> &Hash {
        &self.0.head
    }
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.0.generation
    }
    #[must_use]
    pub fn lineage(&self) -> &PrincipalId {
        &self.0.signer_lineage
    }
}

/// The outcome of folding one verified ack into the freshness tracker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AckOutcome {
    /// The ack attests THIS node's current head — it counts toward corroborated-freshness.
    CorroboratesHead,
    /// The ack names a *different* head — a detected gap (§7.4.3). Never counted toward freshness;
    /// a locator that says "converge before trusting," carrying the peer's head/generation so the
    /// caller can catch up. `ahead` is true iff the peer's generation is strictly newer.
    DetectedGap {
        peer_head: Hash,
        peer_generation: u64,
        ahead: bool,
    },
}

/// Accumulates head-attestations for one group. Freshness = distinct **lineages** attesting the
/// node's current head. Only verified acks enter (typestate). A duplicate lineage does not
/// inflate the count.
#[derive(Debug)]
pub struct FreshnessTracker {
    group_id: GroupId,
    my_head: Hash,
    my_generation: u64,
    by_head: HashMap<Hash, BTreeSet<PrincipalId>>,
}

impl FreshnessTracker {
    #[must_use]
    pub fn new(group_id: GroupId, my_head: Hash, my_generation: u64) -> Self {
        Self {
            group_id,
            my_head,
            my_generation,
            by_head: HashMap::new(),
        }
    }

    /// Fold one verified ack. Acks for a different group are ignored (not this tracker's concern).
    pub fn record(&mut self, ack: &VerifiedHeadAck) -> AckOutcome {
        if ack.0.group_id != self.group_id {
            // Wrong group: a detected gap that is not even about us; treat as non-corroborating.
            return AckOutcome::DetectedGap {
                peer_head: ack.0.head,
                peer_generation: ack.0.generation,
                ahead: false,
            };
        }
        if ack.0.head == self.my_head {
            self.by_head
                .entry(ack.0.head)
                .or_default()
                .insert(ack.0.signer_lineage);
            AckOutcome::CorroboratesHead
        } else {
            AckOutcome::DetectedGap {
                peer_head: ack.0.head,
                peer_generation: ack.0.generation,
                ahead: ack.0.generation > self.my_generation,
            }
        }
    }

    /// Distinct lineages attesting THIS node's current head — the freshness the §7.4 gate consumes.
    #[must_use]
    pub fn freshness(&self) -> u64 {
        self.by_head
            .get(&self.my_head)
            .map_or(0, |s| s.len() as u64)
    }
}
