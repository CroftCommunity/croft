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
use keylayer_openmls::store::PersistentProvider;
use keylayer_openmls::OpenMlsKeyLayer;
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

/// What an offered record claims, before anything is folded.
///
/// Deliberately has no group TITLE. Titles are local truth on each device
/// (`store_redb::local`, roadmap row E141) and are never folded, so a record
/// cannot carry one and a joining device names the group itself. The two phones
/// showing different names for one group is expected, not a defect.
// No `Eq`: `Role` does not implement it, and deriving a weaker bound here than
// the core offers would be the tail wagging the dog.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordClaims {
    /// The group every assertion in the record belongs to.
    pub group: GroupId,
    /// Who authored the genesis, if the record carries one.
    pub founder: Option<PrincipalId>,
    /// Who the record would seat, and in what role.
    pub seats: Vec<(PrincipalId, social_tree_core::model::Role)>,
    /// Whether this device's own principal is among them.
    pub would_seat_me: bool,
    /// How many assertions the record carries.
    pub assertion_count: usize,
    /// Whether this device has already folded state for this group.
    pub already_folded: bool,
}

/// Everything one local identity needs to hold a conversation.
///
/// `Debug` is derived on nothing here and implemented by hand below: the signer
/// holds secret key material, so the derived form would print it into any log
/// line or test failure that touched a session.
pub struct Session {
    /// The credential registry this session's fold resolves against.
    ///
    /// Held so that accepting a record can register the authors it introduces.
    /// The fold has its own handle to the same registry — it is an `Arc`
    /// inside — so registering here is visible there.
    resolver: RegistryCredentialResolver,
    /// The canonical bytes of the most recently authored envelope.
    ///
    /// See `author` for why this exists rather than being returned.
    last_authored: Option<Vec<u8>>,
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
    /// The MLS side, in its own store beside the governance one.
    ///
    /// **A separate file, deliberately.** redb takes an exclusive lock per
    /// file, so sharing one would serialise every governance read behind every
    /// MLS write for no benefit. They also fail independently: a corrupt
    /// governance store is a lost timeline, a corrupt MLS store is a group that
    /// can no longer decrypt, and keeping them apart keeps those two outcomes
    /// from arriving together.
    keys: OpenMlsKeyLayer<PersistentProvider>,
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
        let seed: [u8; 32] = signing_key.try_into().map_err(|_| {
            // The LENGTH, never the bytes. Everything about a signing key that
            // is safe to log is its length.
            tracing::warn!(target: "croft.ffi", got = signing_key.len(), "refused a signing key of the wrong length");
            SessionError::BadKeyLength {
                got: signing_key.len(),
            }
        })?;
        let signer = Ed25519Signer::from_seed(seed);
        let device = DeviceId::new(signer.device_id().0);
        let principal = PrincipalId::new(signer.device_id().0);

        let resolver = RegistryCredentialResolver::default();
        resolver.register(
            PortDeviceId(signer.device_id().0),
            PortPrincipalId(*principal.as_bytes()),
        );

        let db = Arc::new(Db::open(path).inspect_err(|e| {
            tracing::warn!(target: "croft.ffi", error = %e, "refused to open the store");
        })?);
        // Beside the governance store, named after it so the pair is obvious
        // in a file listing and so two identities cannot collide.
        let mls_path = path.with_extension("mls.redb");
        let mut keys = OpenMlsKeyLayer::persistent(principal, &mls_path).map_err(|e| {
            tracing::warn!(target: "croft.ffi", error = %e, "refused to open the MLS store");
            SessionError::Storage {
                reason: e.to_string(),
            }
        })?;
        // A group from a previous run, if there was one. Silent when there is
        // not — a fresh install is not a failure.
        keys.load_group().map_err(|e| SessionError::Storage {
            reason: e.to_string(),
        })?;
        let next_lamport = max_lamport_for_device(&db, &device)?.map_or(0, |m| m + 1);
        let fold = DerivedFold::new(Arc::clone(&db), Ed25519Verifier, resolver.clone());

        let mut session = Session {
            db,
            fold,
            signer,
            device,
            principal,
            next_lamport,
            model: Model::default(),
            keys,
            resolver,
            last_authored: None,
        };
        // A session that opens onto an empty screen and fills in later is a
        // session that shows the user nothing for a beat, and it is also one
        // whose first refresh is untested. Load now.
        session.refresh()?;
        tracing::debug!(target: "croft.ffi", device = ?session.device, next_lamport, "session opened");
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

        // Seat real MLS for the group we just founded. Governance says who may
        // be in the room; MLS is what makes the room a room. Before S2 a group
        // was the first without the second, and nothing in the projection
        // showed the difference.
        self.keys
            .create_group()
            .map_err(|e| SessionError::Storage {
                reason: e.to_string(),
            })?;

        local::put_group_title(&self.db, &group, title)?;
        self.refresh()?;
        tracing::debug!(target: "croft.ffi", group = ?group, "group founded");
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
                    return Err(self.refuse(SessionError::NoGroupSelected));
                }
                if self.model.draft.trim().is_empty() {
                    return Err(self.refuse(SessionError::EmptyDraft));
                }
            }
            Intent::SelectGroup(group) => {
                if !self.model.groups.iter().any(|g| g.id == *group) {
                    return Err(self.refuse(SessionError::NoSuchGroup { group: *group }));
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
        tracing::debug!(target: "croft.ffi", "intent applied");
        Ok(chat_core::project(&self.model))
    }

    /// The current projection, without applying an intent.
    #[must_use]
    pub fn view(&self) -> ChatView {
        chat_core::project(&self.model)
    }

    // -----------------------------------------------------------------------

    /// Log a refusal on its way out, and hand it back unchanged.
    ///
    /// Every refusal goes through here so that none of them can be added later
    /// without one — a boundary where SOME failures are logged is worse than
    /// one where none are, because the gaps look like absences of failure.
    fn refuse(&self, e: SessionError) -> SessionError {
        tracing::warn!(target: "croft.ffi", reason = %e, "refused");
        e
    }

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
        // Kept because the pond's effect loop has no return channel: `perform`
        // returns `()`, so `send_sealed` cannot be handed the envelope it needs
        // to seal. Overwritten on every author and read at most once, by the
        // send that caused it.
        self.last_authored = Some(env.canonical_bytes_with_sig());
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

impl Session {
    /// Whether a real MLS group is seated.
    ///
    /// Distinct from "a group exists in the fold": governance decides who may
    /// be in the room, MLS is what makes the room a room, and a projection
    /// looks identical either way. So it needs asking directly.
    #[must_use]
    pub fn has_mls_group(&self) -> bool {
        self.keys.group_id().is_some()
    }

    /// The seated group's MLS epoch, if there is one.
    #[must_use]
    pub fn mls_epoch(&self) -> Option<u64> {
        self.keys.epoch()
    }

    /// This device's key package, for someone else to invite it with.
    ///
    /// # Errors
    /// [`SessionError::Storage`] when the key layer refuses.
    pub fn mls_key_package(&self) -> Result<Vec<u8>, SessionError> {
        self.keys
            .key_package_bytes()
            .map_err(|e| SessionError::Refused {
                reason: e.to_string(),
            })
    }

    /// Seal `plaintext` for the seated group.
    ///
    /// # Errors
    /// [`SessionError::Refused`] when there is no group, or MLS refuses.
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, SessionError> {
        let out = self
            .keys
            .seal(plaintext)
            .map_err(|e| SessionError::Refused {
                reason: e.to_string(),
            })?;
        tracing::debug!(
            target: "croft.ffi",
            epoch = self.keys.epoch(),
            bytes = out.len(),
            "sealed",
        );
        Ok(out)
    }

    /// This group's governance record, as replayable envelopes.
    ///
    /// What an inviter offers a joining device. See
    /// [`store_redb::read::governance_record`] for why it is envelopes rather
    /// than derived state.
    ///
    /// # Errors
    /// [`SessionError::NoGroupSelected`] with no group; storage errors from the
    /// read.
    pub fn group_record(&self) -> Result<Vec<Vec<u8>>, SessionError> {
        let group = self
            .model
            .selected_group
            .or_else(|| self.model.groups.first().map(|g| g.id))
            .ok_or(SessionError::NoGroupSelected)?;
        Ok(read::governance_record(&self.db, &group)?)
    }

    /// What an offered record CLAIMS, without folding any of it.
    ///
    /// **This must not change anything, and that is the design.** Accepting a
    /// group's record is not something a scan does to you: it is a bounded
    /// exchange — scan, see who this is and what they claim, accept — whose
    /// output is a trust judgment in a relationship. This call is the middle of
    /// that exchange, so it verifies and reports and touches neither the store
    /// nor the credential registry. [`Session::accept_record`] is the other
    /// half, and the person is what goes between them.
    ///
    /// Every signature is checked here. That check is self-contained: a device
    /// id IS an Ed25519 public key, so a valid signature proves possession of
    /// that device's key without anyone having to vouch for it first. What it
    /// does NOT prove is that the device may act for the principal it names —
    /// that is the credential question, and answering it is exactly what
    /// accepting the record decides.
    ///
    /// # Errors
    /// [`SessionError::Refused`] when the offer is malformed, points elsewhere,
    /// or carries an envelope whose signature does not hold.
    pub fn read_record(&self, offer: &[u8]) -> Result<RecordClaims, SessionError> {
        use social_tree_core::ports::Verifier;

        let offered = transport_iroh::record::decode_offer(offer).map_err(|e| {
            self.refuse(SessionError::Refused {
                reason: e.to_string(),
            })
        })?;

        let raws = match offered {
            transport_iroh::record::RecordOffer::Inline(envelopes) => envelopes,
            // Decoded, understood, and refused — which is the whole reason the
            // form is on the wire before anything can fetch it. A generic parse
            // error here would send whoever reads the log hunting for damage.
            transport_iroh::record::RecordOffer::Elsewhere(where_to) => {
                return Err(self.refuse(SessionError::Refused {
                    reason: format!(
                        "this invite points elsewhere for its record ({where_to}), and this build \
                         cannot fetch one"
                    ),
                }))
            }
        };

        let verifier = Ed25519Verifier;
        let mut group = None;
        let mut seats = Vec::new();
        let mut founder = None;

        for (i, raw) in raws.iter().enumerate() {
            let env = social_tree_core::wire::decode_envelope_from_canonical(raw).map_err(|e| {
                self.refuse(SessionError::Refused {
                    reason: format!("assertion {i} in this record does not decode: {e}"),
                })
            })?;

            verifier
                .verify(
                    &PortDeviceId(*env.author_device.as_bytes()),
                    &env.canonical_bytes(),
                    &env.signature,
                )
                .map_err(|e| {
                    self.refuse(SessionError::Refused {
                        reason: format!("assertion {i} in this record is not properly signed: {e}"),
                    })
                })?;

            // One record describes one group. A mixed offer is either a bug or
            // an attempt to smuggle a second group past the person deciding.
            match group {
                None => group = Some(env.group),
                Some(g) if g == env.group => {}
                Some(_) => {
                    return Err(self.refuse(SessionError::Refused {
                        reason: "this record mixes assertions from more than one group".to_string(),
                    }))
                }
            }

            if env.assertion_type == AssertionType::GroupGenesis {
                founder = Some(env.author_principal);
            }
            if env.assertion_type == AssertionType::MembershipAdd {
                if let Some((principal, role)) =
                    store_redb::payload::decode_membership_add_payload(&env.payload)
                {
                    seats.push((principal, role));
                }
            }
        }

        let group = group.ok_or_else(|| {
            self.refuse(SessionError::Refused {
                reason: "this record names no group".to_string(),
            })
        })?;

        Ok(RecordClaims {
            group,
            founder,
            would_seat_me: seats.iter().any(|(p, _)| *p == self.principal),
            seats,
            assertion_count: raws.len(),
            already_folded: read::group_state(&self.db, &group)?.is_some(),
        })
    }

    /// Accept an offered record: register its authors and fold it.
    ///
    /// The recorded output of the judgment [`Session::read_record`] informed.
    /// Everything that call verifies is verified again here rather than trusted
    /// across the gap — the person may have taken a while to decide, and a
    /// second read of the same bytes costs nothing next to folding the wrong
    /// ones.
    ///
    /// **What accepting means, stated plainly.** It registers that the record's
    /// authors may act for the principals their assertions name, for this group.
    /// That is trust on first use: the courier is not proving they are entitled
    /// to say this, the person is deciding to proceed. The exposure is bounded
    /// to this group, and the envelopes are re-verified by this device's own
    /// fold rather than taken on the sender's word.
    ///
    /// Returns how many assertions were newly folded. Zero is a normal answer:
    /// gossip delivers the same artifact more than once whenever the swarm has
    /// more than one path, so a repeat accept is an ordinary event.
    ///
    /// # Errors
    /// Whatever [`Session::read_record`] refuses, plus a fold refusal naming
    /// which assertion would not go in.
    pub fn accept_record(&mut self, offer: &[u8]) -> Result<usize, SessionError> {
        let claims = self.read_record(offer)?;

        let raws = match transport_iroh::record::decode_offer(offer) {
            Ok(transport_iroh::record::RecordOffer::Inline(e)) => e,
            // read_record already refused both other cases.
            _ => unreachable!("read_record accepted this offer"),
        };

        let mut folded = 0usize;
        for (i, raw) in raws.iter().enumerate() {
            let env = social_tree_core::wire::decode_envelope_from_canonical(raw)
                .map_err(|e| SessionError::Refused { reason: e })?;

            // The credential this record asks us to accept. Registered before
            // the ingest because the fold resolves it during verification, and
            // only for authors of THIS record.
            self.resolver.register(
                PortDeviceId(*env.author_device.as_bytes()),
                PortPrincipalId(*env.author_principal.as_bytes()),
            );

            match self.fold.ingest(&env) {
                Ok(_) => folded += 1,
                Err(e) => {
                    // An assertion already folded is the repeat-delivery case
                    // and is not a failure; anything else is, and names its
                    // index so the offer can be inspected.
                    let said = e.to_string();
                    if said.contains("duplicate") || said.contains("already") {
                        continue;
                    }
                    return Err(self.refuse(SessionError::Refused {
                        reason: format!("assertion {i} in this record would not fold: {said}"),
                    }));
                }
            }
        }

        tracing::debug!(
            target: "croft.ffi",
            group = ?claims.group,
            folded,
            offered = raws.len(),
            "accepted a record",
        );
        self.refresh()?;
        Ok(folded)
    }

    /// Send the current draft as a SEALED ASSERTION, returning the wire bytes.
    ///
    /// This is the method a conversation is actually made of, and it is not
    /// `seal(plaintext)`. What crosses is the **assertion envelope**: authored,
    /// signed, folded locally, then sealed whole. Three consequences, all of
    /// them the point:
    ///
    /// - the far device learns the AUTHOR from inside the envelope, so rung 5's
    ///   "with the sender's short principal" needs nothing from the transport;
    /// - the line is folded here BEFORE it is sealed, so the sender sees their
    ///   own message immediately rather than waiting on an acknowledgement that
    ///   this transport will never send;
    /// - the envelope keeps its signature, so a member cannot be impersonated
    ///   by whoever happens to be able to reach the swarm.
    ///
    /// Sealing a bare plaintext would lose all three and is why `seal` stays a
    /// lower-level primitive rather than becoming this.
    ///
    /// # Errors
    /// [`SessionError::NoGroupSelected`], [`SessionError::EmptyDraft`], or
    /// [`SessionError::Refused`] when governance or MLS refuses.
    pub fn send_sealed(&mut self) -> Result<Vec<u8>, SessionError> {
        // Dispatch does the refusing, the authoring and the folding, so the
        // local half of a sealed send is exactly the local send — one path, not
        // two that can drift.
        self.last_authored = None;
        self.dispatch(Intent::SendMessage)?;

        let envelope = self.last_authored.take().ok_or_else(|| {
            // Unreachable while `Effect::Send` authors, and worth a refusal
            // rather than an unwrap: if the pond ever stops emitting that
            // effect, this says so instead of panicking on a phone.
            self.refuse(SessionError::Refused {
                reason: "the send produced no assertion to seal".to_string(),
            })
        })?;

        let sealed = self.seal(&envelope)?;
        tracing::debug!(
            target: "croft.ffi",
            epoch = self.keys.epoch(),
            bytes = sealed.len(),
            "sealed an assertion for the swarm",
        );
        Ok(sealed)
    }

    /// Open a sealed assertion from the swarm and fold it in.
    ///
    /// Returns whether the fold accepted it. `false` is not an error: a
    /// duplicate is the normal consequence of gossip, which delivers a message
    /// to a member more than once whenever the swarm has more than one path.
    /// Treating that as a failure would fill the screen with refusals during a
    /// perfectly healthy run.
    ///
    /// # Errors
    /// [`SessionError::Refused`] when MLS will not open it — a stranger's
    /// traffic, or an epoch this device cannot reach — or when what comes out
    /// is not an envelope this build can read.
    pub fn receive_sealed(&mut self, wire: &[u8]) -> Result<bool, SessionError> {
        let raw = self.open_sealed(wire)?;

        // The ONE decoder (`social_tree_core::wire`). The corpus once carried
        // three copies and one of them silently mis-parsed v2, which is the
        // reason that module exists and the reason this does not hand-roll it.
        let envelope =
            social_tree_core::wire::decode_envelope_from_canonical(&raw).map_err(|e| {
                self.refuse(SessionError::Refused {
                    reason: format!("that was not an assertion this build can read: {e}"),
                })
            })?;

        let group = envelope.group;
        let accepted = match self.fold.ingest(&envelope) {
            Ok(_) => true,
            Err(e) => {
                // A refused envelope is logged and reported, never folded and
                // never silently dropped: on two phones this is the difference
                // between "they never sent it" and "we would not take it".
                tracing::warn!(
                    target: "croft.ffi",
                    error = %e,
                    "the fold refused an assertion from the swarm",
                );
                false
            }
        };

        if accepted {
            let channel = self.model.selected_channel;
            self.reload_timeline(group, channel)?;
        }
        Ok(accepted)
    }

    /// Open a sealed message from the seated group.
    ///
    /// # Errors
    /// [`SessionError::Refused`] when MLS refuses — which is what a message
    /// from a group this device is not in looks like, and what an epoch
    /// mismatch looks like. Both are refusals with words rather than silence.
    pub fn open_sealed(&mut self, wire: &[u8]) -> Result<Vec<u8>, SessionError> {
        self.keys.open(wire).map_err(|e| {
            tracing::warn!(target: "croft.ffi", epoch = self.keys.epoch(), error = %e, "could not open");
            SessionError::Refused {
                reason: e.to_string(),
            }
        })
    }
}

impl Session {
    /// Invite the holder of `key_package` into the seated group.
    ///
    /// The full arc, in the order the architecture requires and not a
    /// shortcut around it: **governance decides first** (a `MembershipAdd`
    /// assertion is authored and folded), then the slip is minted *from the
    /// folded state*, then MLS enacts it. Reversing those would let the crypto
    /// seat someone the record never admitted — which is the whole failure
    /// mode the two-admission split exists to prevent.
    ///
    /// The invitee's identity comes out of their key package rather than being
    /// supplied alongside it. Taking it out-of-band would let a caller name one
    /// person and hand over another's key package, and nothing downstream would
    /// notice.
    ///
    /// Returns the Welcome for delivery to the invitee. **Delivery is not this
    /// crate's business** — S2 rides iroh-gossip device-to-device (Q2), and
    /// keeping the artifact and its transport apart is what lets the transport
    /// change without touching any of this.
    ///
    /// # Errors
    /// [`SessionError::NoGroupSelected`] with no group; [`SessionError::Refused`]
    /// when governance or MLS refuses.
    pub fn invite(&mut self, key_package: &[u8]) -> Result<Vec<u8>, SessionError> {
        let group = self
            .model
            .selected_group
            .or_else(|| self.model.groups.first().map(|g| g.id))
            .ok_or(SessionError::NoGroupSelected)?;

        let invitee = self
            .keys
            .principal_in_key_package(key_package)
            .map_err(|e| SessionError::Refused {
                reason: e.to_string(),
            })?;

        // 1. The governance decision, folded. Without this the slip cannot be
        //    minted at all — `authorize_invite_enactment` reads the folded
        //    state, so an unfolded decision is simply not there.
        self.author(
            AssertionType::MembershipAdd,
            group,
            vec![],
            store_redb::payload::encode_membership_add_payload(&invitee, Role::Member),
        )?;

        // 2. The slip, minted FROM that state.
        let state = read::group_state(&self.db, &group)?.ok_or_else(|| SessionError::Refused {
            reason: "the group has no folded state to authorize against".to_string(),
        })?;
        let slip = social_tree_core::admission::authorize_invite_enactment(&invitee, &state)
            .map_err(|e| SessionError::Refused {
                reason: format!("{e:?}"),
            })?;

        // 3. The enactment, on real MLS.
        self.keys
            .deposit_key_package(invitee, key_package)
            .map_err(|e| SessionError::Refused {
                reason: e.to_string(),
            })?;
        let artifacts = {
            use social_tree_core::ports::keylayer::KeyLayer as _;
            self.keys
                .add_with_welcome(slip)
                .map_err(|e| SessionError::Refused {
                    reason: e.to_string(),
                })?
        };

        tracing::debug!(
            target: "croft.ffi",
            epoch = self.keys.epoch(),
            "invited, epoch advanced",
        );
        self.refresh()?;
        Ok(artifacts.welcome)
    }

    /// Seat this device from a Welcome someone sent.
    ///
    /// # Errors
    /// [`SessionError::Refused`] for bytes that are not a Welcome this device
    /// can seat from — including a Welcome for a group it was never added to.
    pub fn accept_invite(&mut self, welcome: &[u8]) -> Result<(), SessionError> {
        let merged = {
            self.keys
                .join_from_welcome(welcome)
                .map_err(|e| SessionError::Refused {
                    reason: e.to_string(),
                })?
        };
        tracing::debug!(target: "croft.ffi", epoch = merged.epoch, "seated from a Welcome");
        Ok(())
    }
}
