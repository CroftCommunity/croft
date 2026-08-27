//! The redb storage realization for croft.
//!
//! Promoted from the discovery corpus (P7 S0). This crate holds the durable
//! half of the client: the table schema, the derived fold that ingests signed
//! assertions into folded group state, and the governance chain over them.
//!
//! It implements no core port trait — see the manifest for why (P0-1).
#![warn(missing_docs)]

/// The table schema: redb table definitions plus the key and value codecs
/// over them. Every table this crate touches is declared here and nowhere
/// else — see [`tables::STATE_BLOB_PRESENCE`] for what the alternative cost.
pub mod tables;

/// The derived fold: ingests signed assertions into folded group state,
/// writing the authoritative and derived tables in one transaction.
pub mod fold_derived;

/// Governance over the fold: forks and their deterministic tiebreak,
/// checkpoints, the Merkle root, and compaction.
pub mod governance;

/// Local truth: what this device holds that is never folded and never sent.
pub mod local;

/// Assertion payload encoders — the writer half of the wire format.
pub mod payload;

/// Queries over the folded tables: what a shell asks, in substrate types.
pub mod read;

#[cfg(test)]
mod tests_stage7;
