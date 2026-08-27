//! The MLS-side identity: one persona = one crypto provider + signer +
//! credential, adapted from `mls_replant::Persona` (the Rung-A lineage).
//! One deliberate change from the ancestor: the leaf credential's identity
//! bytes ARE the core's 32-byte `PrincipalId` — that is the bridge
//! `stage_commit` reads a joiner's lineage claim from, so nothing about
//! identity arrives out-of-band.

use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use social_tree_core::model::PrincipalId;

/// The ciphersuite the S-series measured (unchanged from the experiments).
pub const CS: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// A persona with its own crypto provider, signer, and credential — private
/// state held as a separate device would hold it.
///
/// Generic over the provider, defaulting to the in-memory one. The default is
/// what keeps every existing caller and the loopback suite unchanged: this is
/// a widening, not a migration. A product shell passes
/// [`crate::store::PersistentProvider`] instead, and that is the only
/// difference between state that survives the app closing and state that does
/// not.
pub struct Persona<P: openmls_traits::OpenMlsProvider = OpenMlsRustCrypto> {
    /// The openmls provider (crypto + storage, including the PSK store).
    pub provider: P,
    /// The leaf signature keypair.
    pub signer: SignatureKeyPair,
    /// The credential whose identity bytes are the principal id.
    pub cwk: CredentialWithKey,
}

impl Persona<OpenMlsRustCrypto> {
    /// A fresh persona whose credential identity is `principal`'s bytes,
    /// holding its MLS state in memory.
    #[must_use]
    pub fn new(principal: &PrincipalId) -> Self {
        Self::over(OpenMlsRustCrypto::default(), principal)
    }
}

impl<P: openmls_traits::OpenMlsProvider> Persona<P> {
    /// A persona over an already-built provider, **reusing** a signer the
    /// provider already holds.
    ///
    /// The reuse is the load-bearing part. Minting a fresh signer for an
    /// identity that already has one looks like it works — the group reloads,
    /// seals succeed — and every other member rejects the signature, because
    /// the new key is not the one in this member's leaf node. The failure has
    /// no local symptom at all: it appears on someone else's device, as a
    /// message that will not open.
    #[must_use]
    pub fn over(provider: P, principal: &PrincipalId) -> Self
    where
        P: crate::store::Bookkeeping,
    {
        let signer = restore_or_mint_signer(&provider);
        let credential = BasicCredential::new(principal.as_bytes().to_vec());
        let cwk = CredentialWithKey {
            credential: credential.into(),
            signature_key: signer.public().into(),
        };
        Self {
            provider,
            signer,
            cwk,
        }
    }

    /// A fresh `KeyPackage` this persona can be added with.
    #[must_use]
    pub fn key_package(&self) -> KeyPackage {
        KeyPackage::builder()
            .build(CS, &self.provider, &self.signer, self.cwk.clone())
            .expect("key package")
            .key_package()
            .clone()
    }
}

/// The signer this provider already holds, or a fresh one it will hold from
/// now on.
///
/// A provider that remembers nothing (the in-memory one) always takes the
/// second branch, which is correct for it: nothing it stored would outlive the
/// process anyway.
fn restore_or_mint_signer<P>(provider: &P) -> SignatureKeyPair
where
    P: openmls_traits::OpenMlsProvider + crate::store::Bookkeeping,
{
    if let Ok(Some(public)) = provider.stored_signature_public_key() {
        if let Ok(existing) =
            SignatureKeyPair::read(provider.storage(), &public, CS.signature_algorithm()).ok_or(())
        {
            return existing;
        }
    }
    let signer = SignatureKeyPair::new(CS.signature_algorithm()).expect("signer");
    signer.store(provider.storage()).expect("store signer");
    let _ = provider.remember_signature_public_key(signer.public());
    signer
}

/// The principal a credential's identity bytes resolve to, if they are the
/// 32-byte shape this adapter mints. `None` for foreign credential shapes —
/// the caller refuses loudly.
#[must_use]
pub fn credential_principal(credential: &Credential) -> Option<PrincipalId> {
    let bytes: [u8; 32] = credential.serialized_content().try_into().ok()?;
    Some(PrincipalId::new(bytes))
}
