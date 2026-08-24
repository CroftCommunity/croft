//! # social-tree-core
//!
//! The Drystone social-tree substrate: groups, membership, standing — the
//! backbone the per-pond feature cores (`call-core`, `feed-core`, the coming
//! `chat-core`) stand on. **This crate is not a pond**: ponds consume it
//! (docs/ADR-0002-core-layering.md).
//!
//! Pure by contract: no storage, no I/O, no async, no clock — enforced
//! mechanically (clippy.toml disallowed-methods, the wasm32 CI arm, and a
//! `--no-default-features` check). Storage is a port held by the shell/adapter,
//! never called from here; the redb realization lives in the discovery corpus
//! (`local_storage_projection`), which consumes this crate pinned by commit.
//!
//! Module map (the group-core template, applied to a substrate):
//! - [`model`] — identifiers, the envelope, roles, rules, the projected state
//! - [`wire`] — the single canonical decoder + the version register
//! - [`update`] — the fold: authorization, detection, CONTESTED, resolution,
//!   deterministic replay, and [`update::evaluate`], the one transition
//! - [`project`] — projections beyond the group state: horizon manifests,
//!   head-currency, completeness
//! - [`ports`] — the capability contracts (verify, sign, resolve) the shell
//!   implements; the core never calls them
//! - [`charter`] — the typed charter: every dial is data (E111/E121); the core
//!   holds no charter constant
//! - [`metrics`] — the no-op-by-default measurement port (§11.11 hooks)
#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod charter;
pub mod metrics;
pub mod model;
pub mod ports;
pub mod project;
pub mod update;
pub mod wire;
