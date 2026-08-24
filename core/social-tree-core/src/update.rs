//! The governance transition — §7.3's fold, pure. Authorization-at-position,
//! the concurrent-contradiction detectors, the CONTESTED set, charter-quorum
//! resolution, and the deterministic replay: every decision the fold makes,
//! computed from data handed in. No storage, no I/O, no clock — the adapter
//! (today `local_storage_projection`, the redb realization) assembles a
//! [`FoldContext`] from what it holds and applies the returned state.
//!
//! Spec-key → mechanism mapping (kept here so the equivalence argument does not
//! become folklore): Part 2 §7.3.1's layered resolution is realized as
//! sequential replay in `merge_cmp` order (lamport → content address) plus
//! projections and hard-stops — G1's withdrawn-alarm analysis is the
//! equivalence argument; the arrival-order pins are its standing test.

use crate::model::*;
use crate::model::{role_to_u8, u8_to_role};

/// Protocol-level fold errors. Deliberately storage-free (E117 P2, R2): the
/// adapter wraps these in its own error type alongside its storage failures.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum FoldError {
    #[error("malformed envelope: {0}")]
    MalformedEnvelope(String),
    #[error("malformed state bytes: {0}")]
    MalformedState(String),
    #[error("authorization failed: {0}")]
    AuthorizationFailed(String),
    #[error("threshold not met: have {have}, need {need}")]
    ThresholdNotMet { have: usize, need: usize },
    #[error("lamport violation for device {device:?}: expected > {expected_gt}, got {got}")]
    LamportViolation {
        device: DeviceId,
        expected_gt: u64,
        got: u64,
    },
    #[error("missing antecedents: have {have}, need {need}")]
    MissingAntecedents { have: usize, need: usize },
    #[error("governance log has no genesis")]
    MissingGenesis,
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
    a: &Hash,
    b: &Hash,
    antecedents_of: &impl Fn(&Hash) -> Option<Vec<Hash>>,
) -> bool {
    a != b && !is_ancestor(a, b, antecedents_of) && !is_ancestor(b, a, antecedents_of)
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
    ancestor: &Hash,
    descendant: &Hash,
    antecedents_of: &impl Fn(&Hash) -> Option<Vec<Hash>>,
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

/// Count distinct **personae** among `approvers`, identified by lineage — the rooting
/// **principal**. Multiple clients of one persona collapse to one.
///
/// This is the guard against a single persona satisfying a k-of-n governance threshold
/// by signing with many devices (§5.7): weight is one-per-persona-by-lineage, never
/// per-client. Callers MUST pass principals already resolved from devices (the
/// credential resolver maps device → principal by signed lineage), never raw device
/// ids, so device count cannot inflate the count.
#[must_use]
pub fn count_personae_by_lineage(approvers: &[PrincipalId]) -> usize {
    use std::collections::HashSet;
    let mut set: HashSet<[u8; 32]> = HashSet::new();
    for p in approvers {
        set.insert(*p.as_bytes());
    }
    set.len()
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
pub fn is_under_determined(members: &[(PrincipalId, Role, u64)]) -> bool {
    !members
        .iter()
        .any(|(_, role, _)| matches!(role, Role::Owner))
}

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
    }
}

fn decode_rule_key(v: u8) -> Result<RuleKey, ()> {
    match v {
        0 => Ok(RuleKey::AddMember),
        1 => Ok(RuleKey::RemoveMember),
        2 => Ok(RuleKey::RoleChange),
        3 => Ok(RuleKey::RuleChange),
        4 => Ok(RuleKey::Resolution),
        _ => Err(()),
    }
}

fn author_role_in<'a>(
    members: &'a [(PrincipalId, Role, u64)],
    author: &PrincipalId,
) -> Option<&'a Role> {
    members
        .iter()
        .find(|(p, _, _)| p == author)
        .map(|(_, r, _)| r)
}

fn role_ge_admin(r: &Role) -> bool {
    matches!(r, Role::Owner | Role::Admin)
}

fn role_ge_owner(r: &Role) -> bool {
    matches!(r, Role::Owner)
}

fn role_ge_member(r: &Role) -> bool {
    matches!(r, Role::Owner | Role::Admin | Role::Member)
}

fn check_authorization(state: &GroupState, env: &AssertionEnvelope) -> Result<(), FoldError> {
    let author = &env.author_principal;
    match &env.assertion_type {
        AssertionType::GroupGenesis => Ok(()),

        AssertionType::MembershipAdd => match author_role_in(&state.members, author) {
            Some(r) if role_ge_admin(r) => Ok(()),
            _ => Err(FoldError::AuthorizationFailed(format!(
                "MembershipAdd requires Owner or Admin; author {:?} is not",
                author
            ))),
        },

        AssertionType::MembershipRemove => match author_role_in(&state.members, author) {
            Some(r) if role_ge_admin(r) => Ok(()),
            _ => Err(FoldError::AuthorizationFailed(format!(
                "MembershipRemove requires Owner or Admin; author {:?} is not",
                author
            ))),
        },

        AssertionType::RoleGrant | AssertionType::RoleRevoke => {
            match author_role_in(&state.members, author) {
                Some(r) if role_ge_owner(r) => Ok(()),
                _ => Err(FoldError::AuthorizationFailed(format!(
                    "{:?} requires Owner; author {:?} is not",
                    env.assertion_type, author
                ))),
            }
        }

        AssertionType::RuleChange => {
            if env.payload.len() < 5 {
                return Err(FoldError::MalformedEnvelope(
                    "RuleChange payload too short".to_string(),
                ));
            }
            decode_rule_key(env.payload[0]).map_err(|_| {
                FoldError::MalformedEnvelope(format!(
                    "RuleChange: unknown rule_key byte {}",
                    env.payload[0]
                ))
            })?;
            match author_role_in(&state.members, author) {
                Some(r) if role_ge_owner(r) => Ok(()),
                _ => Err(FoldError::AuthorizationFailed(format!(
                    "RuleChange requires Owner; author {:?} is not",
                    author
                ))),
            }
        }

        AssertionType::AttachmentAdd | AssertionType::Message | AssertionType::ArtifactRef => {
            match author_role_in(&state.members, author) {
                Some(r) if role_ge_member(r) => Ok(()),
                _ => Err(FoldError::AuthorizationFailed(format!(
                    "{:?} requires membership; author {:?} is not a member",
                    env.assertion_type, author
                ))),
            }
        }

        // Approval (V5′): an approver of a governance act must itself be
        // governance-eligible (Owner/Admin) — it is co-authoring the act.
        AssertionType::Approval => match author_role_in(&state.members, author) {
            Some(r) if role_ge_admin(r) => Ok(()),
            _ => Err(FoldError::AuthorizationFailed(format!(
                "Approval requires Owner or Admin; author {author:?} is not"
            ))),
        },

        // Resolution (§7.3.2): a governed act closing one open contradiction pair.
        // Payload is exactly the ordered pair; the quorum gate (resolution_threshold,
        // default 2 — never silently single-author) is enforced at Step 5.6 like every
        // other governance threshold.
        AssertionType::Resolution => {
            if env.payload.len() != 64 {
                return Err(FoldError::MalformedEnvelope(format!(
                    "Resolution payload must be exactly 64 bytes (the ordered pair), got {}",
                    env.payload.len()
                )));
            }
            if env.payload[..32] >= env.payload[32..64] {
                return Err(FoldError::MalformedEnvelope(
                    "Resolution pair must be strictly lexicographically ordered".to_string(),
                ));
            }
            match author_role_in(&state.members, author) {
                Some(r) if role_ge_admin(r) => Ok(()),
                _ => Err(FoldError::AuthorizationFailed(format!(
                    "Resolution requires Owner or Admin; author {author:?} is not"
                ))),
            }
        }

        // I5 gate: Vouch must have non-empty context and valid strength.
        AssertionType::Vouch => {
            if env.payload.len() < 37 {
                return Err(FoldError::MalformedEnvelope(
                    "Vouch payload too short".to_string(),
                ));
            }
            let ctx_len = u32::from_be_bytes(env.payload[32..36].try_into().unwrap()) as usize;
            if ctx_len == 0 {
                return Err(FoldError::AuthorizationFailed(
                    "Vouch must have non-empty context".to_string(),
                ));
            }
            let required = 32 + 4 + ctx_len + 1;
            if env.payload.len() < required {
                return Err(FoldError::MalformedEnvelope(format!(
                    "Vouch payload truncated: need {}, have {}",
                    required,
                    env.payload.len()
                )));
            }
            let strength_byte = env.payload[32 + 4 + ctx_len];
            if strength_byte > 2 {
                return Err(FoldError::AuthorizationFailed(format!(
                    "Vouch has invalid strength byte {}",
                    strength_byte
                )));
            }
            Ok(())
        }
    }
}

/// Is this assertion type a governance fact (it takes a gov-log slot and can
/// move the projected state)? Adapters use this to decide slot assembly.
pub fn is_governance(t: &AssertionType) -> bool {
    matches!(
        t,
        AssertionType::GroupGenesis
            | AssertionType::MembershipAdd
            | AssertionType::MembershipRemove
            | AssertionType::RoleGrant
            | AssertionType::RoleRevoke
            | AssertionType::RuleChange
            | AssertionType::Resolution
    )
}

/// The k-of-n threshold governing an act, from the rules in effect at its position.
/// Genesis and non-governance acts have no gate (1).
fn threshold_for(t: &AssertionType, rules: &GroupRules) -> u32 {
    match t {
        AssertionType::MembershipAdd => rules.add_member_threshold,
        AssertionType::MembershipRemove => rules.remove_member_threshold,
        AssertionType::RoleGrant | AssertionType::RoleRevoke => rules.role_change_threshold,
        AssertionType::RuleChange => rules.rule_change_threshold,
        AssertionType::Resolution => rules.resolution_threshold,
        _ => 1,
    }
}

/// The 32-byte approval **subject** for a `RuleChange` act: a content hash of its
/// payload (`rule_key ‖ new_value`). A RuleChange has no principal subject, so approvers
/// name `(RuleChange, this)` — computable from the proposed change before the act exists,
/// and the act derives the identical value from its own payload. Public so approvers and
/// tests compute the same subject. The `(type, subject)` pair in `approval_matches` keeps
/// this distinct from a real principal subject even on a hash collision.
#[must_use]
pub fn rule_change_approval_subject(payload: &[u8]) -> [u8; 32] {
    *blake3::hash(payload).as_bytes()
}

/// The subject an act's approvals must name. For membership/role acts it is the target
/// principal (first 32 payload bytes); for a `RuleChange` it is the content hash of the
/// proposed change (see [`rule_change_approval_subject`]). `None` for acts with no
/// threshold subject (genesis, data-plane, `Approval` itself).
fn act_subject(env: &AssertionEnvelope) -> Option<PrincipalId> {
    match env.assertion_type {
        AssertionType::MembershipAdd
        | AssertionType::MembershipRemove
        | AssertionType::RoleGrant
        | AssertionType::RoleRevoke => {
            if env.payload.len() < 32 {
                return None;
            }
            let mut b = [0u8; 32];
            b.copy_from_slice(&env.payload[..32]);
            Some(PrincipalId::new(b))
        }
        AssertionType::RuleChange | AssertionType::Resolution => {
            Some(PrincipalId::new(rule_change_approval_subject(&env.payload)))
        }
        _ => None,
    }
}

/// Does `approval` approve `(want_type, subject)`? Payload = act_type(2) ‖ subject(32).
fn approval_matches(approval: &AssertionEnvelope, want_type: u16, subject: &PrincipalId) -> bool {
    approval.assertion_type == AssertionType::Approval
        && approval.payload.len() >= 34
        && u16::from_be_bytes([approval.payload[0], approval.payload[1]]) == want_type
        && &approval.payload[2..34] == subject.as_bytes()
}

fn genesis_initial_state(env: &AssertionEnvelope, hash: Hash) -> Result<GroupState, FoldError> {
    if env.payload.len() < 50 {
        return Err(FoldError::MalformedEnvelope(format!(
            "GroupGenesis payload too short: {} bytes",
            env.payload.len()
        )));
    }
    let add_member_threshold = u32::from_be_bytes(env.payload[2..6].try_into().unwrap());
    let remove_member_threshold = u32::from_be_bytes(env.payload[6..10].try_into().unwrap());
    let role_change_threshold = u32::from_be_bytes(env.payload[10..14].try_into().unwrap());
    let rule_change_threshold = u32::from_be_bytes(env.payload[14..18].try_into().unwrap());
    Ok(GroupState {
        version: GROUP_STATE_WIRE_VERSION,
        computed_at_gov_head: hash,
        computed_at_gov_seq: 0,
        members: vec![(env.author_principal, Role::Owner, env.lamport)],
        rules: GroupRules {
            add_member_threshold,
            remove_member_threshold,
            role_change_threshold,
            rule_change_threshold,
            // Not in the genesis payload: minted at the product default (owner decision
            // 2026-08-21 — "two, so no one gets a one-signature verdict") and dialed
            // thereafter by governed RuleChange like every other threshold.
            resolution_threshold: 2,
        },
        fork_status: ForkStatus::Clean,
    })
}

/// Apply a governance assertion to produce a new `GroupState`.
fn apply_governance(
    state: &GroupState,
    env: &AssertionEnvelope,
    hash: Hash,
    gov_seq: u64,
) -> Result<GroupState, FoldError> {
    let mut next = GroupState {
        version: state.version,
        computed_at_gov_head: hash,
        computed_at_gov_seq: gov_seq,
        members: state.members.clone(),
        rules: state.rules.clone(),
        fork_status: state.fork_status.clone(),
    };

    match env.assertion_type {
        AssertionType::GroupGenesis => {
            if !next
                .members
                .iter()
                .any(|(p, _, _)| p == &env.author_principal)
            {
                next.members
                    .push((env.author_principal, Role::Owner, env.lamport));
            }
        }

        AssertionType::MembershipAdd => {
            if env.payload.len() < 33 {
                return Err(FoldError::MalformedEnvelope(
                    "MembershipAdd payload too short".to_string(),
                ));
            }
            let mut pid_bytes = [0u8; 32];
            pid_bytes.copy_from_slice(&env.payload[..32]);
            let invitee = PrincipalId::new(pid_bytes);
            let role = u8_to_role(env.payload[32]).ok_or_else(|| {
                FoldError::MalformedEnvelope(format!(
                    "MembershipAdd: unknown role byte {}",
                    env.payload[32]
                ))
            })?;
            if let Some(entry) = next.members.iter_mut().find(|(p, _, _)| *p == invitee) {
                entry.1 = role;
            } else {
                next.members.push((invitee, role, env.lamport));
            }
        }

        AssertionType::MembershipRemove => {
            if env.payload.len() < 32 {
                return Err(FoldError::MalformedEnvelope(
                    "MembershipRemove payload too short".to_string(),
                ));
            }
            let mut pid_bytes = [0u8; 32];
            pid_bytes.copy_from_slice(&env.payload[..32]);
            let subject = PrincipalId::new(pid_bytes);
            // Soft-remove: retain in list but mark with a sentinel role byte.
            // We keep the entry for history; the edge will be marked present=false.
            // For GroupState members we actually retain them but the edge marking
            // communicates absence. However the spec says "soft-remove for history",
            // so we keep the record.
            next.members.retain(|(p, _, _)| *p != subject);
        }

        AssertionType::RoleGrant => {
            if env.payload.len() < 33 {
                return Err(FoldError::MalformedEnvelope(
                    "RoleGrant payload too short".to_string(),
                ));
            }
            let mut pid_bytes = [0u8; 32];
            pid_bytes.copy_from_slice(&env.payload[..32]);
            let subject = PrincipalId::new(pid_bytes);
            let new_role = u8_to_role(env.payload[32]).ok_or_else(|| {
                FoldError::MalformedEnvelope(format!(
                    "RoleGrant: unknown role byte {}",
                    env.payload[32]
                ))
            })?;
            if let Some(entry) = next.members.iter_mut().find(|(p, _, _)| *p == subject) {
                entry.1 = new_role;
            }
        }

        AssertionType::RoleRevoke => {
            if env.payload.len() < 32 {
                return Err(FoldError::MalformedEnvelope(
                    "RoleRevoke payload too short".to_string(),
                ));
            }
            let mut pid_bytes = [0u8; 32];
            pid_bytes.copy_from_slice(&env.payload[..32]);
            let subject = PrincipalId::new(pid_bytes);
            if let Some(entry) = next.members.iter_mut().find(|(p, _, _)| *p == subject) {
                entry.1 = Role::Member;
            }
        }

        AssertionType::RuleChange => {
            if env.payload.len() < 5 {
                return Err(FoldError::MalformedEnvelope(
                    "RuleChange payload too short".to_string(),
                ));
            }
            let rule_key = decode_rule_key(env.payload[0]).map_err(|_| {
                FoldError::MalformedEnvelope(format!(
                    "RuleChange: unknown rule_key byte {}",
                    env.payload[0]
                ))
            })?;
            let new_value = u32::from_be_bytes(env.payload[1..5].try_into().unwrap());
            match rule_key {
                RuleKey::AddMember => next.rules.add_member_threshold = new_value,
                RuleKey::RemoveMember => next.rules.remove_member_threshold = new_value,
                RuleKey::RoleChange => next.rules.role_change_threshold = new_value,
                RuleKey::RuleChange => next.rules.rule_change_threshold = new_value,
                RuleKey::Resolution => next.rules.resolution_threshold = new_value,
            }
        }

        _ => {}
    }
    Ok(next)
}

/// Subject principal (first 32 bytes) of a `MembershipRemove` payload, if this is
/// one and it is well-formed.
fn remove_subject(env: &AssertionEnvelope) -> Option<PrincipalId> {
    if env.assertion_type != AssertionType::MembershipRemove || env.payload.len() < 32 {
        return None;
    }
    let mut b = [0u8; 32];
    b.copy_from_slice(&env.payload[..32]);
    Some(PrincipalId::new(b))
}

/// Add subject (first 32 bytes of a `MembershipAdd` payload), if well-formed.
fn add_subject(env: &AssertionEnvelope) -> Option<PrincipalId> {
    if env.assertion_type != AssertionType::MembershipAdd || env.payload.len() < 32 {
        return None;
    }
    let mut b = [0u8; 32];
    b.copy_from_slice(&env.payload[..32]);
    Some(PrincipalId::new(b))
}

/// Subject (first 32 bytes) of a `RoleGrant`/`RoleRevoke` payload, if well-formed.
fn role_subject(env: &AssertionEnvelope) -> Option<PrincipalId> {
    if !matches!(
        env.assertion_type,
        AssertionType::RoleGrant | AssertionType::RoleRevoke
    ) || env.payload.len() < 32
    {
        return None;
    }
    let mut b = [0u8; 32];
    b.copy_from_slice(&env.payload[..32]);
    Some(PrincipalId::new(b))
}

/// The role a `RoleGrant`/`RoleRevoke` establishes for its subject (grant → the granted
/// role byte; revoke → Member). None if not a well-formed role act.
fn resulting_role(env: &AssertionEnvelope) -> Option<u8> {
    match env.assertion_type {
        AssertionType::RoleGrant if env.payload.len() >= 33 => Some(env.payload[32]),
        AssertionType::RoleRevoke if env.payload.len() >= 32 => Some(role_to_u8(&Role::Member)),
        _ => None,
    }
}

/// The `(rule_key byte, new_value)` a `RuleChange` payload encodes, if well-formed
/// (`rule_key ‖ new_value` — one byte then a big-endian u32, the §7.2 layout).
fn rulechange_target(env: &AssertionEnvelope) -> Option<(u8, u32)> {
    if env.assertion_type != AssertionType::RuleChange || env.payload.len() < 5 {
        return None;
    }
    let value = u32::from_be_bytes(env.payload[1..5].try_into().ok()?);
    Some((env.payload[0], value))
}

/// The lexicographically smaller of two hashes — a deterministic, order-independent
/// label for a conflicting pair.
fn min_hash(a: Hash, b: Hash) -> Hash {
    if a.as_bytes() <= b.as_bytes() {
        a
    } else {
        b
    }
}

/// Detect a mutual-expulsion contradiction for an incoming `MembershipRemove` `g`.
/// Returns the hash of the admitted partner remove `F` when `F` removed `g`'s author,
/// `F` was authored by `g`'s subject, and `F` is causally concurrent with `g` (A⊗B).
/// Concurrency (not mere co-existence) is required, so a later remove that causally
/// *followed* the first is a normal sequential act, not a contradiction.
fn detect_mutual_expulsion(
    log: &[(Hash, AssertionEnvelope)],
    g: &AssertionEnvelope,
    g_hash: &Hash,
) -> Option<Hash> {
    // Concurrency must be POSITIVELY established, which needs a causal claim. A fact
    // with no antecedents makes none, so it is not provably concurrent with anything —
    // treat it as sequential (no contradiction) rather than false-trip the escalation
    // channel. (In a real deployment governance facts always carry antecedents; the
    // empty case is a bare/legacy fact.) This positively-established-concurrency contract
    // is shared across the concurrent-contradiction predicate family (removed-then-included,
    // role-thrash, competing-RuleChange); the empty-antecedent-folds-sequential consequence
    // is deliberate and was decided knowingly (RUN-03 audit, 2026-07-14; see
    // detect_competing_rulechange for the F8 RuleChange marquee case).
    if g.antecedents.is_empty() {
        return None;
    }
    let g_subject = remove_subject(g)?; // X
    let g_author = g.author_principal; // Y
    let lookup = |k: &Hash| -> Option<Vec<Hash>> {
        if k == g_hash {
            return Some(g.antecedents.clone());
        }
        log.iter()
            .find(|(h, _)| h == k)
            .map(|(_, e)| e.antecedents.clone())
    };
    for (f_hash, f) in log {
        if f.antecedents.is_empty() {
            continue;
        }
        let Some(f_subject) = remove_subject(f) else {
            continue;
        };
        if f_subject == g_author
            && f.author_principal == g_subject
            && are_concurrent(f_hash, g_hash, &lookup)
        {
            return Some(*f_hash);
        }
    }
    None
}

/// Detect a removed-then-included contradiction for an incoming `MembershipAdd` or
/// `MembershipRemove` on subject S: an admitted, causally-concurrent fact of the
/// *opposite* kind on the *same* S (an add/remove race with no causal order to decide
/// "in or out"). Returns `(partner_hash, remove_hash)` — the admitted partner fact, and
/// the hash of whichever of the pair is the remove (the replay-excluded side, retaining
/// S in the substrate while the projection reports S `CONTESTED`). Unlike mutual
/// expulsion, neither author is removed, so both facts are authorized; this fires on
/// the authorized path.
fn detect_removed_then_included(
    log: &[(Hash, AssertionEnvelope)],
    incoming: &AssertionEnvelope,
    incoming_hash: &Hash,
) -> Option<(Hash, Hash)> {
    // Concurrency must be positively established (see detect_mutual_expulsion): a fact
    // with no antecedents is not provably concurrent, so a bare add-then-remove of one
    // subject is a normal sequential edit, not a contradiction.
    if incoming.antecedents.is_empty() {
        return None;
    }
    let incoming_is_remove = incoming.assertion_type == AssertionType::MembershipRemove;
    let subject = if incoming_is_remove {
        remove_subject(incoming)?
    } else {
        add_subject(incoming)?
    };
    let lookup = |k: &Hash| -> Option<Vec<Hash>> {
        if k == incoming_hash {
            return Some(incoming.antecedents.clone());
        }
        log.iter()
            .find(|(h, _)| h == k)
            .map(|(_, e)| e.antecedents.clone())
    };
    for (f_hash, f) in log {
        if f.antecedents.is_empty() {
            continue;
        }
        let f_subject = if incoming_is_remove {
            add_subject(f)
        } else {
            remove_subject(f)
        };
        let Some(f_subject) = f_subject else {
            continue;
        };
        if f_subject == subject && are_concurrent(f_hash, incoming_hash, &lookup) {
            let remove_hash = if incoming_is_remove {
                *incoming_hash
            } else {
                *f_hash
            };
            return Some((*f_hash, remove_hash));
        }
    }
    None
}

/// Detect a role-thrash contradiction for an incoming `RoleGrant`/`RoleRevoke` on
/// subject S: an admitted, causally-concurrent role act on the same S whose *resulting
/// role differs* — "what role?" with no causal order to decide. This covers grant-vs-
/// revoke and grant-vs-grant-to-different-roles alike, while two acts with the *same*
/// resulting role (two identical grants, or revoke-vs-revoke) stay benign. Returns
/// `(partner_hash, label)`; resolution excludes the partner and does not apply the
/// incoming, reverting S to its pre-thrash role — no verdict on the contested change.
fn detect_role_thrash(
    log: &[(Hash, AssertionEnvelope)],
    incoming: &AssertionEnvelope,
    incoming_hash: &Hash,
) -> Option<(Hash, Hash)> {
    if incoming.antecedents.is_empty() {
        return None;
    }
    let subject = role_subject(incoming)?;
    let incoming_role = resulting_role(incoming)?;
    let lookup = |k: &Hash| -> Option<Vec<Hash>> {
        if k == incoming_hash {
            return Some(incoming.antecedents.clone());
        }
        log.iter()
            .find(|(h, _)| h == k)
            .map(|(_, e)| e.antecedents.clone())
    };
    for (f_hash, f) in log {
        if f.antecedents.is_empty() {
            continue;
        }
        let Some(f_subject) = role_subject(f) else {
            continue;
        };
        let Some(f_role) = resulting_role(f) else {
            continue;
        };
        if f_subject == subject
            && f_role != incoming_role
            && are_concurrent(f_hash, incoming_hash, &lookup)
        {
            return Some((*f_hash, min_hash(*incoming_hash, *f_hash)));
        }
    }
    None
}

/// Detect a competing-RuleChange contradiction (§7.6.1, F8) for an incoming admitted
/// `RuleChange` on rule R: an admitted, causally-concurrent RuleChange on the *same*
/// `rule_key` whose *new_value differs* — two constitutions for R with no causal order to
/// decide which governs, exactly the §7.6-class genuine contradiction RUN-02 F8 decided
/// must hard-stop rather than be silently content-address-tiebroken. Two concurrent
/// RuleChanges with the *same* value are concordant (no contradiction); RuleChanges on
/// *different* rule_keys never conflict. Returns `(partner_hash, label)`; resolution
/// excludes the partner and does not apply the incoming, so R keeps its pre-conflict value
/// — no verdict on either change. Mirrors `detect_role_thrash`, and surfaces identically
/// (`Contradiction(min_hash(...))`).
fn detect_competing_rulechange(
    log: &[(Hash, AssertionEnvelope)],
    incoming: &AssertionEnvelope,
    incoming_hash: &Hash,
) -> Option<(Hash, Hash)> {
    // Concurrency must be positively established: a RuleChange with empty antecedents makes no
    // causal claim, so bare re-sets never contradict and fold as sequential amendments in
    // canonical (merge_cmp) order. Consequence, deliberate: a threshold-1 rule can flap between
    // concurrent setters deterministically but without a contradiction banner. Every quorum-met
    // change carries its approvals as antecedents (Part 2 §7.2 R7), so the F8 marquee case always
    // trips this predicate. If the silent flap ever proves socially wrong for a Group, the
    // remedies are a Part 2 note or raising that rule's threshold. Decided knowingly (RUN-03
    // audit, 2026-07-14).
    if incoming.antecedents.is_empty() {
        return None;
    }
    let (rule_key, new_value) = rulechange_target(incoming)?;
    let lookup = |k: &Hash| -> Option<Vec<Hash>> {
        if k == incoming_hash {
            return Some(incoming.antecedents.clone());
        }
        log.iter()
            .find(|(h, _)| h == k)
            .map(|(_, e)| e.antecedents.clone())
    };
    for (f_hash, f) in log {
        if f.antecedents.is_empty() {
            continue;
        }
        let Some((f_key, f_value)) = rulechange_target(f) else {
            continue;
        };
        if f_key == rule_key
            && f_value != new_value
            && are_concurrent(f_hash, incoming_hash, &lookup)
        {
            return Some((*f_hash, min_hash(*incoming_hash, *f_hash)));
        }
    }
    None
}

/// Resolve a detected concurrent contradiction. Recompute membership by replaying this
/// group's governance log in canonical (`merge_cmp`) order **excluding** every hash in
/// `exclude` (the conflicting removes; the incoming fact is not in the log yet). The
/// contested parties are thereby retained — no verdict is rendered — and the result is
/// byte-identical regardless of arrival order, which is what fixes the divergence. The
/// state is flagged `Contradiction(label)` with the caller's canonical pair label.
/// Build the `ContestedEntry` for a detected mutual expulsion: the pair as data, both
/// parties as contested subjects, both removes withheld from the replay (no verdict).
fn mutual_expulsion_entry(g: &AssertionEnvelope, g_hash: &Hash, partner: Hash) -> ContestedEntry {
    let mut subjects = vec![g.author_principal];
    if let Some(s) = remove_subject(g) {
        if !subjects.contains(&s) {
            subjects.push(s);
        }
    }
    subjects.sort();
    let mut excluded = vec![partner, *g_hash];
    excluded.sort();
    ContestedEntry {
        pair: ContestedEntry::order_pair(partner, *g_hash),
        subjects,
        excluded,
    }
}

/// Detection sweep for the authorized-path contradiction shapes (both actors survive,
/// so both facts are authorized): removed-then-included, role thrash, and competing
/// RuleChange. Canonical entry regardless of which half of the pair arrived second.
fn detect_authorized_contested(
    log: &[(Hash, AssertionEnvelope)],
    env: &AssertionEnvelope,
    hash: &Hash,
) -> Option<ContestedEntry> {
    match env.assertion_type {
        AssertionType::MembershipAdd | AssertionType::MembershipRemove => {
            detect_removed_then_included(log, env, hash).map(|(partner, remove_hash)| {
                let subject = if env.assertion_type == AssertionType::MembershipRemove {
                    remove_subject(env)
                } else {
                    add_subject(env)
                }
                .expect("detection required a well-formed subject");
                ContestedEntry {
                    pair: ContestedEntry::order_pair(partner, *hash),
                    subjects: vec![subject],
                    excluded: vec![remove_hash],
                }
            })
        }
        AssertionType::RoleGrant | AssertionType::RoleRevoke => {
            detect_role_thrash(log, env, hash).map(|(partner, _)| {
                let mut excluded = vec![partner, *hash];
                excluded.sort();
                ContestedEntry {
                    pair: ContestedEntry::order_pair(partner, *hash),
                    // A role contest does not contest membership (§7.3.2 scopes
                    // CONTESTED to the membership projection).
                    subjects: vec![],
                    excluded,
                }
            })
        }
        AssertionType::RuleChange => {
            detect_competing_rulechange(log, env, hash).map(|(partner, _)| {
                let mut excluded = vec![partner, *hash];
                excluded.sort();
                ContestedEntry {
                    pair: ContestedEntry::order_pair(partner, *hash),
                    subjects: vec![],
                    excluded,
                }
            })
        }
        _ => None,
    }
}

/// The facts permanently withheld by pairs already CLOSED by admitted `Resolution`
/// facts in `log`: closing a pair is not un-deciding it — the contested facts never
/// re-apply; the group re-decides forward with new governance (§7.3.2). Derived from
/// the log itself so every replay (live, contradiction, resolution, rebuild) computes
/// the identical exclusion set with no extra state to carry.
fn resolved_excluded(log: &[(Hash, AssertionEnvelope)]) -> Vec<Hash> {
    let mut out = Vec::new();
    for (_, e) in log {
        if e.assertion_type == AssertionType::Resolution && e.payload.len() == 64 {
            let mut a = [0u8; 32];
            a.copy_from_slice(&e.payload[..32]);
            let mut b = [0u8; 32];
            b.copy_from_slice(&e.payload[32..64]);
            out.push(Hash::new(a));
            out.push(Hash::new(b));
        }
    }
    out
}

/// Deterministic full replay of the governance log in canonical (`merge_cmp`) order,
/// withholding `excluded` — the order-independent construction contradictions and
/// resolutions both rest on. The caller sets `fork_status`.
fn replay_excluding(
    log: &[(Hash, AssertionEnvelope)],
    excluded: &[Hash],
    head_hash: Hash,
    head_seq: u64,
) -> Result<GroupState, FoldError> {
    let mut envs: Vec<&(Hash, AssertionEnvelope)> =
        log.iter().filter(|(h, _)| !excluded.contains(h)).collect();
    envs.sort_by(|a, b| crate::model::merge_cmp(&a.1, &b.1));

    let genesis = envs
        .iter()
        .find(|(_, e)| e.assertion_type == AssertionType::GroupGenesis)
        .ok_or(FoldError::MissingGenesis)?;
    let mut ns = genesis_initial_state(&genesis.1, genesis.0)?;
    for ((h, env), seq) in envs
        .iter()
        .filter(|(_, e)| e.assertion_type != AssertionType::GroupGenesis)
        .zip(1u64..)
    {
        ns = apply_governance(&ns, env, *h, seq)?;
    }
    ns.computed_at_gov_head = head_hash;
    ns.computed_at_gov_seq = head_seq;
    Ok(ns)
}

/// The one governance state transition, shared by the live ingest path and the rebuild
/// replay so the two can never diverge (rebuild previously ran no detection at all — a
/// contested store silently lost its hard-stop on rebuild). Storage-free: takes the
/// admitted governance log as data. This is the surface Phase 2 extracts into the core.
fn compute_next_governance_state(
    log: &[(Hash, AssertionEnvelope)],
    current_state: &GroupState,
    envelope: &AssertionEnvelope,
    hash: Hash,
    gov_seq: u64,
    contested_new: Option<ContestedEntry>,
    fork_opt: Option<ForkStatus>,
) -> Result<GroupState, FoldError> {
    let mut ns = if envelope.assertion_type == AssertionType::Resolution {
        // §7.3.2: a Resolution closes exactly one open pair. Naming a pair that is not
        // open is refused loudly — a resolution of nothing is either a replay artifact
        // or an attempt to pre-authorize a verdict, and both must surface.
        let mut a = [0u8; 32];
        a.copy_from_slice(&envelope.payload[..32]);
        let mut b = [0u8; 32];
        b.copy_from_slice(&envelope.payload[32..64]);
        let pair = (Hash::new(a), Hash::new(b));
        let entries = current_state.contested_entries();
        let Some(idx) = entries.iter().position(|e| e.pair == pair) else {
            return Err(FoldError::AuthorizationFailed(
                "Resolution names no open contradiction pair".to_string(),
            ));
        };
        let remaining: Vec<ContestedEntry> = entries
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != idx)
            .map(|(_, e)| e.clone())
            .collect();
        let mut excluded: Vec<Hash> = resolved_excluded(log);
        excluded.push(pair.0);
        excluded.push(pair.1);
        for e in &remaining {
            excluded.extend_from_slice(&e.excluded);
        }
        excluded.sort();
        excluded.dedup();
        let mut ns = replay_excluding(log, &excluded, hash, gov_seq)?;
        ns.fork_status = if remaining.is_empty() {
            ForkStatus::Clean
        } else {
            ForkStatus::Contested(remaining)
        };
        ns
    } else if let Some(entry) = &contested_new {
        // A new contradiction joins whatever is already open — set-valued, so two
        // simultaneously open pairs are representable (the retired single slot's
        // structural failure). Replay withholds every open entry's excluded facts
        // plus everything already closed by resolutions.
        let mut entries: Vec<ContestedEntry> = current_state.contested_entries().to_vec();
        if !entries.contains(entry) {
            entries.push(entry.clone());
        }
        entries.sort();
        let mut excluded: Vec<Hash> = resolved_excluded(log);
        for e in &entries {
            excluded.extend_from_slice(&e.excluded);
        }
        excluded.sort();
        excluded.dedup();
        let mut ns = replay_excluding(log, &excluded, hash, gov_seq)?;
        ns.fork_status = ForkStatus::Contested(entries);
        ns
    } else if envelope.assertion_type == AssertionType::GroupGenesis {
        genesis_initial_state(envelope, hash)?
    } else {
        apply_governance(current_state, envelope, hash, gov_seq)?
    };

    // A slot-collision fork or under-determination never overrides a detected
    // contradiction (all are hard-stops; the contradiction is the precise one).
    if contested_new.is_none() {
        if let Some(ref fs) = fork_opt {
            ns.fork_status = fs.clone();
        }
    }
    // §7.6.1 under-determination: if this governance step left a group with no Owner
    // (required role vacant, no admissible successor), the fold hard-stops rather than
    // folding onward on a headless group. A fork or contradiction already surfaced
    // above takes precedence (all are hard-stops).
    if matches!(ns.fork_status, ForkStatus::Clean) && is_under_determined(&ns.members) {
        ns.fork_status = ForkStatus::UnderDetermined;
    }
    Ok(ns)
}

/// The outcome of a successfully evaluated-and-applied assertion.
#[derive(Debug, Clone, PartialEq)]
pub enum IngestResult {
    /// The assertion was new and has been applied.
    Applied {
        /// The content address of the applied assertion.
        hash: Hash,
    },
    /// The assertion was already present; no writes were made.
    Duplicate,
}

// ---------------------------------------------------------------------------
// The one transition: evaluate a fact against what the store holds
// ---------------------------------------------------------------------------

/// The occupancy of the governance-log slot a governance fact targets: genesis
/// always claims slot 0; every other governance fact claims the next slot. An
/// occupied target is a slot-collision fork.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotOccupancy {
    /// The sequence this fact would take.
    pub target_seq: u64,
    /// The hash already recorded at that sequence, if any.
    pub existing: Option<Hash>,
}

/// Everything the fold's decision needs, assembled by the adapter from what it
/// holds. The core never reads storage: this context IS the state-residency
/// inversion (E117 P2, R1) — state lives here as data, and the fold is a pure
/// function of it.
#[derive(Debug, Clone)]
pub struct FoldContext<'a> {
    /// The stored group state, if the group exists.
    pub current_state: Option<GroupState>,
    /// The admitted governance log for the group, in admission order.
    pub governance_log: &'a [(Hash, AssertionEnvelope)],
    /// The author device's highest admitted lamport, if any.
    pub last_device_lamport: Option<u64>,
    /// How many of the fact's declared antecedents the store holds.
    pub antecedents_present: usize,
    /// The held antecedent envelopes (the approval-gathering surface).
    pub antecedent_envelopes: Vec<AssertionEnvelope>,
    /// For governance facts: the targeted slot and its occupant.
    pub gov_slot: Option<SlotOccupancy>,
}

/// A successfully evaluated fact.
#[derive(Debug, Clone, PartialEq)]
pub enum Evaluation {
    /// A governance fact: the state to store and the slot it takes.
    Governance {
        /// The next projected state.
        next_state: GroupState,
        /// The governance sequence this fact occupies.
        gov_seq: u64,
    },
    /// A data-plane fact: authorized; no governance transition.
    DataPlane,
}

/// The state a group has before any genesis: the empty default, with the
/// resolution threshold at the product default (2) like every minted charter.
#[must_use]
pub fn empty_state() -> GroupState {
    GroupState {
        version: GROUP_STATE_WIRE_VERSION,
        computed_at_gov_head: Hash::new([0u8; 32]),
        computed_at_gov_seq: 0,
        members: Vec::new(),
        rules: GroupRules {
            add_member_threshold: 1,
            remove_member_threshold: 1,
            role_change_threshold: 1,
            rule_change_threshold: 1,
            resolution_threshold: 2,
        },
        fork_status: ForkStatus::Clean,
    }
}

/// Evaluate one assertion against the context — authorization-at-position, the
/// concurrent-contradiction detectors, lamport monotonicity, governance
/// completeness (§7.5.2's frontier-closure gate), k-of-n thresholds (V5′), the
/// slot-fork label, and the governance transition, in the live fold's order.
/// Pure: same context and fact in, same decision out, on every node and in the
/// rebuild replay alike.
///
/// The adapter remains responsible for what only it can know: duplicate
/// detection, signature verification, and credential resolution happen before
/// this call; persistence of the returned state happens after it.
pub fn evaluate(
    env: &AssertionEnvelope,
    ctx: &FoldContext<'_>,
    metrics: &impl crate::metrics::Metrics,
) -> Result<Evaluation, FoldError> {
    let hash = crate::model::envelope_hash(env);
    let current_state = match (&ctx.current_state, env.assertion_type) {
        (Some(s), _) => s.clone(),
        (None, AssertionType::GroupGenesis) => genesis_initial_state(env, hash)?,
        (None, _) => empty_state(),
    };

    // Authorization at position, with the mutual-expulsion carve-out: the second
    // half of A⊗B is a hard-stop, not a plain rejection.
    let contested_new: Option<ContestedEntry> = match check_authorization(&current_state, env) {
        Ok(()) => {
            if matches!(
                env.assertion_type,
                AssertionType::MembershipAdd
                    | AssertionType::MembershipRemove
                    | AssertionType::RoleGrant
                    | AssertionType::RoleRevoke
                    | AssertionType::RuleChange
            ) {
                detect_authorized_contested(ctx.governance_log, env, &hash)
            } else {
                None
            }
        }
        Err(e) => {
            let entry = if env.assertion_type == AssertionType::MembershipRemove {
                detect_mutual_expulsion(ctx.governance_log, env, &hash)
                    .map(|partner| mutual_expulsion_entry(env, &hash, partner))
            } else {
                None
            };
            match entry {
                Some(en) => Some(en),
                None => return Err(e),
            }
        }
    };

    // Lamport monotonicity per device.
    if let Some(last) = ctx.last_device_lamport {
        if env.lamport <= last {
            return Err(FoldError::LamportViolation {
                device: env.author_device,
                expected_gt: last,
                got: env.lamport,
            });
        }
    }

    // Governance completeness (§7.5.2): a governance fact folds only over its
    // full declared causal past.
    if is_governance(&env.assertion_type)
        && !env.antecedents.is_empty()
        && ctx.antecedents_present < env.antecedents.len()
    {
        return Err(FoldError::MissingAntecedents {
            have: ctx.antecedents_present,
            need: env.antecedents.len(),
        });
    }

    // k-of-n threshold (V5′): distinct approver personae, counted by lineage.
    if is_governance(&env.assertion_type) {
        if let Some(subject) = act_subject(env) {
            let required = threshold_for(&env.assertion_type, &current_state.rules);
            if required > 1 {
                let mut approvers = vec![env.author_principal];
                let want_type = env.assertion_type.to_u16();
                for ant in &ctx.antecedent_envelopes {
                    if approval_matches(ant, want_type, &subject) {
                        approvers.push(ant.author_principal);
                    }
                }
                let have = count_personae_by_lineage(&approvers);
                if have < required as usize {
                    return Err(FoldError::ThresholdNotMet {
                        have,
                        need: required as usize,
                    });
                }
            }
        }
    }

    if !is_governance(&env.assertion_type) {
        return Ok(Evaluation::DataPlane);
    }

    let slot = ctx.gov_slot.clone().unwrap_or(SlotOccupancy {
        target_seq: 0,
        existing: None,
    });

    // Slot-collision fork: the label is the maximum over every contender ever
    // observed for the slot — a pure function of the contender set, identical
    // under every arrival order.
    let fork_opt = slot.existing.map(|existing_hash| {
        let mut worst = if hash.as_bytes() > existing_hash.as_bytes() {
            hash
        } else {
            existing_hash
        };
        if let ForkStatus::ForkedFrom(prior) = &current_state.fork_status {
            if prior.as_bytes() > worst.as_bytes() {
                worst = *prior;
            }
        }
        ForkStatus::ForkedFrom(worst)
    });

    let next_state = compute_next_governance_state(
        ctx.governance_log,
        &current_state,
        env,
        hash,
        slot.target_seq,
        contested_new,
        fork_opt,
    )?;
    metrics.fact_folded();
    metrics.contested_open(next_state.contested_entries().len());
    Ok(Evaluation::Governance {
        next_state,
        gov_seq: slot.target_seq,
    })
}
