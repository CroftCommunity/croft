//! Queries over the folded tables.
//!
//! What a shell needs to ask the store, and nothing more. These return
//! **substrate types** — ids, principals, roles, message bodies — never view
//! models. That line is deliberate and load-bearing: assembling views inside
//! the store is exactly what made the corpus's `surface.rs` impossible to
//! split, because every read there both opened a table and built a UI struct,
//! so there was no seam to cut along. The pond decides what a view looks like;
//! this module only says what is true.
//!
//! Nothing here folds or mutates. Every function opens a read transaction,
//! answers, and drops it.

use social_tree_core::model::{
    decode_message_payload, GroupId, GroupState, Hash, KindTag, PrincipalId, Role, TypedId,
};

use crate::fold_derived::FoldError;
use crate::tables::{
    encode_gov_log_key, Db, EdgeMeta, EdgeType, AUTH_ASSERTIONS, AUTH_GOV_LOG, IDX_EDGES_OUT,
    STATE_GROUP,
};

/// One message as the store holds it.
///
/// Carries its author and its lamport, not just its body: a timeline line with
/// no attribution and no order is a rendering, not a fact, and the shell cannot
/// reconstruct either of them afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    /// Lamport stamp — the total order the timeline is sorted by.
    pub lamport: u64,
    /// Who wrote it.
    pub author: PrincipalId,
    /// The body.
    pub body: String,
}

/// The groups `principal` is a member of, in id order.
///
/// Reads the MEMBER_OF edges out of the principal's node. A tombstoned edge (a
/// membership that was removed) is skipped — `EdgeMeta::present` is the fact,
/// and the row staying behind is how removal keeps its history.
pub fn groups_for_principal(db: &Db, principal: &PrincipalId) -> Result<Vec<GroupId>, FoldError> {
    let principal_node = TypedId::new(KindTag::Principal, Hash::new(*principal.as_bytes()));
    let mut out = Vec::new();
    for (target, meta) in edges_out(db, &principal_node, EdgeType::MemberOf)? {
        if !meta.present {
            continue;
        }
        out.push(GroupId::new(*target.hash().as_bytes()));
    }
    out.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    Ok(out)
}

/// The messages in `group`, oldest first.
///
/// Ordering is by lamport and then by content hash, never by the order redb
/// happens to return the edges: edge keys are ordered by target id, which is a
/// content hash and therefore has nothing to do with when a message was
/// written. The hash tiebreak keeps the order total and identical on every
/// device, which is the same discipline the fold's own comparator follows.
pub fn messages_in_group(db: &Db, group: &GroupId) -> Result<Vec<StoredMessage>, FoldError> {
    let group_node = TypedId::new(KindTag::Group, Hash::new(*group.as_bytes()));

    let read_txn = db
        .inner()
        .begin_read()
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let assertions = read_txn
        .open_table(AUTH_ASSERTIONS)
        .map_err(|e| FoldError::StorageError(e.to_string()))?;

    let mut rows: Vec<(u64, [u8; 32], StoredMessage)> = Vec::new();
    for (target, meta) in edges_out(db, &group_node, EdgeType::References)? {
        // The group references artifacts of several kinds; only chat ones are
        // timeline lines.
        if target.kind() != KindTag::ArtifactChat || !meta.present {
            continue;
        }
        // `since_assertion` is the envelope that created the edge — the message
        // itself. The node id is a hash OF that envelope's hash, so it cannot
        // be walked back; the edge is the only route to the body.
        let raw = match assertions
            .get(meta.since_assertion.as_bytes().as_ref())
            .map_err(|e| FoldError::StorageError(e.to_string()))?
        {
            Some(v) => v.value().to_vec(),
            // An edge whose assertion is missing is a torn store, not an empty
            // timeline. Say so rather than quietly rendering fewer messages.
            None => {
                return Err(FoldError::StorageError(format!(
                    "message edge points at assertion {:?}, which is not in the store",
                    meta.since_assertion
                )))
            }
        };
        // The stored record is `u8 version || canonical_bytes_with_sig`.
        if raw.is_empty() {
            return Err(FoldError::StorageError(
                "stored assertion record is empty".to_string(),
            ));
        }
        let env = social_tree_core::wire::decode_envelope_from_canonical(&raw[1..])
            .map_err(FoldError::MalformedEnvelope)?;
        let (body, _reply_to, _channel) =
            decode_message_payload(&env.payload).ok_or_else(|| {
                FoldError::MalformedEnvelope("message payload did not decode".to_string())
            })?;
        rows.push((
            env.lamport,
            *meta.since_assertion.as_bytes(),
            StoredMessage {
                lamport: env.lamport,
                author: env.author_principal,
                body,
            },
        ));
    }

    rows.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    Ok(rows.into_iter().map(|(_, _, m)| m).collect())
}

/// The folded membership of `group`: who is seated, in what role, since when.
///
/// A group with no folded state yet reads as an empty roster rather than an
/// error — "nobody is seated" is a true answer for a group this device has
/// never folded, and an error would make an ordinary startup look like a fault.
pub fn members_of_group(
    db: &Db,
    group: &GroupId,
) -> Result<Vec<(PrincipalId, Role, u64)>, FoldError> {
    Ok(group_state(db, group)?
        .map(|s| s.members)
        .unwrap_or_default())
}

/// The full folded [`GroupState`] for `group`, if this device has one.
pub fn group_state(db: &Db, group: &GroupId) -> Result<Option<GroupState>, FoldError> {
    let read_txn = db
        .inner()
        .begin_read()
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let table = read_txn
        .open_table(STATE_GROUP)
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    match table
        .get(group.as_bytes().as_ref())
        .map_err(|e| FoldError::StorageError(e.to_string()))?
    {
        None => Ok(None),
        Some(v) => Ok(Some(GroupState::from_bytes(v.value())?)),
    }
}

/// Every edge of `edge_type` out of `source`, as (target, meta).
///
/// The key layout is `source(33) || edge_type(2) || target(33)`, so the scan is
/// a bounded range over a 35-byte prefix rather than a filter over the whole
/// table — which is the reason the key is laid out that way.
fn edges_out(
    db: &Db,
    source: &TypedId,
    edge_type: EdgeType,
) -> Result<Vec<(TypedId, EdgeMeta)>, FoldError> {
    // The bounds are the same key with the target end swept from all-zero to
    // all-one, which is every possible target under this (source, type) prefix.
    let mut low = [0u8; 68];
    low[..33].copy_from_slice(source.as_bytes());
    low[33..35].copy_from_slice(&edge_type.to_be_bytes());
    let mut high = low;
    high[35..].fill(0xff);

    let read_txn = db
        .inner()
        .begin_read()
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let table = read_txn
        .open_table(IDX_EDGES_OUT)
        .map_err(|e| FoldError::StorageError(e.to_string()))?;

    let mut out = Vec::new();
    for item in table
        .range(low.as_slice()..=high.as_slice())
        .map_err(|e| FoldError::StorageError(e.to_string()))?
    {
        let (k, v) = item.map_err(|e| FoldError::StorageError(e.to_string()))?;
        let (_, _, target) = crate::tables::decode_edge_out_key(k.value())
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        let meta = EdgeMeta::from_bytes(v.value())?;
        out.push((target, meta));
    }
    Ok(out)
}

/// The group's governance record: every genesis and membership assertion that
/// seats it, as replayable envelopes in sequence order.
///
/// **What this is for (P7 S2).** A device seated by an MLS Welcome holds key
/// material and nothing else — it does not know the group exists, who is in it,
/// or that the inviter is entitled to act for the principal their assertions
/// name. Handing it this record is what closes that gap, and the joining device
/// folds these exactly as if it had received them itself.
///
/// **Envelopes, not derived state, and that is the whole design.** The folded
/// tables carry no signatures, so a joiner given derived state would have to
/// take the sender's word for all of it. Given the envelopes it re-verifies
/// every one against its own fold — the sender is a courier, not an authority.
///
/// Governance only: messages are not in the governance log and do not belong in
/// an invite. They arrive over the wire, sealed, as they are sent.
///
/// A group this device has never folded reads as an EMPTY record rather than an
/// error, the same choice [`members_of_group`] makes and for the same reason: it
/// is a true answer, and an error would make an ordinary startup look like a
/// fault.
///
/// # Errors
/// [`FoldError::StorageError`] when a log entry points at an assertion the store
/// does not hold — a torn store, which must be said out loud rather than
/// silently yielding a shorter record that would strand the joiner.
pub fn governance_record(db: &Db, group: &GroupId) -> Result<Vec<Vec<u8>>, FoldError> {
    let read_txn = db
        .inner()
        .begin_read()
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let log = read_txn
        .open_table(AUTH_GOV_LOG)
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let assertions = read_txn
        .open_table(AUTH_ASSERTIONS)
        .map_err(|e| FoldError::StorageError(e.to_string()))?;

    // Prefix scan over GroupId(32) || gov_seq(8, big-endian). Big-endian is
    // what makes lexicographic key order equal numeric sequence order, so this
    // range is already in replay order and must not be re-sorted.
    let start = encode_gov_log_key(group, 0);
    let end = encode_gov_log_key(group, u64::MAX);

    let mut out = Vec::new();
    for entry in log
        .range(start.as_slice()..=end.as_slice())
        .map_err(|e| FoldError::StorageError(e.to_string()))?
    {
        let (_key, value) = entry.map_err(|e| FoldError::StorageError(e.to_string()))?;
        let hash = value.value().to_vec();
        let raw = assertions
            .get(hash.as_slice())
            .map_err(|e| FoldError::StorageError(e.to_string()))?
            .ok_or_else(|| {
                FoldError::StorageError(format!(
                    "governance log points at assertion {hash:?}, which is not in the store"
                ))
            })?
            .value()
            .to_vec();
        if raw.is_empty() {
            return Err(FoldError::StorageError(
                "stored assertion record is empty".to_string(),
            ));
        }
        // Strip the record's own version byte; what remains is
        // `canonical_bytes_with_sig`, which is what a joiner replays.
        out.push(raw[1..].to_vec());
    }
    Ok(out)
}
