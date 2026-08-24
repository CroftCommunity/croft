//! Projections beyond the per-group state: the reconciliation-horizon manifest,
//! head-currency (corroborated freshness), head acks, and completeness-ahead.
//! All pure functions over model types; the membership projection itself lives
//! on [`crate::model::GroupState::membership`] beside its type.

pub mod completeness_ahead;
pub mod head_ack;
pub mod head_currency;
pub mod horizon;
pub mod horizon_checkpoint;
