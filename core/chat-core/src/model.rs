//! The chat tenant's model, intents, and effects.
//!
//! Pure data — no I/O, no substrate handles. The shell feeds the model snapshots
//! it has read from the `Session` (via [`Intent::Refresh`]) and performs the
//! emitted [`Effect`]s against the `Session`.

use social_tree_core::model::{GroupId, TypedId};

/// A group as shown in the left-pane tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupRef {
    /// The group's id.
    pub id: GroupId,
    /// A human label (falls back to a short id when untitled).
    pub title: String,
    /// Number of members.
    pub member_count: usize,
}

/// A channel within a group, as the pond names it: the typed artifact id
/// plus a display name. The session facade (discovery side) has its own
/// twin on the read surface; the shell converts — the pond depends on the
/// substrate only, never on a facade (ADR-0002's dependency direction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelRef {
    /// The channel's typed id (used to scope sends and timelines).
    pub id: TypedId,
    /// The channel's display name.
    pub name: String,
}

/// One rendered message line in the timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageLine {
    /// Lamport order (`u64::MAX` marks an unconfirmed optimistic local line).
    pub lamport: u64,
    /// Author label.
    pub author: String,
    /// Message body.
    pub body: String,
}

impl MessageLine {
    /// Lamport value used for an optimistic, not-yet-confirmed local line, so it
    /// sorts after every confirmed message.
    pub const OPTIMISTIC: u64 = u64::MAX;
}

/// The chat tenant's full UI model.
#[derive(Debug, Clone, Default)]
pub struct Model {
    /// All groups the user belongs to (left-pane tree).
    pub groups: Vec<GroupRef>,
    /// The currently selected group, if any.
    pub selected_group: Option<GroupId>,
    /// Channels of the selected group.
    pub channels: Vec<ChannelRef>,
    /// The currently selected channel, if any (else the group-level timeline).
    pub selected_channel: Option<TypedId>,
    /// The timeline for the selected group/channel.
    pub timeline: Vec<MessageLine>,
    /// The message the user is composing.
    pub draft: String,
    /// Set when the selected group is forked (a contradiction the substrate
    /// flagged). The shell shows this as a blocking banner and does not present a
    /// silent winner (the §7.6 hard-stop).
    pub fork: Option<String>,
}

/// A read-only snapshot the shell pushes into the model on refresh.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// The user's groups.
    pub groups: Vec<GroupRef>,
    /// Channels of `group`.
    pub channels: Vec<ChannelRef>,
    /// Which group `timeline` belongs to (used to drop stale refreshes).
    pub group: Option<GroupId>,
    /// Which channel `timeline` belongs to (`None` = group-level).
    pub channel: Option<TypedId>,
    /// The timeline for `group`/`channel`.
    pub timeline: Vec<MessageLine>,
    /// Fork status of `group` (`Some` when contradicted).
    pub fork: Option<String>,
}

/// Things the user (or shell) asks the core to do.
#[derive(Debug, Clone)]
pub enum Intent {
    /// Select a group to view (resets to its group-level timeline).
    SelectGroup(GroupId),
    /// Select a channel within the current group.
    SelectChannel(TypedId),
    /// Append a character to the draft.
    TypeChar(char),
    /// Delete the last draft character.
    Backspace,
    /// Send the current draft to the selected group/channel.
    SendMessage,
    /// Adopt fresh data read from the session.
    Refresh(Snapshot),
}

/// Side effects the shell performs against the `Session`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Send `body` to `group` (and `channel` when set).
    Send {
        /// Target group.
        group: GroupId,
        /// Target channel (`None` = group-level).
        channel: Option<TypedId>,
        /// Message body.
        body: String,
    },
    /// (Re)load the timeline for `group`/`channel`.
    LoadTimeline {
        /// Group to load.
        group: GroupId,
        /// Channel to load (`None` = group-level).
        channel: Option<TypedId>,
    },
}
