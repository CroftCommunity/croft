//! **The KeyLayer port's native realization: real openmls behind
//! `social-tree-core`'s trait (E117 P4, ADR-0003 decision 4).**
//!
//! Adapted, not rebuilt, from the meer-queue lineage (S16/S23–S26): group
//! construction and joining from `mls_replant`, seal/open with named errors
//! from `meer-queue::mls`, the external-commit + PSK path from the S24/C4
//! harness. The exact version pins are the ones the experiments resolved.
//!
//! What the trait carries and what stays inherent, deliberately:
//! - The **trait** methods are the decision-relevant surface — staging a
//!   commit into claims, and the two slip-gated enactments
//!   (`merge_admission`, `add_with_welcome`). A3 rides the types.
//! - The **inherent** methods are shell plumbing — creating/joining groups,
//!   minting and depositing KeyPackages, storing token secrets (S23's
//!   ledger material), serving GroupInfo, building the returner's external
//!   commit, seal/open. None of them answers "admit?".
//! - `enact_departure` is inherent and NOT yet slip-gated: P4's two paths
//!   are admission; the removal enactment joins the slip discipline when
//!   the eviction machinery lands (noted in ADR-0003's consequences).
//!
//! MLS state lives entirely here (ADR-0003 decision 3): the `MlsGroup`,
//! the PSK store, the provider. No method exposes key material.

#![warn(missing_docs)]

use openmls_rust_crypto::OpenMlsRustCrypto;
use store::Bookkeeping as _;

mod identity;

/// MLS state that survives the process: openmls's `StorageProvider` over redb
/// (P7 S2, P0-2). Public because the shell constructs one and hands it to the
/// provider — the storage location is the shell's business, not this crate's.
pub mod store;

use std::collections::HashMap;

use openmls::group::StagedCommit;
use openmls::messages::group_info::VerifiableGroupInfo;
use openmls::prelude::*;
use openmls::schedule::psk::{PreSharedKeyId, Psk};
use tls_codec::{Deserialize as _, Serialize as _};

pub use identity::{credential_principal, Persona, CS};

use social_tree_core::admission::{AdmissionClaims, InviteApproval, MergeApproval, TokenId};
use social_tree_core::model::PrincipalId;
use social_tree_core::ports::keylayer::{InviteArtifacts, KeyLayer, KeyLayerError, MergedEpoch};

fn group_config() -> MlsGroupCreateConfig {
    MlsGroupCreateConfig::builder()
        .ciphersuite(CS)
        .number_of_resumption_psks(8)
        .use_ratchet_tree_extension(true)
        .build()
}

/// The PSK id for a core [`TokenId`] — the token's 32 bytes verbatim, so
/// the countable `psk_proposals()` entry IS the token the chain names.
fn psk_id_for(token: &TokenId) -> PreSharedKeyId {
    PreSharedKeyId::external(token.as_bytes().to_vec(), vec![7u8; 32])
}

/// One member's key layer: their persona, their (at most one) live group,
/// and the commits staged awaiting a decision.
pub struct OpenMlsKeyLayer<
    P: openmls_traits::OpenMlsProvider + store::Bookkeeping = OpenMlsRustCrypto,
> {
    principal: PrincipalId,
    persona: Persona<P>,
    group: Option<MlsGroup>,
    pending_key_packages: HashMap<PrincipalId, KeyPackage>,
    staged: HashMap<[u8; 32], AdmissionClaims>,
    staged_commits: HashMap<[u8; 32], StagedCommit>,
}

impl OpenMlsKeyLayer<OpenMlsRustCrypto> {
    /// A fresh key layer for `principal`, holding its MLS state in memory.
    ///
    /// The loopback suites use this, and so does anything that wants a key
    /// layer without a filesystem. Nothing about it changed when persistence
    /// arrived.
    #[must_use]
    pub fn new(principal: PrincipalId) -> Self {
        Self::over(Persona::new(&principal), principal)
    }
}

impl<P: openmls_traits::OpenMlsProvider + store::Bookkeeping> OpenMlsKeyLayer<P> {
    fn over(persona: Persona<P>, principal: PrincipalId) -> Self {
        Self {
            principal,
            persona,
            group: None,
            pending_key_packages: HashMap::new(),
            staged: HashMap::new(),
            staged_commits: HashMap::new(),
        }
    }

    /// The provider this layer holds — what it is, is the difference between
    /// state that survives the app closing and state that does not.
    #[must_use]
    pub fn provider(&self) -> &P {
        &self.persona.provider
    }

    /// The live group's epoch, if a group is loaded.
    ///
    /// S2's observability asks for epoch transitions to be legible, and an
    /// epoch that silently failed to survive a restart is the failure this
    /// whole phase guards against — so it needs to be readable, not inferred.
    #[must_use]
    pub fn epoch(&self) -> Option<u64> {
        self.group.as_ref().map(|g| g.epoch().as_u64())
    }

    /// The live group's id, if a group is loaded.
    #[must_use]
    pub fn group_id(&self) -> Option<Vec<u8>> {
        self.group
            .as_ref()
            .map(|g| g.group_id().as_slice().to_vec())
    }

    /// Whether a key package handed over by `principal` is held.
    #[must_use]
    pub fn has_pending_key_package(&self, principal: &PrincipalId) -> bool {
        self.pending_key_packages.contains_key(principal)
    }

    /// Whether any commit is staged awaiting a decision.
    ///
    /// Exposed for the P0-3 pin: staged state must NOT come back from the
    /// store, and a test of only the surviving half would pass just as well if
    /// everything persisted.
    #[must_use]
    pub fn staged_is_empty(&self) -> bool {
        self.staged.is_empty() && self.staged_commits.is_empty()
    }

    /// The principal this layer serves.
    #[must_use]
    pub fn principal(&self) -> PrincipalId {
        self.principal
    }

    /// Plant a fresh group with this persona as the only member.
    ///
    /// # Errors
    /// [`KeyLayerError::Process`] on an openmls failure.
    pub fn create_group(&mut self) -> Result<(), KeyLayerError> {
        let g = MlsGroup::new(
            &self.persona.provider,
            &self.persona.signer,
            &group_config(),
            self.persona.cwk.clone(),
        )
        .map_err(|e| KeyLayerError::Process(e.to_string()))?;
        // Whatever this layer is persisting to, tell it the group exists. The
        // hook is a no-op for the in-memory provider and is what makes the
        // group findable again for a persistent one.
        self.persona
            .provider
            .remember_group_id(g.group_id().as_slice())
            .map_err(KeyLayerError::Process)?;
        self.group = Some(g);
        Ok(())
    }

    /// A serialized fresh `KeyPackage` this persona can be invited with.
    ///
    /// # Errors
    /// [`KeyLayerError::Process`] on serialization failure.
    pub fn key_package_bytes(&self) -> Result<Vec<u8>, KeyLayerError> {
        self.persona
            .key_package()
            .tls_serialize_detached()
            .map_err(|e| KeyLayerError::Process(e.to_string()))
    }

    /// Hold `principal`'s KeyPackage for a later slip-gated
    /// [`KeyLayer::add_with_welcome`]. Artifact delivery is shell plumbing;
    /// holding bytes decides nothing.
    ///
    /// # Errors
    /// [`KeyLayerError::Parse`] when the bytes are not a KeyPackage.
    pub fn deposit_key_package(
        &mut self,
        principal: PrincipalId,
        kp_wire: &[u8],
    ) -> Result<(), KeyLayerError> {
        let kp_in = KeyPackageIn::tls_deserialize_exact(kp_wire)
            .map_err(|e| KeyLayerError::Parse(e.to_string()))?;
        let kp = kp_in
            .validate(self.persona.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(|e| KeyLayerError::Parse(e.to_string()))?;
        // Kept before it is inserted, so a crash between the two loses the
        // in-memory copy and not the durable one. The other order would leave
        // the layer believing it holds a key package the store never saw.
        self.persona
            .provider
            .remember_key_package(principal.as_bytes(), kp_wire)
            .map_err(KeyLayerError::Process)?;
        self.pending_key_packages.insert(principal, kp);
        Ok(())
    }

    /// Seat from a Welcome (the invitee's half of the invite path). The
    /// group config carries the ratchet tree in the Welcome, so no
    /// separate tree artifact is needed.
    ///
    /// # Errors
    /// [`KeyLayerError::Parse`] for non-Welcome bytes;
    /// [`KeyLayerError::Process`] when seating fails.
    pub fn join_from_welcome(&mut self, welcome_wire: &[u8]) -> Result<MergedEpoch, KeyLayerError> {
        let msg = MlsMessageIn::tls_deserialize_exact(welcome_wire)
            .map_err(|e| KeyLayerError::Parse(e.to_string()))?;
        let MlsMessageBodyIn::Welcome(welcome) = msg.extract() else {
            return Err(KeyLayerError::Parse("not a Welcome".to_string()));
        };
        let g = StagedWelcome::new_from_welcome(
            &self.persona.provider,
            &MlsGroupJoinConfig::default(),
            welcome,
            None,
        )
        .map_err(|e| KeyLayerError::Process(e.to_string()))?
        .into_group(&self.persona.provider)
        .map_err(|e| KeyLayerError::Process(e.to_string()))?;
        let epoch = g.epoch().as_u64();
        self.group = Some(g);
        Ok(MergedEpoch { epoch })
    }

    /// Deposit a token's secret into this member's provider — S23's ledger
    /// material, group state every incumbent holds. Possession carries no
    /// standing weight; the chain's issuance fact is what admits.
    ///
    /// # Errors
    /// [`KeyLayerError::Process`] when the provider refuses the write.
    pub fn store_token(&self, token: &TokenId, secret: &[u8]) -> Result<(), KeyLayerError> {
        psk_id_for(token)
            .store(&self.persona.provider, secret)
            .map_err(|e| KeyLayerError::Process(e.to_string()))
    }

    /// The served pull artifact: `GroupInfo` with the ratchet tree bundled
    /// (epoch-bound, deliberately perishable — §11.7's serving door).
    ///
    /// # Errors
    /// [`KeyLayerError::Process`] on export failure.
    pub fn group_info_with_tree(&mut self) -> Result<Vec<u8>, KeyLayerError> {
        let (crypto, signer) = (self.persona.provider.crypto(), &self.persona.signer);
        let out = self
            .group
            .as_mut()
            .ok_or_else(|| KeyLayerError::Process("no live group".to_string()))?
            .export_group_info(crypto, signer, true)
            .map_err(|e| KeyLayerError::Process(e.to_string()))?;
        out.tls_serialize_detached()
            .map_err(|e| KeyLayerError::Process(e.to_string()))
    }

    /// The returner's half of the token-return path: build a REAL external
    /// commit from served `GroupInfo`, carrying `token` as a PSK proposal,
    /// and hold the resulting group as this member's live group. The
    /// returned wire is what an incumbent stages.
    ///
    /// # Errors
    /// [`KeyLayerError::Parse`] for non-GroupInfo bytes;
    /// [`KeyLayerError::Process`] when the commit cannot be built (e.g.
    /// the token secret is not in this provider).
    pub fn return_via_external_commit(
        &mut self,
        group_info_wire: &[u8],
        token: &TokenId,
    ) -> Result<Vec<u8>, KeyLayerError> {
        let msg = MlsMessageIn::tls_deserialize_exact(group_info_wire)
            .map_err(|e| KeyLayerError::Parse(e.to_string()))?;
        let gi: VerifiableGroupInfo = match msg.extract() {
            MlsMessageBodyIn::GroupInfo(gi) => gi,
            _ => return Err(KeyLayerError::Parse("not a GroupInfo".to_string())),
        };
        let (group, bundle) = MlsGroup::external_commit_builder()
            .with_config(MlsGroupJoinConfig::default())
            .build_group(&self.persona.provider, gi, self.persona.cwk.clone())
            .map_err(|e| KeyLayerError::Process(e.to_string()))?
            .add_psk_proposal(PreSharedKeyProposal::new(psk_id_for(token)))
            .load_psks(self.persona.provider.storage())
            .map_err(|e| KeyLayerError::Process(e.to_string()))?
            .build(
                self.persona.provider.rand(),
                self.persona.provider.crypto(),
                &self.persona.signer,
                |_| true,
            )
            .map_err(|e| KeyLayerError::Process(e.to_string()))?
            .finalize(&self.persona.provider)
            .map_err(|e| KeyLayerError::Process(e.to_string()))?;
        let wire = bundle
            .commit()
            .tls_serialize_detached()
            .map_err(|e| KeyLayerError::Process(e.to_string()))?;
        self.group = Some(group);
        Ok(wire)
    }

    /// Enact a departure: remove `subject`'s leaf and merge. Loopback
    /// plumbing for P4's dormancy step — NOT yet slip-gated; the removal
    /// enactment joins the slip discipline with the eviction machinery
    /// (ADR-0003 consequences).
    ///
    /// # Errors
    /// [`KeyLayerError::Process`] when the subject holds no leaf or the
    /// commit fails.
    pub fn enact_departure(&mut self, subject: &PrincipalId) -> Result<(), KeyLayerError> {
        let group = self
            .group
            .as_mut()
            .ok_or_else(|| KeyLayerError::Process("no live group".to_string()))?;
        let leaf = group
            .members()
            .find(|m| credential_principal(&m.credential).as_ref() == Some(subject))
            .map(|m| m.index)
            .ok_or_else(|| KeyLayerError::Process("subject holds no leaf".to_string()))?;
        group
            .remove_members(&self.persona.provider, &self.persona.signer, &[leaf])
            .map_err(|e| KeyLayerError::Process(e.to_string()))?;
        group
            .merge_pending_commit(&self.persona.provider)
            .map_err(|e| KeyLayerError::Process(e.to_string()))?;
        Ok(())
    }

    /// Seal an application message to the group (real AEAD).
    ///
    /// # Errors
    /// [`KeyLayerError::Process`] when the library refuses (e.g. evicted).
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, KeyLayerError> {
        let (provider, signer) = (&self.persona.provider, &self.persona.signer);
        let out = self
            .group
            .as_mut()
            .ok_or_else(|| KeyLayerError::Process("no live group".to_string()))?
            .create_message(provider, signer, plaintext)
            .map_err(|e| KeyLayerError::Process(e.to_string()))?;
        out.tls_serialize_detached()
            .map_err(|e| KeyLayerError::Process(e.to_string()))
    }

    /// Open a sealed application message. Every failure names where it
    /// failed (the S7 lesson).
    ///
    /// # Errors
    /// [`KeyLayerError::Parse`] for non-MLS bytes;
    /// [`KeyLayerError::Process`] when the group refuses or the content is
    /// not an application message.
    pub fn open(&mut self, wire: &[u8]) -> Result<Vec<u8>, KeyLayerError> {
        let provider = &self.persona.provider;
        let msg = MlsMessageIn::tls_deserialize_exact(wire)
            .map_err(|e| KeyLayerError::Parse(e.to_string()))?;
        let protocol: ProtocolMessage = msg
            .try_into_protocol_message()
            .map_err(|e| KeyLayerError::Parse(e.to_string()))?;
        let processed = self
            .group
            .as_mut()
            .ok_or_else(|| KeyLayerError::Process("no live group".to_string()))?
            .process_message(provider, protocol)
            .map_err(|e| KeyLayerError::Process(e.to_string()))?;
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => Ok(app.into_bytes()),
            other => Err(KeyLayerError::Process(format!(
                "expected an application message, got {other:?}"
            ))),
        }
    }

    /// The live group's member count (a diagnostic, not a membership view —
    /// the fold owns membership).
    ///
    /// # Errors
    /// [`KeyLayerError::Process`] when there is no live group.
    pub fn member_count(&self) -> Result<usize, KeyLayerError> {
        Ok(self
            .group
            .as_ref()
            .ok_or_else(|| KeyLayerError::Process("no live group".to_string()))?
            .members()
            .count())
    }
}

impl<P: openmls_traits::OpenMlsProvider + store::Bookkeeping> KeyLayer for OpenMlsKeyLayer<P> {
    fn stage_commit(&mut self, wire: &[u8]) -> Result<AdmissionClaims, KeyLayerError> {
        let content_address = *blake3::hash(wire).as_bytes();
        let provider = &self.persona.provider;
        let msg = MlsMessageIn::tls_deserialize_exact(wire)
            .map_err(|e| KeyLayerError::Parse(e.to_string()))?;
        let protocol: ProtocolMessage = msg
            .try_into_protocol_message()
            .map_err(|e| KeyLayerError::Parse(e.to_string()))?;
        let group = self
            .group
            .as_mut()
            .ok_or_else(|| KeyLayerError::Process("no live group".to_string()))?;
        let epoch = group.epoch().as_u64();
        let processed = group
            .process_message(provider, protocol)
            .map_err(|e| KeyLayerError::Process(e.to_string()))?;

        let joiner_lineage = credential_principal(processed.credential()).ok_or_else(|| {
            KeyLayerError::Process("the joiner's credential is not a 32-byte principal".to_string())
        })?;

        let ProcessedMessageContent::StagedCommitMessage(staged) = processed.into_content() else {
            return Err(KeyLayerError::Process("not a staged commit".to_string()));
        };

        // The countable psk_proposals() entry (S23): the presented token.
        // The proposal type exposes no public getter, but it is a TLS wire
        // type wrapping exactly one PreSharedKeyId — serialize and re-parse.
        let presented_token = staged
            .psk_proposals()
            .find_map(|qp| {
                let bytes = qp.psk_proposal().tls_serialize_detached().ok()?;
                let psk_id = PreSharedKeyId::tls_deserialize_exact(&bytes).ok()?;
                match psk_id.psk() {
                    Psk::External(ext) => {
                        let id: [u8; 32] = ext.psk_id().try_into().ok()?;
                        Some(TokenId::new(id))
                    }
                    Psk::Resumption(_) => None,
                }
            })
            .ok_or_else(|| {
                KeyLayerError::Process("no external-PSK proposal rides the commit".to_string())
            })?;

        let claims = AdmissionClaims {
            joiner_lineage,
            presented_token,
            commit_content_address: social_tree_core::model::Hash::new(content_address),
            commit_position: epoch,
        };
        self.staged.insert(content_address, claims);
        self.staged_commits.insert(content_address, *staged);
        Ok(claims)
    }

    fn merge_admission(&mut self, approval: MergeApproval) -> Result<MergedEpoch, KeyLayerError> {
        let fact = *approval.fact();
        let key = *fact.event.as_bytes();
        let staged = self
            .staged_commits
            .remove(&key)
            .ok_or(KeyLayerError::UnknownCommit(fact.event))?;
        self.staged.remove(&key);
        let provider = &self.persona.provider;
        let group = self
            .group
            .as_mut()
            .ok_or_else(|| KeyLayerError::Process("no live group".to_string()))?;
        group
            .merge_staged_commit(provider, staged)
            .map_err(|e| KeyLayerError::Process(e.to_string()))?;
        Ok(MergedEpoch {
            epoch: group.epoch().as_u64(),
        })
    }

    fn add_with_welcome(
        &mut self,
        approval: InviteApproval,
    ) -> Result<InviteArtifacts, KeyLayerError> {
        let invitee = *approval.invitee();
        let kp = self.pending_key_packages.remove(&invitee).ok_or_else(|| {
            KeyLayerError::Process(format!("no KeyPackage deposited for {invitee:?}"))
        })?;
        let (provider, signer) = (&self.persona.provider, &self.persona.signer);
        let group = self
            .group
            .as_mut()
            .ok_or_else(|| KeyLayerError::Process("no live group".to_string()))?;
        let (commit, welcome, _gi) = group
            .add_members(provider, signer, &[kp])
            .map_err(|e| KeyLayerError::Process(e.to_string()))?;
        group
            .merge_pending_commit(provider)
            .map_err(|e| KeyLayerError::Process(e.to_string()))?;
        Ok(InviteArtifacts {
            commit_wire: commit
                .tls_serialize_detached()
                .map_err(|e| KeyLayerError::Process(e.to_string()))?,
            welcome: welcome
                .tls_serialize_detached()
                .map_err(|e| KeyLayerError::Process(e.to_string()))?,
        })
    }
}

// ---------------------------------------------------------------------------
// The persistent key layer (P7 S2)
// ---------------------------------------------------------------------------

impl OpenMlsKeyLayer<store::PersistentProvider> {
    /// A key layer whose MLS state lives at `path` and survives the process.
    ///
    /// The store is **per identity**, not per device: redb holds an exclusive
    /// lock, and two personas sharing one file would be both a lock fight and a
    /// cross-persona leak of exactly the material that must not leak.
    ///
    /// # Errors
    /// [`KeyLayerError::Process`] when the store refuses — a version it does
    /// not speak, or a lock another handle already holds. Refusing here rather
    /// than opening empty is deliberate: an empty-looking store is
    /// indistinguishable from a fresh install, and a "fresh install" that is
    /// really a locked-out one would let someone start a second group under an
    /// identity that already has one.
    pub fn persistent(
        principal: PrincipalId,
        path: &std::path::Path,
    ) -> Result<Self, KeyLayerError> {
        let provider = store::PersistentProvider::open(path)
            .map_err(|e| KeyLayerError::Process(e.to_string()))?;

        // A signer already in the store is this identity's signer, and minting
        // a second would silently make the reloaded group unopenable by its own
        // member. Only a genuinely fresh store gets a new one.
        let persona = Persona::over(provider, &principal);
        let mut layer = Self::over(persona, principal);
        layer.restore_pending_key_packages()?;
        Ok(layer)
    }

    /// Every group id this identity has kept.
    ///
    /// # Errors
    /// [`KeyLayerError::Process`] when the store refuses.
    pub fn stored_group_ids(&self) -> Result<Vec<Vec<u8>>, KeyLayerError> {
        self.persona
            .provider
            .stored_group_ids()
            .map_err(KeyLayerError::Process)
    }

    /// Load the single group this identity holds, if it kept one.
    ///
    /// openmls has no enumeration API, so this reads the id croft kept beside
    /// the MLS state and calls `MlsGroup::load` with it. Without that id the
    /// group is unreachable no matter how completely it was stored.
    ///
    /// # Errors
    /// [`KeyLayerError::Process`] when the store refuses or the group will not
    /// load.
    pub fn load_group(&mut self) -> Result<bool, KeyLayerError> {
        let Some(id_bytes) = self.stored_group_ids()?.into_iter().next() else {
            return Ok(false);
        };
        let group_id = GroupId::from_slice(&id_bytes);
        let loaded = MlsGroup::load(self.persona.provider.storage(), &group_id)
            .map_err(|e| KeyLayerError::Process(e.to_string()))?;
        self.group = loaded;
        Ok(self.group.is_some())
    }

    /// Bring back the key packages other people handed over.
    ///
    /// P0-3's provenance split, on the restore side: these came from someone
    /// else and cannot be regenerated here, so they are read back. `staged` and
    /// `staged_commits` are deliberately left empty — they replay from the
    /// governance log, and restoring them here would create a second source for
    /// a fact the fold already owns.
    fn restore_pending_key_packages(&mut self) -> Result<(), KeyLayerError> {
        for (principal_bytes, wire) in self
            .persona
            .provider
            .stored_key_packages()
            .map_err(KeyLayerError::Process)?
        {
            let kp_in = KeyPackageIn::tls_deserialize_exact(&wire)
                .map_err(|e| KeyLayerError::Parse(e.to_string()))?;
            let kp = kp_in
                .validate(self.persona.provider.crypto(), ProtocolVersion::Mls10)
                .map_err(|e| KeyLayerError::Parse(e.to_string()))?;
            self.pending_key_packages
                .insert(PrincipalId::new(principal_bytes), kp);
        }
        Ok(())
    }
}
