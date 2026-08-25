//! **The KeyLayer port — an artifact carrier that never answers "admit?"
//! (E117 P4, ADR-0003).**
//!
//! The contract between the membership brain and whatever key machinery
//! realizes group encryption (openmls natively; the wasm realization is
//! `[confirm]` until probed). Two operations, and their asymmetry is the
//! design:
//!
//! - [`KeyLayer::stage_commit`] reads wire bytes and returns
//!   [`AdmissionClaims`](crate::admission::AdmissionClaims) — **data, never
//!   a decision**. What the commit *says*, for the core to judge.
//! - [`KeyLayer::merge_admission`] is the one membership-mutating operation,
//!   and it demands a [`MergeApproval`](crate::admission::MergeApproval) —
//!   a value only [`evaluate_admission`](crate::admission::evaluate_admission)
//!   can construct, carrying the minted admission fact inside it. An adapter
//!   answering "admit?" on its own is not forbidden; it is unrepresentable.
//!
//! The shell owns the orchestration (parse → decide → enact) and holds the
//! port, per ADR-0001; the core never calls this trait. MLS state — ratchet
//! state, the PSK store, provider storage — lives entirely on the adapter's
//! side of this line (ADR-0003 decision 3), and no method here exposes key
//! material: backup/export/recovery is the named-but-not-designed
//! `KeyCustody` seam (§7.3.9), which this trait deliberately does not serve.

use thiserror::Error;

use super::super::admission::{AdmissionClaims, MergeApproval};
use crate::model::Hash;

/// What can go wrong at the key layer. Every variant names *where* it
/// failed — "it failed" and "it was refused at this point with this error"
/// are different security stories (the S7 lesson).
#[derive(Debug, Error)]
pub enum KeyLayerError {
    /// The bytes did not parse as a commit at all.
    #[error("not a parseable commit: {0}")]
    Parse(String),
    /// The bytes parsed but the key machinery refused to process them
    /// (library error text carried verbatim, never paraphrased).
    #[error("the key layer refused to process the commit: {0}")]
    Process(String),
    /// An approval names an event this adapter never staged: the approval
    /// authorizes a merge, it does not conjure one.
    #[error("no staged commit for approved event {0:?}")]
    UnknownCommit(Hash),
}

/// The result of an executed merge: the epoch the admission minted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergedEpoch {
    /// The new epoch every member derives after processing the commit.
    pub epoch: u64,
}

/// The key-layer contract. Realizations: the openmls adapter (native,
/// adapted from the meer-queue lineage), in-memory mocks for seam tests.
pub trait KeyLayer {
    /// Parse and cryptographically validate a `NewMemberCommit`'s wire
    /// bytes, returning what the commit claims — as data for
    /// [`evaluate_admission`](crate::admission::evaluate_admission), never
    /// as a decision. Cryptographic validity MUST NOT imply admission
    /// (A3): a staged commit is a question, not a member.
    ///
    /// # Errors
    /// [`KeyLayerError::Parse`] for bytes that are not a commit;
    /// [`KeyLayerError::Process`] when the key machinery refuses them.
    fn stage_commit(&mut self, wire: &[u8]) -> Result<AdmissionClaims, KeyLayerError>;

    /// Execute the merge the approval authorizes. The only
    /// membership-mutating operation on the port, and it cannot be called
    /// without a [`MergeApproval`] — which carries the admission fact the
    /// shell deposits to the governance log with this merge, or neither
    /// happens.
    ///
    /// # Errors
    /// [`KeyLayerError::UnknownCommit`] when the approved event was never
    /// staged here; [`KeyLayerError::Process`] when the key machinery
    /// fails the merge.
    fn merge_admission(&mut self, approval: MergeApproval) -> Result<MergedEpoch, KeyLayerError>;
}
