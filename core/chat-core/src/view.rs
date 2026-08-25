//! Platform-agnostic view models for the two panes.
//!
//! These are what a renderer (the ratatui shell, or a test backend) draws — no
//! ratatui types here, so the projection is testable without a terminal.

use social_tree_core::model::{GroupId, PrincipalId, TypedId};

/// The whole rendered chat surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatView {
    /// Left pane: the social-graph tree.
    pub tree: GraphTreeView,
    /// Right pane: the message timeline.
    pub timeline: TimelineView,
    /// The composing draft (input line).
    pub draft: String,
    /// Set when the selected group is forked — the shell renders a blocking
    /// banner and refuses to present a silent winner.
    pub fork: Option<String>,
    /// The truthful membership panel for the selected group.
    pub members: MembersPaneView,
}

/// The membership panel: the fold's roster with standing spelled out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MembersPaneView {
    /// Rows in roster order.
    pub rows: Vec<MemberRowView>,
}

/// One rendered member row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberRowView {
    /// The member's principal (the shell shortens for display).
    pub principal: PrincipalId,
    /// Role label.
    pub role: String,
    /// Standing, in the words the product committed to: empty for seated,
    /// "membership pending resolution" (E108), "admission voided" (E116).
    pub standing_label: String,
    /// Whether this session's user has personally muted the member (E134).
    pub muted: bool,
}

/// Left-pane tree: a flat, render-and-navigate-ready row list. Channels of the
/// selected group appear (indented) immediately after their group row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GraphTreeView {
    /// Rows in display order (groups, with the selected group's channels nested).
    pub rows: Vec<TreeRow>,
}

/// One selectable row in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeRow {
    /// A group row.
    Group(GroupNode),
    /// A channel row (nested under its group).
    Channel(ChannelNode),
}

/// A group row in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupNode {
    /// The group's id.
    pub id: GroupId,
    /// Display label.
    pub title: String,
    /// Member count (shown beside the title).
    pub member_count: usize,
    /// Whether this group is the selected one.
    pub selected: bool,
}

/// A channel row in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelNode {
    /// The channel's typed id.
    pub id: TypedId,
    /// Display name.
    pub name: String,
    /// Whether this channel is the selected one.
    pub selected: bool,
}

/// Right-pane timeline.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimelineView {
    /// Message lines, in display order.
    pub lines: Vec<TimelineLineView>,
}

/// One rendered message line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineLineView {
    /// Author label.
    pub author: String,
    /// Message body.
    pub body: String,
    /// True while the line is an unconfirmed optimistic local send.
    pub pending: bool,
    /// True when the author is personally muted — the line renders MARKED
    /// (the shell collapses it), never silently dropped.
    pub muted: bool,
}
