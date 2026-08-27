//! What can go wrong, in the session's own words.
//!
//! Every variant crosses the FFI as a **typed** foreign error carrying the
//! detail that makes it actionable — a length, an id, the store's own message.
//! That is not decoration. The failure mode this guards against is specific and
//! common: an error becomes a null on the way across, the null becomes an empty
//! list, and the screen shows a calm empty conversation where the truth was
//! "this did not work". A caller who receives `NoSuchGroup { group }` cannot
//! accidentally render it as "no messages".

use social_tree_core::model::GroupId;
use store_redb::fold_derived::FoldError;
use store_redb::tables::DbError;

/// A refusal from the session.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// A signing key was not 32 bytes.
    #[error("a signing key is 32 bytes, got {got}")]
    BadKeyLength {
        /// The length actually supplied.
        got: usize,
    },

    /// A send was attempted with no group selected.
    ///
    /// `chat_core::update` drops this intent rather than failing, which is
    /// correct for a total reducer and wrong for a boundary: a dropped intent
    /// is indistinguishable from a successful one once it has crossed.
    #[error("no group is selected, so there is nowhere to send")]
    NoGroupSelected,

    /// A send was attempted with nothing to send.
    #[error("the draft is empty, so there is nothing to send")]
    EmptyDraft,

    /// A group was named that this session is not a member of.
    #[error("no such group in this session's membership")]
    NoSuchGroup {
        /// The group that was asked for.
        group: GroupId,
    },

    /// The store refused. Carries its own words rather than a generic failure,
    /// because "the store said no" and "the store said no because the parent
    /// directory does not exist" lead to different next actions.
    #[error("storage: {reason}")]
    Storage {
        /// What the store said.
        reason: String,
    },

    /// The fold refused an assertion this session authored — a governance or
    /// wire-level refusal, not an I/O one.
    #[error("the fold refused: {reason}")]
    Refused {
        /// The fold's own words.
        reason: String,
    },
}

impl From<DbError> for SessionError {
    fn from(e: DbError) -> Self {
        SessionError::Storage {
            reason: e.to_string(),
        }
    }
}

impl From<FoldError> for SessionError {
    fn from(e: FoldError) -> Self {
        // The fold's storage failures and its governance refusals reach a
        // caller through different doors: one is "the disk is wrong", the
        // other is "the record says no". Flattening them would make an
        // unwritable directory look like a rejected message.
        match e {
            FoldError::StorageError(reason) => SessionError::Storage { reason },
            other => SessionError::Refused {
                reason: other.to_string(),
            },
        }
    }
}
