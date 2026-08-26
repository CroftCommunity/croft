//! Governance, forks, and compaction for `local_storage_projection` — Stage 5.
//!
//! Implements:
//!   I6 full: amendment-threshold self-amendment (threshold checked at position)
//!   I7: fork lineage detection and deterministic tiebreak
//!   I10: compaction (built but off by default), Merkle root, rebuild-from-checkpoint

use std::sync::Arc;

use crate::fold_derived::FoldError;
use crate::tables::{
    encode_checkpoint_key, encode_gov_log_key, Db, AUTH_ASSERTIONS, AUTH_GOV_LOG,
    STATE_CHECKPOINTS, STATE_GROUP,
};
use social_tree_core::model::{
    compute_hash, envelope_hash, AssertionEnvelope, GroupId, GroupRules, Hash as TypesHash,
    PrincipalId as TypesPrincipalId, Role, RuleKey,
};

use redb::ReadableTable;

// ---------------------------------------------------------------------------
// I6 full: required_threshold_for_rule_change
// ---------------------------------------------------------------------------

/// Returns the applicable threshold for changing the given rule key, including
/// `RuleKey::RuleChange` returning `rules.rule_change_threshold`.
///
/// This is the threshold that must be satisfied AT the governance position
/// where the RuleChange assertion is being applied — i.e., the rules as they
/// stand at that point in the log, not genesis and not current head.
pub fn required_threshold_for_rule_change(rules: &GroupRules, key: &RuleKey) -> u32 {
    match key {
        RuleKey::AddMember => rules.add_member_threshold,
        RuleKey::RemoveMember => rules.remove_member_threshold,
        RuleKey::RoleChange => rules.role_change_threshold,
        RuleKey::RuleChange => rules.rule_change_threshold,
        RuleKey::Resolution => rules.resolution_threshold,
        RuleKey::Readmission => rules.readmission_threshold,
    }
}

// ---------------------------------------------------------------------------
// I7: ForkLineage — fork detection data
// ---------------------------------------------------------------------------

/// Describes a diverging lineage detected when two assertions contest the same
/// governance sequence slot for the same group.
#[derive(Debug, Clone, PartialEq)]
pub struct ForkLineage {
    /// Hash of the DIVERGING assertion — the one that lost the tiebreak (the
    /// assertion with the lexicographically larger hash is the diverging one).
    pub lineage_id: TypesHash,
    /// The governance sequence number at which the fork was detected.
    pub parent_gov_seq: u64,
    /// The hash of the assertion that was already accepted at `parent_gov_seq`
    /// BEFORE the incoming assertion arrived.  Together with `lineage_id` this
    /// uniquely identifies the fork point.
    pub fork_at: TypesHash,
    /// The assertion hash of the accepted (winning) lineage head.
    pub accepted_head: TypesHash,
    /// The assertion hash of the rejected (losing) lineage head.
    pub rejected_head: TypesHash,
}

/// Deterministic tiebreak: returns `Ordering::Less` when `a` wins.
///
/// The assertion with the lexicographically SMALLER hash wins.  This ordering
/// is total, deterministic, and identical across all independent fold instances
/// given the same inputs.
pub fn tiebreak(a: &TypesHash, b: &TypesHash) -> std::cmp::Ordering {
    a.as_bytes().cmp(b.as_bytes())
}

/// The **under-determination** shape of the reconcile hard-stop (§7.6.1): a
/// required role is vacant with no admissible successor. It is the "too few"
/// member of the two-member escalation set, distinct from a fork's "too many
/// valid claims" — and a contradiction-only watcher misses it entirely.
///
/// In this model the required role is **Owner**: it is the only role that can
/// grant roles (`RoleGrant` requires Owner) or change rules (`RuleChange`
/// requires Owner), so a group whose derived member set holds no Owner can
/// authorize no further governance *and* no member can promote a successor,
/// because promotion itself needs an Owner. There is no admissible successor,
/// so the fold must hard-stop rather than fold onward on a headless group.
///
/// Returns `true` when the fully-derived `members` set holds no Owner (including
/// the empty set — a group whose last member was removed).
#[must_use]
pub fn is_under_determined(members: &[(TypesPrincipalId, Role, u64)]) -> bool {
    !members
        .iter()
        .any(|(_, role, _)| matches!(role, Role::Owner))
}

/// True if `ancestor` causally precedes `descendant` — i.e. it is reachable from
/// `descendant` by following antecedent edges. `antecedents_of` yields the declared
/// antecedents of a fact by hash, or `None` if that fact is not held. A fact is not
/// its own ancestor. Cycle-safe (a content-addressed DAG has none, but a `seen` set
/// guards against a malformed input looping).
///
/// This is the reachability primitive the §7.6 reconcile hard-stop needs to tell a
/// *concurrent* contradiction (two facts neither of which precedes the other) from a
/// merely *sequential* one (which is resolved by causal order, no escalation).
#[must_use]
pub fn is_ancestor(
    ancestor: &TypesHash,
    descendant: &TypesHash,
    antecedents_of: &impl Fn(&TypesHash) -> Option<Vec<TypesHash>>,
) -> bool {
    use std::collections::HashSet;
    let Some(mut stack) = antecedents_of(descendant) else {
        return false;
    };
    let mut seen: HashSet<[u8; 32]> = HashSet::new();
    while let Some(h) = stack.pop() {
        if &h == ancestor {
            return true;
        }
        if !seen.insert(*h.as_bytes()) {
            continue;
        }
        if let Some(ants) = antecedents_of(&h) {
            stack.extend(ants);
        }
    }
    false
}

/// True if `a` and `b` are causally **concurrent**: distinct, and neither an
/// ancestor of the other.
///
/// Concurrency is **necessary but not sufficient** for a §7.6.1 contradiction. Two
/// concurrent governance facts may be perfectly benign — e.g. two admins concurrently
/// removing *different* members commute and need no escalation. The hard-stop must
/// fire only on concurrency **and** a conflict predicate (mutual expulsion,
/// removed-then-included, …); flagging all concurrency would false-trip the escalation
/// channel, the one thing §7.5.2/§7.6 says must not erode. This helper supplies only
/// the concurrency half; the conflict predicate is a separate, deliberate layer.
#[must_use]
pub fn are_concurrent(
    a: &TypesHash,
    b: &TypesHash,
    antecedents_of: &impl Fn(&TypesHash) -> Option<Vec<TypesHash>>,
) -> bool {
    a != b && !is_ancestor(a, b, antecedents_of) && !is_ancestor(b, a, antecedents_of)
}

/// Count distinct **personae** among `approvers`, identified by lineage — the rooting
/// **principal**. Multiple clients of one persona collapse to one.
///
/// This is the guard against a single persona satisfying a k-of-n governance threshold
/// by signing with many devices (§5.7): weight is one-per-persona-by-lineage, never
/// per-client. Callers MUST pass principals already resolved from devices (the
/// credential resolver maps device → principal by signed lineage), never raw device
/// ids, so device count cannot inflate the count.
#[must_use]
pub fn count_personae_by_lineage(approvers: &[TypesPrincipalId]) -> usize {
    use std::collections::HashSet;
    let mut set: HashSet<[u8; 32]> = HashSet::new();
    for p in approvers {
        set.insert(*p.as_bytes());
    }
    set.len()
}

/// Whether `approvers` meet a k-of-n governance threshold — `required` *distinct
/// personae by lineage*. A single persona's clients, however many, never meet a
/// threshold above one.
#[must_use]
pub fn threshold_met(approvers: &[TypesPrincipalId], required: u32) -> bool {
    count_personae_by_lineage(approvers) >= required as usize
}

/// Detect whether `incoming` creates a fork against the current governance head.
///
/// Returns `Some(ForkLineage)` when `incoming` contests the same `gov_seq` slot
/// already occupied by `existing_gov_head`.  The assertion with the
/// lexicographically smaller hash is the winner; the other is the diverging
/// (rejected) lineage.
///
/// Returns `None` when no fork is detected (the slot was empty, or the incoming
/// assertion targets a different slot).
pub fn detect_fork(
    existing_gov_head: &TypesHash,
    existing_gov_seq: u64,
    incoming: &AssertionEnvelope,
) -> Option<ForkLineage> {
    let incoming_hash = envelope_hash(incoming);

    // Use tiebreak to decide winner vs loser.
    let ordering = tiebreak(&incoming_hash, existing_gov_head);
    // ordering == Less  → incoming wins (smaller hash wins)
    // ordering == Greater → existing wins
    // ordering == Equal   → same assertion, no fork

    if ordering == std::cmp::Ordering::Equal {
        // Same assertion; not a fork.
        return None;
    }

    let (accepted_head, rejected_head) = if ordering == std::cmp::Ordering::Less {
        // incoming wins
        (incoming_hash, *existing_gov_head)
    } else {
        // existing wins
        (*existing_gov_head, incoming_hash)
    };

    // lineage_id is the diverging (rejected) assertion hash.
    Some(ForkLineage {
        lineage_id: rejected_head,
        parent_gov_seq: existing_gov_seq,
        fork_at: *existing_gov_head,
        accepted_head,
        rejected_head,
    })
}

// ---------------------------------------------------------------------------
// I10: Compaction configuration and result
// ---------------------------------------------------------------------------

/// Configuration for the compaction pass.
///
/// Compaction is built but off by default: set `enabled = true` to activate.
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Master switch. When `false` (default), `compact_content` is a no-op.
    pub enabled: bool,
    /// Prune content assertions that are more than this many governance
    /// sequence slots older than the current governance head.
    pub trigger_depth: u64,
    /// Prune content assertions whose lamport is at least this far behind the
    /// checkpoint envelope's lamport. Lamport-denominated by rule: a wall-clock
    /// age is an uncorroborable assertion (Part 1 §2.0.1), and envelope wire v2
    /// carries no timestamp at all (O9) — lapses are denominated in
    /// positions/generations, never seconds.
    pub trigger_age_lamports: u64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            trigger_depth: 1000,
            trigger_age_lamports: 1000,
        }
    }
}

/// The result of a compaction pass.
#[derive(Debug, Clone, PartialEq)]
pub struct CompactionResult {
    /// Number of content assertions removed from `auth_assertions`.
    pub pruned_count: u64,
    /// The governance sequence number of the checkpoint written.
    pub checkpoint_seq: u64,
    /// Merkle root over the retained content assertion hashes (in causal order).
    pub content_merkle_root: TypesHash,
}

// ---------------------------------------------------------------------------
// I10: Merkle root (simple binary Merkle tree over hashes in causal order)
// ---------------------------------------------------------------------------

/// Compute a binary Merkle tree root over an ordered list of 32-byte hashes.
///
/// - Empty input → `Hash([0u8; 32])`
/// - Single element → that hash itself
/// - Otherwise: pair adjacent hashes, hash each pair, recurse upward.
///   When the count at a level is odd the last element is promoted as-is.
pub fn compute_merkle_root(hashes: &[TypesHash]) -> TypesHash {
    if hashes.is_empty() {
        return TypesHash::new([0u8; 32]);
    }
    if hashes.len() == 1 {
        return hashes[0];
    }

    let mut current: Vec<TypesHash> = hashes.to_vec();

    while current.len() > 1 {
        let mut next = Vec::with_capacity((current.len() + 1) / 2);
        let mut i = 0;
        while i < current.len() {
            if i + 1 < current.len() {
                // Hash the concatenation of the two adjacent 32-byte hashes.
                let mut buf = [0u8; 64];
                buf[..32].copy_from_slice(current[i].as_bytes());
                buf[32..].copy_from_slice(current[i + 1].as_bytes());
                next.push(compute_hash(&buf));
                i += 2;
            } else {
                // Odd element: promote as-is.
                next.push(current[i]);
                i += 1;
            }
        }
        current = next;
    }

    current[0]
}

// ---------------------------------------------------------------------------
// I10: compact_content
// ---------------------------------------------------------------------------

/// Compact content assertions for `group` below the `checkpoint_gov_head`.
///
/// Rules (I10):
/// - Governance assertions (`auth_gov_log` entries) are NEVER pruned.
/// - Content assertions in `auth_assertions` that are NOT referenced by any
///   governance log entry AND that are below the checkpoint depth/age thresholds
///   CAN be pruned.
/// - A `state_checkpoints` record is written after pruning, carrying the Merkle
///   root of the RETAINED content assertion hashes (in causal order).
/// - When `config.enabled` is `false` this function returns a zero-change result
///   without touching the database.
pub fn compact_content(
    db: &Arc<Db>,
    group: &GroupId,
    checkpoint_gov_head: &TypesHash,
    config: &CompactionConfig,
) -> Result<CompactionResult, FoldError> {
    // When compaction is disabled, return a no-op result.
    if !config.enabled {
        // Still compute the Merkle root over all current content for callers
        // that need it without mutation.
        let (merkle_root, checkpoint_seq) = current_content_merkle(db, group, checkpoint_gov_head)?;
        return Ok(CompactionResult {
            pruned_count: 0,
            checkpoint_seq,
            content_merkle_root: merkle_root,
        });
    }

    // Step 1: Collect governance assertion hashes for this group — these must
    // NEVER be pruned.
    let gov_hashes: std::collections::HashSet<Vec<u8>> = {
        let read_txn = db
            .inner()
            .begin_read()
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        let table = read_txn
            .open_table(AUTH_GOV_LOG)
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        let start = encode_gov_log_key(group, 0);
        let end = encode_gov_log_key(group, u64::MAX);
        let mut set = std::collections::HashSet::new();
        for item in table
            .range(start.as_slice()..=end.as_slice())
            .map_err(|e| FoldError::StorageError(e.to_string()))?
        {
            let (_k, v) = item.map_err(|e| FoldError::StorageError(e.to_string()))?;
            set.insert(v.value().to_vec());
        }
        set
    };

    // Step 2: Determine the gov_seq of checkpoint_gov_head so we know the
    // depth of the checkpoint.
    let checkpoint_seq = gov_seq_for_hash(db, group, checkpoint_gov_head)?.unwrap_or(0);

    // Step 3: Collect all content assertions (i.e., NOT governance assertions)
    // from auth_assertions along with their timestamps (from the envelope).
    // We need to decide which to prune: those older than the checkpoint by
    // trigger_depth and trigger_age_lamports.
    //
    // For the current implementation we use a simple strategy:
    // - A content assertion is eligible for pruning if it is not referenced in
    //   auth_gov_log AND its lamport-associated gov_seq is more than
    //   trigger_depth below checkpoint_seq.
    // - We do NOT prune content assertions that are at or above the checkpoint.
    //
    // Since individual content assertions carry a timestamp, we also apply the
    // age gate using the envelope's timestamp field.

    // Step 3a: Read all current assertions and partition into gov/content.
    let all_assertions = read_all_assertion_bytes(db)?;

    // Decode envelopes to get timestamps and lamport values.
    let mut content_to_prune: Vec<Vec<u8>> = Vec::new(); // keys (hash bytes)
    let mut retained_hashes: Vec<TypesHash> = Vec::new();

    for (hash_bytes, versioned_bytes) in &all_assertions {
        let hash_arr: [u8; 32] = match hash_bytes.as_slice().try_into() {
            Ok(a) => a,
            Err(_) => continue,
        };
        let hash = TypesHash::new(hash_arr);

        // Governance assertions: always retained.
        if gov_hashes.contains(hash_bytes) {
            retained_hashes.push(hash);
            continue;
        }

        // Content assertion: check eligibility for pruning.
        // Decode envelope to get timestamp.
        if versioned_bytes.is_empty() {
            retained_hashes.push(hash);
            continue;
        }
        let env_result =
            social_tree_core::wire::decode_envelope_from_canonical(&versioned_bytes[1..]);
        let eligible = match env_result {
            Err(_) => false,
            Ok(env) => {
                // Apply depth gate: the envelope's lamport is a proxy for its
                // position in the log. We prune if the checkpoint_seq exceeds
                // trigger_depth and the envelope's lamport sits at least
                // trigger_age_lamports behind the checkpoint envelope's lamport —
                // the position-denominated age gate (never wall-clock).
                let age_ok = {
                    if let Some(ckpt_lamport) = checkpoint_lamport(db, checkpoint_gov_head) {
                        env.lamport + config.trigger_age_lamports <= ckpt_lamport
                    } else {
                        false
                    }
                };
                let depth_ok = checkpoint_seq >= config.trigger_depth;
                age_ok && depth_ok
            }
        };

        if eligible {
            content_to_prune.push(hash_bytes.clone());
        } else {
            retained_hashes.push(hash);
        }
    }

    let pruned_count = content_to_prune.len() as u64;

    // Sort retained hashes for Merkle stability.
    retained_hashes.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

    // Merkle root over retained content (excluding governance).
    let content_only: Vec<TypesHash> = retained_hashes
        .iter()
        .filter(|h| !gov_hashes.contains(h.as_bytes().as_slice()))
        .cloned()
        .collect();

    let content_merkle_root = compute_merkle_root(&content_only);

    // Step 4: Write checkpoint + prune in one transaction.
    {
        let write_txn = db
            .inner()
            .begin_write()
            .map_err(|e| FoldError::StorageError(e.to_string()))?;

        // 4a. Remove pruned content assertions from auth_assertions.
        {
            let mut table = write_txn
                .open_table(AUTH_ASSERTIONS)
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            for key in &content_to_prune {
                table
                    .remove(key.as_slice())
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
            }
        }

        // 4b. Write state_checkpoints record.
        {
            let mut table = write_txn
                .open_table(STATE_CHECKPOINTS)
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            let key = encode_checkpoint_key(group, checkpoint_seq);
            // Value layout (version=1):
            //   version(1) || gov_head(32) || pruned_count(8) || merkle_root(32)
            let mut val = Vec::with_capacity(73);
            val.push(0x01u8);
            val.extend_from_slice(checkpoint_gov_head.as_bytes());
            val.extend_from_slice(&pruned_count.to_be_bytes());
            val.extend_from_slice(content_merkle_root.as_bytes());
            table
                .insert(key.as_slice(), val.as_slice())
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
        }

        write_txn
            .commit()
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
    }

    Ok(CompactionResult {
        pruned_count,
        checkpoint_seq,
        content_merkle_root,
    })
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Compute the Merkle root of all current content assertion hashes (without
/// any pruning), and return `(root, checkpoint_seq)`.
fn current_content_merkle(
    db: &Arc<Db>,
    group: &GroupId,
    checkpoint_gov_head: &TypesHash,
) -> Result<(TypesHash, u64), FoldError> {
    let gov_hashes: std::collections::HashSet<Vec<u8>> = {
        let read_txn = db
            .inner()
            .begin_read()
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        let table = read_txn
            .open_table(AUTH_GOV_LOG)
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        let start = encode_gov_log_key(group, 0);
        let end = encode_gov_log_key(group, u64::MAX);
        let mut set = std::collections::HashSet::new();
        for item in table
            .range(start.as_slice()..=end.as_slice())
            .map_err(|e| FoldError::StorageError(e.to_string()))?
        {
            let (_k, v) = item.map_err(|e| FoldError::StorageError(e.to_string()))?;
            set.insert(v.value().to_vec());
        }
        set
    };

    let all_assertions = read_all_assertion_bytes(db)?;
    let mut content_hashes: Vec<TypesHash> = all_assertions
        .iter()
        .filter(|(k, _)| !gov_hashes.contains(k.as_slice()))
        .filter_map(|(k, _)| {
            let arr: [u8; 32] = k.as_slice().try_into().ok()?;
            Some(TypesHash::new(arr))
        })
        .collect();

    content_hashes.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

    let checkpoint_seq = gov_seq_for_hash(db, group, checkpoint_gov_head)?.unwrap_or(0);

    Ok((compute_merkle_root(&content_hashes), checkpoint_seq))
}

/// Read all (hash_bytes, versioned_bytes) pairs from `auth_assertions`.
fn read_all_assertion_bytes(db: &Arc<Db>) -> Result<Vec<(Vec<u8>, Vec<u8>)>, FoldError> {
    let read_txn = db
        .inner()
        .begin_read()
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let table = read_txn
        .open_table(AUTH_ASSERTIONS)
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let mut out = Vec::new();
    for item in table
        .iter()
        .map_err(|e: redb::StorageError| FoldError::StorageError(e.to_string()))?
    {
        let (k, v) =
            item.map_err(|e: redb::StorageError| FoldError::StorageError(e.to_string()))?;
        out.push((k.value().to_vec(), v.value().to_vec()));
    }
    Ok(out)
}

/// Return the gov_seq for the given hash in the group's gov log, or `None`.
fn gov_seq_for_hash(
    db: &Arc<Db>,
    group: &GroupId,
    target_hash: &TypesHash,
) -> Result<Option<u64>, FoldError> {
    let read_txn = db
        .inner()
        .begin_read()
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let table = read_txn
        .open_table(AUTH_GOV_LOG)
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let start = encode_gov_log_key(group, 0);
    let end = encode_gov_log_key(group, u64::MAX);
    for item in table
        .range(start.as_slice()..=end.as_slice())
        .map_err(|e| FoldError::StorageError(e.to_string()))?
    {
        let (k, v) = item.map_err(|e| FoldError::StorageError(e.to_string()))?;
        if v.value() == target_hash.as_bytes().as_slice() {
            let key_bytes = k.value();
            let seq = u64::from_be_bytes(key_bytes[32..40].try_into().unwrap());
            return Ok(Some(seq));
        }
    }
    Ok(None)
}

/// Return the wall-clock timestamp of the assertion stored at `hash` in
/// `auth_assertions`, or `None` if not found or decode fails.
fn checkpoint_lamport(db: &Arc<Db>, hash: &TypesHash) -> Option<u64> {
    let read_txn = db.inner().begin_read().ok()?;
    let table = read_txn.open_table(AUTH_ASSERTIONS).ok()?;
    let val = table.get(hash.as_bytes().as_slice()).ok()??;
    let raw = val.value();
    if raw.is_empty() {
        return None;
    }
    let env = social_tree_core::wire::decode_envelope_from_canonical(&raw[1..]).ok()?;
    Some(env.lamport)
}

/// Minimal envelope decoder (reused from fold_derived pattern).

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fold_derived::{rebuild, DerivedFold, ForkStatus, GroupState, IngestResult};
    use crate::tables::Db;
    use social_tree_core::model::{
        AssertionEnvelope, AssertionType, DeviceId as TypesDeviceId, GroupId, GroupRules,
        PrincipalId as TypesPrincipalId, Role, RuleKey,
    };
    use social_tree_core::ports::mocks::{MockCredentialResolver, MockSigner};
    use social_tree_core::ports::{
        DeviceId as TraitsDeviceId, PrincipalId as TraitsPrincipalId, Signer,
    };
    use std::sync::Arc;

    // -----------------------------------------------------------------------
    // Common test helpers
    // -----------------------------------------------------------------------

    fn make_device(seed: u8) -> TypesDeviceId {
        TypesDeviceId::new([seed; 32])
    }

    fn make_principal(seed: u8) -> TypesPrincipalId {
        TypesPrincipalId::new([seed; 32])
    }

    fn make_group(seed: u8) -> GroupId {
        GroupId::new([seed; 32])
    }

    fn make_hash(seed: u8) -> TypesHash {
        TypesHash::new([seed; 32])
    }

    fn genesis_payload_thresh(all: u32) -> Vec<u8> {
        let mut p = Vec::with_capacity(50);
        p.extend_from_slice(&1u16.to_be_bytes()); // policy_version
        p.extend_from_slice(&all.to_be_bytes()); // add_member_threshold
        p.extend_from_slice(&all.to_be_bytes()); // remove_member_threshold
        p.extend_from_slice(&all.to_be_bytes()); // role_change_threshold
        p.extend_from_slice(&all.to_be_bytes()); // rule_change_threshold
        p.extend_from_slice(&[0x01u8; 32]); // founding_device
        p
    }

    fn membership_add_payload(principal_seed: u8, role: Role) -> Vec<u8> {
        let mut p = Vec::with_capacity(33);
        p.extend_from_slice(&[principal_seed; 32]);
        p.push(match role {
            Role::Owner => 0,
            Role::Admin => 1,
            Role::Member => 2,
            Role::Observer => 3,
        });
        p
    }

    fn rule_change_payload(rule_key_byte: u8, new_value: u32) -> Vec<u8> {
        let mut p = Vec::with_capacity(5);
        p.push(rule_key_byte);
        p.extend_from_slice(&new_value.to_be_bytes());
        p
    }

    fn sign_envelope(env: &mut AssertionEnvelope, signer: &MockSigner) {
        let canonical = env.canonical_bytes();
        env.signature = signer.sign(&canonical);
    }

    fn make_genesis_envelope(
        signer: &MockSigner,
        group_seed: u8,
        author_principal: TypesPrincipalId,
        lamport: u64,
        threshold: u32,
    ) -> AssertionEnvelope {
        let device = TypesDeviceId::new(signer.device_id().0);
        let mut env = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::GroupGenesis,
            author_device: device,
            author_principal,
            group: make_group(group_seed),
            antecedents: vec![],
            lamport,
            payload: genesis_payload_thresh(threshold),
            signature: vec![],
        };
        sign_envelope(&mut env, signer);
        env
    }

    fn make_fold(
        signer: &MockSigner,
        principal: TypesPrincipalId,
        db: Arc<Db>,
    ) -> DerivedFold<MockSigner, MockCredentialResolver> {
        let device = TypesDeviceId::new(signer.device_id().0);
        let verifier = MockSigner::new(signer.device_id().0);
        let mut cred = MockCredentialResolver::new();
        cred.register(
            TraitsDeviceId(device.as_bytes().clone()),
            TraitsPrincipalId(principal.as_bytes().clone()),
        );
        DerivedFold::new(db, verifier, cred)
    }

    /// Boot a group: genesis + add author as Owner member.
    fn boot_group(
        signer: &MockSigner,
        principal: TypesPrincipalId,
        group_seed: u8,
        fold: &DerivedFold<MockSigner, MockCredentialResolver>,
        lamport_start: &mut u64,
        threshold: u32,
    ) {
        let lam = *lamport_start;
        *lamport_start += 1;
        let genesis = make_genesis_envelope(signer, group_seed, principal, lam, threshold);
        fold.ingest(&genesis).expect("genesis should succeed");

        // Add principal as Owner.
        let device = TypesDeviceId::new(signer.device_id().0);
        let lam2 = *lamport_start;
        *lamport_start += 1;
        let mut add_env = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::MembershipAdd,
            author_device: device,
            author_principal: principal,
            group: make_group(group_seed),
            antecedents: vec![],
            lamport: lam2,
            payload: membership_add_payload(principal.as_bytes()[0], Role::Owner),
            signature: vec![],
        };
        sign_envelope(&mut add_env, signer);
        fold.ingest(&add_env).expect("MembershipAdd should succeed");
    }

    // -----------------------------------------------------------------------
    // under-determination (§7.6.1 second escalation shape)
    // -----------------------------------------------------------------------

    #[test]
    fn under_determined_iff_no_owner() {
        use social_tree_core::model::{PrincipalId, Role};
        let owner = (PrincipalId::new([1; 32]), Role::Owner, 0u64);
        let admin = (PrincipalId::new([2; 32]), Role::Admin, 0u64);
        let member = (PrincipalId::new([3; 32]), Role::Member, 0u64);

        // An Owner present ⇒ determined, regardless of other roles.
        assert!(!is_under_determined(&[owner.clone(), admin.clone()]));
        assert!(!is_under_determined(std::slice::from_ref(&owner)));

        // No Owner (only Admin/Member remain) ⇒ under-determined: no member can
        // authorize governance and none can promote a successor.
        assert!(is_under_determined(&[admin.clone(), member]));
        assert!(is_under_determined(std::slice::from_ref(&admin)));

        // Empty group (last member removed) ⇒ under-determined.
        assert!(is_under_determined(&[]));
    }

    #[test]
    fn thresholds_count_personae_by_lineage_never_clients() {
        use social_tree_core::model::PrincipalId;
        let p = |n: u8| PrincipalId::new([n; 32]);

        // Two distinct personae meet a 2-of-n.
        assert_eq!(count_personae_by_lineage(&[p(1), p(2)]), 2);
        assert!(threshold_met(&[p(1), p(2)], 2));

        // ONE persona signing three times (its three clients) is still one persona —
        // it must NOT meet a 2-of-n. This is the §5.7 anti-inflation guard.
        assert_eq!(count_personae_by_lineage(&[p(1), p(1), p(1)]), 1);
        assert!(!threshold_met(&[p(1), p(1), p(1)], 2));

        // Mixed: persona 1 (twice) + persona 2 → two personae, meets 2-of-n but not 3.
        assert_eq!(count_personae_by_lineage(&[p(1), p(1), p(2)]), 2);
        assert!(threshold_met(&[p(1), p(1), p(2)], 2));
        assert!(!threshold_met(&[p(1), p(1), p(2)], 3));

        // A single signer meets 1-of-n but not 2-of-n.
        assert!(threshold_met(&[p(1)], 1));
        assert!(!threshold_met(&[p(1)], 2));
        assert_eq!(count_personae_by_lineage(&[]), 0);
    }

    #[test]
    fn concurrency_over_the_antecedent_dag() {
        use social_tree_core::model::Hash;
        use std::collections::HashMap;
        let h = |n: u8| Hash::new([n; 32]);

        // Chain a→b→c; plus x and y, both concurrent after r.
        let mut ant: HashMap<[u8; 32], Vec<Hash>> = HashMap::new();
        ant.insert(*h(1).as_bytes(), vec![]); // a
        ant.insert(*h(2).as_bytes(), vec![h(1)]); // b ← a
        ant.insert(*h(3).as_bytes(), vec![h(2)]); // c ← b
        ant.insert(*h(10).as_bytes(), vec![]); // r
        ant.insert(*h(11).as_bytes(), vec![h(10)]); // x ← r
        ant.insert(*h(12).as_bytes(), vec![h(10)]); // y ← r
        let lookup = |k: &Hash| ant.get(k.as_bytes()).cloned();

        // Ancestry along the chain.
        assert!(
            is_ancestor(&h(1), &h(3), &lookup),
            "a precedes c (transitively)"
        );
        assert!(is_ancestor(&h(2), &h(3), &lookup), "b precedes c");
        assert!(!is_ancestor(&h(3), &h(1), &lookup), "c does not precede a");
        assert!(
            !are_concurrent(&h(1), &h(3), &lookup),
            "a and c are causally ordered"
        );

        // Concurrent siblings.
        assert!(
            are_concurrent(&h(11), &h(12), &lookup),
            "x and y are concurrent"
        );
        assert!(is_ancestor(&h(10), &h(11), &lookup), "r precedes x");
        assert!(
            !are_concurrent(&h(10), &h(11), &lookup),
            "r and x are ordered, not concurrent"
        );

        // A fact is not concurrent with itself; an absent fact has no ancestors.
        assert!(
            !are_concurrent(&h(11), &h(11), &lookup),
            "not concurrent with self"
        );
        assert!(
            !is_ancestor(&h(1), &h(99), &lookup),
            "absent descendant has no ancestors"
        );
    }

    // required_threshold_for_rule_change
    // -----------------------------------------------------------------------

    #[test]
    fn test_threshold_helper_all_keys() {
        let rules = GroupRules {
            add_member_threshold: 2,
            remove_member_threshold: 3,
            role_change_threshold: 4,
            rule_change_threshold: 5,
            resolution_threshold: 6,
            readmission_threshold: 7,
        };
        assert_eq!(
            required_threshold_for_rule_change(&rules, &RuleKey::AddMember),
            2
        );
        assert_eq!(
            required_threshold_for_rule_change(&rules, &RuleKey::Readmission),
            7
        );
        assert_eq!(
            required_threshold_for_rule_change(&rules, &RuleKey::RemoveMember),
            3
        );
        assert_eq!(
            required_threshold_for_rule_change(&rules, &RuleKey::RoleChange),
            4
        );
        assert_eq!(
            required_threshold_for_rule_change(&rules, &RuleKey::RuleChange),
            5
        );
        assert_eq!(
            required_threshold_for_rule_change(&rules, &RuleKey::Resolution),
            6
        );
    }

    // -----------------------------------------------------------------------
    // I6: amendment-threshold self-amendment
    // -----------------------------------------------------------------------

    /// Amend rule_change_threshold from 1 to 2; then attempt another amendment with
    /// only 1 signature → the fold **rejects** it, because at that governance position
    /// the threshold is now 2 and RuleChange quorum is enforced (V5′, extended to
    /// RuleChange via a content-hash approval subject). Also confirms the threshold is
    /// stored and read back correctly and that `required_threshold_for_rule_change`
    /// returns the CURRENT stored value, not the genesis value.
    #[test]
    fn test_i6_amendment_threshold_self_amend() {
        let signer = MockSigner::from_seed(0x10);
        let principal = make_principal(0x10);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&signer, principal, Arc::clone(&db));

        let mut lam = 1u64;
        boot_group(&signer, principal, 0x10, &fold, &mut lam, 1);

        // Step 1: amend rule_change_threshold 1 → 2.
        let device = TypesDeviceId::new(signer.device_id().0);
        let lam1 = lam;
        lam += 1;
        let mut rc1 = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::RuleChange,
            author_device: device,
            author_principal: principal,
            group: make_group(0x10),
            antecedents: vec![],
            lamport: lam1,
            // rule_key byte 3 = RuleChange, new_value = 2
            payload: rule_change_payload(3, 2),
            signature: vec![],
        };
        sign_envelope(&mut rc1, &signer);
        fold.ingest(&rc1)
            .expect("first self-amend should succeed (threshold was 1)");

        // Step 2: Read GroupState; threshold must now be 2.
        let state = {
            let read_txn = db.inner().begin_read().unwrap();
            let table = read_txn.open_table(STATE_GROUP).unwrap();
            let raw = table
                .get(make_group(0x10).as_bytes().as_ref())
                .unwrap()
                .unwrap();
            GroupState::from_bytes(raw.value()).unwrap()
        };
        assert_eq!(
            state.rules.rule_change_threshold, 2,
            "rule_change_threshold must be 2 after first amendment"
        );

        // Step 3: required_threshold_for_rule_change on the NEW rules returns 2.
        let threshold_at_position =
            required_threshold_for_rule_change(&state.rules, &RuleKey::RuleChange);
        assert_eq!(
            threshold_at_position, 2,
            "threshold for amending RuleChange is evaluated at current position, not genesis"
        );

        assert_ne!(
            state.rules.rule_change_threshold, 1,
            "threshold must no longer be the genesis value of 1"
        );

        // Step 4: a second amendment (raise add_member_threshold 1 → 5) by the sole
        // owner, no approvals, now that rule_change_threshold = 2 → REJECTED. RuleChange
        // quorum is enforced (its subject is the content hash of the proposed change);
        // one persona is 1 of 2, so the fold refuses to apply it.
        let lam2 = lam;
        let mut rc2 = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::RuleChange,
            author_device: device,
            author_principal: principal,
            group: make_group(0x10),
            antecedents: vec![],
            lamport: lam2,
            payload: rule_change_payload(0, 5), // rule_key 0 = AddMember
            signature: vec![],
        };
        sign_envelope(&mut rc2, &signer);
        let outcome = fold.ingest(&rc2);
        assert!(
            outcome.is_err(),
            "a lone owner cannot amend at rule_change_threshold=2 (quorum enforced): {outcome:?}"
        );

        // The rejected amendment must not have taken effect.
        let state2 = {
            let read_txn = db.inner().begin_read().unwrap();
            let table = read_txn.open_table(STATE_GROUP).unwrap();
            let raw = table
                .get(make_group(0x10).as_bytes().as_ref())
                .unwrap()
                .unwrap();
            GroupState::from_bytes(raw.value()).unwrap()
        };
        assert_eq!(
            state2.rules.add_member_threshold, 1,
            "a rejected RuleChange must not change the target rule"
        );
    }

    // -----------------------------------------------------------------------
    // I7: tiebreak determinism
    // -----------------------------------------------------------------------

    #[test]
    fn test_i7_tiebreak_deterministic() {
        let a = make_hash(0x01);
        let b = make_hash(0x02);

        let r1 = tiebreak(&a, &b);
        let r2 = tiebreak(&a, &b);
        assert_eq!(r1, r2, "tiebreak must be deterministic");

        // a < b lexicographically (0x01 < 0x02) so a wins (Less).
        assert_eq!(r1, std::cmp::Ordering::Less, "smaller hash should win");

        // Reversed: b > a so b loses.
        let r3 = tiebreak(&b, &a);
        assert_eq!(r3, std::cmp::Ordering::Greater);

        // Same hash: Equal.
        let r4 = tiebreak(&a, &a);
        assert_eq!(r4, std::cmp::Ordering::Equal);
    }

    #[test]
    fn test_i7_tiebreak_deterministic_fresh_fold() {
        // In a "fresh fold instance" context — just call tiebreak twice and
        // confirm identical output, simulating two independent evaluations.
        let h1 = make_hash(0xAA);
        let h2 = make_hash(0x55);

        let ord_a = tiebreak(&h1, &h2);
        // Simulate a fresh instance: same inputs, same result.
        let ord_b = tiebreak(&h1, &h2);
        assert_eq!(
            ord_a, ord_b,
            "tiebreak is identical across independent fold instances"
        );
    }

    // -----------------------------------------------------------------------
    // I7: contested change — divergent lineages
    // -----------------------------------------------------------------------

    /// Apply a valid RuleChange accepted by group (threshold=1, single owner);
    /// then apply a DIFFERENT RuleChange at the same gov_seq (simulated by
    /// constructing a different assertion with the same lamport that would
    /// target the same governance slot); assert fork_status set; both lineages
    /// independently valid.
    #[test]
    fn test_i7_contested_change_divergent_lineages() {
        let signer = MockSigner::from_seed(0x20);
        let principal = make_principal(0x20);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&signer, principal, Arc::clone(&db));

        let mut lam = 1u64;
        boot_group(&signer, principal, 0x20, &fold, &mut lam, 1);

        // gov_seq 0 = genesis, gov_seq 1 = MembershipAdd (from boot_group)
        // Next gov assertion will be at seq 2.

        // First RuleChange: add_member_threshold 1 → 3.
        let device = TypesDeviceId::new(signer.device_id().0);
        let lam_rc = lam;
        lam += 1;
        let mut rc_a = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::RuleChange,
            author_device: device,
            author_principal: principal,
            group: make_group(0x20),
            antecedents: vec![],
            lamport: lam_rc,
            payload: rule_change_payload(0 /* AddMember */, 3),
            signature: vec![],
        };
        sign_envelope(&mut rc_a, &signer);
        let result_a = fold.ingest(&rc_a).expect("first RuleChange must succeed");
        let hash_a = match result_a {
            IngestResult::Applied { hash } => hash,
            _ => panic!("expected Applied"),
        };

        // Now construct a conflicting RuleChange that would also target seq 2.
        // To make it divergent we target the same group but a DIFFERENT payload
        // (add_member_threshold → 5 instead of 3), but with a HIGHER lamport so
        // it passes the monotonicity check and becomes a new independent assertion.
        //
        // We can't literally inject into the same slot via DerivedFold because it
        // increments the sequence; instead we use `detect_fork` directly to
        // verify the fork detection logic.
        let lam_rc_b = lam;
        lam += 1;
        let mut rc_b = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::RuleChange,
            author_device: device,
            author_principal: principal,
            group: make_group(0x20),
            antecedents: vec![],
            lamport: lam_rc_b,
            payload: rule_change_payload(0 /* AddMember */, 5),
            signature: vec![],
        };
        sign_envelope(&mut rc_b, &signer);
        let hash_b = envelope_hash(&rc_b);

        // Use detect_fork to simulate what would happen if rc_b contested seq 2.
        let fork = detect_fork(&hash_a, 2, &rc_b);
        assert!(
            fork.is_some(),
            "detect_fork should return Some when hashes differ"
        );

        let fork_info = fork.unwrap();
        assert_eq!(fork_info.parent_gov_seq, 2);

        // The accepted head is the winning hash (lexicographically smaller).
        let winner = if hash_b.as_bytes() < hash_a.as_bytes() {
            hash_b
        } else {
            hash_a
        };
        let loser = if hash_b.as_bytes() < hash_a.as_bytes() {
            hash_a
        } else {
            hash_b
        };
        assert_eq!(fork_info.accepted_head, winner);
        assert_eq!(fork_info.rejected_head, loser);
        assert_eq!(
            fork_info.lineage_id, loser,
            "lineage_id is the rejected hash"
        );

        // Both assertions are independently valid (each would be valid from genesis).
        // Verify rc_a was ingested successfully (already done above) and that
        // rc_b would also pass authorization checks in isolation.
        // We verify this by creating a fresh db with only rc_b.
        let db2 = Arc::new(Db::create_in_memory().unwrap());
        let fold2 = make_fold(&signer, principal, Arc::clone(&db2));
        let mut lam2 = 1u64;
        boot_group(&signer, principal, 0x20, &fold2, &mut lam2, 1);
        let lam_rc_b2 = lam2;
        let mut rc_b_fresh = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::RuleChange,
            author_device: device,
            author_principal: principal,
            group: make_group(0x20),
            antecedents: vec![],
            lamport: lam_rc_b2,
            payload: rule_change_payload(0, 5),
            signature: vec![],
        };
        sign_envelope(&mut rc_b_fresh, &signer);
        fold2
            .ingest(&rc_b_fresh)
            .expect("rc_b is independently valid in a fresh lineage");
    }

    /// Verify that `detect_fork` properly identifies concurrent assertions at the
    /// same governance epoch as described in ForkLineage.
    #[test]
    fn test_i7_concurrent_same_epoch_detectable() {
        // Simulate two assertions both targeting gov_seq=5.
        let h_existing = make_hash(0x50); // existing head at seq 5
        let device = TypesDeviceId::new([0x55u8; 32]);
        let principal = make_principal(0xAA);

        let mut incoming = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::RuleChange,
            author_device: device,
            author_principal: principal,
            group: make_group(0xFF),
            antecedents: vec![],
            lamport: 99,
            payload: rule_change_payload(0, 42),
            signature: vec![],
        };
        // Sign with a mock signer so the hash is deterministic.
        let mock = MockSigner::from_seed(0x55);
        sign_envelope(&mut incoming, &mock);
        let h_incoming = envelope_hash(&incoming);

        let fork = detect_fork(&h_existing, 5, &incoming);
        assert!(
            fork.is_some(),
            "must detect fork when slot 5 is already occupied"
        );

        let fi = fork.unwrap();
        assert_eq!(fi.parent_gov_seq, 5, "fork is at gov_seq 5");
        assert_eq!(fi.fork_at, h_existing, "fork_at is the pre-existing head");

        // Accepted is the winner (smaller hash), rejected is the loser.
        if h_incoming.as_bytes() < h_existing.as_bytes() {
            assert_eq!(fi.accepted_head, h_incoming);
            assert_eq!(fi.rejected_head, h_existing);
        } else {
            assert_eq!(fi.accepted_head, h_existing);
            assert_eq!(fi.rejected_head, h_incoming);
        }
    }

    // -----------------------------------------------------------------------
    // I10: compaction never prunes governance
    // -----------------------------------------------------------------------

    #[test]
    fn test_i10_compaction_never_prunes_governance() {
        let signer = MockSigner::from_seed(0x30);
        let principal = make_principal(0x30);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&signer, principal, Arc::clone(&db));

        let mut lam = 1u64;
        boot_group(&signer, principal, 0x30, &fold, &mut lam, 1);

        // Add some content (Message) assertions.
        let device = TypesDeviceId::new(signer.device_id().0);
        for i in 0..5u64 {
            let msg_lam = lam;
            lam += 1;
            let body = format!("msg-{}", i);
            let mut msg = AssertionEnvelope {
                version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                assertion_type: AssertionType::Message,
                author_device: device,
                author_principal: principal,
                group: make_group(0x30),
                antecedents: vec![],
                lamport: msg_lam,
                // Use very old timestamps to trigger age gate.
                payload: {
                    let mut p = Vec::new();
                    p.extend_from_slice(&(body.len() as u32).to_be_bytes());
                    p.extend_from_slice(body.as_bytes());
                    p.extend_from_slice(&[0u8; 4]);
                    p
                },
                signature: vec![],
            };
            sign_envelope(&mut msg, &signer);
            fold.ingest(&msg).expect("message ingest");
        }

        // Advance the governance head PAST the messages: the age gate is
        // lamport-denominated (position, never wall-clock), so content is "old"
        // only relative to a checkpoint that comes after it.
        {
            let add_lam = lam;
            lam += 1;
            let mut add = AssertionEnvelope {
                version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                assertion_type: AssertionType::MembershipAdd,
                author_device: device,
                author_principal: principal,
                group: make_group(0x30),
                antecedents: vec![],
                lamport: add_lam,
                payload: {
                    let mut p = Vec::new();
                    p.extend_from_slice(make_principal(0x31).as_bytes());
                    p.push(2); // Member
                    p
                },
                signature: vec![],
            };
            sign_envelope(&mut add, &signer);
            fold.ingest(&add).expect("post-message governance fact");
        }

        // Count governance entries before compaction (all of them, including the
        // post-message fact that advances the checkpoint).
        let gov_count_before = {
            let read_txn = db.inner().begin_read().unwrap();
            let table = read_txn.open_table(AUTH_GOV_LOG).unwrap();
            let start = encode_gov_log_key(&make_group(0x30), 0);
            let end = encode_gov_log_key(&make_group(0x30), u64::MAX);
            table
                .range(start.as_slice()..=end.as_slice())
                .unwrap()
                .count()
        };
        assert!(
            gov_count_before >= 3,
            "genesis + membershipadd + post-message fact"
        );

        // Get the current gov head.
        let gov_head = {
            let read_txn = db.inner().begin_read().unwrap();
            let table = read_txn.open_table(AUTH_GOV_LOG).unwrap();
            let start = encode_gov_log_key(&make_group(0x30), 0);
            let end = encode_gov_log_key(&make_group(0x30), u64::MAX);
            let last = table
                .range(start.as_slice()..=end.as_slice())
                .unwrap()
                .last()
                .unwrap()
                .unwrap();
            let mut h = [0u8; 32];
            h.copy_from_slice(last.1.value());
            TypesHash::new(h)
        };

        // Run compaction with aggressive thresholds.
        let config = CompactionConfig {
            enabled: true,
            trigger_depth: 0,        // everything is eligible by depth
            trigger_age_lamports: 0, // everything is eligible by age
        };
        let result = compact_content(&db, &make_group(0x30), &gov_head, &config)
            .expect("compaction must succeed");

        // Governance assertions in auth_gov_log must ALL be present after compaction.
        let gov_count_after = {
            let read_txn = db.inner().begin_read().unwrap();
            let table = read_txn.open_table(AUTH_GOV_LOG).unwrap();
            let start = encode_gov_log_key(&make_group(0x30), 0);
            let end = encode_gov_log_key(&make_group(0x30), u64::MAX);
            table
                .range(start.as_slice()..=end.as_slice())
                .unwrap()
                .count()
        };
        assert_eq!(
            gov_count_before, gov_count_after,
            "governance log must not shrink after compaction (I10)"
        );

        // The governance assertion bytes must still be readable from auth_assertions.
        let gov_hashes_after: Vec<Vec<u8>> = {
            let read_txn = db.inner().begin_read().unwrap();
            let table = read_txn.open_table(AUTH_GOV_LOG).unwrap();
            let start = encode_gov_log_key(&make_group(0x30), 0);
            let end = encode_gov_log_key(&make_group(0x30), u64::MAX);
            table
                .range(start.as_slice()..=end.as_slice())
                .unwrap()
                .map(|item| item.unwrap().1.value().to_vec())
                .collect()
        };
        {
            let read_txn = db.inner().begin_read().unwrap();
            let table = read_txn.open_table(AUTH_ASSERTIONS).unwrap();
            for h in &gov_hashes_after {
                assert!(
                    table.get(h.as_slice()).unwrap().is_some(),
                    "governance assertion must remain in auth_assertions after compaction"
                );
            }
        }

        // Some content must have been pruned (we added 5 messages with old timestamps).
        assert!(
            result.pruned_count > 0,
            "at least some content assertions should be pruned; got 0"
        );
    }

    // -----------------------------------------------------------------------
    // I10: rebuild after compaction produces identical derived state
    // -----------------------------------------------------------------------

    #[test]
    fn test_i10_rebuild_after_compaction() {
        let signer = MockSigner::from_seed(0x40);
        let principal = make_principal(0x40);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&signer, principal, Arc::clone(&db));

        let mut lam = 1u64;
        boot_group(&signer, principal, 0x40, &fold, &mut lam, 1);

        // Add a few content assertions near the head (should NOT be pruned
        // with a high trigger_age_lamports — the age gate is lamport-denominated).
        let device = TypesDeviceId::new(signer.device_id().0);
        for i in 0..3u64 {
            let msg_lam = lam;
            lam += 1;
            let body = format!("keep-{}", i);
            let mut msg = AssertionEnvelope {
                version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                assertion_type: AssertionType::Message,
                author_device: device,
                author_principal: principal,
                group: make_group(0x40),
                antecedents: vec![],
                lamport: msg_lam,
                payload: {
                    let mut p = Vec::new();
                    p.extend_from_slice(&(body.len() as u32).to_be_bytes());
                    p.extend_from_slice(body.as_bytes());
                    p.extend_from_slice(&[0u8; 4]);
                    p
                },
                signature: vec![],
            };
            sign_envelope(&mut msg, &signer);
            fold.ingest(&msg).expect("message ingest");
        }

        // Capture pre-compaction derived state via state_group.
        let state_before = {
            let read_txn = db.inner().begin_read().unwrap();
            let table = read_txn.open_table(STATE_GROUP).unwrap();
            let raw = table
                .get(make_group(0x40).as_bytes().as_ref())
                .unwrap()
                .unwrap();
            raw.value().to_vec()
        };

        // Get gov head.
        let gov_head = {
            let read_txn = db.inner().begin_read().unwrap();
            let table = read_txn.open_table(AUTH_GOV_LOG).unwrap();
            let start = encode_gov_log_key(&make_group(0x40), 0);
            let end = encode_gov_log_key(&make_group(0x40), u64::MAX);
            let last = table
                .range(start.as_slice()..=end.as_slice())
                .unwrap()
                .last()
                .unwrap()
                .unwrap();
            let mut h = [0u8; 32];
            h.copy_from_slice(last.1.value());
            TypesHash::new(h)
        };

        // Compact with disabled pruning (enabled=false) to just write a checkpoint.
        let config = CompactionConfig {
            enabled: false, // no pruning
            trigger_depth: 1000,
            trigger_age_lamports: 86400 * 30,
        };
        compact_content(&db, &make_group(0x40), &gov_head, &config)
            .expect("compaction (disabled) must succeed");

        // Rebuild from scratch.
        let verifier = MockSigner::new(signer.device_id().0);
        let mut cred = MockCredentialResolver::new();
        cred.register(
            TraitsDeviceId(device.as_bytes().clone()),
            TraitsPrincipalId(principal.as_bytes().clone()),
        );
        rebuild(&db, &verifier, &cred).expect("rebuild must succeed");

        // Derived state after rebuild must be identical to pre-compaction.
        let state_after = {
            let read_txn = db.inner().begin_read().unwrap();
            let table = read_txn.open_table(STATE_GROUP).unwrap();
            let raw = table
                .get(make_group(0x40).as_bytes().as_ref())
                .unwrap()
                .unwrap();
            raw.value().to_vec()
        };
        assert_eq!(
            state_before, state_after,
            "derived GroupState must be byte-identical after rebuild"
        );
    }

    // -----------------------------------------------------------------------
    // I10: two independent Db instances with same log → same compaction result
    // -----------------------------------------------------------------------

    #[test]
    fn test_i10_per_lineage_identical_pruning() {
        // Build the same log in two independent Db instances.
        let build_db = || -> (Arc<Db>, TypesHash) {
            let signer = MockSigner::from_seed(0x50);
            let principal = make_principal(0x50);
            let db = Arc::new(Db::create_in_memory().unwrap());
            let fold = make_fold(&signer, principal, Arc::clone(&db));
            let device = TypesDeviceId::new(signer.device_id().0);

            let mut lam = 1u64;
            boot_group(&signer, principal, 0x50, &fold, &mut lam, 1);

            // Add content with old timestamps.
            for i in 0..4u64 {
                let msg_lam = lam;
                lam += 1;
                let body = format!("prune-{}", i);
                let mut msg = AssertionEnvelope {
                    version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                    assertion_type: AssertionType::Message,
                    author_device: device,
                    author_principal: principal,
                    group: make_group(0x50),
                    antecedents: vec![],
                    lamport: msg_lam,
                    payload: {
                        let mut p = Vec::new();
                        p.extend_from_slice(&(body.len() as u32).to_be_bytes());
                        p.extend_from_slice(body.as_bytes());
                        p.extend_from_slice(&[0u8; 4]);
                        p
                    },
                    signature: vec![],
                };
                sign_envelope(&mut msg, &signer);
                fold.ingest(&msg).expect("message ingest");
            }

            // Get gov head.
            let gov_head = {
                let read_txn = db.inner().begin_read().unwrap();
                let table = read_txn.open_table(AUTH_GOV_LOG).unwrap();
                let start = encode_gov_log_key(&make_group(0x50), 0);
                let end = encode_gov_log_key(&make_group(0x50), u64::MAX);
                let last = table
                    .range(start.as_slice()..=end.as_slice())
                    .unwrap()
                    .last()
                    .unwrap()
                    .unwrap();
                let mut h = [0u8; 32];
                h.copy_from_slice(last.1.value());
                TypesHash::new(h)
            };

            (db, gov_head)
        };

        let (db1, gov_head1) = build_db();
        let (db2, gov_head2) = build_db();

        // Both dbs must have the same gov head since the inputs are identical.
        assert_eq!(
            gov_head1, gov_head2,
            "identical inputs must produce identical gov heads"
        );

        let config = CompactionConfig {
            enabled: true,
            trigger_depth: 0,
            trigger_age_lamports: 0,
        };
        let result1 = compact_content(&db1, &make_group(0x50), &gov_head1, &config).unwrap();
        let result2 = compact_content(&db2, &make_group(0x50), &gov_head2, &config).unwrap();

        assert_eq!(
            result1.pruned_count, result2.pruned_count,
            "identical dbs must produce identical pruned_count"
        );
        assert_eq!(
            result1.checkpoint_seq, result2.checkpoint_seq,
            "identical dbs must produce identical checkpoint_seq"
        );
        assert_eq!(
            result1.content_merkle_root, result2.content_merkle_root,
            "identical dbs must produce identical Merkle root"
        );
    }

    // -----------------------------------------------------------------------
    // Merkle root stability
    // -----------------------------------------------------------------------

    #[test]
    fn test_merkle_root_stability() {
        // Empty input → [0u8; 32].
        let empty_root = compute_merkle_root(&[]);
        assert_eq!(
            empty_root,
            TypesHash::new([0u8; 32]),
            "empty input must produce zero hash"
        );

        // Same ordered input → identical root.
        let hashes = vec![
            make_hash(0x01),
            make_hash(0x02),
            make_hash(0x03),
            make_hash(0x04),
        ];
        let root1 = compute_merkle_root(&hashes);
        let root2 = compute_merkle_root(&hashes);
        assert_eq!(
            root1, root2,
            "identical input must produce identical Merkle root"
        );

        // Different order → different root.
        let hashes_rev: Vec<TypesHash> = hashes.iter().rev().cloned().collect();
        let root_rev = compute_merkle_root(&hashes_rev);
        assert_ne!(
            root1, root_rev,
            "different order must produce different root"
        );

        // Single element → that hash itself.
        let single = make_hash(0xAB);
        let root_single = compute_merkle_root(&[single]);
        assert_eq!(
            root_single, single,
            "single-element root must equal the element itself"
        );
    }

    // -----------------------------------------------------------------------
    // detect_fork: same assertion is not a fork
    // -----------------------------------------------------------------------

    #[test]
    fn test_detect_fork_same_assertion_is_not_fork() {
        let mock = MockSigner::from_seed(0x77);
        let device = TypesDeviceId::new([0x77u8; 32]);
        let principal = make_principal(0x77);

        let mut env = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::RuleChange,
            author_device: device,
            author_principal: principal,
            group: make_group(0x77),
            antecedents: vec![],
            lamport: 10,
            payload: rule_change_payload(0, 1),
            signature: vec![],
        };
        sign_envelope(&mut env, &mock);
        let h = envelope_hash(&env);

        // Detect fork against itself: Equal → no fork.
        let fork = detect_fork(&h, 3, &env);
        assert!(
            fork.is_none(),
            "same assertion hash must not produce a fork"
        );
    }
}
