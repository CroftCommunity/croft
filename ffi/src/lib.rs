//! The uniffi surface over the croft core.
//!
//! This file is deliberately thin. It holds the boundary types and the
//! `#[uniffi::export]` annotations and nothing else; the machinery lives in
//! [`session`], in ordinary Rust with ordinary Rust types, where it can be
//! tested without generating a binding or starting a JVM.
//!
//! The boundary has its own types rather than re-exporting the pond's, because
//! the two have different constraints. uniffi has no `char`, no foreign
//! newtype, and no `BTreeSet`; the pond has all three and should not give them
//! up for a binding generator. Translating once, here, is cheaper than bending
//! `chat_core` into a shape uniffi likes.
//!
//! Refusals cross as typed errors carrying their detail — see [`error`] for why
//! that is load-bearing rather than decorative.

/// What can go wrong, in the session's own words.
pub mod error;
/// One substrate instance, the ports beside it, and the pond's loop over both.
pub mod session;

use std::path::PathBuf;
use std::sync::Mutex;

use social_tree_core::model::GroupId;

uniffi::setup_scaffolding!();

// ---------------------------------------------------------------------------
// Boundary types
// ---------------------------------------------------------------------------

/// A refusal, as the foreign side sees it.
///
/// One variant per way a call can fail, each carrying what makes it
/// actionable. Flattening these into one message-carrying variant would be
/// less code and strictly worse: a caller can branch on `NoGroupSelected` and
/// cannot branch on a string.
///
/// **Every variant also carries `reason`, and that is not redundancy.** uniffi
/// builds a generated exception's `message` from the variant's FIELDS, not
/// from the Rust `Display` impl — so before this field existed, the fieldless
/// variants crossed the boundary with `message == ""`. `NoGroupSelected` and
/// `EmptyDraft`, the two refusals a person is most likely to hit, were exactly
/// the fieldless ones: the typed exception arrived and the sentence did not,
/// and a shell rendering `e.message` rendered nothing. Found by S1 the first
/// time a surface tried to show a refusal to a user.
///
/// The words stay in the `#[error]` attributes — one source of truth — and
/// ride across in this field. Translating them shell-side would put a product
/// commitment in every shell, which is how they drift apart.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    /// A signing key was not 32 bytes.
    #[error("a signing key is 32 bytes, got {got}")]
    BadKeyLength {
        /// The length actually supplied.
        got: u32,
        /// The refusal in words, for showing a person.
        reason: String,
    },
    /// A group id was not 32 bytes.
    #[error("a group id is 32 bytes, got {got}")]
    BadGroupIdLength {
        /// The length actually supplied.
        got: u32,
        /// The refusal in words, for showing a person.
        reason: String,
    },
    /// A `TypeChar` carried something other than exactly one character.
    ///
    /// uniffi has no `char`, so the intent carries a string and the boundary
    /// checks it. Counted in characters, not bytes or UTF-16 units: an emoji
    /// is one keypress on the phone and must be one here.
    #[error("TypeChar takes exactly one character, got {count}")]
    NotOneCharacter {
        /// How many characters actually arrived.
        count: u32,
        /// The refusal in words, for showing a person.
        reason: String,
    },
    /// A send was attempted with no group selected.
    #[error("no group is selected, so there is nowhere to send")]
    NoGroupSelected {
        /// The refusal in words, for showing a person.
        reason: String,
    },
    /// A send was attempted with an empty draft.
    #[error("the draft is empty, so there is nothing to send")]
    EmptyDraft {
        /// The refusal in words, for showing a person.
        reason: String,
    },
    /// A group was named that this session is not a member of.
    #[error("no such group in this session's membership")]
    NoSuchGroup {
        /// The group that was asked for, as bytes.
        group: Vec<u8>,
        /// The refusal in words, for showing a person.
        reason: String,
    },
    /// The store refused, in its own words.
    #[error("storage: {reason}")]
    Storage {
        /// What the store said.
        reason: String,
    },
    /// The fold refused an assertion, in its own words.
    #[error("the fold refused: {reason}")]
    Refused {
        /// The fold's own words.
        reason: String,
    },
}

impl From<error::SessionError> for FfiError {
    fn from(e: error::SessionError) -> Self {
        use error::SessionError as S;
        // Taken once, before the match consumes `e`: this IS the sentence from
        // the `#[error]` attribute, and taking it here is what keeps the words
        // in exactly one place.
        let reason = e.to_string();
        match e {
            // `usize` does not cross; the cast is safe because these are
            // lengths of things that already fit in memory.
            S::BadKeyLength { got } => FfiError::BadKeyLength {
                got: got as u32,
                reason,
            },
            S::NoGroupSelected => FfiError::NoGroupSelected { reason },
            S::EmptyDraft => FfiError::EmptyDraft { reason },
            S::NoSuchGroup { group } => FfiError::NoSuchGroup {
                group: group.as_bytes().to_vec(),
                reason,
            },
            S::Storage { .. } => FfiError::Storage { reason },
            S::Refused { .. } => FfiError::Refused { reason },
        }
    }
}

impl FfiError {
    /// The refusal in words — never empty, whichever variant this is.
    ///
    /// The accessor exists so a caller does not have to know which variants
    /// happen to be fieldless; see the type's own docs for why `message` alone
    /// could not be trusted.
    #[must_use]
    pub fn reason(&self) -> &str {
        match self {
            FfiError::BadKeyLength { reason, .. }
            | FfiError::BadGroupIdLength { reason, .. }
            | FfiError::NotOneCharacter { reason, .. }
            | FfiError::NoGroupSelected { reason }
            | FfiError::EmptyDraft { reason }
            | FfiError::NoSuchGroup { reason, .. }
            | FfiError::Storage { reason }
            | FfiError::Refused { reason } => reason,
        }
    }
}

/// What the shell asks the pond to do.
///
/// A mirror of `chat_core::model::Intent`, not a re-export: uniffi has no
/// `char` and no foreign newtypes, and the pond should not give either up to
/// suit a binding generator.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum Intent {
    /// Select a group to view. 32 bytes.
    SelectGroup {
        /// The group's id.
        group: Vec<u8>,
    },
    /// Append one character to the draft.
    TypeChar {
        /// Exactly one character.
        c: String,
    },
    /// Delete the last draft character.
    Backspace,
    /// Send the current draft to the selected group.
    SendMessage,
    /// Re-read the world from the store.
    Refresh,
}

/// One rendered timeline line.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TimelineLine {
    /// Author label.
    pub author: String,
    /// Message body.
    pub body: String,
    /// True while the line is an unconfirmed optimistic local send.
    pub pending: bool,
    /// True when the author is personally muted — the line renders MARKED,
    /// never silently dropped.
    pub muted: bool,
}

/// The right pane.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Timeline {
    /// Lines in display order.
    pub lines: Vec<TimelineLine>,
}

/// A group row in the left-pane tree.
#[derive(Debug, Clone, uniffi::Record)]
pub struct GroupRow {
    /// The group's id, 32 bytes.
    pub id: Vec<u8>,
    /// Display label — the local title, or a short id when unnamed.
    pub title: String,
    /// How many members the fold has seated.
    pub member_count: u32,
    /// Whether this is the selected group.
    pub selected: bool,
}

/// A channel row in the left-pane tree.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ChannelRow {
    /// The channel's typed id, 33 bytes.
    pub id: Vec<u8>,
    /// Display name.
    pub name: String,
    /// Whether this is the selected channel.
    pub selected: bool,
}

/// One selectable row in the tree.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum TreeRow {
    /// A group row.
    Group(GroupRow),
    /// A channel row, nested under its group.
    Channel(ChannelRow),
}

/// The left pane.
#[derive(Debug, Clone, uniffi::Record)]
pub struct GraphTree {
    /// Rows in display order.
    pub rows: Vec<TreeRow>,
}

/// One member of the selected group.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MemberRow {
    /// The member's principal, 32 bytes.
    pub principal: Vec<u8>,
    /// Role label.
    pub role: String,
    /// Standing in the words the product committed to: empty for seated,
    /// "membership pending resolution", or "admission voided". A shell that
    /// substitutes its own words here is a shell that can flatter the record.
    pub standing_label: String,
    /// Whether this session's user has personally muted the member.
    pub muted: bool,
}

/// The membership panel.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MembersPane {
    /// Rows in roster order.
    pub rows: Vec<MemberRow>,
}

/// The whole rendered chat surface.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ChatView {
    /// Left pane.
    pub tree: GraphTree,
    /// Right pane.
    pub timeline: Timeline,
    /// The composing draft.
    pub draft: String,
    /// Set when the selected group is forked — the shell must render a blocking
    /// banner and must not present a silent winner.
    pub fork: Option<String>,
    /// The truthful membership panel.
    pub members: MembersPane,
}

impl From<chat_core::view::ChatView> for ChatView {
    fn from(v: chat_core::view::ChatView) -> Self {
        ChatView {
            tree: GraphTree {
                rows: v
                    .tree
                    .rows
                    .into_iter()
                    .map(|r| match r {
                        chat_core::view::TreeRow::Group(g) => TreeRow::Group(GroupRow {
                            id: g.id.as_bytes().to_vec(),
                            title: g.title,
                            member_count: g.member_count as u32,
                            selected: g.selected,
                        }),
                        chat_core::view::TreeRow::Channel(c) => TreeRow::Channel(ChannelRow {
                            id: c.id.as_bytes().to_vec(),
                            name: c.name,
                            selected: c.selected,
                        }),
                    })
                    .collect(),
            },
            timeline: Timeline {
                lines: v
                    .timeline
                    .lines
                    .into_iter()
                    .map(|l| TimelineLine {
                        author: l.author,
                        body: l.body,
                        pending: l.pending,
                        muted: l.muted,
                    })
                    .collect(),
            },
            draft: v.draft,
            fork: v.fork,
            members: MembersPane {
                rows: v
                    .members
                    .rows
                    .into_iter()
                    .map(|m| MemberRow {
                        principal: m.principal.as_bytes().to_vec(),
                        role: m.role,
                        standing_label: m.standing_label,
                        muted: m.muted,
                    })
                    .collect(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// The object the shell holds
// ---------------------------------------------------------------------------

/// One chat session: the substrate instance, its ports, and the pond's loop.
///
/// The shell holds exactly one of these. It is `Send + Sync` behind a `Mutex`
/// because uniffi objects are shared across whatever threads the platform
/// chooses, and because the session's methods mutate — a lock is the honest
/// representation of "one session, one at a time", and the alternative would be
/// to pretend the model can be read while it is being written.
#[derive(uniffi::Object)]
pub struct ChatSession {
    inner: Mutex<session::Session>,
}

#[uniffi::export]
impl ChatSession {
    /// Open (or create) the store at `path` for the identity `signing_key`.
    #[uniffi::constructor]
    pub fn open(path: String, signing_key: Vec<u8>) -> Result<Self, FfiError> {
        let session = session::Session::open(&PathBuf::from(path), &signing_key)?;
        Ok(ChatSession {
            inner: Mutex::new(session),
        })
    }

    /// Found a new group with this device as Owner, named locally on this
    /// device. Returns its 32-byte id.
    pub fn create_group(&self, title: String) -> Result<Vec<u8>, FfiError> {
        let mut session = self.lock();
        Ok(session.create_group(&title)?.as_bytes().to_vec())
    }

    /// Apply one intent and return the resulting projection.
    pub fn dispatch(&self, intent: Intent) -> Result<ChatView, FfiError> {
        let mut session = self.lock();
        Ok(session.dispatch(intent.try_into()?)?.into())
    }

    /// The current projection, without applying an intent.
    pub fn view(&self) -> ChatView {
        self.lock().view().into()
    }

    /// This identity's principal, 32 bytes.
    pub fn principal(&self) -> Vec<u8> {
        self.lock().principal().as_bytes().to_vec()
    }
}

impl ChatSession {
    /// The lock, with the poisoning case named.
    ///
    /// A poisoned mutex means a previous call panicked while holding the
    /// session. The state behind it is not known to be consistent, so this
    /// takes the guard anyway rather than pretending otherwise: the panic
    /// itself is the defect to fix, and hiding it behind a second failure mode
    /// makes it harder to find. `Session`'s own methods do not panic — they
    /// return `Result` — so reaching this is a bug, not a condition.
    fn lock(&self) -> std::sync::MutexGuard<'_, session::Session> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl TryFrom<Intent> for chat_core::model::Intent {
    type Error = FfiError;

    fn try_from(i: Intent) -> Result<Self, FfiError> {
        Ok(match i {
            Intent::SelectGroup { group } => {
                chat_core::model::Intent::SelectGroup(group_id(&group)?)
            }
            Intent::TypeChar { c } => {
                // Characters, not bytes and not UTF-16 units: an emoji is one
                // keypress on the phone and has to be one character here.
                let mut chars = c.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => chat_core::model::Intent::TypeChar(ch),
                    _ => {
                        let count = c.chars().count() as u32;
                        return Err(FfiError::NotOneCharacter {
                            count,
                            reason: format!("TypeChar takes exactly one character, got {count}"),
                        });
                    }
                }
            }
            Intent::Backspace => chat_core::model::Intent::Backspace,
            Intent::SendMessage => chat_core::model::Intent::SendMessage,
            // The shell asks for a refresh; the session reads the store and
            // hands the pond the snapshot. A `Snapshot` the FOREIGN side could
            // fill in would let a shell tell the pond something the store never
            // said, which is a door worth not opening.
            Intent::Refresh => chat_core::model::Intent::Refresh(Default::default()),
        })
    }
}

/// A 32-byte group id from the foreign side, or a refusal naming the length.
fn group_id(bytes: &[u8]) -> Result<GroupId, FfiError> {
    let raw: [u8; 32] = bytes.try_into().map_err(|_| FfiError::BadGroupIdLength {
        got: bytes.len() as u32,
        reason: format!("a group id is 32 bytes, got {}", bytes.len()),
    })?;
    Ok(GroupId::new(raw))
}
