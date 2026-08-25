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
pub struct Persona {
    /// The openmls provider (crypto + storage, including the PSK store).
    pub provider: OpenMlsRustCrypto,
    /// The leaf signature keypair.
    pub signer: SignatureKeyPair,
    /// The credential whose identity bytes are the principal id.
    pub cwk: CredentialWithKey,
}

impl Persona {
    /// A fresh persona whose credential identity is `principal`'s bytes.
    #[must_use]
    pub fn new(principal: &PrincipalId) -> Self {
        let provider = OpenMlsRustCrypto::default();
        let signer = SignatureKeyPair::new(CS.signature_algorithm()).expect("signer");
        signer.store(provider.storage()).expect("store signer");
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

/// The principal a credential's identity bytes resolve to, if they are the
/// 32-byte shape this adapter mints. `None` for foreign credential shapes —
/// the caller refuses loudly.
#[must_use]
pub fn credential_principal(credential: &Credential) -> Option<PrincipalId> {
    let bytes: [u8; 32] = credential.serialized_content().try_into().ok()?;
    Some(PrincipalId::new(bytes))
}
