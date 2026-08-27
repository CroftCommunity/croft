//! The session: one substrate instance, the ports beside it, and the pond's
//! loop driven over both.
//!
//! ADR-0002 says `update` is `(model, intent) -> (model, effects)` and that a
//! core emits effects rather than performing them. This module is where that
//! sentence becomes concrete for a shell: it holds the model, calls
//! `chat_core::update`, **performs** the effects the pond emits against the
//! store and the signer it owns, and projects. The pond never touches a port;
//! it never learns that redb exists.
//!
//! Deliberately Rust-side rather than Kotlin-side. The ports are Rust — the
//! store, the signer, the fold — so an effect handler in Kotlin would need a
//! second FFI crossing per effect and would put the substrate's write path in
//! the least-tested language in the stack. The platform shell above this owns
//! the screen; this owns the machinery.
//!
//! No `uniffi` in this file. Everything here is ordinary Rust with ordinary
//! Rust types, testable without generating a binding or starting a JVM; the
//! boundary types and the `#[uniffi::export]` annotations live in `lib.rs`.
//! That separation is what lets the interesting logic be tested at Rust speed
//! and leaves the FFI layer thin enough to read in one sitting.

use std::path::Path;
use std::sync::Arc;

use chat_core::model::{GroupRef, Intent, MessageLine, Model, Snapshot};
use chat_core::view::ChatView;
use social_tree_core::model::{
    encode_message_payload, envelope_hash, AssertionEnvelope, AssertionType, DeviceId, GroupId,
    Hash, PrincipalId, Role, ENVELOPE_WIRE_VERSION,
};
use social_tree_core::ports::ed25519::{
    Ed25519Signer, Ed25519Verifier, RegistryCredentialResolver,
};
use social_tree_core::ports::{DeviceId as PortDeviceId, PrincipalId as PortPrincipalId, Signer};
use store_redb::fold_derived::{max_lamport_for_device, DerivedFold};
use store_redb::payload::{encode_genesis_payload, encode_membership_add_payload, GenesisRules};
use store_redb::tables::Db;
use store_redb::{local, read};

use crate::error::SessionError;

/// Everything one local identity needs to hold a conversation.
///
/// `Debug` is derived on nothing here and implemented by hand below: the signer
/// holds secret key material, so the derived form would print it into any log
/// line or test failure that touched a session.
pub struct Session {
    db: Arc<Db>,
    fold: DerivedFold<Ed25519Verifier, RegistryCredentialResolver>,
    signer: Ed25519Signer,
    device: DeviceId,
    principal: PrincipalId,
    /// The next lamport this device will use. Held in memory and seeded from
    /// the store on open, because the alternative — a scan per assertion — puts
    /// a read on the write path for a value only this device advances.
    next_lamport: u64,
    model: Model,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The signing key never appears. The device id is its PUBLIC half and
        // is safe to show; that is what a device id is.
        f.debug_struct("Session")
            .field("device", &self.device)
            .field("next_lamport", &self.next_lamport)
            .field("groups", &self.model.groups.len())
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Open (or create) the store at `path` for the identity `signing_key`.
    ///
    /// The key is the identity: the device id is its ed25519 verifying key, and
    /// the principal is currently the same bytes. Those are genuinely different
    /// things — one device, one persona, is an S0 simplification and not the
    /// end state; S3's DID↔persona binding is what separates them.
    pub fn open(path: &Path, signing_key: &[u8]) -> Result<Self, SessionError> {
        let seed: [u8; 32] = signing_key
            .try_into()
            .map_err(|_| SessionError::BadKeyLength {
                got: signing_key.len(),
            })?;
        let signer = Ed25519Signer::from_seed(seed);
        let device = DeviceId::new(signer.device_id().0);
        let principal = PrincipalId::new(signer.device_id().0);

        let resolver = RegistryCredentialResolver::default();
        resolver.register(
            PortDeviceId(signer.device_id().0),
            PortPrincipalId(*principal.as_bytes()),
        );

        let db = Arc::new(Db::open(path)?);
        let next_lamport = max_lamport_for_device(&db, &device)?.map_or(0, |m| m + 1);
        let fold = DerivedFold::new(Arc::clone(&db), Ed25519Verifier, resolver);

        let mut session = Session {
            db,
            fold,
            signer,
            device,
            principal,
            next_lamport,
            model: Model::default(),
        };
        // A session that opens onto an empty screen and fills in later is a
        // session that shows the user nothing for a beat, and it is also one
        // whose first refresh is untested. Load now.
        session.refresh()?;
        Ok(session)
    }

    /// This identity's principal.
    #[must_use]
    pub fn principal(&self) -> PrincipalId {
        self.principal
    }

    /// Found a new group with this device as its Owner, named locally.
    ///
    /// Two assertions, not one. Genesis seats the author as Owner in the folded
    /// state; the `MembershipAdd` is what writes the MEMBER_OF edge, and that
    /// edge is how "the groups I am in" is answered. Founding without it
    /// produces a group that exists and that its own founder cannot find.
    pub fn create_group(&mut self, title: &str) -> Result<GroupId, SessionError> {
        // The id is derived, not random: this crate has no business holding an
        // RNG when the inputs already make a unique id, and a deterministic id
        // is one a test can assert about. Device and lamport together are
        // unique by construction — the lamport never repeats for this device —
        // so the title only adds a nicety.
        let mut material = Vec::with_capacity(32 + 8 + title.len());
        material.extend_from_slice(self.device.as_bytes());
        material.extend_from_slice(&self.next_lamport.to_be_bytes());
        material.extend_from_slice(title.as_bytes());
        let group = GroupId::new(*social_tree_core::model::compute_hash(&material).as_bytes());

        let genesis = self.author(
            AssertionType::GroupGenesis,
            group,
            vec![],
            encode_genesis_payload(GenesisRules::SOLO_FOUNDER, &self.device),
        )?;
        self.author(
            AssertionType::MembershipAdd,
            group,
            vec![genesis],
            encode_membership_add_payload(&self.principal, Role::Owner),
        )?;

        local::put_group_title(&self.db, &group, title)?;
        self.refresh()?;
        Ok(group)
    }

    /// Apply one intent: update the model, perform what the pond asks for, and
    /// project.
    ///
    /// The order matters and is the pond's, not this module's. `update` runs
    /// first and returns effects; the effects are performed here; a send is
    /// then followed by a reload so the optimistic line the pond appended is
    /// replaced by the confirmed one it just wrote.
    pub fn dispatch(&mut self, intent: Intent) -> Result<ChatView, SessionError> {
        // Refusals the pond cannot express. `chat_core::update` is total and
        // panic-free by design — it drops what it cannot apply rather than
        // failing — which is right for a reducer and wrong for a boundary. A
        // dropped intent at the FFI looks exactly like a successful one from
        // Kotlin, so the boundary checks first and says no out loud.
        match &intent {
            Intent::SendMessage => {
                if self.model.selected_group.is_none() {
                    return Err(SessionError::NoGroupSelected);
                }
                if self.model.draft.trim().is_empty() {
                    return Err(SessionError::EmptyDraft);
                }
            }
            Intent::SelectGroup(group) => {
                if !self.model.groups.iter().any(|g| g.id == *group) {
                    return Err(SessionError::NoSuchGroup { group: *group });
                }
            }
            // A refresh is a request to go and READ, and the snapshot is the
            // answer — so the session supplies it. Passing a caller's snapshot
            // through to the pond would hand it whatever the shell felt like
            // saying; passing an empty one through would succeed and blank the
            // screen, which looks exactly like having no groups.
            Intent::Refresh(_) => {
                self.refresh()?;
                return Ok(chat_core::project(&self.model));
            }
            _ => {}
        }

        let (model, effects) = chat_core::update(std::mem::take(&mut self.model), intent);
        self.model = model;

        for effect in effects {
            self.perform(effect)?;
        }
        Ok(chat_core::project(&self.model))
    }

    /// The current projection, without applying an intent.
    #[must_use]
    pub fn view(&self) -> ChatView {
        chat_core::project(&self.model)
    }

    // -----------------------------------------------------------------------

    /// Perform one effect the pond emitted.
    fn perform(&mut self, effect: chat_core::model::Effect) -> Result<(), SessionError> {
        use chat_core::model::Effect;
        match effect {
            Effect::Send {
                group,
                channel,
                body,
            } => {
                self.author(
                    AssertionType::Message,
                    group,
                    vec![],
                    encode_message_payload(&body, None, channel),
                )?;
                // The pond appended an optimistic line; reloading replaces it
                // with the one the store confirmed, which is the line that
                // carries a real lamport and a real author.
                self.reload_timeline(group, channel)
            }
            Effect::LoadTimeline { group, channel } => self.reload_timeline(group, channel),
            // The mute set is local truth the shell owns (E134). It has no
            // home in this store yet; when it gets one it lands in
            // `store_redb::local` beside the group titles, for the same reason.
            Effect::PersistMuted(_) => Ok(()),
        }
    }

    /// Sign an envelope and fold it in, returning its hash.
    fn author(
        &mut self,
        assertion_type: AssertionType,
        group: GroupId,
        antecedents: Vec<Hash>,
        payload: Vec<u8>,
    ) -> Result<Hash, SessionError> {
        let mut env = AssertionEnvelope {
            version: ENVELOPE_WIRE_VERSION,
            assertion_type,
            author_device: self.device,
            author_principal: self.principal,
            group,
            antecedents,
            lamport: self.next_lamport,
            payload,
            signature: vec![],
        };
        env.signature = self.signer.sign(&env.canonical_bytes());
        self.fold.ingest(&env)?;
        // Advanced only after the ingest succeeds. Advancing first would burn a
        // lamport on every refused assertion, leaving gaps that read as missing
        // history rather than as nothing having happened.
        self.next_lamport += 1;
        Ok(envelope_hash(&env))
    }

    /// Re-read the whole visible world into the model.
    fn refresh(&mut self) -> Result<(), SessionError> {
        let group = self.model.selected_group;
        let channel = self.model.selected_channel;
        let snapshot = self.snapshot(group, channel)?;
        let (model, _) =
            chat_core::update(std::mem::take(&mut self.model), Intent::Refresh(snapshot));
        self.model = model;
        Ok(())
    }

    fn reload_timeline(
        &mut self,
        group: GroupId,
        channel: Option<social_tree_core::model::TypedId>,
    ) -> Result<(), SessionError> {
        let snapshot = self.snapshot(Some(group), channel)?;
        let (model, _) =
            chat_core::update(std::mem::take(&mut self.model), Intent::Refresh(snapshot));
        self.model = model;
        Ok(())
    }

    /// Read the store into the shape the pond consumes.
    fn snapshot(
        &self,
        group: Option<GroupId>,
        channel: Option<social_tree_core::model::TypedId>,
    ) -> Result<Snapshot, SessionError> {
        let mut groups = Vec::new();
        for id in read::groups_for_principal(&self.db, &self.principal)? {
            let member_count = read::members_of_group(&self.db, &id)?.len();
            groups.push(GroupRef {
                // An unnamed group shows a short id rather than an empty row —
                // `chat_core`'s own documented fallback.
                title: local::group_title(&self.db, &id)?.unwrap_or_else(|| short_id(&id)),
                id,
                member_count,
            });
        }

        let (timeline, members) = match group {
            None => (Vec::new(), Vec::new()),
            Some(id) => {
                let lines = read::messages_in_group(&self.db, &id)?
                    .into_iter()
                    .map(|m| MessageLine {
                        lamport: m.lamport,
                        author: short_principal(&m.author),
                        author_principal: Some(m.author),
                        body: m.body,
                    })
                    .collect();
                let members = read::members_of_group(&self.db, &id)?
                    .into_iter()
                    .map(|(principal, role, _since)| chat_core::model::MemberRow {
                        principal,
                        role: role_label(role),
                        // The fold's CONTESTED and ceiling states are what
                        // populate this honestly; S0 reads a clean roster, so
                        // everyone here is seated. When the contested paths are
                        // wired the mapping belongs right here, and nowhere
                        // else — a shell that decides standing for itself is a
                        // shell that can flatter the record.
                        standing: chat_core::model::Standing::Seated,
                    })
                    .collect();
                (lines, members)
            }
        };

        Ok(Snapshot {
            groups,
            channels: Vec::new(),
            group,
            channel,
            timeline,
            fork: None,
            members,
        })
    }
}

/// The first four bytes of a group id, hex — enough to tell two apart on a
/// screen, short enough to sit in a tree row.
fn short_id(group: &GroupId) -> String {
    group.as_bytes()[..4]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn short_principal(principal: &PrincipalId) -> String {
    principal.as_bytes()[..4]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn role_label(role: Role) -> String {
    match role {
        Role::Owner => "owner",
        Role::Admin => "admin",
        Role::Member => "member",
        Role::Observer => "observer",
    }
    .to_string()
}
