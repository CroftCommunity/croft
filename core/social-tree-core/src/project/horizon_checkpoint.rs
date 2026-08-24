//! EXP-H2 (RUN-12 Part 6) — the horizon checkpoint as a foldable **fact**.
//!
//! EXP-H1 (RUN-07, `horizon.rs`) proved the horizon *manifest* computes byte-identically
//! across members and arrival orders; the checkpoint itself was still only a pure
//! function. This lands the **fact form**: a member records a horizon-checkpoint fact
//! carrying its own folded manifest `(frontier digest, manifest)`, and a second member
//! that independently folded the identical set records a co-signing fact naming the same
//! digests. The **corroboration count** for a `(frontier, manifest)` pair is the number
//! of DISTINCT members whose independent fold produced it.
//!
//! §7.3.3 semantics are UNCHANGED (Part 2): a co-signature is corroboration of an
//! *independent identical fold*, **never** a substitute for local validation. A member
//! records only the manifest its OWN fold produced (`CheckpointFact::record` takes a
//! member's own `GroupState`), so a member whose fold does not match contributes nothing
//! to another manifest's corroboration — there is no false corroboration.
//!
//! Scope (deliberate, experiment-grade): fact shapes only, test-only serialization, NO
//! wire pinning. The manifest encoding stays `[gates-release]` (§7.6.9) and the §7.6.9
//! worked example stays `Design`. This module adds **no authority**: a checkpoint fact
//! carries no threshold and gates nothing — an open contradiction remains open truth
//! however many members corroborate its presence (decay is presentation, §7.6.9).

use std::collections::BTreeSet;

use crate::model::GroupState;
use crate::project::horizon::{horizon_manifest, HorizonManifest};

/// An experiment-grade horizon-checkpoint **fact**: one member's assertion that, at a
/// cadence boundary, it independently folded a set whose horizon manifest is exactly
/// `manifest`. `signer` is the recording member's lineage principal — co-signatures are
/// unioned by distinct lineage (§7.3.4), a persona's clients collapsing to one. This is
/// NOT the `[gates-release]` §7.6.9 checkpoint encoding; it is a test-only fact shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointFact {
    /// The recording member's lineage principal (32 bytes).
    pub signer: [u8; 32],
    /// The manifest this member's OWN fold produced — `(frontier_head, open_contradictions)`.
    pub manifest: HorizonManifest,
}

impl CheckpointFact {
    /// Record a checkpoint fact from a member's OWN folded state. The member computes the
    /// manifest itself (it never copies a peer's), which is exactly what makes a later
    /// co-signature corroboration of an independent identical fold rather than a trusted
    /// summary of one member's opinion (§7.3.3).
    #[must_use]
    pub fn record(signer: [u8; 32], state: &GroupState) -> Self {
        Self {
            signer,
            manifest: horizon_manifest(state),
        }
    }
}

/// The **corroboration count** for `manifest`: the number of DISTINCT signers (by lineage)
/// whose recorded checkpoint fact names the identical manifest. Deterministic and
/// order-independent — it is a set fold over the facts, so any permutation of the same
/// facts yields the same count. A member whose fold produced a *different* manifest
/// contributes nothing here: no false corroboration.
#[must_use]
pub fn corroboration_count(facts: &[CheckpointFact], manifest: &HorizonManifest) -> usize {
    let mut signers: BTreeSet<[u8; 32]> = BTreeSet::new();
    for f in facts {
        if &f.manifest == manifest {
            signers.insert(f.signer);
        }
    }
    signers.len()
}

/// A test-only byte serialization of a manifest, for byte-identity comparison across
/// members, arrival orders, and successive checkpoints. NOT the `[gates-release]` §7.6.9
/// manifest encoding — it exists only to turn a manifest into a comparable byte string.
#[must_use]
pub fn manifest_bytes(m: &HorizonManifest) -> Vec<u8> {
    let mut b = Vec::with_capacity(36 + m.open_contradictions.len() * 32);
    b.extend_from_slice(m.frontier_head.as_bytes());
    b.extend_from_slice(&(m.open_contradictions.len() as u32).to_be_bytes());
    for h in &m.open_contradictions {
        b.extend_from_slice(h.as_bytes());
    }
    b
}
