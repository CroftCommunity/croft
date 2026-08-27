//! Local truth: facts this device holds that are never folded and never sent.
//!
//! Everything else in this crate is a consequence of a signed assertion. What
//! lives here is not: it is a local convenience, and keeping it in a module of
//! its own is how that stays visible. The moment one of these becomes a shared
//! fact it moves out of here and grows a wire format, an author, and a
//! signature.

use crate::tables::{Db, DbError, LOCAL_GROUP_TITLES};
use social_tree_core::model::GroupId;

/// This device's name for `group`, or `None` if it has never named it.
///
/// `None` and `Some("")` are different answers: unnamed, versus named the empty
/// string. The projection falls back to a short id only for the first.
pub fn group_title(db: &Db, group: &GroupId) -> Result<Option<String>, DbError> {
    let txn = db.inner().begin_read()?;
    let table = txn.open_table(LOCAL_GROUP_TITLES)?;
    match table.get(&group.as_bytes()[..])? {
        None => Ok(None),
        Some(v) => {
            // Refused, not lossily decoded: `from_utf8_lossy` would hand the UI
            // a name full of replacement characters and report success.
            let title = std::str::from_utf8(v.value()).map_err(|e| {
                DbError::Deserialize(format!("group title is not valid UTF-8: {e}"))
            })?;
            Ok(Some(title.to_owned()))
        }
    }
}

/// Name `group` on this device, replacing any previous name.
pub fn put_group_title(db: &Db, group: &GroupId, title: &str) -> Result<(), DbError> {
    let txn = db.inner().begin_write()?;
    {
        let mut table = txn.open_table(LOCAL_GROUP_TITLES)?;
        table.insert(&group.as_bytes()[..], title.as_bytes())?;
    }
    txn.commit()?;
    Ok(())
}
