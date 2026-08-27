//! The redb table layer: the schema, and the codecs over it.
//!
//! All tables use raw `&'static [u8]` keys and values; the encoding and
//! decoding live in this module. Each table constant documents the byte layout
//! of its key and value.
//!
//! **Every table this crate touches is declared here.** In the corpus this
//! module was promoted from, ten table constants were declared here *and*
//! re-declared by string literal in three other files, while two more tables
//! existed only as literals and never appeared here at all — so the canonical
//! list was both duplicated and incomplete, and a single typo would have
//! opened a fresh empty table rather than failing. [`ALL_TABLES`] is now the
//! one list, and [`Db`] creates from it.

use redb::{backends::InMemoryBackend, Database, TableDefinition};
use social_tree_core::model::{DeviceId, GroupId, Hash, KindTag, PrincipalId, TypedId};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by the table layer.
///
/// redb's own error types are boxed. They are large — `TransactionError` alone
/// is 160 bytes — and this enum is the `Err` half of every codec and every
/// store call, so unboxed they would widen a `Result` that is overwhelmingly
/// `Ok` from 24 bytes to 168 on every return in the crate. Boxing moves that
/// cost onto the failure path, which is where it belongs; the error itself is
/// preserved whole.
#[derive(Debug, Error)]
pub enum DbError {
    /// redb could not read or write the underlying storage.
    #[error("redb storage error: {0}")]
    Storage(Box<redb::StorageError>),

    /// The database file could not be opened or created.
    #[error("redb database error: {0}")]
    Database(Box<redb::DatabaseError>),

    /// A table could not be opened — a missing table means the schema was
    /// never created (see [`Db::open`], which creates it).
    #[error("redb table error: {0}")]
    Table(Box<redb::TableError>),

    /// A read or write transaction could not be begun.
    #[error("redb transaction error: {0}")]
    Transaction(Box<redb::TransactionError>),

    /// A write transaction failed to commit; nothing in it landed.
    #[error("redb commit error: {0}")]
    Commit(Box<redb::CommitError>),

    /// Stored bytes did not decode as the record their table promises. The
    /// string carries what was wrong; the store never guesses past a bad read.
    #[error("deserialization error: {0}")]
    Deserialize(String),

    /// A composite key was the wrong width for its table's layout.
    #[error("invalid key length: expected {expected}, got {got}")]
    KeyLength {
        /// The width the table's key layout requires.
        expected: usize,
        /// The width actually supplied.
        got: usize,
    },
}

// `#[from]` cannot box on the way in, and `?` at every call site is worth
// keeping, so the five conversions are written out.
macro_rules! boxed_from {
    ($($src:ty => $variant:ident),+ $(,)?) => {$(
        impl From<$src> for DbError {
            fn from(e: $src) -> Self {
                DbError::$variant(Box::new(e))
            }
        }
    )+};
}

boxed_from! {
    redb::StorageError => Storage,
    redb::DatabaseError => Database,
    redb::TableError => Table,
    redb::TransactionError => Transaction,
    redb::CommitError => Commit,
}

// ---------------------------------------------------------------------------
// Table definitions
// ---------------------------------------------------------------------------

/// AUTH_ASSERTIONS
///
/// Key:   32-byte content Hash (raw)
/// Value: u8 version=1 || canonical_bytes_with_sig
pub const AUTH_ASSERTIONS: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("auth_assertions_v1");

/// AUTH_ASSERTIONS_BY_DEVICE
///
/// Key:   DeviceId(32) || lamport(8, big-endian) = 40 bytes
/// Value: 32-byte Hash
///
/// Range scan: prefix DeviceId(32) to iterate all assertions for a device
/// in lamport order (ascending, because keys are sorted lexicographically and
/// big-endian u64 preserves numeric order).
pub const AUTH_ASSERTIONS_BY_DEVICE: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("auth_assertions_by_device_v1");

/// AUTH_GOV_LOG
///
/// Key:   GroupId(32) || gov_seq(8, big-endian) = 40 bytes
/// Value: 32-byte Hash (governance assertion hash)
///
/// Range scan: prefix GroupId(32) to iterate governance entries in sequence order.
pub const AUTH_GOV_LOG: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("auth_gov_log_v1");

/// AUTH_ARTIFACTS
///
/// Key:   32-byte content Hash
/// Value: versioned artifact record bytes (u8 version || payload)
pub const AUTH_ARTIFACTS: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("auth_artifacts_v1");

/// AUTH_GENESIS
///
/// Key:   32-byte GroupId
/// Value: versioned genesis record bytes (u8 version || payload)
pub const AUTH_GENESIS: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("auth_genesis_v1");

/// IDX_EDGES_OUT
///
/// Key:   source_typed_id(33) || edge_type(2, big-endian) || target_typed_id(33) = 68 bytes
/// Value: versioned EdgeMeta bytes
///
/// Range scan: prefix source_typed_id(33) → all outbound edges from source.
/// Sub-range:  prefix source_typed_id(33) || edge_type(2) → edges of a specific type.
pub const IDX_EDGES_OUT: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("idx_edges_out_v1");

/// IDX_EDGES_IN
///
/// Key:   target_typed_id(33) || edge_type(2, big-endian) || source_typed_id(33) = 68 bytes
/// Value: versioned EdgeMeta bytes (identical content to IDX_EDGES_OUT entry)
///
/// Range scan: prefix target_typed_id(33) → all inbound edges to target.
pub const IDX_EDGES_IN: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("idx_edges_in_v1");

/// IDX_NODES
///
/// Key:   typed_id(33)
/// Value: versioned NodeCard bytes
pub const IDX_NODES: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("idx_nodes_v1");

/// STATE_GROUP
///
/// Key:   32-byte GroupId
/// Value: versioned GroupState bytes
pub const STATE_GROUP: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("state_group_v1");

/// STATE_CHECKPOINTS
///
/// Key:   GroupId(32) || ckpt_seq(8, big-endian) = 40 bytes
/// Value: versioned Checkpoint bytes
///
/// Range scan: prefix GroupId(32) to iterate checkpoints in sequence order.
pub const STATE_CHECKPOINTS: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("state_checkpoints_v1");

/// STATE_BLOB_PRESENCE
///
/// Key:   32-byte content Hash
/// Value: presence marker (existence is the fact; the value is unused)
///
/// Promoted into this module during P7 S0. In the corpus this table was
/// declared only by string literal in `fold_derived.rs` and `surface.rs` and
/// never here — the exact drift this module now exists to prevent.
pub const STATE_BLOB_PRESENCE: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("state_blob_presence_v1");

/// META
///
/// Key:   an ASCII property name
/// Value: the property's bytes
///
/// One key today, `b"comparator_version"` → `[u8; 1]`: the `merge_cmp` version
/// the derived tables were last folded under. Stamped by `rebuild`; absent on
/// stores that predate comparator versioning, which `needs_rebuild` reads as
/// v1. Promoted here for the same reason as STATE_BLOB_PRESENCE.
pub const META: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("meta_v1");

// ---------------------------------------------------------------------------
// EdgeType enum
// ---------------------------------------------------------------------------

/// Two-byte discriminant for edge relationships between typed entities.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeType {
    /// The source principal is a member of the target group.
    MemberOf = 0x0001,
    /// The source artifact carries the target blob as an attachment.
    HasAttachment = 0x0002,
    /// The source artifact refers to the target.
    References = 0x0003,
    /// The source principal vouches for the target principal.
    Vouches = 0x0004,
}

impl EdgeType {
    /// Encode as big-endian u16 bytes.
    pub fn to_be_bytes(self) -> [u8; 2] {
        (self as u16).to_be_bytes()
    }

    /// Decode from big-endian u16 bytes; returns `None` for unknown values.
    pub fn from_be_bytes(b: [u8; 2]) -> Option<Self> {
        match u16::from_be_bytes(b) {
            0x0001 => Some(EdgeType::MemberOf),
            0x0002 => Some(EdgeType::HasAttachment),
            0x0003 => Some(EdgeType::References),
            0x0004 => Some(EdgeType::Vouches),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Key encoding / decoding
// ---------------------------------------------------------------------------

/// Encode an `AUTH_ASSERTIONS_BY_DEVICE` key.
///
/// Layout: DeviceId(32) || lamport(8, big-endian) = 40 bytes
pub fn encode_by_device_key(device: &DeviceId, lamport: u64) -> [u8; 40] {
    let mut buf = [0u8; 40];
    buf[..32].copy_from_slice(device.as_bytes());
    buf[32..40].copy_from_slice(&lamport.to_be_bytes());
    buf
}

/// Decode an `AUTH_ASSERTIONS_BY_DEVICE` key.
///
/// Refuses a key of the wrong width with [`DbError::KeyLength`].
pub fn decode_by_device_key(bytes: &[u8]) -> Result<(DeviceId, u64), DbError> {
    let bytes: &[u8; 40] = bytes.try_into().map_err(|_| DbError::KeyLength {
        expected: 40,
        got: bytes.len(),
    })?;
    let mut dev = [0u8; 32];
    dev.copy_from_slice(&bytes[..32]);
    let lamport = u64::from_be_bytes(
        bytes[32..40]
            .try_into()
            .expect("not possible: an 8-byte window of a 40-byte array"),
    );
    Ok((DeviceId::new(dev), lamport))
}

/// Encode an `AUTH_GOV_LOG` key.
///
/// Layout: GroupId(32) || gov_seq(8, big-endian) = 40 bytes
pub fn encode_gov_log_key(group: &GroupId, gov_seq: u64) -> [u8; 40] {
    let mut buf = [0u8; 40];
    buf[..32].copy_from_slice(group.as_bytes());
    buf[32..40].copy_from_slice(&gov_seq.to_be_bytes());
    buf
}

/// Decode an `AUTH_GOV_LOG` key.
///
/// Refuses a key of the wrong width with [`DbError::KeyLength`].
pub fn decode_gov_log_key(bytes: &[u8]) -> Result<(GroupId, u64), DbError> {
    let bytes: &[u8; 40] = bytes.try_into().map_err(|_| DbError::KeyLength {
        expected: 40,
        got: bytes.len(),
    })?;
    let mut grp = [0u8; 32];
    grp.copy_from_slice(&bytes[..32]);
    let gov_seq = u64::from_be_bytes(
        bytes[32..40]
            .try_into()
            .expect("not possible: an 8-byte window of a 40-byte array"),
    );
    Ok((GroupId::new(grp), gov_seq))
}

/// Encode an `IDX_EDGES_OUT` key.
///
/// Layout: source_typed_id(33) || edge_type(2, big-endian) || target_typed_id(33) = 68 bytes
pub fn encode_edge_out_key(source: &TypedId, edge_type: EdgeType, target: &TypedId) -> [u8; 68] {
    let mut buf = [0u8; 68];
    buf[..33].copy_from_slice(source.as_bytes());
    buf[33..35].copy_from_slice(&edge_type.to_be_bytes());
    buf[35..68].copy_from_slice(target.as_bytes());
    buf
}

/// Decode an `IDX_EDGES_OUT` key.
///
/// Refuses rather than panics. The bytes come from the store, so an unknown
/// edge-type discriminant or an invalid kind tag is what a forward-version or
/// corrupted database looks like on a read — not a programming error. The
/// refusal names the value it did not recognise.
pub fn decode_edge_out_key(bytes: &[u8]) -> Result<(TypedId, EdgeType, TypedId), DbError> {
    let bytes: &[u8; 68] = bytes.try_into().map_err(|_| DbError::KeyLength {
        expected: 68,
        got: bytes.len(),
    })?;
    let raw_et: [u8; 2] = bytes[33..35]
        .try_into()
        .expect("not possible: a 2-byte window of a 68-byte array");
    let et = EdgeType::from_be_bytes(raw_et).ok_or_else(|| {
        DbError::Deserialize(format!(
            "decode_edge_out_key: unknown EdgeType discriminant {}",
            u16::from_be_bytes(raw_et)
        ))
    })?;
    // Reconstruct TypedId via the public KindTag + Hash constructor.
    let src_kind = KindTag::from_u8(bytes[0]).ok_or_else(|| {
        DbError::Deserialize(format!(
            "decode_edge_out_key: invalid source KindTag {}",
            bytes[0]
        ))
    })?;
    let mut src_hash = [0u8; 32];
    src_hash.copy_from_slice(&bytes[1..33]);
    let tgt_kind = KindTag::from_u8(bytes[35]).ok_or_else(|| {
        DbError::Deserialize(format!(
            "decode_edge_out_key: invalid target KindTag {}",
            bytes[35]
        ))
    })?;
    let mut tgt_hash = [0u8; 32];
    tgt_hash.copy_from_slice(&bytes[36..68]);
    Ok((
        TypedId::new(src_kind, Hash::new(src_hash)),
        et,
        TypedId::new(tgt_kind, Hash::new(tgt_hash)),
    ))
}

/// Encode an `IDX_EDGES_IN` key (inverted direction).
///
/// Layout: target_typed_id(33) || edge_type(2, big-endian) || source_typed_id(33) = 68 bytes
pub fn encode_edge_in_key(target: &TypedId, edge_type: EdgeType, source: &TypedId) -> [u8; 68] {
    let mut buf = [0u8; 68];
    buf[..33].copy_from_slice(target.as_bytes());
    buf[33..35].copy_from_slice(&edge_type.to_be_bytes());
    buf[35..68].copy_from_slice(source.as_bytes());
    buf
}

/// Encode a `STATE_CHECKPOINTS` key.
///
/// Layout: GroupId(32) || ckpt_seq(8, big-endian) = 40 bytes
pub fn encode_checkpoint_key(group: &GroupId, seq: u64) -> [u8; 40] {
    let mut buf = [0u8; 40];
    buf[..32].copy_from_slice(group.as_bytes());
    buf[32..40].copy_from_slice(&seq.to_be_bytes());
    buf
}

// ---------------------------------------------------------------------------
// EdgeMeta — derived-family edge record
// ---------------------------------------------------------------------------

/// Metadata attached to a directed edge in the derived graph.
///
/// Wire layout (version = 1):
/// - version        : 1 byte  (always 0x01)
/// - since_lamport  : 8 bytes (big-endian u64)
/// - since_assertion: 32 bytes (Hash raw)
/// - present        : 1 byte  (0x00 = absent, 0x01 = present)
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeMeta {
    /// Wire version of this record (see the layout above).
    pub version: u8,
    /// Lamport stamp of the assertion that established the edge.
    pub since_lamport: u64,
    /// Hash of the assertion that established the edge.
    pub since_assertion: Hash,
    /// Whether the edge currently holds; a removed edge stays as a tombstone.
    pub present: bool,
    // attributes: empty, reserved for future use
}

impl EdgeMeta {
    /// Serialize to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(42);
        buf.push(self.version);
        buf.extend_from_slice(&self.since_lamport.to_be_bytes());
        buf.extend_from_slice(self.since_assertion.as_bytes());
        buf.push(if self.present { 0x01 } else { 0x00 });
        buf
    }

    /// Deserialize from bytes.
    pub fn from_bytes(b: &[u8]) -> Result<Self, DbError> {
        if b.len() < 42 {
            return Err(DbError::Deserialize(format!(
                "EdgeMeta: expected >= 42 bytes, got {}",
                b.len()
            )));
        }
        let version = b[0];
        let since_lamport = u64::from_be_bytes(
            b[1..9]
                .try_into()
                .expect("not possible: the length was checked as >= 42 immediately above"),
        );
        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(&b[9..41]);
        let since_assertion = Hash::new(hash_bytes);
        let present = b[41] != 0x00;
        Ok(EdgeMeta {
            version,
            since_lamport,
            since_assertion,
            present,
        })
    }
}

// ---------------------------------------------------------------------------
// NodeCard — derived-family node record
// ---------------------------------------------------------------------------

/// A projection of a node in the derived graph.
///
/// Wire layout (version = 1):
/// - version    : 1 byte  (always 0x01)
/// - kind       : 1 byte  (KindTag discriminant)
/// - present    : 1 byte  (0x00 = absent, 0x01 = present)
/// - title_len  : 4 bytes (big-endian u32)
/// - title      : title_len bytes (UTF-8)
/// - created_by : 32 bytes (PrincipalId raw)
/// - created_at : 8 bytes (big-endian u64, Unix seconds)
/// - has_blob   : 1 byte  (0x00 = None, 0x01 = Some)
/// - blob_hash  : 32 bytes (only present when has_blob == 0x01)
#[derive(Debug, Clone, PartialEq)]
pub struct NodeCard {
    /// Wire version of this record (see the layout above).
    pub version: u8,
    /// What kind of entity the node is.
    pub kind: KindTag,
    /// Whether the node currently exists; a removed node stays as a tombstone.
    pub present: bool,
    /// Display title.
    pub title: String,
    /// The principal that created the node.
    pub created_by: PrincipalId,
    /// Creation time in Unix seconds, as recorded by the writer.
    pub created_at: u64,
    /// Content hash of the attached blob, when the node carries one.
    pub blob_hash: Option<Hash>,
}

impl NodeCard {
    /// Serialize to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let title_bytes = self.title.as_bytes();
        let title_len = title_bytes.len() as u32;
        let capacity = 1
            + 1
            + 1
            + 4
            + title_bytes.len()
            + 32
            + 8
            + 1
            + if self.blob_hash.is_some() { 32 } else { 0 };
        let mut buf = Vec::with_capacity(capacity);

        buf.push(self.version);
        buf.push(self.kind as u8);
        buf.push(if self.present { 0x01 } else { 0x00 });
        buf.extend_from_slice(&title_len.to_be_bytes());
        buf.extend_from_slice(title_bytes);
        buf.extend_from_slice(self.created_by.as_bytes());
        buf.extend_from_slice(&self.created_at.to_be_bytes());
        match &self.blob_hash {
            None => buf.push(0x00),
            Some(h) => {
                buf.push(0x01);
                buf.extend_from_slice(h.as_bytes());
            }
        }
        buf
    }

    /// Deserialize from bytes.
    pub fn from_bytes(b: &[u8]) -> Result<Self, DbError> {
        // Minimum: 1+1+1+4+0+32+8+1 = 48
        if b.len() < 48 {
            return Err(DbError::Deserialize(format!(
                "NodeCard: expected >= 48 bytes, got {}",
                b.len()
            )));
        }
        let version = b[0];
        let kind = KindTag::from_u8(b[1]).ok_or_else(|| {
            DbError::Deserialize(format!("NodeCard: unknown KindTag byte 0x{:02x}", b[1]))
        })?;
        let present = b[2] != 0x00;
        let title_len = u32::from_be_bytes(
            b[3..7]
                .try_into()
                .expect("not possible: the header length was checked immediately above"),
        ) as usize;
        let title_end = 7 + title_len;
        if b.len() < title_end + 32 + 8 + 1 {
            return Err(DbError::Deserialize(format!(
                "NodeCard: truncated at title (need {} bytes, have {})",
                title_end + 41,
                b.len()
            )));
        }
        let title = std::str::from_utf8(&b[7..title_end])
            .map_err(|e| DbError::Deserialize(format!("NodeCard: invalid UTF-8 in title: {}", e)))?
            .to_owned();
        let mut pb = [0u8; 32];
        pb.copy_from_slice(&b[title_end..title_end + 32]);
        let created_by = PrincipalId::new(pb);
        let created_at = u64::from_be_bytes(
            b[title_end + 32..title_end + 40]
                .try_into()
                .expect("not possible: the length was checked as >= title_end + 41 above"),
        );
        let has_blob_offset = title_end + 40;
        if b.len() < has_blob_offset + 1 {
            return Err(DbError::Deserialize(
                "NodeCard: missing has_blob byte".to_string(),
            ));
        }
        let blob_hash = if b[has_blob_offset] == 0x00 {
            None
        } else {
            if b.len() < has_blob_offset + 1 + 32 {
                return Err(DbError::Deserialize(
                    "NodeCard: truncated blob_hash".to_string(),
                ));
            }
            let mut hb = [0u8; 32];
            hb.copy_from_slice(&b[has_blob_offset + 1..has_blob_offset + 33]);
            Some(Hash::new(hb))
        };
        Ok(NodeCard {
            version,
            kind,
            present,
            title,
            created_by,
            created_at,
            blob_hash,
        })
    }
}

// ---------------------------------------------------------------------------
// Database wrapper
// ---------------------------------------------------------------------------

/// Wrapper around a `redb::Database` that carries the table schema.
pub struct Db {
    inner: Database,
}

/// Every table this crate reads or writes, in one list.
///
/// The list exists so [`Db::ensure_schema`] cannot silently fall behind the
/// declarations above: adding a table without adding it here is the one way
/// left to reintroduce the drift this module was reorganised to prevent.
const ALL_TABLES: &[TableDefinition<'static, &'static [u8], &'static [u8]>] = &[
    AUTH_ASSERTIONS,
    AUTH_ASSERTIONS_BY_DEVICE,
    AUTH_GOV_LOG,
    AUTH_ARTIFACTS,
    AUTH_GENESIS,
    IDX_EDGES_OUT,
    IDX_EDGES_IN,
    IDX_NODES,
    STATE_GROUP,
    STATE_CHECKPOINTS,
    STATE_BLOB_PRESENCE,
    META,
];

impl Db {
    /// Open (or create) a database file at `path`.
    pub fn open(path: &std::path::Path) -> Result<Self, DbError> {
        let db = Self {
            inner: Database::create(path)?,
        };
        db.ensure_schema()?;
        Ok(db)
    }

    /// Create every declared table, so a reader does not depend on a writer
    /// having gone first.
    ///
    /// Found by the promotion's own pins (P7 S0): redb answers a read against a
    /// never-written table with `TableDoesNotExist`, not with "empty". In the
    /// corpus nothing hit that, because `DerivedFold::new` opened all nine
    /// tables in a write transaction before anyone read — and it did so from a
    /// constructor that could only `.expect()` when that failed, a fallible
    /// constructor that did not say so.
    ///
    /// Making the schema a property of the *database* fixes both: any reader
    /// sees empty tables rather than an error regardless of construction
    /// order, and the failure surfaces through the `Result` that `open` and
    /// `create_in_memory` already return. The fold no longer needs to do it,
    /// so its constructor no longer needs to panic.
    fn ensure_schema(&self) -> Result<(), DbError> {
        let txn = self.inner.begin_write()?;
        for table in ALL_TABLES {
            txn.open_table(*table)?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Create a database that lives entirely in memory and vanishes with the
    /// handle.
    ///
    /// Two deliberate changes from the corpus version this was promoted from
    /// (P7 S0):
    ///
    /// **It is no longer `#[cfg(test)]`.** That gate is invisible across a
    /// crate boundary, so every consumer outside this crate — the ffi crate's
    /// pins, the corpus's own suites — was locked out of the only constructor
    /// that does not touch the filesystem. The core hit and documented this
    /// same footgun (`social-tree-core::ports::mocks`). Ungated, but still not
    /// a production path: [`Db::open`] is what a shell calls.
    ///
    /// **It uses redb's own [`InMemoryBackend`] instead of an unlinked temp
    /// file.** The old trick — create a named temp file, then `remove_file` it
    /// and rely on the open fd — is a POSIX behaviour, and the comment saying
    /// "on Linux" was doing real work: it leaves a file behind on Windows and
    /// writes to disk everywhere. redb ships a backend for exactly this, and
    /// reaching for it is also what makes the wasm story credible (P0-1's
    /// evidence): no filesystem is required to run this crate at all.
    pub fn create_in_memory() -> Result<Self, DbError> {
        let db = Self {
            inner: Database::builder().create_with_backend(InMemoryBackend::new())?,
        };
        db.ensure_schema()?;
        Ok(db)
    }

    /// Access the underlying `redb::Database`.
    pub fn inner(&self) -> &Database {
        &self.inner
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use social_tree_core::model::{DeviceId, GroupId, Hash, KindTag, PrincipalId, TypedId};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_hash(seed: u8) -> Hash {
        Hash::new([seed; 32])
    }

    fn make_device(seed: u8) -> DeviceId {
        DeviceId::new([seed; 32])
    }

    fn make_group(seed: u8) -> GroupId {
        GroupId::new([seed; 32])
    }

    fn make_typed_id(kind: KindTag, seed: u8) -> TypedId {
        TypedId::new(kind, make_hash(seed))
    }

    fn make_principal(seed: u8) -> PrincipalId {
        PrincipalId::new([seed; 32])
    }

    // -----------------------------------------------------------------------
    // Key encoding round-trips
    // -----------------------------------------------------------------------

    #[test]
    fn by_device_key_round_trip() {
        let dev = make_device(0xAB);
        let lamport = 0x0102030405060708u64;
        let enc = encode_by_device_key(&dev, lamport);
        assert_eq!(enc.len(), 40);
        let (dev2, lam2) = decode_by_device_key(&enc).expect("a key this test just encoded");
        assert_eq!(dev2.as_bytes(), dev.as_bytes());
        assert_eq!(lam2, lamport);
    }

    #[test]
    fn gov_log_key_round_trip() {
        let grp = make_group(0x77);
        let seq = 999u64;
        let enc = encode_gov_log_key(&grp, seq);
        assert_eq!(enc.len(), 40);
        let (grp2, seq2) = decode_gov_log_key(&enc).expect("a key this test just encoded");
        assert_eq!(grp2.as_bytes(), grp.as_bytes());
        assert_eq!(seq2, seq);
    }

    #[test]
    fn edge_out_key_round_trip() {
        let src = make_typed_id(KindTag::Principal, 0x11);
        let tgt = make_typed_id(KindTag::Group, 0x22);
        let et = EdgeType::MemberOf;
        let enc = encode_edge_out_key(&src, et, &tgt);
        assert_eq!(enc.len(), 68);
        let (src2, et2, tgt2) = decode_edge_out_key(&enc).expect("a key this test just encoded");
        assert_eq!(src2.as_bytes(), src.as_bytes());
        assert_eq!(et2, et);
        assert_eq!(tgt2.as_bytes(), tgt.as_bytes());
    }

    #[test]
    fn edge_in_key_length() {
        let src = make_typed_id(KindTag::Device, 0x33);
        let tgt = make_typed_id(KindTag::ArtifactChat, 0x44);
        let enc = encode_edge_in_key(&tgt, EdgeType::HasAttachment, &src);
        assert_eq!(enc.len(), 68);
        // First 33 bytes are the target
        assert_eq!(&enc[..33], tgt.as_bytes());
    }

    #[test]
    fn checkpoint_key_encoding() {
        let grp = make_group(0xCC);
        let seq = 42u64;
        let enc = encode_checkpoint_key(&grp, seq);
        assert_eq!(enc.len(), 40);
        assert_eq!(&enc[..32], grp.as_bytes().as_ref());
        assert_eq!(u64::from_be_bytes(enc[32..].try_into().unwrap()), seq);
    }

    #[test]
    fn edge_type_be_bytes_round_trip() {
        for et in [
            EdgeType::MemberOf,
            EdgeType::HasAttachment,
            EdgeType::References,
            EdgeType::Vouches,
        ] {
            let b = et.to_be_bytes();
            let et2 = EdgeType::from_be_bytes(b).expect("round-trip failed");
            assert_eq!(et2, et);
        }
    }

    // -----------------------------------------------------------------------
    // EdgeMeta serialization round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn edge_meta_round_trip() {
        let em = EdgeMeta {
            version: 1,
            since_lamport: 12345678,
            since_assertion: make_hash(0xDE),
            present: true,
        };
        let bytes = em.to_bytes();
        assert_eq!(bytes.len(), 42);
        let em2 = EdgeMeta::from_bytes(&bytes).expect("deserialization failed");
        assert_eq!(em2.version, em.version);
        assert_eq!(em2.since_lamport, em.since_lamport);
        assert_eq!(
            em2.since_assertion.as_bytes(),
            em.since_assertion.as_bytes()
        );
        assert_eq!(em2.present, em.present);
    }

    #[test]
    fn edge_meta_absent_round_trip() {
        let em = EdgeMeta {
            version: 1,
            since_lamport: 0,
            since_assertion: make_hash(0x00),
            present: false,
        };
        let em2 = EdgeMeta::from_bytes(&em.to_bytes()).unwrap();
        assert!(!em2.present);
    }

    // -----------------------------------------------------------------------
    // NodeCard serialization round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn node_card_round_trip_with_blob() {
        let nc = NodeCard {
            version: 1,
            kind: KindTag::ArtifactNote,
            present: true,
            title: "Hello, world!".to_owned(),
            created_by: make_principal(0x55),
            created_at: 1_700_000_000,
            blob_hash: Some(make_hash(0xBB)),
        };
        let bytes = nc.to_bytes();
        let nc2 = NodeCard::from_bytes(&bytes).expect("deserialization failed");
        assert_eq!(nc2.version, nc.version);
        assert_eq!(nc2.kind, nc.kind);
        assert_eq!(nc2.present, nc.present);
        assert_eq!(nc2.title, nc.title);
        assert_eq!(nc2.created_by.as_bytes(), nc.created_by.as_bytes());
        assert_eq!(nc2.created_at, nc.created_at);
        assert_eq!(
            nc2.blob_hash.unwrap().as_bytes(),
            nc.blob_hash.unwrap().as_bytes()
        );
    }

    #[test]
    fn node_card_round_trip_no_blob() {
        let nc = NodeCard {
            version: 1,
            kind: KindTag::Group,
            present: false,
            title: String::new(),
            created_by: make_principal(0x01),
            created_at: 0,
            blob_hash: None,
        };
        let nc2 = NodeCard::from_bytes(&nc.to_bytes()).unwrap();
        assert!(nc2.blob_hash.is_none());
        assert_eq!(nc2.title, "");
    }

    // -----------------------------------------------------------------------
    // Open Db in a temp directory
    // -----------------------------------------------------------------------

    #[test]
    fn open_db_in_temp_dir() {
        let dir = tempfile::tempdir().expect("tempdir failed");
        let path = dir.path().join("test.redb");
        let db = Db::open(&path).expect("Db::open failed");
        drop(db);
        assert!(path.exists());
    }

    #[test]
    fn create_in_memory_db() {
        let db = Db::create_in_memory().expect("create_in_memory failed");
        // Verify we can write and read.
        let write_txn = db.inner().begin_write().unwrap();
        {
            let mut table = write_txn.open_table(AUTH_GENESIS).unwrap();
            let key: [u8; 32] = [0xAAu8; 32];
            let val: &[u8] = b"\x01hello";
            table.insert(key.as_slice(), val).unwrap();
        }
        write_txn.commit().unwrap();

        let read_txn = db.inner().begin_read().unwrap();
        let table = read_txn.open_table(AUTH_GENESIS).unwrap();
        let v = table.get([0xAAu8; 32].as_slice()).unwrap().unwrap();
        assert_eq!(v.value(), b"\x01hello");
    }

    // -----------------------------------------------------------------------
    // Prefix scan correctness: 2 prefixes × 5 entries each → scan returns 5
    // -----------------------------------------------------------------------

    #[test]
    fn prefix_scan_correctness() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("prefix.redb")).unwrap();

        let dev_a = make_device(0x01);
        let dev_b = make_device(0x02);

        // Insert 5 entries for dev_a and 5 for dev_b.
        let write_txn = db.inner().begin_write().unwrap();
        {
            let mut table = write_txn.open_table(AUTH_ASSERTIONS_BY_DEVICE).unwrap();
            for i in 0u64..5 {
                let key_a = encode_by_device_key(&dev_a, i);
                let key_b = encode_by_device_key(&dev_b, i);
                let hash_a = make_hash((i as u8).wrapping_add(10));
                let hash_b = make_hash((i as u8).wrapping_add(20));
                table
                    .insert(key_a.as_slice(), hash_a.as_bytes().as_slice())
                    .unwrap();
                table
                    .insert(key_b.as_slice(), hash_b.as_bytes().as_slice())
                    .unwrap();
            }
        }
        write_txn.commit().unwrap();

        // Scan prefix for dev_a using a range from dev_a||0 to dev_a||u64::MAX (inclusive).
        let read_txn = db.inner().begin_read().unwrap();
        let table = read_txn.open_table(AUTH_ASSERTIONS_BY_DEVICE).unwrap();

        let start = encode_by_device_key(&dev_a, 0);
        let end = encode_by_device_key(&dev_a, u64::MAX);

        let count = table
            .range(start.as_slice()..=end.as_slice())
            .unwrap()
            .count();
        assert_eq!(
            count, 5,
            "expected exactly 5 entries for dev_a, got {}",
            count
        );
    }

    // -----------------------------------------------------------------------
    // MVCC: read transaction sees old data after a concurrent write
    // -----------------------------------------------------------------------

    #[test]
    fn mvcc_read_sees_old_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("mvcc.redb")).unwrap();

        // Initial write: key = [0xAA; 32], value = b"old"
        let key = [0xAAu8; 32];
        {
            let write_txn = db.inner().begin_write().unwrap();
            {
                let mut table = write_txn.open_table(AUTH_GENESIS).unwrap();
                table.insert(key.as_slice(), b"old".as_slice()).unwrap();
            }
            write_txn.commit().unwrap();
        }

        // Begin a read transaction (snapshot at this point).
        let read_txn = db.inner().begin_read().unwrap();

        // Now write a new value in a separate write transaction.
        {
            let write_txn2 = db.inner().begin_write().unwrap();
            {
                let mut table = write_txn2.open_table(AUTH_GENESIS).unwrap();
                table.insert(key.as_slice(), b"new".as_slice()).unwrap();
            }
            write_txn2.commit().unwrap();
        }

        // The read transaction should still see "old".
        let table = read_txn.open_table(AUTH_GENESIS).unwrap();
        let val = table.get(key.as_slice()).unwrap().unwrap();
        assert_eq!(
            val.value(),
            b"old",
            "MVCC violation: read transaction saw updated value"
        );
    }
}
