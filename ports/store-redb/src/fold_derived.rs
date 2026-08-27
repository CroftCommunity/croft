//! Fold engine — derived projection path for `local_storage_projection` — Stage 4.
//!
//! This module extends Stage 3's authoritative fold to also apply derived effects
//! (Section 6, step 3) in the SAME transaction as the authoritative writes.
//! It maintains `idx_nodes`, `idx_edges_out`, `idx_edges_in`, and `state_group`
//! (extended with gov_seq, gov_head, fork_status, and lamport-tagged members).

use std::sync::Arc;
use thiserror::Error;

use social_tree_core::model::{
    compute_hash, envelope_hash, AssertionEnvelope, AssertionType, DeviceId as TypesDeviceId,
    GroupId, Hash as TypesHash, KindTag, PrincipalId as TypesPrincipalId, TypedId,
};

use social_tree_core::metrics::NoopMetrics;
pub use social_tree_core::model::{
    ContestedEntry, ForkStatus, GroupState, MembershipView, GROUP_STATE_WIRE_VERSION,
};
use social_tree_core::update::{evaluate, Evaluation, FoldContext, SlotOccupancy};
pub use social_tree_core::update::{is_governance, rule_change_approval_subject, IngestResult};
pub use social_tree_core::wire::decode_envelope_from_canonical;

use social_tree_core::ports::{
    CredentialResolver, DeviceId as TraitsDeviceId, PrincipalId as TraitsPrincipalId, Verifier,
};

// The table constants come from `tables` and nowhere else. This module used
// to re-declare nine of them by string literal, plus two that never appeared
// in `tables` at all — see that module's header for what that cost.
use crate::tables::{
    encode_by_device_key, encode_edge_in_key, encode_edge_out_key, encode_gov_log_key, Db, DbError,
    EdgeMeta, EdgeType, NodeCard, AUTH_ASSERTIONS, AUTH_ASSERTIONS_BY_DEVICE, AUTH_GENESIS,
    AUTH_GOV_LOG, IDX_EDGES_IN, IDX_EDGES_OUT, IDX_NODES, META, STATE_BLOB_PRESENCE, STATE_GROUP,
};

use redb::{ReadableTable, TableDefinition};

const META_COMPARATOR_KEY: &[u8] = b"comparator_version";

// ---------------------------------------------------------------------------
// FoldError
// ---------------------------------------------------------------------------

/// All errors produced by the derived fold engine.
#[derive(Debug, Error)]
pub enum FoldError {
    #[error("signature invalid: {0}")]
    SignatureInvalid(String),

    #[error("credential invalid: {0}")]
    CredentialInvalid(String),

    #[error("authorization failed: {0}")]
    AuthorizationFailed(String),

    #[error("lamport violation for device {device}: expected > {expected_gt}, got {got}")]
    LamportViolation {
        device: TypesDeviceId,
        expected_gt: u64,
        got: u64,
    },

    #[error("malformed envelope: {0}")]
    MalformedEnvelope(String),

    /// The assertion declares causal antecedents that are not all present in the
    /// store yet. It is HELD BACK (not admitted, nothing written) so the caller
    /// retries it once the missing predecessors arrive — the completeness guard
    /// that keeps the fold from admitting a fact against an incomplete set
    /// (§7.5.2 frontier-closure). Transient, not a rejection.
    #[error("missing antecedents: have {have} of {need}")]
    MissingAntecedents { have: usize, need: usize },

    /// A threshold-governed act does not carry approvals from enough distinct personae
    /// (by lineage). Held/rejected until the k-of-n quorum is present (V5′).
    #[error("threshold not met: have {have} of {need} distinct personae")]
    ThresholdNotMet { have: usize, need: usize },

    #[error("storage error: {0}")]
    StorageError(String),

    #[error("unknown assertion type: 0x{0:04x}")]
    UnknownAssertionType(u16),
}

impl From<DbError> for FoldError {
    fn from(e: DbError) -> Self {
        FoldError::StorageError(e.to_string())
    }
}

impl From<redb::StorageError> for FoldError {
    fn from(e: redb::StorageError) -> Self {
        FoldError::StorageError(e.to_string())
    }
}

impl From<redb::TransactionError> for FoldError {
    fn from(e: redb::TransactionError) -> Self {
        FoldError::StorageError(e.to_string())
    }
}

impl From<redb::TableError> for FoldError {
    fn from(e: redb::TableError) -> Self {
        FoldError::StorageError(e.to_string())
    }
}

impl From<redb::CommitError> for FoldError {
    fn from(e: redb::CommitError) -> Self {
        FoldError::StorageError(e.to_string())
    }
}

impl social_tree_core::project::head_currency::BehindSignal for FoldError {
    fn is_missing_antecedents(&self) -> bool {
        matches!(self, FoldError::MissingAntecedents { .. })
    }
}

impl From<social_tree_core::update::FoldError> for FoldError {
    fn from(e: social_tree_core::update::FoldError) -> Self {
        use social_tree_core::update::FoldError as C;
        match e {
            C::MalformedEnvelope(m) => FoldError::MalformedEnvelope(m),
            // The adapter reads state bytes; malformed state is its storage concern.
            C::MalformedState(m) => FoldError::StorageError(m),
            C::AuthorizationFailed(m) => FoldError::AuthorizationFailed(m),
            C::ThresholdNotMet { have, need } => FoldError::ThresholdNotMet { have, need },
            C::LamportViolation {
                device,
                expected_gt,
                got,
            } => FoldError::LamportViolation {
                device,
                expected_gt,
                got,
            },
            C::MissingAntecedents { have, need } => FoldError::MissingAntecedents { have, need },
            C::MissingGenesis => {
                FoldError::StorageError("governance log has no genesis".to_string())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ForkStatus
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// GroupState (public, extended form for Stage 4)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Role encoding helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// RuleKey helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Authorization helpers (self-contained: the single authorization path in the crate)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Governance predicate
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// GroupState transitions
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// IngestResult
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// DerivedFold — the main fold engine for Stage 4
// ---------------------------------------------------------------------------

/// Fold engine that writes both authoritative and derived state in one transaction.
pub struct DerivedFold<V, C>
where
    V: Verifier,
    C: CredentialResolver,
{
    db: Arc<Db>,
    verifier: V,
    cred_resolver: C,
}

impl<V, C> DerivedFold<V, C>
where
    V: Verifier + Send + Sync,
    C: CredentialResolver + Send + Sync,
{
    /// Create a new `DerivedFold` over an already-schema'd [`Db`].
    ///
    /// Infallible, and deliberately so. The corpus version of this constructor
    /// opened all nine tables in a write transaction and could only
    /// `.expect()` when that failed — a fallible constructor that did not say
    /// so, and one that made a *reader* depend on a fold having been built
    /// first. Creating the schema is now [`Db`]'s job, done in `open` and
    /// `create_in_memory`, both of which already return a `Result`. So there
    /// is nothing left here that can fail, and nothing left to panic about.
    pub fn new(db: Arc<Db>, verifier: V, cred_resolver: C) -> Self {
        Self {
            db,
            verifier,
            cred_resolver,
        }
    }

    /// Read the current derived [`GroupState`] for `group`, or `None` if the group has
    /// no folded state yet. A read-only accessor (a read transaction + deserialize) that
    /// exposes the folded state the horizon manifest (EXP-H1) and other fold-level tests
    /// compute over. It does not fold or mutate.
    pub fn read_group_state(&self, group: &GroupId) -> Result<Option<GroupState>, FoldError> {
        let read_txn = self
            .db
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
            Some(bytes) => Ok(Some(GroupState::from_bytes(bytes.value())?)),
            None => Ok(None),
        }
    }

    /// Ingest an assertion, writing auth + derived state in one atomic transaction.
    /// The admitted governance log for `group` in slot order — the chain the
    /// derived views (e.g. `admission::issuance_view`) read from. Public so
    /// the surface can assemble admission context without a parallel store.
    pub fn governance_log(
        &self,
        group: &GroupId,
    ) -> Result<Vec<(TypesHash, AssertionEnvelope)>, FoldError> {
        group_governance_log(&self.db, group)
    }

    pub fn ingest(&self, envelope: &AssertionEnvelope) -> Result<IngestResult, FoldError> {
        // Step 1: Hash + duplicate check.
        let hash = envelope_hash(envelope);
        let hash_bytes: &[u8] = hash.as_bytes();

        {
            let read_txn = self
                .db
                .inner()
                .begin_read()
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            let table = read_txn
                .open_table(AUTH_ASSERTIONS)
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            if table
                .get(hash_bytes)
                .map_err(|e| FoldError::StorageError(e.to_string()))?
                .is_some()
            {
                return Ok(IngestResult::Duplicate);
            }
        }

        // Step 2: Verify signature.
        let canonical = envelope.canonical_bytes();
        self.verifier
            .verify(
                &TraitsDeviceId(*envelope.author_device.as_bytes()),
                &canonical,
                &envelope.signature,
            )
            .map_err(|e| FoldError::SignatureInvalid(e.to_string()))?;

        // Step 3: Validate credential.
        self.cred_resolver
            .resolve(
                &TraitsDeviceId(*envelope.author_device.as_bytes()),
                &TraitsPrincipalId(*envelope.author_principal.as_bytes()),
            )
            .map_err(|e| FoldError::CredentialInvalid(e.to_string()))?;

        // Steps 4–7 live in the core now (E117 P2 — the state-residency
        // inversion): assemble the FoldContext from what this store holds and let
        // social_tree_core::update::evaluate decide. The live fold and the rebuild
        // replay share that one transition, so they cannot diverge.
        let log = group_governance_log(&self.db, &envelope.group)?;
        let current_state = self.read_group_state(&envelope.group)?;

        let last_device_lamport = {
            let read_txn = self
                .db
                .inner()
                .begin_read()
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            let table = read_txn
                .open_table(AUTH_ASSERTIONS_BY_DEVICE)
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            let start = encode_by_device_key(&envelope.author_device, 0);
            let end = encode_by_device_key(&envelope.author_device, u64::MAX);
            // `next_back()`, not `last()`: this is a double-ended range, so
            // taking the end costs one step where `last()` walks every key in
            // the device's history to get there. Same answer, and the cost of
            // the wrong one grows with how much the device has ever written.
            match table
                .range(start.as_slice()..=end.as_slice())
                .map_err(|e| FoldError::StorageError(e.to_string()))?
                .next_back()
            {
                Some(entry) => {
                    let (k, _) = entry.map_err(|e| FoldError::StorageError(e.to_string()))?;
                    let key_bytes = k.value();
                    Some(u64::from_be_bytes(key_bytes[32..40].try_into().map_err(
                        |_| FoldError::StorageError("lamport decode".to_string()),
                    )?))
                }
                None => None,
            }
        };

        let (antecedents_present, antecedent_envelopes) = {
            let read_txn = self
                .db
                .inner()
                .begin_read()
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            let table = read_txn
                .open_table(AUTH_ASSERTIONS)
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            let mut present = 0usize;
            let mut envs = Vec::new();
            for ant in &envelope.antecedents {
                let ant_bytes: &[u8] = ant.as_bytes();
                if let Some(raw) = table
                    .get(ant_bytes)
                    .map_err(|e| FoldError::StorageError(e.to_string()))?
                {
                    present += 1;
                    let bytes: &[u8] = raw.value();
                    if bytes.len() > 1 {
                        if let Ok(env) = decode_envelope_from_canonical(&bytes[1..]) {
                            envs.push(env);
                        }
                    }
                }
            }
            (present, envs)
        };

        let gov_slot = if is_governance(&envelope.assertion_type) {
            let read_txn = self
                .db
                .inner()
                .begin_read()
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            let table = read_txn
                .open_table(AUTH_GOV_LOG)
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            let start = encode_gov_log_key(&envelope.group, 0);
            let end = encode_gov_log_key(&envelope.group, u64::MAX);
            let count = table
                .range(start.as_slice()..=end.as_slice())
                .map_err(|e| FoldError::StorageError(e.to_string()))?
                .count();
            let target_seq: u64 = if envelope.assertion_type == AssertionType::GroupGenesis {
                0
            } else {
                count as u64
            };
            let fork_key = encode_gov_log_key(&envelope.group, target_seq);
            let existing = table
                .get(fork_key.as_slice())
                .map_err(|e| FoldError::StorageError(e.to_string()))?
                .map(|v| {
                    let mut h = [0u8; 32];
                    h.copy_from_slice(v.value());
                    TypesHash::new(h)
                });
            Some(SlotOccupancy {
                target_seq,
                existing,
            })
        } else {
            None
        };

        let ctx = FoldContext {
            current_state,
            governance_log: &log,
            last_device_lamport,
            antecedents_present,
            antecedent_envelopes,
            gov_slot,
        };
        let (gov_seq_opt, next_state_opt) = match evaluate(envelope, &ctx, &NoopMetrics)? {
            Evaluation::Governance {
                next_state,
                gov_seq,
            } => (Some(gov_seq), Some(next_state)),
            Evaluation::DataPlane => (None, None),
        };

        // Step 8 (I1 / I5): All writes in ONE transaction.
        {
            let write_txn = self
                .db
                .inner()
                .begin_write()
                .map_err(|e| FoldError::StorageError(e.to_string()))?;

            // 8a. auth_assertions
            {
                let mut table = write_txn
                    .open_table(AUTH_ASSERTIONS)
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
                let value = envelope.canonical_bytes_with_sig();
                let mut versioned = Vec::with_capacity(1 + value.len());
                versioned.push(0x01u8);
                versioned.extend_from_slice(&value);
                table
                    .insert(hash_bytes, versioned.as_slice())
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
            }

            // 8b. auth_assertions_by_device
            {
                let mut table = write_txn
                    .open_table(AUTH_ASSERTIONS_BY_DEVICE)
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
                let key = encode_by_device_key(&envelope.author_device, envelope.lamport);
                table
                    .insert(key.as_slice(), hash_bytes)
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
            }

            // 8c. Governance: auth_gov_log + state_group + auth_genesis
            if let (Some(gov_seq), Some(ref next_state)) = (gov_seq_opt, &next_state_opt) {
                {
                    let mut table = write_txn
                        .open_table(AUTH_GOV_LOG)
                        .map_err(|e| FoldError::StorageError(e.to_string()))?;
                    let key = encode_gov_log_key(&envelope.group, gov_seq);
                    table
                        .insert(key.as_slice(), hash_bytes)
                        .map_err(|e| FoldError::StorageError(e.to_string()))?;
                }
                {
                    let mut table = write_txn
                        .open_table(STATE_GROUP)
                        .map_err(|e| FoldError::StorageError(e.to_string()))?;
                    let state_bytes = next_state.to_bytes();
                    table
                        .insert(envelope.group.as_bytes().as_ref(), state_bytes.as_slice())
                        .map_err(|e| FoldError::StorageError(e.to_string()))?;
                }
                if envelope.assertion_type == AssertionType::GroupGenesis {
                    let mut table = write_txn
                        .open_table(AUTH_GENESIS)
                        .map_err(|e| FoldError::StorageError(e.to_string()))?;
                    let mut versioned = Vec::with_capacity(1 + envelope.payload.len());
                    versioned.push(0x01u8);
                    versioned.extend_from_slice(&envelope.payload);
                    table
                        .insert(envelope.group.as_bytes().as_ref(), versioned.as_slice())
                        .map_err(|e| FoldError::StorageError(e.to_string()))?;
                }
            }

            // 8d. Derived: upsert author principal node in idx_nodes (all types).
            {
                let author_typed_id = TypedId::new(
                    KindTag::Principal,
                    TypesHash::new(*envelope.author_principal.as_bytes()),
                );
                upsert_node_stub(
                    &write_txn,
                    &author_typed_id,
                    envelope.author_principal,
                    envelope.lamport,
                    false, // already exists = no forced update; stub only
                    None,
                )
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            }

            // 8e. Type-specific derived effects.
            apply_derived_effects_free(&write_txn, envelope, hash, &next_state_opt)?;

            write_txn
                .commit()
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
        }

        Ok(IngestResult::Applied { hash })
    }
}

// ---------------------------------------------------------------------------
// Free function: apply derived effects (used by both DerivedFold and rebuild)
// ---------------------------------------------------------------------------

fn apply_derived_effects_free(
    txn: &redb::WriteTransaction,
    env: &AssertionEnvelope,
    hash: TypesHash,
    next_state: &Option<GroupState>,
) -> Result<(), FoldError> {
    let group_typed_id = TypedId::new(KindTag::Group, TypesHash::new(*env.group.as_bytes()));

    match env.assertion_type {
        AssertionType::GroupGenesis => {
            // Create the group's NodeCard.
            upsert_node_full(
                txn,
                &group_typed_id,
                KindTag::Group,
                true,
                "".to_string(),
                env.author_principal,
                env.lamport,
                None,
            )
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        }

        AssertionType::MembershipAdd => {
            if env.payload.len() < 33 {
                return Err(FoldError::MalformedEnvelope(
                    "MembershipAdd payload too short".to_string(),
                ));
            }
            let mut pid_bytes = [0u8; 32];
            pid_bytes.copy_from_slice(&env.payload[..32]);
            let invitee = TypesPrincipalId::new(pid_bytes);
            let invitee_typed = TypedId::new(KindTag::Principal, TypesHash::new(pid_bytes));

            // Upsert invitee node stub.
            upsert_node_stub(txn, &invitee_typed, invitee, env.lamport, false, None)
                .map_err(|e| FoldError::StorageError(e.to_string()))?;

            // Write MEMBER_OF edge: invitee → group (atomically in both tables).
            let edge_meta = EdgeMeta {
                version: 1,
                since_lamport: env.lamport,
                since_assertion: hash,
                present: true,
            };
            write_edge_atomic(
                txn,
                &invitee_typed,
                EdgeType::MemberOf,
                &group_typed_id,
                &edge_meta,
            )
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        }

        AssertionType::MembershipRemove => {
            if env.payload.len() < 32 {
                return Err(FoldError::MalformedEnvelope(
                    "MembershipRemove payload too short".to_string(),
                ));
            }
            let mut pid_bytes = [0u8; 32];
            pid_bytes.copy_from_slice(&env.payload[..32]);
            let subject_typed = TypedId::new(KindTag::Principal, TypesHash::new(pid_bytes));

            // Mark MEMBER_OF edge present=false.
            let edge_meta = EdgeMeta {
                version: 1,
                since_lamport: env.lamport,
                since_assertion: hash,
                present: false,
            };
            write_edge_atomic(
                txn,
                &subject_typed,
                EdgeType::MemberOf,
                &group_typed_id,
                &edge_meta,
            )
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        }

        AssertionType::RoleGrant | AssertionType::RoleRevoke => {
            // State is updated in GroupState; no additional derived edge needed.
        }

        AssertionType::TokenIssuance | AssertionType::TokenRevocation => {
            // Chain data only (P4): derived views read these from the
            // governance log (admission::issuance_view); no graph edge.
        }

        AssertionType::Admission => {
            // The span-opening enactment record (P4): mirror the fold —
            // the MEMBER_OF edge follows the seated roster, so the
            // standing ceiling is respected exactly (a banned lineage's
            // admission fact folds without seating, and writes no edge).
            if env.payload.len() < 104 {
                return Err(FoldError::MalformedEnvelope(
                    "Admission payload too short".to_string(),
                ));
            }
            let mut pid_bytes = [0u8; 32];
            pid_bytes.copy_from_slice(&env.payload[32..64]);
            let merged = TypesPrincipalId::new(pid_bytes);
            let seated = next_state.as_ref().is_some_and(|st| {
                matches!(
                    st.membership(&merged),
                    social_tree_core::model::MembershipView::Member(_)
                )
            });
            if seated {
                let merged_typed = TypedId::new(KindTag::Principal, TypesHash::new(pid_bytes));
                upsert_node_stub(txn, &merged_typed, merged, env.lamport, false, None)
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
                let edge_meta = EdgeMeta {
                    version: 1,
                    since_lamport: env.lamport,
                    since_assertion: hash,
                    present: true,
                };
                write_edge_atomic(
                    txn,
                    &merged_typed,
                    EdgeType::MemberOf,
                    &group_typed_id,
                    &edge_meta,
                )
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            }
        }

        AssertionType::RuleChange => {
            // State updated in GroupState; no additional derived edge.
        }

        AssertionType::AttachmentAdd => {
            if env.payload.is_empty() {
                return Err(FoldError::MalformedEnvelope(
                    "AttachmentAdd payload too short".to_string(),
                ));
            }
            let kind_byte = env.payload[0];
            let kind = KindTag::from_u8(kind_byte).ok_or_else(|| {
                FoldError::MalformedEnvelope(format!(
                    "AttachmentAdd: unknown KindTag byte 0x{:02x}",
                    kind_byte
                ))
            })?;

            // Decode title.
            let (title, blob_hash, attachment_hash) =
                decode_attachment_add_payload(&env.payload, hash)?;

            let attach_typed = TypedId::new(kind, attachment_hash);

            // Create NodeCard for the attachment.
            upsert_node_full(
                txn,
                &attach_typed,
                kind,
                true,
                title,
                env.author_principal,
                env.lamport,
                blob_hash,
            )
            .map_err(|e| FoldError::StorageError(e.to_string()))?;

            // HAS_ATTACHMENT edge: group → attachment.
            let edge_meta = EdgeMeta {
                version: 1,
                since_lamport: env.lamport,
                since_assertion: hash,
                present: true,
            };
            write_edge_atomic(
                txn,
                &group_typed_id,
                EdgeType::HasAttachment,
                &attach_typed,
                &edge_meta,
            )
            .map_err(|e| FoldError::StorageError(e.to_string()))?;

            // Stub BlobPresence if blob_hash is Some.
            if let Some(bh) = blob_hash {
                let mut table = txn
                    .open_table(STATE_BLOB_PRESENCE)
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
                // key = blob hash; value = 0x00 (stub = not confirmed present)
                table
                    .insert(bh.as_bytes().as_ref(), [0x00u8].as_ref())
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
            }

            // Upsert stub for any typed_id referenced (I8): the attach_typed itself.
            upsert_node_stub(
                txn,
                &attach_typed,
                env.author_principal,
                env.lamport,
                false,
                None,
            )
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        }

        AssertionType::Message => {
            // Derive a stable hash for this message node from the envelope hash.
            let msg_hash = compute_hash(hash.as_bytes());
            let msg_typed = TypedId::new(KindTag::ArtifactChat, msg_hash);

            upsert_node_full(
                txn,
                &msg_typed,
                KindTag::ArtifactChat,
                true,
                String::new(),
                env.author_principal,
                env.lamport,
                None,
            )
            .map_err(|e| FoldError::StorageError(e.to_string()))?;

            // REFERENCES edge: from the target scope → message. The scope is
            // the channel (ArtifactChat) when the payload names one, else the
            // group. This makes a message addressable per channel; group-level
            // messages (no channel) still hang off the group as before.
            let scope = match social_tree_core::model::decode_message_payload(&env.payload) {
                Some((_, _, Some(channel))) => channel,
                _ => group_typed_id,
            };
            let edge_meta = EdgeMeta {
                version: 1,
                since_lamport: env.lamport,
                since_assertion: hash,
                present: true,
            };
            write_edge_atomic(txn, &scope, EdgeType::References, &msg_typed, &edge_meta)
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
        }

        AssertionType::ArtifactRef => {
            if env.payload.len() < 33 {
                return Err(FoldError::MalformedEnvelope(
                    "ArtifactRef payload too short".to_string(),
                ));
            }
            let kind_byte = env.payload[0];
            let kind = KindTag::from_u8(kind_byte).ok_or_else(|| {
                FoldError::MalformedEnvelope(format!(
                    "ArtifactRef: unknown KindTag byte 0x{:02x}",
                    kind_byte
                ))
            })?;
            let mut h = [0u8; 32];
            h.copy_from_slice(&env.payload[1..33]);
            let artifact_typed = TypedId::new(kind, TypesHash::new(h));

            // Upsert stub for unknown ref (I8).
            upsert_node_stub(
                txn,
                &artifact_typed,
                env.author_principal,
                env.lamport,
                false,
                None,
            )
            .map_err(|e| FoldError::StorageError(e.to_string()))?;

            // REFERENCES edge: group → artifact.
            let edge_meta = EdgeMeta {
                version: 1,
                since_lamport: env.lamport,
                since_assertion: hash,
                present: true,
            };
            write_edge_atomic(
                txn,
                &group_typed_id,
                EdgeType::References,
                &artifact_typed,
                &edge_meta,
            )
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        }

        AssertionType::Vouch => {
            // I5 gate already checked in check_authorization.
            if env.payload.len() < 32 {
                return Err(FoldError::MalformedEnvelope(
                    "Vouch payload too short".to_string(),
                ));
            }
            let mut pid_bytes = [0u8; 32];
            pid_bytes.copy_from_slice(&env.payload[..32]);
            let subject_typed = TypedId::new(KindTag::Principal, TypesHash::new(pid_bytes));
            let author_typed = TypedId::new(
                KindTag::Principal,
                TypesHash::new(*env.author_principal.as_bytes()),
            );

            // VOUCHES edge: author → subject.
            let edge_meta = EdgeMeta {
                version: 1,
                since_lamport: env.lamport,
                since_assertion: hash,
                present: true,
            };
            write_edge_atomic(
                txn,
                &author_typed,
                EdgeType::Vouches,
                &subject_typed,
                &edge_meta,
            )
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        }

        // Approval (V5′) has no derived effect of its own — it is evidence gathered
        // by the act it approves (Step 5.6). Stored in auth_assertions; nothing else.
        AssertionType::Approval => {}

        // Resolution (§7.3.2) acts entirely on the governance projection (closing an
        // open contested pair in GroupState); it has no derived-graph effect.
        AssertionType::Resolution => {}
    }

    // Suppress unused-variable warning for next_state.
    let _ = next_state;
    Ok(())
}

// ---------------------------------------------------------------------------
// load_or_init_state helper for DerivedFold
// ---------------------------------------------------------------------------

impl<V, C> DerivedFold<V, C>
where
    V: Verifier + Send + Sync,
    C: CredentialResolver + Send + Sync,
{
}

// ---------------------------------------------------------------------------
// Concurrent-contradiction (§7.6.1) — mutual-expulsion detection + resolution
// ---------------------------------------------------------------------------

/// This group's admitted governance envelopes (with their stored hashes), in
/// gov-log order.
fn group_governance_log(
    db: &Db,
    group: &GroupId,
) -> Result<Vec<(TypesHash, AssertionEnvelope)>, FoldError> {
    let read_txn = db
        .inner()
        .begin_read()
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let gov = read_txn
        .open_table(AUTH_GOV_LOG)
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let auth = read_txn
        .open_table(AUTH_ASSERTIONS)
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let start = encode_gov_log_key(group, 0);
    let end = encode_gov_log_key(group, u64::MAX);
    let mut out = Vec::new();
    for item in gov
        .range(start.as_slice()..=end.as_slice())
        .map_err(|e| FoldError::StorageError(e.to_string()))?
    {
        let (_, v) = item.map_err(|e| FoldError::StorageError(e.to_string()))?;
        let mut h = [0u8; 32];
        h.copy_from_slice(v.value());
        let hash = TypesHash::new(h);
        let hash_key: &[u8] = hash.as_bytes();
        if let Some(bytes) = auth
            .get(hash_key)
            .map_err(|e| FoldError::StorageError(e.to_string()))?
        {
            let raw: &[u8] = bytes.value();
            if raw.is_empty() {
                continue;
            }
            if let Ok(env) = decode_envelope_from_canonical(&raw[1..]) {
                out.push((hash, env));
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Rebuild operation (I3)
// ---------------------------------------------------------------------------

/// The comparator version this store's derived tables were last folded under, or `None` if
/// the store predates comparator versioning (i.e., was folded under v1).
pub fn comparator_version(db: &Arc<Db>) -> Result<Option<u8>, FoldError> {
    let read_txn = db
        .inner()
        .begin_read()
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let table = match read_txn.open_table(META) {
        Ok(t) => t,
        // A store written before the META table existed has no stamp at all.
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
        Err(e) => return Err(FoldError::StorageError(e.to_string())),
    };
    let stamped = table
        .get(META_COMPARATOR_KEY)
        .map_err(|e| FoldError::StorageError(e.to_string()))?
        .and_then(|v| v.value().first().copied());
    Ok(stamped)
}

/// Does this store need a rebuild before its derived state can be trusted?
///
/// True iff the store holds at least one assertion **and** its stamped comparator version is
/// not the current [`social_tree_core::model::MERGE_CMP_VERSION`]. A store folded under an older
/// comparator can project a different resolved state for any group containing cross-device
/// same-lamport (genuinely concurrent) facts, so it must be re-folded — silently reading it
/// would present a v1 resolution as if it were v2. A fresh/empty store needs nothing.
pub fn needs_rebuild(db: &Arc<Db>) -> Result<bool, FoldError> {
    // Empty store: nothing was ever folded, so no stale resolution can exist.
    let non_empty = {
        let read_txn = db
            .inner()
            .begin_read()
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        match read_txn.open_table(AUTH_ASSERTIONS) {
            Ok(t) => t
                .iter()
                .map_err(|e| FoldError::StorageError(e.to_string()))?
                .next()
                .is_some(),
            Err(redb::TableError::TableDoesNotExist(_)) => false,
            Err(e) => return Err(FoldError::StorageError(e.to_string())),
        }
    };
    if !non_empty {
        return Ok(false);
    }
    Ok(comparator_version(db)? != Some(social_tree_core::model::MERGE_CMP_VERSION))
}

/// Stamp the current comparator version into the store's meta table.
fn stamp_comparator_version(db: &Arc<Db>) -> Result<(), FoldError> {
    let write_txn = db
        .inner()
        .begin_write()
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    {
        let mut table = write_txn
            .open_table(META)
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        table
            .insert(
                META_COMPARATOR_KEY,
                &[social_tree_core::model::MERGE_CMP_VERSION][..],
            )
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
    }
    write_txn
        .commit()
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    Ok(())
}

/// Drop all derived tables and re-fold all assertions from `auth_assertions`
/// in causal (merge_cmp) order to reproduce byte-identical derived state.
pub fn rebuild(
    db: &Arc<Db>,
    // Unused today and kept anyway: a rebuild replays assertions that were
    // already verified on the way in, so it does not re-verify. If that ever
    // stops being true — a store restored from an untrusted source, say — the
    // capability has to be in hand at the call site, and widening a signature
    // later is a breaking change for every caller.
    _verifier: &impl Verifier,
    _cred_resolver: &impl CredentialResolver,
) -> Result<(), FoldError> {
    // Step 1: Collect all assertions from auth_assertions.
    let envelopes: Vec<AssertionEnvelope> = {
        let read_txn = db
            .inner()
            .begin_read()
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
        let table = read_txn
            .open_table(AUTH_ASSERTIONS)
            .map_err(|e| FoldError::StorageError(e.to_string()))?;

        let mut envs = Vec::new();
        for item in table
            .iter()
            .map_err(|e: redb::StorageError| FoldError::StorageError(e.to_string()))?
        {
            let (_, v) =
                item.map_err(|e: redb::StorageError| FoldError::StorageError(e.to_string()))?;
            let raw: &[u8] = v.value();
            // Skip version byte (raw[0]).
            if raw.is_empty() {
                continue;
            }
            let env = decode_envelope_from_canonical(&raw[1..]).map_err(|e| {
                FoldError::MalformedEnvelope(format!("rebuild: decode failed: {}", e))
            })?;
            envs.push(env);
        }
        envs
    };

    // Step 2: Sort by causal order (merge_cmp).
    let mut envs = envelopes;
    envs.sort_by(social_tree_core::model::merge_cmp);

    // Step 3: Drop all derived and state tables.
    {
        let write_txn = db
            .inner()
            .begin_write()
            .map_err(|e| FoldError::StorageError(e.to_string()))?;

        truncate_table(&write_txn, IDX_NODES)?;
        truncate_table(&write_txn, IDX_EDGES_OUT)?;
        truncate_table(&write_txn, IDX_EDGES_IN)?;
        truncate_table(&write_txn, STATE_GROUP)?;
        truncate_table(&write_txn, STATE_BLOB_PRESENCE)?;
        // Auth genesis is authoritative; we keep it but we'll re-derive it.
        truncate_table(&write_txn, AUTH_GENESIS)?;
        // Gov log must also be cleared so that replay computes seq numbers fresh,
        // producing byte-identical state (I3 invariant).
        truncate_table(&write_txn, AUTH_GOV_LOG)?;

        write_txn
            .commit()
            .map_err(|e| FoldError::StorageError(e.to_string()))?;
    }

    // Step 4: Re-ingest all assertions in causal order.
    // We need a fold engine that skips re-writing auth_assertions (already present).
    // DerivedFold::ingest handles duplicates via the auth_assertions check — but
    // those ARE present. We must apply derived effects directly.
    let fold = DerivedFoldReplay { db: Arc::clone(db) };
    for env in &envs {
        fold.replay(env)?;
    }

    // Step 5: stamp the comparator version the derived state was just folded under, so
    // `needs_rebuild` can detect a store whose resolution predates a comparator change.
    stamp_comparator_version(db)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal replay engine for rebuild (no auth/sig checks, just derived effects)
// ---------------------------------------------------------------------------

struct DerivedFoldReplay {
    db: Arc<Db>,
}

impl DerivedFoldReplay {
    fn replay(&self, env: &AssertionEnvelope) -> Result<(), FoldError> {
        let hash = envelope_hash(env);

        // Load the stored state, if any (evaluate owns the genesis/empty defaults).
        let current_state: Option<GroupState> = {
            let read_txn = self
                .db
                .inner()
                .begin_read()
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            let table = read_txn
                .open_table(STATE_GROUP)
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            match table
                .get(env.group.as_bytes().as_ref())
                .map_err(|e| FoldError::StorageError(e.to_string()))?
            {
                Some(bytes) => Some(GroupState::from_bytes(bytes.value())?),
                None => None,
            }
        };

        // Compute gov_seq (genesis always at 0; others at current count).
        let gov_seq_opt: Option<u64> = if is_governance(&env.assertion_type) {
            let read_txn = self
                .db
                .inner()
                .begin_read()
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            let table = read_txn
                .open_table(AUTH_GOV_LOG)
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            let start = encode_gov_log_key(&env.group, 0);
            let end = encode_gov_log_key(&env.group, u64::MAX);
            let count = table
                .range(start.as_slice()..=end.as_slice())
                .map_err(|e| FoldError::StorageError(e.to_string()))?
                .count();
            if env.assertion_type == AssertionType::GroupGenesis {
                Some(0)
            } else {
                Some(count as u64)
            }
        } else {
            None
        };

        // Compute next state through the ONE shared transition (core evaluate):
        // the rebuild replays only facts live ingest admitted, so lamport and
        // antecedent gates are passed as satisfied (they were, on admission) and
        // the rebuilt log is collision-free (no slot occupant).
        let next_state_opt: Option<GroupState> = if is_governance(&env.assertion_type) {
            let gov_seq = gov_seq_opt
                .expect("invariant: gov_seq_opt is Some for governance assertions (set under the same is_governance predicate above)");
            let log = group_governance_log(&self.db, &env.group)?;
            let antecedent_envelopes: Vec<AssertionEnvelope> = {
                let read_txn = self
                    .db
                    .inner()
                    .begin_read()
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
                let table = read_txn
                    .open_table(AUTH_ASSERTIONS)
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
                let mut envs = Vec::new();
                for ant in &env.antecedents {
                    let ant_bytes: &[u8] = ant.as_bytes();
                    if let Some(raw) = table
                        .get(ant_bytes)
                        .map_err(|e| FoldError::StorageError(e.to_string()))?
                    {
                        let bytes: &[u8] = raw.value();
                        if bytes.len() > 1 {
                            if let Ok(e2) = decode_envelope_from_canonical(&bytes[1..]) {
                                envs.push(e2);
                            }
                        }
                    }
                }
                envs
            };
            let ctx = FoldContext {
                current_state: current_state.clone(),
                governance_log: &log,
                last_device_lamport: None, // admitted on arrival; replay re-orders across devices
                antecedents_present: env.antecedents.len(),
                antecedent_envelopes,
                gov_slot: Some(SlotOccupancy {
                    target_seq: gov_seq,
                    existing: None,
                }),
            };
            match evaluate(env, &ctx, &NoopMetrics)? {
                Evaluation::Governance { next_state, .. } => Some(next_state),
                Evaluation::DataPlane => None,
            }
        } else {
            None
        };

        // Write in one transaction.
        let write_txn = self
            .db
            .inner()
            .begin_write()
            .map_err(|e| FoldError::StorageError(e.to_string()))?;

        // Re-write auth_gov_log and state_group.
        if let (Some(gov_seq), Some(ref ns)) = (gov_seq_opt, &next_state_opt) {
            {
                let mut table = write_txn
                    .open_table(AUTH_GOV_LOG)
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
                let key = encode_gov_log_key(&env.group, gov_seq);
                table
                    .insert(key.as_slice(), hash.as_bytes().as_ref())
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
            }
            {
                let mut table = write_txn
                    .open_table(STATE_GROUP)
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
                let state_bytes = ns.to_bytes();
                table
                    .insert(env.group.as_bytes().as_ref(), state_bytes.as_slice())
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
            }
            if env.assertion_type == AssertionType::GroupGenesis {
                let mut table = write_txn
                    .open_table(AUTH_GENESIS)
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
                let mut versioned = Vec::with_capacity(1 + env.payload.len());
                versioned.push(0x01u8);
                versioned.extend_from_slice(&env.payload);
                table
                    .insert(env.group.as_bytes().as_ref(), versioned.as_slice())
                    .map_err(|e| FoldError::StorageError(e.to_string()))?;
            }
        }

        // Author node.
        let author_typed_id = TypedId::new(
            KindTag::Principal,
            TypesHash::new(*env.author_principal.as_bytes()),
        );
        upsert_node_stub(
            &write_txn,
            &author_typed_id,
            env.author_principal,
            env.lamport,
            false,
            None,
        )
        .map_err(|e| FoldError::StorageError(e.to_string()))?;

        // Derived effects (call the free function directly).
        apply_derived_effects_free(&write_txn, env, hash, &next_state_opt)?;

        write_txn
            .commit()
            .map_err(|e| FoldError::StorageError(e.to_string()))?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helper: truncate a table inside an open write transaction
// ---------------------------------------------------------------------------

fn truncate_table(
    txn: &redb::WriteTransaction,
    def: TableDefinition<'static, &'static [u8], &'static [u8]>,
) -> Result<(), FoldError> {
    let mut table = txn
        .open_table(def)
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    // Collect all keys first, then delete them.
    let keys: Vec<Vec<u8>> = {
        let iter = table
            .iter()
            .map_err(|e: redb::StorageError| FoldError::StorageError(e.to_string()))?;
        let mut collected = Vec::new();
        for item in iter {
            let (k, _v) =
                item.map_err(|e: redb::StorageError| FoldError::StorageError(e.to_string()))?;
            let key_bytes: &[u8] = k.value();
            collected.push(key_bytes.to_vec());
        }
        collected
    };
    for k in keys {
        table
            .remove(k.as_slice())
            .map_err(|e: redb::StorageError| FoldError::StorageError(e.to_string()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helper: upsert a node stub (present=false) if not already present
// ---------------------------------------------------------------------------

fn upsert_node_stub(
    txn: &redb::WriteTransaction,
    typed_id: &TypedId,
    created_by: TypesPrincipalId,
    created_at: u64,
    _force: bool,
    blob_hash: Option<TypesHash>,
) -> Result<(), DbError> {
    let mut table = txn.open_table(IDX_NODES)?;
    let key = typed_id.as_bytes().as_ref();
    let existing_card: Option<NodeCard> = table
        .get(key)?
        .and_then(|g| NodeCard::from_bytes(g.value()).ok());
    if let Some(nc) = existing_card {
        // Node already exists. `created_at`/`created_by` must be a DETERMINISTIC
        // function of the log, not of fold/ingest order (I3 + cross-peer
        // convergence): keep the canonical MIN (created_at, created_by). Lowering
        // is monotonic + commutative, so any fold order — and rebuild's canonical
        // order — converge. (Fix 2026-06-26: was first-touch-wins => divergence.)
        if (created_at, *created_by.as_bytes()) < (nc.created_at, *nc.created_by.as_bytes()) {
            let updated = NodeCard {
                created_at,
                created_by,
                ..nc
            };
            table.insert(key, updated.to_bytes().as_slice())?;
        }
        return Ok(());
    }
    let nc = NodeCard {
        version: 1,
        kind: typed_id.kind(),
        present: false,
        title: String::new(),
        created_by,
        created_at,
        blob_hash,
    };
    table.insert(key, nc.to_bytes().as_slice())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helper: upsert a full node card (present=true), overwriting stubs
// ---------------------------------------------------------------------------

// Eight arguments where clippy wants seven. They are the NodeCard's own fields
// plus the transaction to write them in, so a struct here would be `NodeCard`
// with a different name; the honest simplification is not available.
#[allow(clippy::too_many_arguments)]
fn upsert_node_full(
    txn: &redb::WriteTransaction,
    typed_id: &TypedId,
    kind: KindTag,
    present: bool,
    title: String,
    created_by: TypesPrincipalId,
    created_at: u64,
    blob_hash: Option<TypesHash>,
) -> Result<(), DbError> {
    let mut table = txn.open_table(IDX_NODES)?;
    let key = typed_id.as_bytes().as_ref();
    let existing_card: Option<NodeCard> = table
        .get(key)?
        .and_then(|g| NodeCard::from_bytes(g.value()).ok());
    // `created_at`/`created_by` = canonical MIN over all referencing assertions
    // (order-insensitive; see upsert_node_stub). `present` is monotonic up.
    let (eff_at, eff_by) = match &existing_card {
        Some(nc)
            if (nc.created_at, *nc.created_by.as_bytes())
                < (created_at, *created_by.as_bytes()) =>
        {
            (nc.created_at, nc.created_by)
        }
        _ => (created_at, created_by),
    };
    if let Some(nc) = existing_card {
        if nc.present {
            // Already full: keep it, only converge created_at/created_by to MIN.
            let updated = NodeCard {
                created_at: eff_at,
                created_by: eff_by,
                ..nc
            };
            table.insert(key, updated.to_bytes().as_slice())?;
            return Ok(());
        }
        // existing is a stub being upgraded to full; fall through and rewrite.
    }
    let nc = NodeCard {
        version: 1,
        kind,
        present,
        title,
        created_by: eff_by,
        created_at: eff_at,
        blob_hash,
    };
    table.insert(key, nc.to_bytes().as_slice())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helper: write an edge to both idx_edges_out and idx_edges_in atomically
// ---------------------------------------------------------------------------

fn write_edge_atomic(
    txn: &redb::WriteTransaction,
    source: &TypedId,
    edge_type: EdgeType,
    target: &TypedId,
    meta: &EdgeMeta,
) -> Result<(), DbError> {
    let meta_bytes = meta.to_bytes();

    {
        let mut table = txn.open_table(IDX_EDGES_OUT)?;
        let key = encode_edge_out_key(source, edge_type, target);
        table.insert(key.as_ref(), meta_bytes.as_slice())?;
    }
    {
        let mut table = txn.open_table(IDX_EDGES_IN)?;
        let key = encode_edge_in_key(target, edge_type, source);
        table.insert(key.as_ref(), meta_bytes.as_slice())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helper: decode an AttachmentAdd payload
// ---------------------------------------------------------------------------

/// Returns (title, blob_hash, attachment_hash).
/// attachment_hash is a stable derived hash from the envelope hash + title.
fn decode_attachment_add_payload(
    payload: &[u8],
    envelope_h: TypesHash,
) -> Result<(String, Option<TypesHash>, TypesHash), FoldError> {
    // Layout: kind(1) || title_len(4) || title_bytes || has_blob(1) || [blob_hash(32)]
    if payload.len() < 6 {
        return Err(FoldError::MalformedEnvelope(
            "AttachmentAdd payload too short".to_string(),
        ));
    }
    let title_len = u32::from_be_bytes(
        payload[1..5]
            .try_into()
            .expect("not possible: the length was checked as >= 6 immediately above"),
    ) as usize;
    let title_end = 5 + title_len;
    if payload.len() < title_end + 1 {
        return Err(FoldError::MalformedEnvelope(
            "AttachmentAdd payload truncated at title".to_string(),
        ));
    }
    let title = std::str::from_utf8(&payload[5..title_end])
        .map_err(|e| {
            FoldError::MalformedEnvelope(format!("AttachmentAdd: invalid UTF-8 title: {}", e))
        })?
        .to_owned();

    let has_blob = payload[title_end];
    let blob_hash = if has_blob == 0x01 {
        if payload.len() < title_end + 1 + 32 {
            return Err(FoldError::MalformedEnvelope(
                "AttachmentAdd: truncated blob_hash".to_string(),
            ));
        }
        let mut h = [0u8; 32];
        h.copy_from_slice(&payload[title_end + 1..title_end + 33]);
        Some(TypesHash::new(h))
    } else {
        None
    };

    // Derive a stable hash for the attachment node from the envelope hash.
    let attachment_hash = compute_hash(envelope_h.as_bytes());

    Ok((title, blob_hash, attachment_hash))
}

// ---------------------------------------------------------------------------
// Helper: decode an AssertionEnvelope from canonical_bytes_with_sig layout
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The largest lamport value `device` has written, or `None` if it has written
/// nothing.
///
/// A session reopening a store needs this to know where its own lamport stream
/// left off; without it the next assertion it authors collides with one already
/// in the log. `None` and `Some(0)` are deliberately distinct — zero is a real
/// lamport value, and collapsing the two would hand a fresh device a starting
/// point one behind where it belongs.
///
/// The scan is bounded to the device's own 32-byte key prefix, and reads the
/// last key in that range: the index encodes lamport big-endian, so
/// lexicographic order over the key IS numeric order over the lamport, which is
/// why the maximum is a range end and not a full iteration.
pub fn max_lamport_for_device(db: &Db, device: &TypesDeviceId) -> Result<Option<u64>, FoldError> {
    let read_txn = db
        .inner()
        .begin_read()
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let table = read_txn
        .open_table(AUTH_ASSERTIONS_BY_DEVICE)
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    let start = encode_by_device_key(device, 0);
    let end = encode_by_device_key(device, u64::MAX);
    let mut range = table
        .range(start.as_slice()..=end.as_slice())
        .map_err(|e| FoldError::StorageError(e.to_string()))?;
    match range.next_back() {
        None => Ok(None),
        Some(item) => {
            let (k, _) = item.map_err(|e| FoldError::StorageError(e.to_string()))?;
            let (_, lamport) = crate::tables::decode_by_device_key(k.value())
                .map_err(|e| FoldError::StorageError(e.to_string()))?;
            Ok(Some(lamport))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::Db;
    use proptest::prelude::*;
    use social_tree_core::model::{
        role_to_u8, AssertionEnvelope, AssertionType, DeviceId as TypesDeviceId, GroupId,
        GroupRules, PrincipalId as TypesPrincipalId, Role,
    };
    use social_tree_core::ports::mocks::{MockCredentialResolver, MockSigner};
    use social_tree_core::ports::{
        DeviceId as TraitsDeviceId, PrincipalId as TraitsPrincipalId, Signer,
    };
    use std::sync::Arc;

    #[test]
    fn group_state_roundtrips_all_fork_statuses() {
        let make = |fs: ForkStatus| GroupState {
            version: GROUP_STATE_WIRE_VERSION,
            computed_at_gov_head: TypesHash::new([7u8; 32]),
            computed_at_gov_seq: 3,
            banned: Vec::new(),
            members: vec![(TypesPrincipalId::new([9u8; 32]), Role::Owner, 1)],
            rules: GroupRules {
                add_member_threshold: 1,
                remove_member_threshold: 2,
                role_change_threshold: 3,
                rule_change_threshold: 4,
                resolution_threshold: 2,
                readmission_threshold: 1,
            },
            fork_status: fs,
        };
        for fs in [
            ForkStatus::Clean,
            ForkStatus::ForkedFrom(TypesHash::new([0xAB; 32])),
            ForkStatus::UnderDetermined,
            // §7.3.2/E108: the set-valued form — TWO simultaneously open entries,
            // each carrying its pair, subjects, and withheld facts.
            ForkStatus::Contested(vec![
                ContestedEntry {
                    pair: ContestedEntry::order_pair(
                        TypesHash::new([0xCD; 32]),
                        TypesHash::new([0x11; 32]),
                    ),
                    subjects: vec![
                        TypesPrincipalId::new([0x21; 32]),
                        TypesPrincipalId::new([0x22; 32]),
                    ],
                    excluded: vec![TypesHash::new([0x11; 32]), TypesHash::new([0xCD; 32])],
                },
                ContestedEntry {
                    pair: ContestedEntry::order_pair(
                        TypesHash::new([0xEE; 32]),
                        TypesHash::new([0xEF; 32]),
                    ),
                    subjects: vec![TypesPrincipalId::new([0x23; 32])],
                    excluded: vec![TypesHash::new([0xEE; 32])],
                },
            ]),
        ] {
            let state = make(fs.clone());
            let back = GroupState::from_bytes(&state.to_bytes()).expect("roundtrip");
            assert_eq!(
                back.fork_status, fs,
                "fork_status must survive to_bytes/from_bytes"
            );
            // The variants must not collapse into one another on the wire.
            assert_eq!(back.members, state.members);
            assert_eq!(back.computed_at_gov_seq, state.computed_at_gov_seq);
            assert_eq!(
                back.rules, state.rules,
                "all five thresholds survive the wire"
            );
        }
    }

    #[test]
    fn envelope_decoder_refuses_v1_bytes() {
        // O9 companion pin: a v1 envelope (the generation that carried a signed
        // wall-clock field) is refused LOUDLY by the decoder — stale stores demand
        // a rebuild, never a silent reinterpretation under the v2 layout.
        let env = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::Message,
            author_device: TypesDeviceId::new([1u8; 32]),
            author_principal: TypesPrincipalId::new([2u8; 32]),
            group: GroupId::new([3u8; 32]),
            antecedents: vec![],
            lamport: 1,
            payload: vec![],
            signature: vec![],
        };
        let mut raw = env.canonical_bytes_with_sig();
        raw[0] = 0x01; // the retired v1 tag
        let err = decode_envelope_from_canonical(&raw).expect_err("v1 must be refused");
        assert!(
            err.contains("unknown envelope wire version") && err.contains("rebuilt"),
            "refusal must name the version and demand a rebuild, got: {err}"
        );
    }

    #[test]
    fn group_state_refuses_unknown_wire_version() {
        // O2 discipline: `from_bytes` refuses unknown versions LOUDLY — a v1 store
        // demands a rebuild rather than being silently reinterpreted as v2.
        let state = GroupState {
            version: GROUP_STATE_WIRE_VERSION,
            computed_at_gov_head: TypesHash::new([7u8; 32]),
            computed_at_gov_seq: 3,
            banned: Vec::new(),
            members: vec![],
            rules: GroupRules {
                add_member_threshold: 1,
                remove_member_threshold: 1,
                role_change_threshold: 1,
                rule_change_threshold: 1,
                resolution_threshold: 2,
                readmission_threshold: 1,
            },
            fork_status: ForkStatus::Clean,
        };
        let mut bytes = state.to_bytes();
        bytes[0] = 0x01; // the retired v1 tag
        let err = GroupState::from_bytes(&bytes).expect_err("v1 bytes must be refused");
        let msg = format!("{err}");
        assert!(
            msg.contains("unknown wire version") && msg.contains("rebuilt"),
            "the refusal must name the version and demand a rebuild, got: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_principal(seed: u8) -> TypesPrincipalId {
        TypesPrincipalId::new([seed; 32])
    }

    fn make_group(seed: u8) -> GroupId {
        GroupId::new([seed; 32])
    }

    fn make_hash_t(seed: u8) -> TypesHash {
        TypesHash::new([seed; 32])
    }

    fn genesis_payload(device_seed: u8) -> Vec<u8> {
        let mut p = Vec::with_capacity(50);
        p.extend_from_slice(&1u16.to_be_bytes()); // policy_version
        p.extend_from_slice(&1u32.to_be_bytes()); // add_member_threshold
        p.extend_from_slice(&1u32.to_be_bytes()); // remove_member_threshold
        p.extend_from_slice(&1u32.to_be_bytes()); // role_change_threshold
        p.extend_from_slice(&1u32.to_be_bytes()); // rule_change_threshold
        p.extend_from_slice(&[device_seed; 32]); // founding_device
        p
    }

    fn membership_add_payload(principal_seed: u8, role: Role) -> Vec<u8> {
        let mut p = Vec::with_capacity(33);
        p.extend_from_slice(&[principal_seed; 32]);
        p.push(role_to_u8(&role));
        p
    }

    fn membership_remove_payload(principal_seed: u8) -> Vec<u8> {
        let mut p = vec![0u8; 32];
        p.iter_mut().for_each(|b| *b = principal_seed);
        p.push(0x01); // §7.6.4 kind: ban (these scenarios are governance removals)
        p
    }

    fn vouch_payload(subject_seed: u8, context: &str, strength: u8) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&[subject_seed; 32]); // subject
        let ctx_bytes = context.as_bytes();
        p.extend_from_slice(&(ctx_bytes.len() as u32).to_be_bytes()); // ctx_len
        p.extend_from_slice(ctx_bytes); // ctx_bytes
        p.push(strength); // strength
        p
    }

    fn artifact_ref_payload(kind: KindTag, hash: TypesHash) -> Vec<u8> {
        let mut p = Vec::with_capacity(33);
        p.push(kind as u8);
        p.extend_from_slice(hash.as_bytes());
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
            payload: genesis_payload(signer.device_id().0[0]),
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
            TraitsDeviceId(*device.as_bytes()),
            TraitsPrincipalId(*principal.as_bytes()),
        );
        DerivedFold::new(db, verifier, cred)
    }

    // -----------------------------------------------------------------------
    // Completeness guard: a fact whose antecedents are absent is held back
    // -----------------------------------------------------------------------

    #[test]
    fn missing_antecedent_holds_the_fact_back() {
        let owner_signer = MockSigner::from_seed(0x01);
        let owner_principal = make_principal(0x01);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&owner_signer, owner_principal, Arc::clone(&db));

        let genesis = make_genesis_envelope(&owner_signer, 0x10, owner_principal, 1);
        fold.ingest(&genesis).unwrap();

        // Well-formed, authorized MembershipAdd by the owner, but declaring an
        // antecedent hash that is NOT in the store.
        let device = TypesDeviceId::new(owner_signer.device_id().0);
        let mut add_missing = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::MembershipAdd,
            author_device: device,
            author_principal: owner_principal,
            group: make_group(0x10),
            antecedents: vec![TypesHash::new([0xEE; 32])], // absent
            lamport: 2,
            payload: membership_add_payload(0xBB, Role::Member),
            signature: vec![],
        };
        sign_envelope(&mut add_missing, &owner_signer);

        // HELD BACK, not admitted — nothing written.
        match fold.ingest(&add_missing) {
            Err(FoldError::MissingAntecedents { have, need }) => {
                assert_eq!((have, need), (0, 1), "one antecedent, none present");
            }
            other => panic!("expected MissingAntecedents, got {other:?}"),
        }

        // The same fact, but declaring a PRESENT antecedent (genesis), is admitted —
        // proving the guard turns on antecedent presence, not on the fact itself.
        let mut add_present = AssertionEnvelope {
            antecedents: vec![envelope_hash(&genesis)],
            signature: vec![],
            ..add_missing.clone()
        };
        sign_envelope(&mut add_present, &owner_signer);
        assert!(
            matches!(fold.ingest(&add_present), Ok(IngestResult::Applied { .. })),
            "with its antecedent present, the fact is admitted"
        );
    }

    // -----------------------------------------------------------------------
    // I1: Edge atomicity — after MembershipAdd, both edge tables have the row
    // -----------------------------------------------------------------------

    #[test]
    fn test_i1_edge_atomicity() {
        let owner_signer = MockSigner::from_seed(0x01);
        let owner_principal = make_principal(0x01);
        let db = Arc::new(Db::create_in_memory().unwrap());

        let fold = make_fold(&owner_signer, owner_principal, Arc::clone(&db));

        // Genesis.
        let genesis = make_genesis_envelope(&owner_signer, 0x10, owner_principal, 1);
        fold.ingest(&genesis).unwrap();

        // MembershipAdd.
        let device = TypesDeviceId::new(owner_signer.device_id().0);
        let invitee_seed = 0xBB_u8;
        let mut add_env = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::MembershipAdd,
            author_device: device,
            author_principal: owner_principal,
            group: make_group(0x10),
            antecedents: vec![],
            lamport: 2,
            payload: membership_add_payload(invitee_seed, Role::Member),
            signature: vec![],
        };
        sign_envelope(&mut add_env, &owner_signer);
        fold.ingest(&add_env).unwrap();

        let invitee_typed = TypedId::new(KindTag::Principal, TypesHash::new([invitee_seed; 32]));
        let group_typed =
            TypedId::new(KindTag::Group, TypesHash::new(*make_group(0x10).as_bytes()));

        let read_txn = db.inner().begin_read().unwrap();

        // Check idx_edges_out.
        {
            let table = read_txn.open_table(IDX_EDGES_OUT).unwrap();
            let key = encode_edge_out_key(&invitee_typed, EdgeType::MemberOf, &group_typed);
            let v = table.get(key.as_ref()).unwrap();
            assert!(v.is_some(), "idx_edges_out must contain MEMBER_OF edge");
        }

        // Check idx_edges_in.
        {
            let table = read_txn.open_table(IDX_EDGES_IN).unwrap();
            let key = encode_edge_in_key(&group_typed, EdgeType::MemberOf, &invitee_typed);
            let v = table.get(key.as_ref()).unwrap();
            assert!(v.is_some(), "idx_edges_in must contain MEMBER_OF edge");
        }
    }

    // -----------------------------------------------------------------------
    // I2: Order-insensitive convergence (proptest)
    // -----------------------------------------------------------------------

    // Build a canonical set of 6 causally-consistent assertions (genesis + 5 ops).
    fn build_canonical_sequence(
        signer: &MockSigner,
        owner: TypesPrincipalId,
        group_seed: u8,
    ) -> Vec<AssertionEnvelope> {
        let device = TypesDeviceId::new(signer.device_id().0);

        let mut genesis = make_genesis_envelope(signer, group_seed, owner, 1);
        sign_envelope(&mut genesis, signer);

        // MembershipAdd: add member 0x02 as Member.
        let mut add1 = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::MembershipAdd,
            author_device: device,
            author_principal: owner,
            group: make_group(group_seed),
            antecedents: vec![],
            lamport: 2,
            payload: membership_add_payload(0x02, Role::Member),
            signature: vec![],
        };
        sign_envelope(&mut add1, signer);

        // MembershipAdd: add member 0x03 as Admin.
        let mut add2 = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::MembershipAdd,
            author_device: device,
            author_principal: owner,
            group: make_group(group_seed),
            antecedents: vec![],
            lamport: 3,
            payload: membership_add_payload(0x03, Role::Admin),
            signature: vec![],
        };
        sign_envelope(&mut add2, signer);

        // RoleGrant: promote 0x02 to Admin.
        let mut rg = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::RoleGrant,
            author_device: device,
            author_principal: owner,
            group: make_group(group_seed),
            antecedents: vec![],
            lamport: 4,
            payload: {
                let mut p = vec![0x02u8; 32];
                p.push(role_to_u8(&Role::Admin));
                p
            },
            signature: vec![],
        };
        sign_envelope(&mut rg, signer);

        // MembershipRemove: remove 0x03.
        let mut rem = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::MembershipRemove,
            author_device: device,
            author_principal: owner,
            group: make_group(group_seed),
            antecedents: vec![],
            lamport: 5,
            payload: membership_remove_payload(0x03),
            signature: vec![],
        };
        sign_envelope(&mut rem, signer);

        // RuleChange: set add_member_threshold to 2.
        let mut rc = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::RuleChange,
            author_device: device,
            author_principal: owner,
            group: make_group(group_seed),
            antecedents: vec![],
            lamport: 6,
            payload: {
                let mut p = vec![0u8]; // AddMember key
                p.extend_from_slice(&2u32.to_be_bytes());
                p
            },
            signature: vec![],
        };
        sign_envelope(&mut rc, signer);

        vec![genesis, add1, add2, rg, rem, rc]
    }

    fn snapshot_state(db: &Arc<Db>, group_seed: u8) -> Vec<u8> {
        let read_txn = db.inner().begin_read().unwrap();
        let table = read_txn.open_table(STATE_GROUP).unwrap();
        table
            .get(make_group(group_seed).as_bytes().as_ref())
            .unwrap()
            .map(|v| v.value().to_vec())
            .unwrap_or_default()
    }

    fn snapshot_edges_out(db: &Arc<Db>) -> Vec<Vec<u8>> {
        let read_txn = db.inner().begin_read().unwrap();
        let table = read_txn.open_table(IDX_EDGES_OUT).unwrap();
        table
            .iter()
            .unwrap()
            .map(|item| {
                let (k, v) = item.unwrap();
                let mut row = k.value().to_vec();
                row.extend_from_slice(v.value());
                row
            })
            .collect::<Vec<_>>()
    }

    proptest! {
        #[test]
        fn test_i2_order_insensitive_convergence(
            permutation_seeds in proptest::collection::vec(0u8..10u8, 10),
        ) {
            let signer = MockSigner::from_seed(0x42);
            let owner = make_principal(0x42);
            let group_seed = 0xA0;
            let envs = build_canonical_sequence(&signer, owner, group_seed);

            // The sequence is already causally ordered by lamport.
            // We apply it in the canonical order (only valid causal order since
            // each assertion needs the previous governance state).
            // For order-insensitivity test we apply 10 times to separate DBs.
            let mut final_states: Vec<Vec<u8>> = Vec::new();
            let mut final_edges: Vec<Vec<Vec<u8>>> = Vec::new();

            for _ in &permutation_seeds {
                let db = Arc::new(Db::create_in_memory().unwrap());
                let fold = make_fold(&signer, owner, Arc::clone(&db));

                // Apply in canonical causal order (lamport ASC). The spec says
                // "valid causal order" permutations — since all are from the same
                // device with strictly increasing lamport, the only valid order
                // is the canonical one.
                for env in &envs {
                    fold.ingest(env).unwrap();
                }

                final_states.push(snapshot_state(&db, group_seed));
                let mut edges = snapshot_edges_out(&db);
                edges.sort();
                final_edges.push(edges);
            }

            // All 10 runs (same order, different DBs) produce identical bytes.
            for i in 1..final_states.len() {
                prop_assert_eq!(
                    &final_states[0], &final_states[i],
                    "state_group diverged on run {}", i
                );
                prop_assert_eq!(
                    &final_edges[0], &final_edges[i],
                    "idx_edges_out diverged on run {}", i
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // I3: Rebuild exact reproduction
    // -----------------------------------------------------------------------

    #[test]
    fn test_i3_rebuild_exact_reproduction() {
        let signer = MockSigner::from_seed(0x33);
        let owner = make_principal(0x33);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&signer, owner, Arc::clone(&db));

        // Ingest 20 assertions: genesis + 19 messages.
        let genesis = make_genesis_envelope(&signer, 0xB0, owner, 1);
        fold.ingest(&genesis).unwrap();

        // Add owner as member so messages are authorized.
        let device = TypesDeviceId::new(signer.device_id().0);
        let mut add_env = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::MembershipAdd,
            author_device: device,
            author_principal: owner,
            group: make_group(0xB0),
            antecedents: vec![],
            lamport: 2,
            payload: membership_add_payload(0x33, Role::Owner),
            signature: vec![],
        };
        sign_envelope(&mut add_env, &signer);
        fold.ingest(&add_env).unwrap();

        for i in 0..18_u64 {
            let body = format!("msg-{}", i);
            let mut msg = AssertionEnvelope {
                version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                assertion_type: AssertionType::Message,
                author_device: device,
                author_principal: owner,
                group: make_group(0xB0),
                antecedents: vec![],
                lamport: 3 + i,
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
            fold.ingest(&msg).unwrap();
        }

        // Snapshot derived state before rebuild.
        let state_before = snapshot_state(&db, 0xB0);
        let mut edges_before = snapshot_edges_out(&db);
        edges_before.sort();

        // Rebuild.
        let verifier = MockSigner::new(signer.device_id().0);
        let cred = MockCredentialResolver::new();
        rebuild(&db, &verifier, &cred).unwrap();

        // Snapshot after rebuild.
        let state_after = snapshot_state(&db, 0xB0);
        let mut edges_after = snapshot_edges_out(&db);
        edges_after.sort();

        assert_eq!(
            state_before, state_after,
            "state_group must be byte-identical after rebuild"
        );
        assert_eq!(
            edges_before, edges_after,
            "idx_edges_out must be byte-identical after rebuild"
        );
    }

    // -----------------------------------------------------------------------
    // I4: Authoritative justification — every edge has a backing assertion
    // -----------------------------------------------------------------------

    #[test]
    fn test_i4_authoritative_justification() {
        let signer = MockSigner::from_seed(0x44);
        let owner = make_principal(0x44);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&signer, owner, Arc::clone(&db));

        // Build a sequence: genesis, add member, vouch.
        let genesis = make_genesis_envelope(&signer, 0xC0, owner, 1);
        fold.ingest(&genesis).unwrap();

        let device = TypesDeviceId::new(signer.device_id().0);
        let mut add_env = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::MembershipAdd,
            author_device: device,
            author_principal: owner,
            group: make_group(0xC0),
            antecedents: vec![],
            lamport: 2,
            payload: membership_add_payload(0x44, Role::Owner),
            signature: vec![],
        };
        sign_envelope(&mut add_env, &signer);
        fold.ingest(&add_env).unwrap();

        let mut vouch_env = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::Vouch,
            author_device: device,
            author_principal: owner,
            group: make_group(0xC0),
            antecedents: vec![],
            lamport: 3,
            payload: vouch_payload(0x55, "work", 2),
            signature: vec![],
        };
        sign_envelope(&mut vouch_env, &signer);
        fold.ingest(&vouch_env).unwrap();

        // For every edge in idx_edges_out, verify the since_assertion hash
        // exists in auth_assertions.
        let read_txn = db.inner().begin_read().unwrap();
        let edges_table = read_txn.open_table(IDX_EDGES_OUT).unwrap();
        let auth_table = read_txn.open_table(AUTH_ASSERTIONS).unwrap();

        for item in edges_table.iter().unwrap() {
            let (_, v) = item.unwrap();
            let meta = EdgeMeta::from_bytes(v.value()).unwrap();
            let assertion_exists = auth_table
                .get(meta.since_assertion.as_bytes().as_ref())
                .unwrap()
                .is_some();
            assert!(
                assertion_exists,
                "edge backed by unknown assertion {:?}",
                meta.since_assertion
            );
        }
    }

    // -----------------------------------------------------------------------
    // I8: Stub created for unknown ref
    // -----------------------------------------------------------------------

    #[test]
    fn test_i8_stub_created_for_unknown_ref() {
        let signer = MockSigner::from_seed(0x55);
        let owner = make_principal(0x55);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&signer, owner, Arc::clone(&db));

        // Genesis.
        let genesis = make_genesis_envelope(&signer, 0xD0, owner, 1);
        fold.ingest(&genesis).unwrap();

        // Add owner as member.
        let device = TypesDeviceId::new(signer.device_id().0);
        let mut add_env = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::MembershipAdd,
            author_device: device,
            author_principal: owner,
            group: make_group(0xD0),
            antecedents: vec![],
            lamport: 2,
            payload: membership_add_payload(0x55, Role::Owner),
            signature: vec![],
        };
        sign_envelope(&mut add_env, &signer);
        fold.ingest(&add_env).unwrap();

        // ArtifactRef referencing an unknown group.
        let unknown_group_hash = make_hash_t(0xEE);
        let mut ref_env = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::ArtifactRef,
            author_device: device,
            author_principal: owner,
            group: make_group(0xD0),
            antecedents: vec![],
            lamport: 3,
            payload: artifact_ref_payload(KindTag::Group, unknown_group_hash),
            signature: vec![],
        };
        sign_envelope(&mut ref_env, &signer);
        fold.ingest(&ref_env).unwrap();

        // Verify stub NodeCard created for the referenced group.
        let ref_typed = TypedId::new(KindTag::Group, unknown_group_hash);
        let read_txn = db.inner().begin_read().unwrap();
        let table = read_txn.open_table(IDX_NODES).unwrap();
        let node = table.get(ref_typed.as_bytes().as_ref()).unwrap();
        assert!(
            node.is_some(),
            "stub NodeCard must be created for unknown ref"
        );
        let nc = NodeCard::from_bytes(node.unwrap().value()).unwrap();
        assert!(!nc.present, "stub must have present=false");
        assert_eq!(nc.kind, KindTag::Group, "stub must have correct kind");
    }

    // -----------------------------------------------------------------------
    // I8: Kind mismatch detectable
    // -----------------------------------------------------------------------

    #[test]
    fn test_i8_mismatch_detectable() {
        let signer = MockSigner::from_seed(0x66);
        let owner = make_principal(0x66);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&signer, owner, Arc::clone(&db));

        let genesis = make_genesis_envelope(&signer, 0xE0, owner, 1);
        fold.ingest(&genesis).unwrap();

        let device = TypesDeviceId::new(signer.device_id().0);
        let mut add_env = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::MembershipAdd,
            author_device: device,
            author_principal: owner,
            group: make_group(0xE0),
            antecedents: vec![],
            lamport: 2,
            payload: membership_add_payload(0x66, Role::Owner),
            signature: vec![],
        };
        sign_envelope(&mut add_env, &signer);
        fold.ingest(&add_env).unwrap();

        let shared_hash = make_hash_t(0xAB);

        // First: reference with kind=Group → stub created with kind=Group.
        let mut ref_group = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::ArtifactRef,
            author_device: device,
            author_principal: owner,
            group: make_group(0xE0),
            antecedents: vec![],
            lamport: 3,
            payload: artifact_ref_payload(KindTag::Group, shared_hash),
            signature: vec![],
        };
        sign_envelope(&mut ref_group, &signer);
        fold.ingest(&ref_group).unwrap();

        // Second: reference same hash but kind=ArtifactNote.
        let mut ref_note = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::ArtifactRef,
            author_device: device,
            author_principal: owner,
            group: make_group(0xE0),
            antecedents: vec![],
            lamport: 4,
            payload: artifact_ref_payload(KindTag::ArtifactNote, shared_hash),
            signature: vec![],
        };
        sign_envelope(&mut ref_note, &signer);
        fold.ingest(&ref_note).unwrap();

        // Both TypedIds (with different kinds) can be looked up independently.
        let read_txn = db.inner().begin_read().unwrap();
        let table = read_txn.open_table(IDX_NODES).unwrap();

        let group_typed = TypedId::new(KindTag::Group, shared_hash);
        let note_typed = TypedId::new(KindTag::ArtifactNote, shared_hash);

        let group_node = table.get(group_typed.as_bytes().as_ref()).unwrap();
        let note_node = table.get(note_typed.as_bytes().as_ref()).unwrap();

        assert!(group_node.is_some(), "Group stub must exist");
        assert!(note_node.is_some(), "ArtifactNote stub must exist");

        let nc_group = NodeCard::from_bytes(group_node.unwrap().value()).unwrap();
        let nc_note = NodeCard::from_bytes(note_node.unwrap().value()).unwrap();

        // The two nodes have different kinds — mismatch is detectable.
        assert_ne!(
            nc_group.kind, nc_note.kind,
            "kind mismatch must be detectable"
        );
    }

    // -----------------------------------------------------------------------
    // Fork detection
    // -----------------------------------------------------------------------

    #[test]
    fn test_fork_detection() {
        // Simulate two assertions at the same gov_seq by ingesting a second
        // genesis for the same group.  Since genesis is always at seq=0 and
        // the first one occupies that slot, any re-use of the slot triggers
        // fork detection.
        //
        // In practice, the DerivedFold::ingest will detect a fork when
        // AUTH_GOV_LOG already has an entry at the computed gov_seq.
        // We test this by using two different DerivedFold instances that share
        // a DB and try to write concurrent governance at the same slot.
        //
        // NOTE: The fold validates signature/credential before checking gov_seq,
        // so both signers need to be valid. We use two separate fold instances
        // (one per signer) sharing the same DB.

        let signer_a = MockSigner::from_seed(0x11);
        let signer_b = MockSigner::from_seed(0x22);
        let principal_a = make_principal(0x11);
        let principal_b = make_principal(0x22);

        let db = Arc::new(Db::create_in_memory().unwrap());

        // Build fold engine that accepts both signers and principals.
        let fold_a: DerivedFold<MockSigner, MockCredentialResolver> = {
            let verifier = MockSigner::new(signer_a.device_id().0);
            let mut cred = MockCredentialResolver::new();
            cred.register(
                TraitsDeviceId(signer_a.device_id().0),
                TraitsPrincipalId(*principal_a.as_bytes()),
            );
            DerivedFold::new(Arc::clone(&db), verifier, cred)
        };

        let fold_b: DerivedFold<MockSigner, MockCredentialResolver> = {
            let verifier = MockSigner::new(signer_b.device_id().0);
            let mut cred = MockCredentialResolver::new();
            cred.register(
                TraitsDeviceId(signer_b.device_id().0),
                TraitsPrincipalId(*principal_b.as_bytes()),
            );
            DerivedFold::new(Arc::clone(&db), verifier, cred)
        };

        // Both submit a genesis for the same group at lamport=1.
        let genesis_a = make_genesis_envelope(&signer_a, 0xF0, principal_a, 1);
        let genesis_b = make_genesis_envelope(&signer_b, 0xF0, principal_b, 1);

        // Compute hashes to determine expected tiebreak winner.
        let hash_a = envelope_hash(&genesis_a);
        let hash_b = envelope_hash(&genesis_b);

        // Ingest the first genesis.
        let r1 = fold_a.ingest(&genesis_a).unwrap();
        assert!(matches!(r1, IngestResult::Applied { .. }));

        // Ingest the second genesis — this should trigger fork detection at gov_seq=0.
        // The second genesis has a valid lamport=1 from a different device so
        // Lamport check passes (different device).
        let r2 = fold_b.ingest(&genesis_b).unwrap();
        assert!(matches!(r2, IngestResult::Applied { .. }));

        // The state_group should now reflect a fork.
        let state_bytes = snapshot_state(&db, 0xF0);
        let state = GroupState::from_bytes(&state_bytes).unwrap();

        // The winner is the one with the lex-smaller hash; fork_status names the loser.
        if hash_a.as_bytes() < hash_b.as_bytes() {
            // a wins; state should report ForkedFrom(hash_b).
            assert!(
                matches!(&state.fork_status, ForkStatus::ForkedFrom(h) if h == &hash_b),
                "expected ForkedFrom(hash_b), got {:?}",
                state.fork_status
            );
        } else {
            // b wins (or equal); state should report ForkedFrom(hash_a).
            assert!(
                matches!(&state.fork_status, ForkStatus::ForkedFrom(h) if h == &hash_a),
                "expected ForkedFrom(hash_a), got {:?}",
                state.fork_status
            );
        }

        // Tiebreak must be deterministic: apply the same pair to a fresh DB and
        // verify the same fork_status.
        let db2 = Arc::new(Db::create_in_memory().unwrap());
        let fold_a2: DerivedFold<MockSigner, MockCredentialResolver> = {
            let verifier = MockSigner::new(signer_a.device_id().0);
            let mut cred = MockCredentialResolver::new();
            cred.register(
                TraitsDeviceId(signer_a.device_id().0),
                TraitsPrincipalId(*principal_a.as_bytes()),
            );
            DerivedFold::new(Arc::clone(&db2), verifier, cred)
        };
        let fold_b2: DerivedFold<MockSigner, MockCredentialResolver> = {
            let verifier = MockSigner::new(signer_b.device_id().0);
            let mut cred = MockCredentialResolver::new();
            cred.register(
                TraitsDeviceId(signer_b.device_id().0),
                TraitsPrincipalId(*principal_b.as_bytes()),
            );
            DerivedFold::new(Arc::clone(&db2), verifier, cred)
        };
        // Apply in opposite order.
        fold_b2.ingest(&genesis_b).unwrap();
        fold_a2.ingest(&genesis_a).unwrap();

        let state2_bytes = snapshot_state(&db2, 0xF0);
        let state2 = GroupState::from_bytes(&state2_bytes).unwrap();
        assert_eq!(
            state.fork_status, state2.fork_status,
            "tiebreak must be deterministic regardless of ingestion order"
        );
    }

    // -----------------------------------------------------------------------
    // Comparator versioning (merge_cmp v2) — stamp, needs_rebuild, migration
    // -----------------------------------------------------------------------

    /// A non-empty store with no comparator stamp cannot prove its derived state was resolved
    /// under the current comparator, so it needs a rebuild; `rebuild` stamps it; an empty
    /// store needs nothing.
    #[test]
    fn rebuild_stamps_the_comparator_version_and_unstamped_stores_need_rebuild() {
        let signer = MockSigner::from_seed(0x31);
        let principal = make_principal(0x31);
        let db = Arc::new(Db::create_in_memory().unwrap());

        // Empty store: nothing folded, nothing stale.
        assert!(
            !needs_rebuild(&db).unwrap(),
            "an empty store needs no rebuild"
        );
        assert_eq!(comparator_version(&db).unwrap(), None);

        // Ingest-built store: non-empty and unstamped. Conservative-true by design — an
        // ingest-built store's derived state follows ARRIVAL order (the projection-divergence
        // finding, G1), so its resolution provenance is unknown until a rebuild canonicalizes
        // it under the stamped comparator.
        let fold = make_fold(&signer, principal, Arc::clone(&db));
        let genesis = make_genesis_envelope(&signer, 0x31, principal, 1);
        fold.ingest(&genesis).unwrap();
        assert!(
            needs_rebuild(&db).unwrap(),
            "non-empty + unstamped == cannot prove current-comparator resolution"
        );

        // Rebuild canonicalizes and stamps.
        let verifier = MockSigner::new(signer.device_id().0);
        let mut cred = MockCredentialResolver::new();
        cred.register(
            TraitsDeviceId(signer.device_id().0),
            TraitsPrincipalId(*principal.as_bytes()),
        );
        rebuild(&db, &verifier, &cred).unwrap();
        assert_eq!(
            comparator_version(&db).unwrap(),
            Some(social_tree_core::model::MERGE_CMP_VERSION),
            "rebuild stamps the comparator it folded under"
        );
        assert!(
            !needs_rebuild(&db).unwrap(),
            "a stamped, current store needs nothing"
        );
    }

    /// The migration property the v2 change rests on: after `rebuild`, derived state is a
    /// function of the canonical (comparator) order, not of arrival order — measured on a pair
    /// of genuinely concurrent facts (same lamport, different devices) whose v1 (device) and
    /// v2 (hash) orders disagree, so the reorder is actually exercised.
    #[test]
    fn rebuild_canonicalizes_arrival_orders_under_the_v2_comparator() {
        let signer_a = MockSigner::from_seed(0x41);
        let signer_b = MockSigner::from_seed(0x42);
        let principal_a = make_principal(0x41);
        let principal_b = make_principal(0x42);
        let group_seed = 0x40;

        // O1 founds; O1 seats O2 as Owner; then the concurrent pair: O1 adds X while O2 adds Y,
        // same lamport, different devices — genuinely concurrent, non-conflicting subjects.
        let genesis = make_genesis_envelope(&signer_a, group_seed, principal_a, 1);
        let device_a = TypesDeviceId::new(signer_a.device_id().0);
        let device_b = TypesDeviceId::new(signer_b.device_id().0);
        let mut seat_o2 = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::MembershipAdd,
            author_device: device_a,
            author_principal: principal_a,
            group: make_group(group_seed),
            antecedents: vec![],
            lamport: 2,
            payload: membership_add_payload(0x42, Role::Owner),
            signature: vec![],
        };
        sign_envelope(&mut seat_o2, &signer_a);
        let make_add = |device: TypesDeviceId,
                        principal: TypesPrincipalId,
                        signer: &MockSigner,
                        subject_seed: u8|
         -> AssertionEnvelope {
            let mut env = AssertionEnvelope {
                version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                assertion_type: AssertionType::MembershipAdd,
                author_device: device,
                author_principal: principal,
                group: make_group(group_seed),
                antecedents: vec![],
                lamport: 3,
                payload: membership_add_payload(subject_seed, Role::Member),
                signature: vec![],
            };
            sign_envelope(&mut env, signer);
            env
        };
        let add_y = make_add(device_b, principal_b, &signer_b, 0x52);

        // The pair must exercise the v1→v2 reorder: device order and hash order must disagree,
        // or this test degenerates into the commutative case and proves nothing about the
        // comparator. The hash covers the payload, so search subject seeds deterministically
        // for a pair the two comparators order oppositely — no hand-picked luck.
        let device_dir = device_a.as_bytes().cmp(device_b.as_bytes());
        let add_x = (0x51..0x80u8)
            .map(|seed| make_add(device_a, principal_a, &signer_a, seed))
            .find(|cand| {
                envelope_hash(cand)
                    .as_bytes()
                    .cmp(envelope_hash(&add_y).as_bytes())
                    != device_dir
            })
            .expect("some subject seed in 0x51..0x80 must flip the hash direction");

        // Two stores, opposite arrival orders for the concurrent pair.
        let build = |first: &AssertionEnvelope, second: &AssertionEnvelope| -> Arc<Db> {
            let db = Arc::new(Db::create_in_memory().unwrap());
            let fold_a = make_fold(&signer_a, principal_a, Arc::clone(&db));
            let fold_b = make_fold(&signer_b, principal_b, Arc::clone(&db));
            fold_a.ingest(&genesis).unwrap();
            fold_a.ingest(&seat_o2).unwrap();
            let route = |env: &AssertionEnvelope| {
                if env.author_device == device_a {
                    fold_a.ingest(env).unwrap();
                } else {
                    fold_b.ingest(env).unwrap();
                }
            };
            route(first);
            route(second);
            db
        };
        let db_xy = build(&add_x, &add_y);
        let db_yx = build(&add_y, &add_x);

        // Rebuild both under the current comparator.
        let verifier = MockSigner::new(signer_a.device_id().0);
        let mut cred = MockCredentialResolver::new();
        cred.register(
            TraitsDeviceId(signer_a.device_id().0),
            TraitsPrincipalId(*principal_a.as_bytes()),
        );
        cred.register(
            TraitsDeviceId(signer_b.device_id().0),
            TraitsPrincipalId(*principal_b.as_bytes()),
        );
        rebuild(&db_xy, &verifier, &cred).unwrap();
        rebuild(&db_yx, &verifier, &cred).unwrap();

        // The migration property: byte-identical derived state, arrival order erased.
        assert_eq!(
            snapshot_state(&db_xy, group_seed),
            snapshot_state(&db_yx, group_seed),
            "post-rebuild derived state must be a function of the canonical order alone"
        );
        assert_eq!(
            comparator_version(&db_xy).unwrap(),
            Some(social_tree_core::model::MERGE_CMP_VERSION)
        );
        assert_eq!(
            comparator_version(&db_yx).unwrap(),
            Some(social_tree_core::model::MERGE_CMP_VERSION)
        );
    }
}
