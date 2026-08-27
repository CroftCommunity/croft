//! MLS state that survives the process: openmls's `StorageProvider` over redb.
//!
//! # Why redb and not upstream's sqlite provider
//!
//! P0-2 decided this shape and required a probe of `openmls_sqlite_storage`
//! 0.2.0 first, on the grounds that if it dominated it should win. It was
//! probed (P7 S2, 2026-08-27) and it does not dominate, for one decisive
//! reason and one supporting one.
//!
//! **It does not support wasm32.** Its own crate docs say so outright. P0-1's
//! entire argument for redb was that redb compiles for wasm32 and carries its
//! own `StorageBackend` seam, so platform variation lands *inside* redb rather
//! than at a croft trait. Taking sqlite for MLS state would put a permanent
//! ceiling under S5's web probe, in exchange for saving an implementation we
//! only write once.
//!
//! **And it would be a second storage engine.** P0-1 recorded "one
//! implementor, no second on the roadmap" as the reason not to invent a
//! storage trait. Adopting sqlite here would quietly create the second engine
//! that decision assumed away — plus a bundled C SQLite in the Android
//! cross-compile.
//!
//! Two things upstream got right were taken rather than reinvented: a
//! **version constant with a stated policy** (below), and the observation that
//! the whole trait is mechanical.
//!
//! # The shape, and where the risk is
//!
//! All 57 trait methods are the same operation: serialize some key parts,
//! serialize a value, store it. They differ only in a label and in which
//! helper they call. So the implementation is **six helpers plus 57
//! delegations**, and every bug that matters lives in the six. The tests point
//! there, not at the delegations.
//!
//! The key layout is upstream's, deliberately: `label ‖ serialized_key ‖
//! VERSION`. It is not better than any other layout, but it is the one the
//! reference implementations use, and a provider whose keys disagree with the
//! rest of the ecosystem is a provider nobody can debug against.
//!
//! # What is deliberately NOT here: the three `extensions-draft-08` methods
//!
//! `openmls_traits` 0.5.0 gates `write_application_export_tree`,
//! `application_export_tree` and `delete_application_export_tree` behind its
//! `extensions-draft-08` feature. They were written, and then removed, because
//! **they cannot be compiled in this dependency graph** — turning the feature
//! on makes upstream's own `openmls_memory_storage` 0.5.0 fail to build, since
//! it does not implement them either. It is pulled in by
//! `openmls_rust_crypto`, so nothing downstream of us can enable the feature
//! today regardless of what we do.
//!
//! Keeping three methods that cannot be compiled, and therefore cannot be
//! tested, would leave code that *looks* done and has never once run — the
//! exact thing G2 exists to prevent. When upstream implements them, adding
//! ours is a small, mechanical, and at that point *verifiable* change.

use std::path::Path;

use openmls_traits::storage::*;
use redb::{Database, TableDefinition};
use serde::Serialize;

/// Everything lives in one table. The label inside the key is what separates
/// the kinds, exactly as it does in the reference implementations — a table per
/// kind would be a second, redundant discriminator that could disagree with the
/// first.
const MLS: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("openmls_state_v1");

/// Store-level facts that are not MLS state. One key today: the version.
const META: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("openmls_meta_v1");

const VERSION_KEY: &[u8] = b"storage_provider_version";

/// What can go wrong holding MLS state.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The store on disk was written by a different version of this provider.
    ///
    /// Refused at `open`, loudly, and this is the variant the whole version
    /// guard exists for. `CURRENT_VERSION` is baked into every openmls storage
    /// key, so a version bump does not produce a read error — it produces a
    /// **miss**, and openmls reads a miss as "this group does not exist". The
    /// group would not fail to load; it would load as nothing, silently, and
    /// the first symptom would be a conversation that cannot be decrypted.
    #[error("this store was written by storage version {found}, and this build speaks {expected}")]
    VersionMismatch {
        /// The version stamped in the store.
        found: u16,
        /// The version this build implements.
        expected: u16,
    },

    /// redb refused.
    #[error("storage: {0}")]
    Storage(String),

    /// A stored value did not decode.
    #[error("stored MLS state did not decode: {0}")]
    Serialization(String),
}

impl From<serde_json::Error> for StorageError {
    fn from(e: serde_json::Error) -> Self {
        StorageError::Serialization(e.to_string())
    }
}

macro_rules! redb_err {
    ($($t:ty),+ $(,)?) => {$(
        impl From<$t> for StorageError {
            fn from(e: $t) -> Self {
                StorageError::Storage(e.to_string())
            }
        }
    )+};
}
redb_err!(
    redb::Error,
    redb::DatabaseError,
    redb::TransactionError,
    redb::TableError,
    redb::StorageError,
    redb::CommitError,
);

/// MLS state on disk, under the app's files dir.
#[derive(Debug)]
pub struct RedbStorage {
    db: Database,
}

impl RedbStorage {
    /// The storage layout version.
    ///
    /// Upstream's policy, adopted verbatim because it is the right one: if
    /// openmls's `CURRENT_VERSION` changes, the read/write/delete paths must be
    /// updated and a migration written, and only *then* may this be bumped to
    /// match. Bumping it first would silently orphan every existing store.
    pub const VERSION: u16 = CURRENT_VERSION;

    /// Open (or create) the MLS store at `path`.
    ///
    /// Refuses a store from a version this build does not speak. Refusing at
    /// open is the point: the alternative is discovering the mismatch one
    /// silent `Ok(None)` at a time, after the group has already failed to load.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let db = Database::create(path)?;
        let store = Self { db };
        store.ensure_tables()?;
        match store.stored_version()? {
            None => store.stamp_version(Self::VERSION)?,
            Some(v) if v == Self::VERSION => {}
            Some(found) => {
                return Err(StorageError::VersionMismatch {
                    found,
                    expected: Self::VERSION,
                })
            }
        }
        Ok(store)
    }

    /// The version stamped in this store, if any.
    pub fn stored_version(&self) -> Result<Option<u16>, StorageError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(META)?;
        Ok(table.get(VERSION_KEY)?.and_then(|v| {
            let b: [u8; 2] = v.value().try_into().ok()?;
            Some(u16::from_be_bytes(b))
        }))
    }

    /// Stamp a version. Test-only: production stamps once, at first open.
    #[doc(hidden)]
    pub fn stamp_version_for_test(&self, version: u16) {
        self.stamp_version(version).expect("stamp");
    }

    fn stamp_version(&self, version: u16) -> Result<(), StorageError> {
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(META)?;
            t.insert(VERSION_KEY, &version.to_be_bytes()[..])?;
        }
        txn.commit()?;
        Ok(())
    }

    fn ensure_tables(&self) -> Result<(), StorageError> {
        let txn = self.db.begin_write()?;
        txn.open_table(MLS)?;
        txn.open_table(META)?;
        txn.commit()?;
        Ok(())
    }

    // -- the six helpers every trait method delegates to --------------------

    /// `label ‖ serialized_key ‖ VERSION` — upstream's layout.
    fn storage_key<K: Serialize>(label: &[u8], key: &K) -> Result<Vec<u8>, StorageError> {
        let mut out = label.to_vec();
        out.extend_from_slice(&serde_json::to_vec(key)?);
        out.extend_from_slice(&Self::VERSION.to_be_bytes());
        Ok(out)
    }

    /// Store one value, committed before this returns.
    ///
    /// Committed per call, not batched. A provider that deferred the commit
    /// would pass every clean-restart test and lose an epoch to a kill — and
    /// losing an epoch is not a lost message, it is a group that can no longer
    /// decrypt.
    fn put<K: Serialize>(&self, label: &[u8], key: &K, value: Vec<u8>) -> Result<(), StorageError> {
        let k = Self::storage_key(label, key)?;
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(MLS)?;
            t.insert(k.as_slice(), value.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    fn get_raw<K: Serialize>(
        &self,
        label: &[u8],
        key: &K,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let k = Self::storage_key(label, key)?;
        let txn = self.db.begin_read()?;
        let t = txn.open_table(MLS)?;
        Ok(t.get(k.as_slice())?.map(|v| v.value().to_vec()))
    }

    fn get<K: Serialize, V: serde::de::DeserializeOwned>(
        &self,
        label: &[u8],
        key: &K,
    ) -> Result<Option<V>, StorageError> {
        match self.get_raw(label, key)? {
            None => Ok(None),
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        }
    }

    /// Read a list, treating "never written" as empty.
    ///
    /// Absent and empty are the same answer for a queue, and openmls relies on
    /// that: a fresh group has no proposals and must not error on being asked.
    fn get_list<K: Serialize, V: serde::de::DeserializeOwned>(
        &self,
        label: &[u8],
        key: &K,
    ) -> Result<Vec<V>, StorageError> {
        let raw: Vec<Vec<u8>> = match self.get_raw(label, key)? {
            None => return Ok(Vec::new()),
            Some(bytes) => serde_json::from_slice(&bytes)?,
        };
        raw.iter()
            .map(|item| serde_json::from_slice(item).map_err(StorageError::from))
            .collect()
    }

    fn append_raw<K: Serialize>(
        &self,
        label: &[u8],
        key: &K,
        value: Vec<u8>,
    ) -> Result<(), StorageError> {
        let mut list: Vec<Vec<u8>> = match self.get_raw(label, key)? {
            None => Vec::new(),
            Some(bytes) => serde_json::from_slice(&bytes)?,
        };
        list.push(value);
        self.put(label, key, serde_json::to_vec(&list)?)
    }

    fn remove_from_list<K: Serialize>(
        &self,
        label: &[u8],
        key: &K,
        value: &[u8],
    ) -> Result<(), StorageError> {
        let mut list: Vec<Vec<u8>> = match self.get_raw(label, key)? {
            None => return Ok(()),
            Some(bytes) => serde_json::from_slice(&bytes)?,
        };
        if let Some(pos) = list.iter().position(|item| item == value) {
            list.remove(pos);
        }
        self.put(label, key, serde_json::to_vec(&list)?)
    }

    fn remove<K: Serialize>(&self, label: &[u8], key: &K) -> Result<(), StorageError> {
        let k = Self::storage_key(label, key)?;
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(MLS)?;
            t.remove(k.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The 56 delegations
// ---------------------------------------------------------------------------
//
// Every one is two lines over the helpers above. The labels are the concept's
// name and are SHARED by each write/read/delete triple — `write_tree`, `tree`
// and `delete_tree` all say `TREE` — so a triple cannot drift apart the way it
// could if each method carried its own string. The one place that matters:
// `write_mls_join_config` / `mls_group_join_config` / `delete_group_config` are
// named inconsistently by the trait itself but are the same fact, and they
// share a label here.

const JOIN_CONFIG: &[u8] = b"join_config";
const LEAF_NODES: &[u8] = b"leaf_nodes";
const PROPOSAL: &[u8] = b"proposal";
const PROPOSAL_REFS: &[u8] = b"proposal_refs";
const TREE: &[u8] = b"tree";
const INTERIM_HASH: &[u8] = b"interim_hash";
const CONTEXT: &[u8] = b"context";
const CONFIRMATION_TAG: &[u8] = b"confirmation_tag";
const GROUP_STATE: &[u8] = b"group_state";
const MESSAGE_SECRETS: &[u8] = b"message_secrets";
const RESUMPTION_PSK: &[u8] = b"resumption_psk";
const LEAF_INDEX: &[u8] = b"leaf_index";
const EPOCH_SECRETS: &[u8] = b"epoch_secrets";
const SIG_KEYPAIR: &[u8] = b"sig_keypair";
const ENC_KEYPAIR: &[u8] = b"enc_keypair";
const EPOCH_KEYPAIRS: &[u8] = b"epoch_keypairs";
const KEY_PACKAGE: &[u8] = b"key_package";
const PSK: &[u8] = b"psk";

impl StorageProvider<CURRENT_VERSION> for RedbStorage {
    type Error = StorageError;

    // -- writers ----------------------------------------------------------

    fn write_mls_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        config: &MlsGroupJoinConfig,
    ) -> Result<(), Self::Error> {
        self.put(JOIN_CONFIG, group_id, serde_json::to_vec(config)?)
    }

    fn append_own_leaf_node<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        leaf_node: &LeafNode,
    ) -> Result<(), Self::Error> {
        self.append_raw(LEAF_NODES, group_id, serde_json::to_vec(leaf_node)?)
    }

    /// Two writes, deliberately: the proposal under `(group, ref)`, and the ref
    /// appended to the group's queue. The queue is what `queued_proposals`
    /// iterates; without it a stored proposal exists and is unreachable.
    fn queue_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
        proposal: &QueuedProposal,
    ) -> Result<(), Self::Error> {
        self.put(
            PROPOSAL,
            &(group_id, proposal_ref),
            serde_json::to_vec(proposal)?,
        )?;
        self.append_raw(PROPOSAL_REFS, group_id, serde_json::to_vec(proposal_ref)?)
    }

    fn write_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        tree: &TreeSync,
    ) -> Result<(), Self::Error> {
        self.put(TREE, group_id, serde_json::to_vec(tree)?)
    }

    fn write_interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        interim_transcript_hash: &InterimTranscriptHash,
    ) -> Result<(), Self::Error> {
        self.put(
            INTERIM_HASH,
            group_id,
            serde_json::to_vec(interim_transcript_hash)?,
        )
    }

    fn write_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_context: &GroupContext,
    ) -> Result<(), Self::Error> {
        self.put(CONTEXT, group_id, serde_json::to_vec(group_context)?)
    }

    fn write_confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        confirmation_tag: &ConfirmationTag,
    ) -> Result<(), Self::Error> {
        self.put(
            CONFIRMATION_TAG,
            group_id,
            serde_json::to_vec(confirmation_tag)?,
        )
    }

    fn write_group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_state: &GroupState,
    ) -> Result<(), Self::Error> {
        self.put(GROUP_STATE, group_id, serde_json::to_vec(group_state)?)
    }

    fn write_message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        message_secrets: &MessageSecrets,
    ) -> Result<(), Self::Error> {
        self.put(
            MESSAGE_SECRETS,
            group_id,
            serde_json::to_vec(message_secrets)?,
        )
    }

    fn write_resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        resumption_psk_store: &ResumptionPskStore,
    ) -> Result<(), Self::Error> {
        self.put(
            RESUMPTION_PSK,
            group_id,
            serde_json::to_vec(resumption_psk_store)?,
        )
    }

    fn write_own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        own_leaf_index: &LeafNodeIndex,
    ) -> Result<(), Self::Error> {
        self.put(LEAF_INDEX, group_id, serde_json::to_vec(own_leaf_index)?)
    }

    fn write_group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_epoch_secrets: &GroupEpochSecrets,
    ) -> Result<(), Self::Error> {
        self.put(
            EPOCH_SECRETS,
            group_id,
            serde_json::to_vec(group_epoch_secrets)?,
        )
    }

    fn write_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
        signature_key_pair: &SignatureKeyPair,
    ) -> Result<(), Self::Error> {
        self.put(
            SIG_KEYPAIR,
            public_key,
            serde_json::to_vec(signature_key_pair)?,
        )
    }

    fn write_encryption_key_pair<
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
        key_pair: &HpkeKeyPair,
    ) -> Result<(), Self::Error> {
        self.put(ENC_KEYPAIR, public_key, serde_json::to_vec(key_pair)?)
    }

    /// Whole-value, not append: the trait hands over the complete set for this
    /// (group, epoch, leaf), so appending would accumulate stale generations of
    /// the same epoch's keys.
    fn write_encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
        key_pairs: &[HpkeKeyPair],
    ) -> Result<(), Self::Error> {
        let items: Vec<Vec<u8>> = key_pairs
            .iter()
            .map(serde_json::to_vec)
            .collect::<Result<_, _>>()?;
        self.put(
            EPOCH_KEYPAIRS,
            &(group_id, epoch, leaf_index),
            serde_json::to_vec(&items)?,
        )
    }

    fn write_key_package<
        HashReference: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &HashReference,
        key_package: &KeyPackage,
    ) -> Result<(), Self::Error> {
        self.put(KEY_PACKAGE, hash_ref, serde_json::to_vec(key_package)?)
    }

    fn write_psk<
        PskId: traits::PskId<CURRENT_VERSION>,
        PskBundle: traits::PskBundle<CURRENT_VERSION>,
    >(
        &self,
        psk_id: &PskId,
        psk: &PskBundle,
    ) -> Result<(), Self::Error> {
        self.put(PSK, psk_id, serde_json::to_vec(psk)?)
    }

    // -- readers ----------------------------------------------------------

    fn mls_group_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MlsGroupJoinConfig>, Self::Error> {
        self.get(JOIN_CONFIG, group_id)
    }

    fn own_leaf_nodes<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<LeafNode>, Self::Error> {
        self.get_list(LEAF_NODES, group_id)
    }

    fn queued_proposal_refs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<ProposalRef>, Self::Error> {
        self.get_list(PROPOSAL_REFS, group_id)
    }

    /// Refs first, then each proposal. A ref whose proposal is missing is a
    /// torn store, not an empty queue — so it is an error rather than a
    /// silently shorter list, for the same reason the message reader refuses a
    /// dangling edge.
    fn queued_proposals<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<(ProposalRef, QueuedProposal)>, Self::Error> {
        let refs: Vec<ProposalRef> = self.get_list(PROPOSAL_REFS, group_id)?;
        refs.into_iter()
            .map(|r| {
                let proposal: QueuedProposal =
                    self.get(PROPOSAL, &(group_id, &r))?.ok_or_else(|| {
                        StorageError::Serialization(
                            "a queued proposal ref has no proposal behind it".to_string(),
                        )
                    })?;
                Ok((r, proposal))
            })
            .collect()
    }

    fn tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<TreeSync>, Self::Error> {
        self.get(TREE, group_id)
    }

    fn group_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupContext>, Self::Error> {
        self.get(CONTEXT, group_id)
    }

    fn interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<InterimTranscriptHash>, Self::Error> {
        self.get(INTERIM_HASH, group_id)
    }

    fn confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ConfirmationTag>, Self::Error> {
        self.get(CONFIRMATION_TAG, group_id)
    }

    fn group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupState>, Self::Error> {
        self.get(GROUP_STATE, group_id)
    }

    fn message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MessageSecrets>, Self::Error> {
        self.get(MESSAGE_SECRETS, group_id)
    }

    fn resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ResumptionPskStore>, Self::Error> {
        self.get(RESUMPTION_PSK, group_id)
    }

    fn own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<LeafNodeIndex>, Self::Error> {
        self.get(LEAF_INDEX, group_id)
    }

    fn group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupEpochSecrets>, Self::Error> {
        self.get(EPOCH_SECRETS, group_id)
    }

    fn signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<Option<SignatureKeyPair>, Self::Error> {
        self.get(SIG_KEYPAIR, public_key)
    }

    fn encryption_key_pair<
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<Option<HpkeKeyPair>, Self::Error> {
        self.get(ENC_KEYPAIR, public_key)
    }

    fn encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
    ) -> Result<Vec<HpkeKeyPair>, Self::Error> {
        self.get_list(EPOCH_KEYPAIRS, &(group_id, epoch, leaf_index))
    }

    fn key_package<
        KeyPackageRef: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &KeyPackageRef,
    ) -> Result<Option<KeyPackage>, Self::Error> {
        self.get(KEY_PACKAGE, hash_ref)
    }

    fn psk<PskBundle: traits::PskBundle<CURRENT_VERSION>, PskId: traits::PskId<CURRENT_VERSION>>(
        &self,
        psk_id: &PskId,
    ) -> Result<Option<PskBundle>, Self::Error> {
        self.get(PSK, psk_id)
    }

    // -- deleters ---------------------------------------------------------

    /// Both halves, mirroring `queue_proposal`. Removing only the proposal
    /// would leave a ref pointing at nothing, which `queued_proposals` reports
    /// as a torn store — a correct complaint about a self-inflicted wound.
    fn remove_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
    ) -> Result<(), Self::Error> {
        self.remove(PROPOSAL, &(group_id, proposal_ref))?;
        self.remove_from_list(PROPOSAL_REFS, group_id, &serde_json::to_vec(proposal_ref)?)
    }

    fn delete_own_leaf_nodes<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(LEAF_NODES, group_id)
    }

    fn delete_group_config<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(JOIN_CONFIG, group_id)
    }

    fn delete_tree<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(TREE, group_id)
    }

    fn delete_confirmation_tag<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(CONFIRMATION_TAG, group_id)
    }

    fn delete_group_state<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(GROUP_STATE, group_id)
    }

    fn delete_context<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(CONTEXT, group_id)
    }

    fn delete_interim_transcript_hash<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(INTERIM_HASH, group_id)
    }

    fn delete_message_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(MESSAGE_SECRETS, group_id)
    }

    fn delete_all_resumption_psk_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(RESUMPTION_PSK, group_id)
    }

    fn delete_own_leaf_index<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(LEAF_INDEX, group_id)
    }

    fn delete_group_epoch_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(EPOCH_SECRETS, group_id)
    }

    /// Drops the refs list only. The proposal bodies are unreachable once the
    /// queue is gone, and openmls treats the queue as the index of record.
    fn clear_proposal_queue<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let refs: Vec<ProposalRef> = self.get_list(PROPOSAL_REFS, group_id)?;
        for r in &refs {
            self.remove(PROPOSAL, &(group_id, r))?;
        }
        self.remove(PROPOSAL_REFS, group_id)
    }

    fn delete_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<(), Self::Error> {
        self.remove(SIG_KEYPAIR, public_key)
    }

    fn delete_encryption_key_pair<EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>>(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<(), Self::Error> {
        self.remove(ENC_KEYPAIR, public_key)
    }

    fn delete_encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
    ) -> Result<(), Self::Error> {
        self.remove(EPOCH_KEYPAIRS, &(group_id, epoch, leaf_index))
    }

    fn delete_key_package<KeyPackageRef: traits::HashReference<CURRENT_VERSION>>(
        &self,
        hash_ref: &KeyPackageRef,
    ) -> Result<(), Self::Error> {
        self.remove(KEY_PACKAGE, hash_ref)
    }

    fn delete_psk<PskKey: traits::PskId<CURRENT_VERSION>>(
        &self,
        psk_id: &PskKey,
    ) -> Result<(), Self::Error> {
        self.remove(PSK, psk_id)
    }
}
