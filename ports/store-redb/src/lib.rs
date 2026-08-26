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
