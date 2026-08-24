//! The social-tree model: identifiers, the assertion envelope, roles, rules, and
//! the projected group state — every type the fold computes over. Pure data; the
//! wire codecs live beside their types (register: ../WIRE-REGISTER.md).
//!
//! All types in this module are considered stable. New variants/fields may be
//! added in an additive manner; existing discriminants and wire layouts are
//! permanently frozen.

use std::fmt;

// ---------------------------------------------------------------------------
// Primitive ID newtypes (32-byte BLAKE3-256 digests)
// ---------------------------------------------------------------------------

macro_rules! impl_id_newtype {
    ($name:ident) => {
        impl $name {
            /// Construct from a raw 32-byte array.
            #[inline]
            pub fn new(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            /// Borrow the raw bytes.
            #[inline]
            pub fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }

            /// Construct from a raw 32-byte array (alias for `new`).
            #[inline]
            pub fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                for byte in &self.0 {
                    write!(f, "{:02x}", byte)?;
                }
                Ok(())
            }
        }
    };
}

/// A 32-byte content-addressed hash (BLAKE3-256).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash([u8; 32]);

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", self)
    }
}

impl_id_newtype!(Hash);

/// A 32-byte opaque principal (user/service) identifier.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PrincipalId([u8; 32]);

impl fmt::Debug for PrincipalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PrincipalId({})", self)
    }
}

impl_id_newtype!(PrincipalId);

/// A 32-byte opaque device identifier.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId([u8; 32]);

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceId({})", self)
    }
}

impl_id_newtype!(DeviceId);

/// A 32-byte group identifier.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GroupId([u8; 32]);

impl fmt::Debug for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GroupId({})", self)
    }
}

impl_id_newtype!(GroupId);

// ---------------------------------------------------------------------------
// Kind tags — frozen, additive
// ---------------------------------------------------------------------------

/// One-byte discriminant that classifies a typed entity.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KindTag {
    Group = 0x01,
    Principal = 0x02,
    Device = 0x03,
    ArtifactChat = 0x04,
    ArtifactNote = 0x05,
    ArtifactLink = 0x06,
    ArtifactGame = 0x07,
}

impl KindTag {
    /// Decode from a raw byte; returns `None` for unknown discriminants.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(KindTag::Group),
            0x02 => Some(KindTag::Principal),
            0x03 => Some(KindTag::Device),
            0x04 => Some(KindTag::ArtifactChat),
            0x05 => Some(KindTag::ArtifactNote),
            0x06 => Some(KindTag::ArtifactLink),
            0x07 => Some(KindTag::ArtifactGame),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// TypedId — KindTag (1 byte) || Hash (32 bytes) = 33 bytes
// ---------------------------------------------------------------------------

/// A typed content-addressed identifier: one `KindTag` byte followed by 32
/// hash bytes, stored as a fixed 33-byte array.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypedId([u8; 33]);

impl TypedId {
    /// Construct from a `KindTag` and a `Hash`.
    pub fn new(kind: KindTag, hash: Hash) -> Self {
        let mut buf = [0u8; 33];
        buf[0] = kind as u8;
        buf[1..].copy_from_slice(hash.as_bytes());
        Self(buf)
    }

    /// Recover the `KindTag`.
    ///
    /// # Panics
    /// Panics if the stored byte is not a valid `KindTag` discriminant.  This
    /// cannot happen for values produced via `TypedId::new`, but may panic for
    /// values deserialized from untrusted bytes.
    pub fn kind(&self) -> KindTag {
        KindTag::from_u8(self.0[0]).expect("TypedId: invalid KindTag byte")
    }

    /// Recover the `Hash`.
    pub fn hash(&self) -> Hash {
        let mut h = [0u8; 32];
        h.copy_from_slice(&self.0[1..]);
        Hash(h)
    }

    /// Borrow the raw 33-byte representation.
    pub fn as_bytes(&self) -> &[u8; 33] {
        &self.0
    }
}

impl fmt::Debug for TypedId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypedId({:?}, {})", self.kind(), self.hash())
    }
}

impl fmt::Display for TypedId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Assertion type enum
// ---------------------------------------------------------------------------

/// Two-byte discriminant that classifies an assertion on the wire.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssertionType {
    GroupGenesis = 0x0001,
    MembershipAdd = 0x0002,
    MembershipRemove = 0x0003,
    RoleGrant = 0x0004,
    RoleRevoke = 0x0005,
    RuleChange = 0x0006,
    AttachmentAdd = 0x0007,
    ArtifactRef = 0x0008,
    Message = 0x0009,
    Vouch = 0x000A,
    /// An approval of a governance act, for k-of-n threshold enforcement (V5′).
    /// Payload: approved act_type (2 bytes BE) ‖ subject principal (32 bytes) — it
    /// approves "an act of this type on this subject". A threshold-k act references k
    /// such approvals as antecedents; the act is authorized only when its distinct
    /// approver personae (by lineage) plus its author meet the threshold.
    Approval = 0x000B,
    /// Closes one open contradiction pair (§7.3.2: the hard-stop replay is not
    /// resolution — resolution is its own fact type). Payload: the pair's two content
    /// addresses, lexicographically ordered (32 ‖ 32). A governed act: charter-quorum
    /// gated via `GroupRules::resolution_threshold` (owner decision 2026-08-21 — the
    /// same k-of-n Approval machinery as every other governance threshold; product
    /// default 2; never silently single-author). Approvals name
    /// `(Resolution, H(payload))`, the RuleChange-style content-hash subject.
    Resolution = 0x000C,
}

impl AssertionType {
    /// Encode as big-endian u16.
    pub fn to_u16(self) -> u16 {
        self as u16
    }

    /// Decode from big-endian u16; returns `None` for unknown values.
    pub fn from_u16(v: u16) -> Option<Self> {
        match v {
            0x0001 => Some(AssertionType::GroupGenesis),
            0x0002 => Some(AssertionType::MembershipAdd),
            0x0003 => Some(AssertionType::MembershipRemove),
            0x0004 => Some(AssertionType::RoleGrant),
            0x0005 => Some(AssertionType::RoleRevoke),
            0x0006 => Some(AssertionType::RuleChange),
            0x0007 => Some(AssertionType::AttachmentAdd),
            0x0008 => Some(AssertionType::ArtifactRef),
            0x0009 => Some(AssertionType::Message),
            0x000A => Some(AssertionType::Vouch),
            0x000B => Some(AssertionType::Approval),
            0x000C => Some(AssertionType::Resolution),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Assertion envelope
// ---------------------------------------------------------------------------

/// The envelope wire schema version this build reads and writes. v2 (O9, E117 P1)
/// removed the signed wall-clock `timestamp` field: Part 1 §2.0.1 makes a wall-clock
/// value an uncorroborable assertion, and signing one into every governance fact's
/// canonical bytes elevated it into what a later reader would mistake for provenance.
/// No comparator or fold input ever consulted it (merge_cmp is lamport → hash); an
/// application wanting a claimed time records it as ordinary payload content.
pub const ENVELOPE_WIRE_VERSION: u8 = 0x02;

/// The canonical container for every mutation in the system.
#[derive(Debug, Clone, PartialEq)]
pub struct AssertionEnvelope {
    /// Wire version; [`ENVELOPE_WIRE_VERSION`] for this generation.
    pub version: u8,
    pub assertion_type: AssertionType,
    pub author_device: DeviceId,
    pub author_principal: PrincipalId,
    pub group: GroupId,
    /// Content hashes of assertions this one causally follows.
    pub antecedents: Vec<Hash>,
    /// Lamport clock value at the time of creation.
    pub lamport: u64,
    /// Type-specific payload bytes.
    pub payload: Vec<u8>,
    /// Detached signature over `canonical_bytes()`.
    pub signature: Vec<u8>,
}

impl AssertionEnvelope {
    /// Deterministic serialization of all fields **except** `signature`.
    ///
    /// Layout (all multi-byte integers are big-endian):
    /// - version          : 1 byte (0x02; v1 carried a signed wall-clock field here —
    ///   dropped, see [`ENVELOPE_WIRE_VERSION`])
    /// - assertion_type   : 2 bytes
    /// - author_device    : 32 bytes
    /// - author_principal : 32 bytes
    /// - group            : 32 bytes
    /// - antecedents      : 4-byte count + count × 32 bytes
    /// - lamport          : 8 bytes
    /// - payload          : 4-byte length + bytes
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let antecedent_count = self.antecedents.len() as u32;
        let payload_len = self.payload.len() as u32;

        let capacity =
            1 + 2 + 32 + 32 + 32 + 4 + (self.antecedents.len() * 32) + 8 + 4 + self.payload.len();

        let mut buf = Vec::with_capacity(capacity);

        buf.push(self.version);
        buf.extend_from_slice(&self.assertion_type.to_u16().to_be_bytes());
        buf.extend_from_slice(self.author_device.as_bytes());
        buf.extend_from_slice(self.author_principal.as_bytes());
        buf.extend_from_slice(self.group.as_bytes());
        buf.extend_from_slice(&antecedent_count.to_be_bytes());
        for h in &self.antecedents {
            buf.extend_from_slice(h.as_bytes());
        }
        buf.extend_from_slice(&self.lamport.to_be_bytes());
        buf.extend_from_slice(&payload_len.to_be_bytes());
        buf.extend_from_slice(&self.payload);

        buf
    }

    /// Same as `canonical_bytes` but with a 4-byte length-prefixed signature
    /// appended at the end.
    pub fn canonical_bytes_with_sig(&self) -> Vec<u8> {
        let mut buf = self.canonical_bytes();
        let sig_len = self.signature.len() as u32;
        buf.extend_from_slice(&sig_len.to_be_bytes());
        buf.extend_from_slice(&self.signature);
        buf
    }
}

// ---------------------------------------------------------------------------
// Hashing helpers
// ---------------------------------------------------------------------------

/// BLAKE3 hash of an arbitrary byte slice.
pub fn compute_hash(bytes: &[u8]) -> Hash {
    let digest = blake3::hash(bytes);
    Hash(*digest.as_bytes())
}

/// BLAKE3 hash of `canonical_bytes_with_sig(env)`.
pub fn envelope_hash(env: &AssertionEnvelope) -> Hash {
    compute_hash(&env.canonical_bytes_with_sig())
}

// ---------------------------------------------------------------------------
// Merge order for per-principal streams
// ---------------------------------------------------------------------------

/// Total order over assertion envelopes for deterministic stream merging.
///
/// Precedence: `lamport` ASC → `author_device` bytes ASC → envelope hash bytes ASC.
/// The canonical replay comparator's version. Stamped into a store's meta table by
/// `fold_derived::rebuild`, and checked by `fold_derived::needs_rebuild`, so a store folded
/// under an older comparator is detected rather than silently re-folded differently.
///
/// - **v1** (unstamped stores): `lamport → author_device → envelope_hash`.
/// - **v2**: `lamport → envelope_hash`. The `author_device` key was removed after the G1
///   experiment (`croft-chat/tests/fold_ordering_keys.rs`, 2026-08-16) measured that it was
///   doing exactly one job — deciding the winner among genuine concurrents before the content
///   address ever ran — which is the party-privileging silent default Part 2 §7.3.1 key 3
///   forbids. Totality and per-device order both survive without it: the envelope hash covers
///   the device, and ingest enforces strictly increasing lamports per device.
pub const MERGE_CMP_VERSION: u8 = 2;

/// Canonical replay order (v2): `lamport`, then the **content address**.
///
/// §7.3.1 key 3: among genuine concurrents the tiebreak is the content address — party-neutral,
/// ungameable, identical everywhere. Any party-privileging key must be opt-in and governed,
/// never a silent default; `author_device` was exactly such a key and was removed in v2
/// (see [`MERGE_CMP_VERSION`]). The hash covers every envelope field including the device, so
/// distinct envelopes never compare `Equal`.
pub fn merge_cmp(a: &AssertionEnvelope, b: &AssertionEnvelope) -> std::cmp::Ordering {
    a.lamport
        .cmp(&b.lamport)
        .then_with(|| envelope_hash(a).as_bytes().cmp(envelope_hash(b).as_bytes()))
}

// ---------------------------------------------------------------------------
// Value version tagging
// ---------------------------------------------------------------------------

/// Wraps any value with an explicit version byte.
#[derive(Debug, Clone, PartialEq)]
pub struct Versioned<T> {
    pub version: u8,
    pub value: T,
}

/// Wrap `value` in a `Versioned` with `version = 1`.
pub fn wrap_v1<T>(value: T) -> Versioned<T> {
    Versioned { version: 1, value }
}

// ---------------------------------------------------------------------------
// Supporting types for payloads
// ---------------------------------------------------------------------------

/// Membership/administrative role within a group.
#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    Owner,
    Admin,
    Member,
    Observer,
}

/// Governance thresholds controlling quorum requirements for group mutations.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupRules {
    pub add_member_threshold: u32,
    pub remove_member_threshold: u32,
    pub role_change_threshold: u32,
    pub rule_change_threshold: u32,
    /// Quorum for closing an open contradiction pair (§7.3.2 resolution fact).
    /// Not carried in the genesis payload: minted at the product default (**2**) and
    /// dialed thereafter by governed `RuleChange` like every other threshold — the
    /// charter-dial species (E111/E121). Whether the spec hard-floors it at 2 rides
    /// the §7.3.2 filing; the fold treats it as charter data.
    pub resolution_threshold: u32,
}

/// Key discriminant for a mutable group rule.
#[derive(Clone, Debug, PartialEq)]
pub enum RuleKey {
    AddMember,
    RemoveMember,
    RoleChange,
    RuleChange,
    Resolution,
}

/// The numeric value stored for a mutable group rule.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleValue(pub u32);

/// Opaque string tag providing context for a vouch assertion.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextTag(pub String);

/// Subjective confidence level carried by a vouch assertion.
#[derive(Clone, Debug, PartialEq)]
pub enum VouchStrength {
    Weak,
    Moderate,
    Strong,
}

// ---------------------------------------------------------------------------
// Concrete payload structs
// ---------------------------------------------------------------------------

/// Establishes a new group and its initial governance rules.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupGenesisPayload {
    pub policy_version: u16,
    pub initial_rules: GroupRules,
    pub founding_device: DeviceId,
}

/// Adds a principal to a group with an initial role.
#[derive(Clone, Debug, PartialEq)]
pub struct MembershipAddPayload {
    pub invitee: PrincipalId,
    pub role: Role,
}

/// Removes a principal from a group.
#[derive(Clone, Debug, PartialEq)]
pub struct MembershipRemovePayload {
    pub subject: PrincipalId,
}

/// Grants a principal a new role within a group.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleGrantPayload {
    pub subject: PrincipalId,
    pub new_role: Role,
}

/// Revokes any elevated role from a principal.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleRevokePayload {
    pub subject: PrincipalId,
}

/// Changes the numeric value of a mutable group rule.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleChangePayload {
    pub rule_key: RuleKey,
    pub new_value: RuleValue,
}

/// Attaches a typed artifact to a group context.
#[derive(Clone, Debug, PartialEq)]
pub struct AttachmentAddPayload {
    pub kind: KindTag,
    pub title: String,
    pub blob_hash: Option<Hash>,
}

/// A human-readable message, optionally replying to a prior message.
#[derive(Clone, Debug, PartialEq)]
pub struct MessagePayload {
    pub body: String,
    pub reply_to: Option<Hash>,
}

/// Encode a Message payload to wire bytes.
///
/// Layout: `body_len(4) || body || reply_marker(4)[+hash(32)] ||
/// channel_marker(1)[+typed(33)]`. `reply_marker` is `0` (none) or `32` (a hash
/// follows). `channel_marker` is `0` (route to the group) or `1` (an
/// `ArtifactChat` `TypedId` follows — route there). Single source of truth for
/// the wire format, shared by the surface writer and the fold reader.
#[must_use]
pub fn encode_message_payload(
    body: &str,
    reply_to: Option<Hash>,
    channel: Option<TypedId>,
) -> Vec<u8> {
    let mut payload = Vec::new();
    let body_bytes = body.as_bytes();
    payload.extend_from_slice(&(body_bytes.len() as u32).to_be_bytes());
    payload.extend_from_slice(body_bytes);
    match reply_to {
        None => payload.extend_from_slice(&[0u8; 4]),
        Some(h) => {
            payload.extend_from_slice(&32u32.to_be_bytes());
            payload.extend_from_slice(h.as_bytes());
        }
    }
    match channel {
        None => payload.push(0x00),
        Some(t) => {
            payload.push(0x01);
            payload.extend_from_slice(t.as_bytes());
        }
    }
    payload
}

/// Decode a Message payload. `None` on malformation.
///
/// Tolerant of legacy payloads that end after the reply marker (no channel
/// byte) — those decode with `channel = None`.
#[must_use]
pub fn decode_message_payload(payload: &[u8]) -> Option<(String, Option<Hash>, Option<TypedId>)> {
    if payload.len() < 4 {
        return None;
    }
    let body_len = u32::from_be_bytes(payload[0..4].try_into().ok()?) as usize;
    let body_start = 4usize;
    let body_end = body_start.checked_add(body_len)?;
    if payload.len() < body_end.checked_add(4)? {
        return None;
    }
    let body = String::from_utf8(payload[body_start..body_end].to_vec()).ok()?;

    let reply_len = u32::from_be_bytes(payload[body_end..body_end + 4].try_into().ok()?) as usize;
    let mut off = body_end + 4;
    let reply_to = match reply_len {
        0 => None,
        32 => {
            let h = read_array32(payload, off)?;
            off += 32;
            Some(Hash::new(h))
        }
        _ => return None,
    };

    let channel = match payload.get(off) {
        None => None, // legacy payload: no channel marker
        Some(0x00) => None,
        Some(0x01) => {
            let raw = payload.get(off + 1..off + 1 + 33)?;
            decode_typed_id(raw)
        }
        Some(_) => return None,
    };

    Some((body, reply_to, channel))
}

fn read_array32(payload: &[u8], off: usize) -> Option<[u8; 32]> {
    let slice = payload.get(off..off + 32)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(slice);
    Some(out)
}

fn decode_typed_id(raw: &[u8]) -> Option<TypedId> {
    if raw.len() != 33 {
        return None;
    }
    let kind = KindTag::from_u8(raw[0])?;
    let mut h = [0u8; 32];
    h.copy_from_slice(&raw[1..33]);
    Some(TypedId::new(kind, Hash::new(h)))
}

/// A reference to an existing typed artifact.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactRefPayload {
    pub artifact: TypedId,
}

/// A vouch by the author attesting to a subject's trustworthiness.
#[derive(Clone, Debug, PartialEq)]
pub struct VouchPayload {
    pub subject: PrincipalId,
    pub context: ContextTag,
    pub strength: VouchStrength,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_device(seed: u8) -> DeviceId {
        DeviceId([seed; 32])
    }

    fn make_principal(seed: u8) -> PrincipalId {
        PrincipalId([seed; 32])
    }

    fn make_group(seed: u8) -> GroupId {
        GroupId([seed; 32])
    }

    fn make_hash(seed: u8) -> Hash {
        Hash([seed; 32])
    }

    fn sample_envelope(lamport: u64, device_seed: u8) -> AssertionEnvelope {
        AssertionEnvelope {
            version: crate::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::Message,
            author_device: make_device(device_seed),
            author_principal: make_principal(0xAA),
            group: make_group(0xBB),
            antecedents: vec![make_hash(0x01), make_hash(0x02)],
            lamport,
            payload: b"hello world".to_vec(),
            signature: b"fakesig".to_vec(),
        }
    }

    // -----------------------------------------------------------------------
    // Round-trip canonical_bytes
    // -----------------------------------------------------------------------

    #[test]
    fn canonical_bytes_round_trip() {
        let env = sample_envelope(42, 0x11);
        let raw = env.canonical_bytes();

        // version — envelope wire v2 (no wall-clock slot)
        assert_eq!(raw[0], ENVELOPE_WIRE_VERSION);

        // assertion_type = Message = 0x0009
        assert_eq!(raw[1], 0x00);
        assert_eq!(raw[2], 0x09);

        // author_device (bytes 3..35)
        assert_eq!(&raw[3..35], &[0x11u8; 32]);

        // author_principal (bytes 35..67)
        assert_eq!(&raw[35..67], &[0xAAu8; 32]);

        // group (bytes 67..99)
        assert_eq!(&raw[67..99], &[0xBBu8; 32]);

        // antecedents: count = 2 (bytes 99..103)
        assert_eq!(u32::from_be_bytes(raw[99..103].try_into().unwrap()), 2);

        // first antecedent (bytes 103..135)
        assert_eq!(&raw[103..135], &[0x01u8; 32]);

        // second antecedent (bytes 135..167)
        assert_eq!(&raw[135..167], &[0x02u8; 32]);

        // lamport = 42 (bytes 167..175)
        assert_eq!(u64::from_be_bytes(raw[167..175].try_into().unwrap()), 42);

        // payload length = 11 (bytes 175..179) — immediately after lamport; the
        // v1 timestamp slot is gone (O9)
        assert_eq!(u32::from_be_bytes(raw[175..179].try_into().unwrap()), 11);

        // payload bytes (bytes 179..190)
        assert_eq!(&raw[179..190], b"hello world");

        // with_sig appends 4-byte length + "fakesig"
        let raw_sig = env.canonical_bytes_with_sig();
        let sig_start = raw.len();
        assert_eq!(
            u32::from_be_bytes(raw_sig[sig_start..sig_start + 4].try_into().unwrap()),
            7
        );
        assert_eq!(&raw_sig[sig_start + 4..], b"fakesig");
    }

    // -----------------------------------------------------------------------
    // Hash stability
    // -----------------------------------------------------------------------

    #[test]
    fn hash_stability_same_bytes() {
        let bytes = b"deterministic input";
        let h1 = compute_hash(bytes);
        let h2 = compute_hash(bytes);
        assert_eq!(h1, h2, "identical inputs must produce identical hashes");

        let h3 = compute_hash(b"different input");
        assert_ne!(h1, h3, "distinct inputs must produce distinct hashes");
    }

    #[test]
    fn envelope_hash_deterministic() {
        let env = sample_envelope(1, 0x55);
        let h1 = envelope_hash(&env);
        let h2 = envelope_hash(&env);
        assert_eq!(h1, h2);

        // A clone with a different lamport must yield a different hash.
        let mut env2 = env.clone();
        env2.lamport = 999;
        assert_ne!(envelope_hash(&env), envelope_hash(&env2));
    }

    // -----------------------------------------------------------------------
    // TypedId round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn typed_id_round_trip() {
        for &kind in &[
            KindTag::Group,
            KindTag::Principal,
            KindTag::Device,
            KindTag::ArtifactChat,
            KindTag::ArtifactNote,
            KindTag::ArtifactLink,
            KindTag::ArtifactGame,
        ] {
            let hash = make_hash(kind as u8);
            let tid = TypedId::new(kind, hash);
            assert_eq!(tid.kind(), kind, "KindTag round-trip failed for {:?}", kind);
            assert_eq!(tid.hash(), hash, "Hash round-trip failed for {:?}", kind);
            assert_eq!(tid.as_bytes()[0], kind as u8);
        }
    }

    // -----------------------------------------------------------------------
    // merge_cmp totality: 100 random triples verify total order
    // -----------------------------------------------------------------------

    /// Deterministic pseudo-random u8 sequence seeded by index.
    fn pseudo_rand_byte(i: usize, j: usize) -> u8 {
        ((i.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
            ^ j.wrapping_mul(2891336453))
            & 0xFF) as u8
    }

    fn pseudo_rand_u64(i: usize, j: usize) -> u64 {
        let lo = pseudo_rand_byte(i, j) as u64;
        let hi = pseudo_rand_byte(i, j + 1) as u64;
        // Keep lamport small so collisions (needed for tie-breaking tests) occur.
        (hi << 8 | lo) % 4
    }

    fn rand_envelope(i: usize) -> AssertionEnvelope {
        let mut device_bytes = [0u8; 32];
        let mut principal_bytes = [0u8; 32];
        let mut group_bytes = [0u8; 32];
        for k in 0..32 {
            device_bytes[k] = pseudo_rand_byte(i, k + 100);
            principal_bytes[k] = pseudo_rand_byte(i, k + 200);
            group_bytes[k] = pseudo_rand_byte(i, k + 300);
        }
        AssertionEnvelope {
            version: crate::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::Message,
            author_device: DeviceId(device_bytes),
            author_principal: PrincipalId(principal_bytes),
            group: GroupId(group_bytes),
            antecedents: vec![],
            lamport: pseudo_rand_u64(i, 0),
            payload: vec![i as u8],
            signature: vec![pseudo_rand_byte(i, 400), pseudo_rand_byte(i, 401)],
        }
    }

    #[test]
    fn merge_cmp_total_order() {
        let envelopes: Vec<AssertionEnvelope> = (0..100).map(rand_envelope).collect();

        // Irreflexivity: a.cmp(a) == Equal for all a.
        for a in &envelopes {
            assert_eq!(
                merge_cmp(a, a),
                std::cmp::Ordering::Equal,
                "merge_cmp must be reflexive (Equal with itself)"
            );
        }

        // Antisymmetry: if a < b then b > a.
        for i in 0..envelopes.len() {
            for j in (i + 1)..envelopes.len() {
                let ab = merge_cmp(&envelopes[i], &envelopes[j]);
                let ba = merge_cmp(&envelopes[j], &envelopes[i]);
                assert_eq!(
                    ab,
                    ba.reverse(),
                    "antisymmetry violated for envelopes {} and {}",
                    i,
                    j
                );
            }
        }

        // Transitivity: if a <= b and b <= c then a <= c.
        for i in 0..envelopes.len() {
            for j in 0..envelopes.len() {
                for k in 0..envelopes.len() {
                    let ab = merge_cmp(&envelopes[i], &envelopes[j]);
                    let bc = merge_cmp(&envelopes[j], &envelopes[k]);
                    let ac = merge_cmp(&envelopes[i], &envelopes[k]);
                    if ab != std::cmp::Ordering::Greater && bc != std::cmp::Ordering::Greater {
                        assert_ne!(
                            ac,
                            std::cmp::Ordering::Greater,
                            "transitivity violated for ({}, {}, {})",
                            i,
                            j,
                            k
                        );
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Enum discriminant values match spec
    // -----------------------------------------------------------------------

    /// v2 regression guard: the comparator must never consult `author_device`. Two envelopes
    /// identical except for the authoring device must order by hash — the party-neutral key —
    /// and never `Equal`. (The v1 comparator ordered these by device; see MERGE_CMP_VERSION.)
    #[test]
    fn merge_cmp_is_blind_to_author_device() {
        let mut a = rand_envelope(1);
        let mut b = a.clone();
        a.author_device = DeviceId::new([0x01; 32]);
        b.author_device = DeviceId::new([0xFE; 32]);
        // Same lamport by construction (b is a clone), devices maximally apart.
        let ha = envelope_hash(&a);
        let hb = envelope_hash(&b);
        let by_hash = ha.as_bytes().cmp(hb.as_bytes());
        assert_ne!(
            by_hash,
            std::cmp::Ordering::Equal,
            "distinct devices must yield distinct hashes"
        );
        assert_eq!(
            merge_cmp(&a, &b),
            by_hash,
            "at equal lamport the CONTENT ADDRESS must decide, not the device"
        );
        // And flipping which device is lex-smaller must not change the outcome.
        std::mem::swap(&mut a.author_device, &mut b.author_device);
        let ha2 = envelope_hash(&a);
        let hb2 = envelope_hash(&b);
        assert_eq!(
            merge_cmp(&a, &b),
            ha2.as_bytes().cmp(hb2.as_bytes()),
            "still the hash after the swap"
        );
    }

    #[test]
    fn kind_tag_discriminants() {
        assert_eq!(KindTag::Group as u8, 0x01);
        assert_eq!(KindTag::Principal as u8, 0x02);
        assert_eq!(KindTag::Device as u8, 0x03);
        assert_eq!(KindTag::ArtifactChat as u8, 0x04);
        assert_eq!(KindTag::ArtifactNote as u8, 0x05);
        assert_eq!(KindTag::ArtifactLink as u8, 0x06);
        assert_eq!(KindTag::ArtifactGame as u8, 0x07);
    }

    #[test]
    fn assertion_type_discriminants() {
        assert_eq!(AssertionType::GroupGenesis as u16, 0x0001);
        assert_eq!(AssertionType::MembershipAdd as u16, 0x0002);
        assert_eq!(AssertionType::MembershipRemove as u16, 0x0003);
        assert_eq!(AssertionType::RoleGrant as u16, 0x0004);
        assert_eq!(AssertionType::RoleRevoke as u16, 0x0005);
        assert_eq!(AssertionType::RuleChange as u16, 0x0006);
        assert_eq!(AssertionType::AttachmentAdd as u16, 0x0007);
        assert_eq!(AssertionType::ArtifactRef as u16, 0x0008);
        assert_eq!(AssertionType::Message as u16, 0x0009);
        assert_eq!(AssertionType::Vouch as u16, 0x000A);
        assert_eq!(AssertionType::Approval as u16, 0x000B);
    }

    #[test]
    fn assertion_type_from_u16_round_trip() {
        for disc in 1u16..=12u16 {
            let at = AssertionType::from_u16(disc).expect("should decode 0x0001..=0x000C");
            assert_eq!(at as u16, disc);
        }
        assert!(AssertionType::from_u16(0x0000).is_none());
        assert!(AssertionType::from_u16(0x000D).is_none());
    }

    // -----------------------------------------------------------------------
    // Versioned / wrap_v1
    // -----------------------------------------------------------------------

    #[test]
    fn versioned_wrap_v1() {
        let v = wrap_v1(42u32);
        assert_eq!(v.version, 1);
        assert_eq!(v.value, 42u32);
    }

    /// **O9 standing pin (E117 P1):** envelope wire v2 carries NO wall-clock slot.
    /// The canonical pre-image is exactly version(1) + type(2) + device(32) +
    /// principal(32) + group(32) + antecedent count(4) + antecedents(n×32) +
    /// lamport(8) + payload_len(4) + payload — byte-for-byte, nothing else signed.
    /// A wall-clock value is an uncorroborable assertion (Part 1 §2.0.1); a claimed
    /// time belongs in payload content, never in the signed envelope. If this pin
    /// breaks because a time field returned, that is the laundering coming back.
    #[test]
    fn canonical_bytes_v2_has_no_wallclock_slot() {
        let env = AssertionEnvelope {
            version: ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::Message,
            author_device: DeviceId::new([1u8; 32]),
            author_principal: PrincipalId::new([2u8; 32]),
            group: GroupId::new([3u8; 32]),
            antecedents: vec![Hash::new([4u8; 32]), Hash::new([5u8; 32])],
            lamport: 7,
            payload: vec![0xAA; 10],
            signature: vec![],
        };
        let bytes = env.canonical_bytes();
        let expected_len = 1 + 2 + 32 + 32 + 32 + 4 + 2 * 32 + 8 + 4 + 10;
        assert_eq!(
            bytes.len(),
            expected_len,
            "canonical pre-image must be exactly the documented v2 fields — no extra slot"
        );
        assert_eq!(bytes[0], ENVELOPE_WIRE_VERSION, "version byte is 0x02");
        // The 8 bytes after the antecedents are the lamport — and the 4 after that
        // are the payload length, i.e. no second u64 (the retired timestamp) between.
        let lamport_off = 1 + 2 + 32 + 32 + 32 + 4 + 2 * 32;
        assert_eq!(
            u64::from_be_bytes(bytes[lamport_off..lamport_off + 8].try_into().unwrap()),
            7
        );
        assert_eq!(
            u32::from_be_bytes(bytes[lamport_off + 8..lamport_off + 12].try_into().unwrap()),
            10,
            "payload length follows lamport immediately"
        );
    }
}

// ---------------------------------------------------------------------------
// Projected group state (moved from the experiment fold at E117 P2)
// ---------------------------------------------------------------------------

use crate::update::FoldError;

/// One open concurrent-governance contradiction (§7.3.2, E108): the conflicting pair
/// carried **as data**, plus what the deterministic replay needs to stay order-independent
/// while the pair is open. Entries are held in a set (`ForkStatus::Contested`) — two
/// simultaneously open contradictions are representable, which the retired single
/// min-hash slot structurally was not.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContestedEntry {
    /// The two conflicting facts' content addresses, lexicographically ordered.
    pub pair: (Hash, Hash),
    /// The membership-contested subjects, sorted: a mutual expulsion contests both
    /// parties; an add/remove race contests the raced subject; role and rule
    /// contradictions contest no membership (empty).
    pub subjects: Vec<PrincipalId>,
    /// The facts the deterministic replay withholds while the pair is open. Withholding
    /// is not a verdict: the projection reports the subjects `CONTESTED`, and closing
    /// the pair is a governed `Resolution` fact, never the replay itself.
    pub excluded: Vec<Hash>,
}

impl ContestedEntry {
    /// Order a raw hash pair lexicographically — the canonical pair form.
    pub fn order_pair(a: Hash, b: Hash) -> (Hash, Hash) {
        if a.as_bytes() <= b.as_bytes() {
            (a, b)
        } else {
            (b, a)
        }
    }
}

/// The total membership view (§7.3.2 rule (c), owner-ratified 2026-08-17): the subject
/// of an open contradiction is **neither** member nor not-member. Deliberately no
/// boolean convenience accessor exists — an untyped mid-resolution state is how a UI
/// starts lying by omission (the Matrix lesson, E117 P1).
#[derive(Debug, Clone, PartialEq)]
pub enum MembershipView {
    Member(Role),
    NotMember,
    /// Membership pending resolution; carries the conflicting pair(s) naming this
    /// subject, as data, so a renderer can present the contradiction factually.
    Contested(Vec<(Hash, Hash)>),
}

/// Whether the group's governance log is clean or has a detected fork.
#[derive(Debug, Clone, PartialEq)]
pub enum ForkStatus {
    Clean,
    /// The group's governance state forked; this is the hash of the assertion
    /// that was displaced (lexicographically larger hash at the same gov_seq).
    /// The "too many valid claims" member of the §7.6.1 escalation set.
    ForkedFrom(Hash),
    /// The group is under-determined: a required role (Owner) is vacant with no
    /// admissible successor. The "too few" member of the §7.6.1 escalation set —
    /// a distinct hard-stop a contradiction-only watcher would miss. See
    /// `governance::is_under_determined`.
    UnderDetermined,
    /// Open concurrent-governance contradictions — the "too many valid claims" shape
    /// for concurrent governance, which slot-collision detection alone misses. Each
    /// entry carries its conflicting pair as data; the set is sorted by pair. The
    /// hard-stop persists per entry until a governed `Resolution` fact closes it
    /// (§7.3.2: the replay is not resolution). See `governance::are_concurrent`.
    Contested(Vec<ContestedEntry>),
}

/// The GroupState wire schema version this build reads and writes.
pub const GROUP_STATE_WIRE_VERSION: u8 = 2;

/// Projected governance state for a group.
///
/// Wire layout (version = 2; v1 is refused with a rebuild demand — pre-1.0, no shim):
///   version            : 1 byte (0x02)
///   computed_at_gov_head: 32 bytes (Hash raw)
///   computed_at_gov_seq : 8 bytes big-endian u64
///   rules              : 5 × 4 bytes big-endian u32
///                        [add_member_threshold, remove_member_threshold,
///                         role_change_threshold, rule_change_threshold,
///                         resolution_threshold]
///   member_count       : 4 bytes big-endian u32
///   members            : member_count × (32 + 1 + 8) bytes
///                        PrincipalId(32) || Role(1) || since_lamport(8)
///   fork_status        : 1 byte (0x00=Clean, 0x01=ForkedFrom, 0x02=UnderDetermined,
///                        0x03=Contested)
///   [fork_hash]        : 32 bytes (only if fork_status == 0x01)
///   [contested]        : only if fork_status == 0x03 —
///                        entry_count u32, then per entry:
///                        pair.0(32) ‖ pair.1(32) ‖ subj_n(u8) ‖ subjects × 32 ‖
///                        excl_n(u8) ‖ excluded × 32
///
/// The `members` list is the deterministic replay substrate; **the read path for
/// membership questions is [`GroupState::membership`]**, which is total over
/// Member / NotMember / Contested and never collapses the third state to a boolean.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupState {
    pub version: u8,
    pub computed_at_gov_head: Hash,
    pub computed_at_gov_seq: u64,
    pub members: Vec<(PrincipalId, Role, u64)>, // (principal, role, since_lamport)
    pub rules: GroupRules,
    pub fork_status: ForkStatus,
}

impl GroupState {
    /// The total membership view for `p` (§7.3.2 rule (c)): the subject of an open
    /// contradiction is `Contested` — neither member nor not-member — carrying the
    /// pair(s) that name it. Everyone else resolves against the replay substrate.
    pub fn membership(&self, p: &PrincipalId) -> MembershipView {
        if let ForkStatus::Contested(entries) = &self.fork_status {
            let pairs: Vec<(Hash, Hash)> = entries
                .iter()
                .filter(|e| e.subjects.contains(p))
                .map(|e| e.pair)
                .collect();
            if !pairs.is_empty() {
                return MembershipView::Contested(pairs);
            }
        }
        match self.members.iter().find(|(m, _, _)| m == p) {
            Some((_, role, _)) => MembershipView::Member(role.clone()),
            None => MembershipView::NotMember,
        }
    }

    /// The open contested entries, if any (empty slice otherwise).
    pub fn contested_entries(&self) -> &[ContestedEntry] {
        match &self.fork_status {
            ForkStatus::Contested(entries) => entries,
            _ => &[],
        }
    }

    /// Serialize to bytes (wire v2).
    pub fn to_bytes(&self) -> Vec<u8> {
        let member_count = self.members.len() as u32;
        let mut buf = Vec::with_capacity(1 + 32 + 8 + 20 + 4 + self.members.len() * 41 + 64);

        buf.push(GROUP_STATE_WIRE_VERSION);
        buf.extend_from_slice(self.computed_at_gov_head.as_bytes());
        buf.extend_from_slice(&self.computed_at_gov_seq.to_be_bytes());
        buf.extend_from_slice(&self.rules.add_member_threshold.to_be_bytes());
        buf.extend_from_slice(&self.rules.remove_member_threshold.to_be_bytes());
        buf.extend_from_slice(&self.rules.role_change_threshold.to_be_bytes());
        buf.extend_from_slice(&self.rules.rule_change_threshold.to_be_bytes());
        buf.extend_from_slice(&self.rules.resolution_threshold.to_be_bytes());
        buf.extend_from_slice(&member_count.to_be_bytes());
        for (pid, role, since) in &self.members {
            buf.extend_from_slice(pid.as_bytes());
            buf.push(role_to_u8(role));
            buf.extend_from_slice(&since.to_be_bytes());
        }
        match &self.fork_status {
            ForkStatus::Clean => buf.push(0x00),
            ForkStatus::ForkedFrom(h) => {
                buf.push(0x01);
                buf.extend_from_slice(h.as_bytes());
            }
            ForkStatus::UnderDetermined => buf.push(0x02),
            ForkStatus::Contested(entries) => {
                buf.push(0x03);
                buf.extend_from_slice(&(entries.len() as u32).to_be_bytes());
                for e in entries {
                    buf.extend_from_slice(e.pair.0.as_bytes());
                    buf.extend_from_slice(e.pair.1.as_bytes());
                    buf.push(e.subjects.len() as u8);
                    for s in &e.subjects {
                        buf.extend_from_slice(s.as_bytes());
                    }
                    buf.push(e.excluded.len() as u8);
                    for x in &e.excluded {
                        buf.extend_from_slice(x.as_bytes());
                    }
                }
            }
        }
        buf
    }

    /// Deserialize from bytes (wire v2 only — an unknown version is refused loudly;
    /// a v1 store demands a rebuild rather than being silently misread).
    pub fn from_bytes(b: &[u8]) -> Result<Self, FoldError> {
        if b.is_empty() {
            return Err(FoldError::MalformedState("GroupState: empty".to_string()));
        }
        if b[0] != GROUP_STATE_WIRE_VERSION {
            return Err(FoldError::MalformedState(format!(
                "GroupState: unknown wire version 0x{:02x} (this build reads 0x{:02x}); \
                 stale stores must be rebuilt, never reinterpreted",
                b[0], GROUP_STATE_WIRE_VERSION
            )));
        }
        // Minimum: 1 + 32 + 8 + 20 + 4 + 1 = 66
        if b.len() < 66 {
            return Err(FoldError::MalformedState(format!(
                "GroupState: too short ({} bytes)",
                b.len()
            )));
        }
        let version = b[0];
        let mut head_bytes = [0u8; 32];
        head_bytes.copy_from_slice(&b[1..33]);
        let computed_at_gov_head = Hash::new(head_bytes);
        let computed_at_gov_seq = u64::from_be_bytes(b[33..41].try_into().unwrap());
        let add_member_threshold = u32::from_be_bytes(b[41..45].try_into().unwrap());
        let remove_member_threshold = u32::from_be_bytes(b[45..49].try_into().unwrap());
        let role_change_threshold = u32::from_be_bytes(b[49..53].try_into().unwrap());
        let rule_change_threshold = u32::from_be_bytes(b[53..57].try_into().unwrap());
        let resolution_threshold = u32::from_be_bytes(b[57..61].try_into().unwrap());
        let member_count = u32::from_be_bytes(b[61..65].try_into().unwrap()) as usize;

        // Each member is 32 + 1 + 8 = 41 bytes.
        let members_end = 65 + member_count * 41;
        if b.len() < members_end + 1 {
            return Err(FoldError::MalformedState(format!(
                "GroupState: need {} bytes for {} members + fork byte, have {}",
                members_end + 1,
                member_count,
                b.len()
            )));
        }
        let mut members = Vec::with_capacity(member_count);
        for i in 0..member_count {
            let off = 65 + i * 41;
            let mut pid_bytes = [0u8; 32];
            pid_bytes.copy_from_slice(&b[off..off + 32]);
            let role = u8_to_role(b[off + 32]).map_err(|_| {
                FoldError::MalformedState(format!("GroupState: unknown role byte {}", b[off + 32]))
            })?;
            let since = u64::from_be_bytes(b[off + 33..off + 41].try_into().unwrap());
            members.push((PrincipalId::new(pid_bytes), role, since));
        }
        let fork_byte = b[members_end];
        let mut off = members_end + 1;
        let take32 = |b: &[u8], off: usize| -> Result<[u8; 32], FoldError> {
            b.get(off..off + 32)
                .ok_or_else(|| FoldError::MalformedState("GroupState: truncated hash".to_string()))
                .map(|s| {
                    let mut h = [0u8; 32];
                    h.copy_from_slice(s);
                    h
                })
        };
        let fork_status = match fork_byte {
            0x00 => ForkStatus::Clean,
            0x02 => ForkStatus::UnderDetermined,
            0x01 => ForkStatus::ForkedFrom(Hash::new(take32(b, off)?)),
            0x03 => {
                let n = u32::from_be_bytes(
                    b.get(off..off + 4)
                        .ok_or_else(|| {
                            FoldError::MalformedState(
                                "GroupState: truncated entry count".to_string(),
                            )
                        })?
                        .try_into()
                        .unwrap(),
                ) as usize;
                off += 4;
                let mut entries = Vec::with_capacity(n);
                for _ in 0..n {
                    let a = Hash::new(take32(b, off)?);
                    off += 32;
                    let z = Hash::new(take32(b, off)?);
                    off += 32;
                    let subj_n = *b.get(off).ok_or_else(|| {
                        FoldError::MalformedState("GroupState: truncated subject count".to_string())
                    })? as usize;
                    off += 1;
                    let mut subjects = Vec::with_capacity(subj_n);
                    for _ in 0..subj_n {
                        subjects.push(PrincipalId::new(take32(b, off)?));
                        off += 32;
                    }
                    let excl_n = *b.get(off).ok_or_else(|| {
                        FoldError::MalformedState(
                            "GroupState: truncated excluded count".to_string(),
                        )
                    })? as usize;
                    off += 1;
                    let mut excluded = Vec::with_capacity(excl_n);
                    for _ in 0..excl_n {
                        excluded.push(Hash::new(take32(b, off)?));
                        off += 32;
                    }
                    entries.push(ContestedEntry {
                        pair: (a, z),
                        subjects,
                        excluded,
                    });
                }
                ForkStatus::Contested(entries)
            }
            other => {
                return Err(FoldError::MalformedState(format!(
                    "GroupState: unknown fork_status byte {other}"
                )));
            }
        };

        Ok(GroupState {
            version,
            computed_at_gov_head,
            computed_at_gov_seq,
            members,
            rules: GroupRules {
                add_member_threshold,
                remove_member_threshold,
                role_change_threshold,
                rule_change_threshold,
                resolution_threshold,
            },
            fork_status,
        })
    }
}

pub(crate) fn role_to_u8(r: &Role) -> u8 {
    match r {
        Role::Owner => 0,
        Role::Admin => 1,
        Role::Member => 2,
        Role::Observer => 3,
    }
}

pub(crate) fn u8_to_role(v: u8) -> Result<Role, ()> {
    match v {
        0 => Ok(Role::Owner),
        1 => Ok(Role::Admin),
        2 => Ok(Role::Member),
        3 => Ok(Role::Observer),
        _ => Err(()),
    }
}
