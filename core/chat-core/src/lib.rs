//! The chat pond (E117 P5) — one tenant on the social tree.
//!
//! Pure core: `Intent` / `Effect` / `update` / `project` / view models for the
//! two-pane chat surface. Depends on `social-tree-core` for substrate types
//! and on nothing else — no storage, no I/O, no clock (ADR-0001/0002).
//!
//! Phases populate this crate: P8 (model + update), P9 (project + view),
//! P14 (channel selection state).
#![warn(missing_docs)]

pub mod model;
pub mod project;
pub mod update;
pub mod view;

pub use model::{ChannelRef, Effect, GroupRef, Intent, MessageLine, Model, Snapshot};
pub use project::project;
pub use update::update;
pub use view::{
    ChannelNode, ChatView, GraphTreeView, GroupNode, TimelineLineView, TimelineView, TreeRow,
};
