//! Ed25519 realizations of the port contracts — REAL crypto, relocated into the
//! core at E117 P3 (the fold's authorship rung: the evidence artifacts ran on
//! stand-ins while this real path lived one crate over in `social-graph-core`;
//! now there is one). Deterministic construction only — no OS randomness — so
//! it is wasm-clean and the crate's no-clock/no-I/O contract holds. Secret
//! material: `SigningKey` zeroizes on drop (ed25519-dalek's default `zeroize`
//! feature); this module adds no `Debug`/`Clone` on key-holding types.
//!
//! The four contracts:
//! - [`Ed25519Signer`] — signs with an ed25519 key (`Signer`).
//! - [`Ed25519Verifier`] — stateless; reconstructs the verifying key from the
//!   `DeviceId` (which *is* the public key) and verifies any device (`Verifier`).
//! - [`RegistryCredentialResolver`] — a device→principal registry
//!   (`CredentialResolver`).
//! - [`MonotonicLamport`] — an atomic, strictly-increasing counter
//!   (`LamportSource`), seedable for restart-resume (P12).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use ed25519_dalek::{Signature, Signer as DalekSigner, SigningKey, VerifyingKey};

use super::{
    CredentialError, CredentialResolver, DeviceId, LamportSource, PrincipalId, Signer, Verifier,
    VerifyError,
};

/// Signs assertions with an identity's ed25519 key.
///
/// The substrate passes a `&impl Signer` per write command (it is not stored),
/// so this type is not `Clone`. It holds secret key material and is therefore
/// not `Debug`.
pub struct Ed25519Signer {
    signing_key: SigningKey,
}

impl Ed25519Signer {
    /// Build a signer from a raw ed25519 signing key.
    #[must_use]
    pub fn from_signing_key(signing_key: SigningKey) -> Self {
        Self { signing_key }
    }

    /// Deterministic signer from a 32-byte seed — the test-ergonomics twin of
    /// the mock signer's `from_seed`, on real crypto.
    #[must_use]
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(&seed),
        }
    }
}

impl Signer for Ed25519Signer {
    fn sign(&self, message: &[u8]) -> Vec<u8> {
        DalekSigner::sign(&self.signing_key, message)
            .to_bytes()
            .to_vec()
    }

    fn device_id(&self) -> DeviceId {
        DeviceId(self.signing_key.verifying_key().to_bytes())
    }
}

/// Verifies ed25519 signatures for any device.
///
/// Stateless: the `DeviceId` is the verifying-key bytes, so the public key is
/// reconstructed per call. One instance verifies every device in a group.
#[derive(Clone, Copy, Default)]
pub struct Ed25519Verifier;

impl Verifier for Ed25519Verifier {
    fn verify(
        &self,
        device_id: &DeviceId,
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), VerifyError> {
        let verifying_key = VerifyingKey::from_bytes(&device_id.0)
            .map_err(|_| VerifyError::UnknownDevice(*device_id))?;
        let sig_bytes: [u8; 64] = signature
            .try_into()
            .map_err(|_| VerifyError::Other("signature must be 64 bytes".to_string()))?;
        let signature = Signature::from_bytes(&sig_bytes);
        verifying_key
            .verify_strict(message, &signature)
            .map_err(|_| VerifyError::InvalidSignature {
                device_id: *device_id,
            })
    }
}

/// Device pubkey bytes → the set of principal ids registered for that device.
type DeviceRegistry = Arc<RwLock<HashMap<[u8; 32], HashSet<[u8; 32]>>>>;

/// A device→principal credential registry.
///
/// `resolve` succeeds only for a registered (device, principal) pair. Cheap to
/// clone (shares the registry via `Arc`), as the substrate's `LocalStore`
/// requires `Clone`.
#[derive(Clone, Default)]
pub struct RegistryCredentialResolver {
    registry: DeviceRegistry,
}

impl RegistryCredentialResolver {
    /// Create an empty resolver.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register that `device` may act for `principal`.
    pub fn register(&self, device: DeviceId, principal: PrincipalId) {
        // A poisoned lock means another thread panicked mid-write; recover the
        // guard rather than propagating the panic into the substrate.
        let mut guard = self
            .registry
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.entry(device.0).or_default().insert(principal.0);
    }
}

impl CredentialResolver for RegistryCredentialResolver {
    fn resolve(&self, device: &DeviceId, principal: &PrincipalId) -> Result<(), CredentialError> {
        let guard = self
            .registry
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match guard.get(&device.0) {
            Some(principals) if principals.contains(&principal.0) => Ok(()),
            _ => Err(CredentialError::NotFound {
                device: *device,
                principal: *principal,
            }),
        }
    }
}

/// A strictly-increasing Lamport counter.
///
/// Clones share the same atomic counter (so one store's writes stay
/// consistent). Seedable via [`MonotonicLamport::starting_at`] for restart
/// resume (P12), and snapshot-able via [`MonotonicLamport::peek`] to persist.
#[derive(Clone)]
pub struct MonotonicLamport {
    counter: Arc<AtomicU64>,
}

impl MonotonicLamport {
    /// A fresh source whose first issued value is 1.
    #[must_use]
    pub fn new() -> Self {
        Self::starting_at(1)
    }

    /// A source whose first issued value is `start` (e.g. `last_persisted + 1`).
    #[must_use]
    pub fn starting_at(start: u64) -> Self {
        Self {
            counter: Arc::new(AtomicU64::new(start)),
        }
    }

    /// The value the *next* call to `next_lamport` would issue, without
    /// consuming it. Used to persist the counter (P12).
    #[must_use]
    pub fn peek(&self) -> u64 {
        self.counter.load(Ordering::SeqCst)
    }
}

impl Default for MonotonicLamport {
    fn default() -> Self {
        Self::new()
    }
}

impl LamportSource for MonotonicLamport {
    fn next_lamport(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_then_verify_round_trips() {
        let signer = Ed25519Signer::from_seed([0x11; 32]);
        let verifier = Ed25519Verifier;

        let message = b"converge or hard-stop";
        let signature = signer.sign(message);

        assert!(
            verifier
                .verify(&signer.device_id(), message, &signature)
                .is_ok(),
            "a real signature must verify"
        );
    }

    #[test]
    fn forged_signature_is_rejected() {
        // Sign with one key, claim it came from another device.
        let real = Ed25519Signer::from_seed([0x22; 32]);
        let other = Ed25519Signer::from_seed([0x33; 32]);
        let verifier = Ed25519Verifier;

        let message = b"i am the owner";
        let signature = real.sign(message);

        assert!(
            verifier
                .verify(&other.device_id(), message, &signature)
                .is_err(),
            "a signature must not verify under a different device id"
        );
    }

    #[test]
    fn tampered_message_is_rejected() {
        // A valid signature over the original message must not verify against a
        // mutated message — a verifier that ignored the body would survive.
        let signer = Ed25519Signer::from_seed([0x44; 32]);
        let verifier = Ed25519Verifier;

        let signature = signer.sign(b"transfer 10");
        assert!(
            verifier
                .verify(&signer.device_id(), b"transfer 99", &signature)
                .is_err(),
            "signature must bind to the exact message bytes"
        );
    }

    #[test]
    fn malformed_signature_length_is_rejected() {
        let signer = Ed25519Signer::from_seed([0x45; 32]);
        let verifier = Ed25519Verifier;
        assert!(
            verifier
                .verify(&signer.device_id(), b"msg", &[0u8; 10])
                .is_err(),
            "a non-64-byte signature must be rejected, not panic"
        );
    }

    #[test]
    fn resolver_resolves_only_registered_pairs() {
        let signer = Ed25519Signer::from_seed([0x55; 32]);
        let device = signer.device_id();
        let principal = PrincipalId([0xA5; 32]);
        let resolver = RegistryCredentialResolver::new();

        // Unregistered -> NotFound.
        assert!(resolver.resolve(&device, &principal).is_err());

        resolver.register(device, principal);
        assert!(resolver.resolve(&device, &principal).is_ok());

        // A different principal for the same device is still rejected.
        let other_principal = PrincipalId([0xA6; 32]);
        assert!(resolver.resolve(&device, &other_principal).is_err());
    }

    #[test]
    fn lamport_is_strictly_increasing_and_shared_across_clones() {
        let source = MonotonicLamport::new();
        let clone = source.clone();
        let a = source.next_lamport();
        let b = clone.next_lamport(); // shares the same counter
        let c = source.next_lamport();
        assert!(a < b && b < c, "values must strictly increase: {a} {b} {c}");
        assert_eq!(a, 1, "first value is 1");
    }

    #[test]
    fn lamport_starting_at_resumes() {
        let source = MonotonicLamport::starting_at(42);
        assert_eq!(source.peek(), 42, "peek does not consume");
        assert_eq!(source.next_lamport(), 42);
        assert_eq!(source.next_lamport(), 43);
    }
}
