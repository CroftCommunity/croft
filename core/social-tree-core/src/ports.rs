//! Injected-trait boundary for `local_storage_projection`.
//!
//! This module defines every capability that the crate consumes from the outside
//! world via dependency injection.  Nothing here touches I/O, storage, or any
//! concrete cryptographic library; all of that lives in the types that *implement*
//! these traits.

use thiserror::Error;

// ---------------------------------------------------------------------------
// Newtypes
// ---------------------------------------------------------------------------

/// A 32-byte content-addressed hash (Blake3 or similar; full semantics in Stage 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash(pub [u8; 32]);

/// A 32-byte opaque device identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub [u8; 32]);

/// A 32-byte opaque principal (user/service) identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrincipalId(pub [u8; 32]);

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors returned by [`Verifier::verify`].
#[derive(Debug, Error)]
pub enum VerifyError {
    /// The signature did not match the message for the given device.
    #[error("signature is invalid for device {device_id:?}")]
    InvalidSignature { device_id: DeviceId },

    /// The device is not known to the verifier.
    #[error("unknown device {0:?}")]
    UnknownDevice(DeviceId),

    /// Any other, implementation-specific verification failure.
    #[error("verification failed: {0}")]
    Other(String),
}

/// Errors returned by [`CredentialResolver::resolve`].
#[derive(Debug, Error)]
pub enum CredentialError {
    /// The (device, principal) pair is not registered.
    #[error("credential not found for device {device:?} / principal {principal:?}")]
    NotFound {
        device: DeviceId,
        principal: PrincipalId,
    },

    /// The credential exists but has been revoked.
    #[error("credential revoked for device {device:?} / principal {principal:?}")]
    Revoked {
        device: DeviceId,
        principal: PrincipalId,
    },

    /// Any other, implementation-specific resolution failure.
    #[error("credential resolution failed: {0}")]
    Other(String),
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Verifies that a `signature` over `message` is valid for `device_id`.
///
/// Implementations are expected to be cheap to clone and `Send + Sync`.
pub trait Verifier: Send + Sync {
    fn verify(
        &self,
        device_id: &DeviceId,
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), VerifyError>;
}

/// Produces signatures and exposes the signing device's identity.
///
/// Callers treat the returned `Vec<u8>` as an opaque byte string; the
/// paired [`Verifier`] is responsible for interpreting the encoding.
pub trait Signer: Send + Sync {
    /// Sign `message` and return the raw signature bytes.
    fn sign(&self, message: &[u8]) -> Vec<u8>;

    /// Return the [`DeviceId`] that corresponds to the signing key.
    fn device_id(&self) -> DeviceId;
}

/// Checks whether a (device, principal) credential pair is currently valid.
///
/// This is intentionally pass/fail; richer policy information belongs in a
/// higher-level authorization layer.
pub trait CredentialResolver: Send + Sync {
    fn resolve(&self, device: &DeviceId, principal: &PrincipalId) -> Result<(), CredentialError>;
}

/// Provides a monotonically increasing Lamport clock value.
///
/// Implementations MUST guarantee that two successive calls return distinct
/// values, even across threads.
pub trait LamportSource: Send + Sync {
    fn next_lamport(&self) -> u64;
}

/// Answers whether a content-addressed blob is locally available.
pub trait BlobPresence: Send + Sync {
    fn is_present(&self, hash: &Hash) -> bool;
}

// ---------------------------------------------------------------------------
// Mock implementations
// ---------------------------------------------------------------------------

#[cfg(any(test, feature = "test-mocks"))]
pub mod mocks {
    use super::{
        BlobPresence, CredentialError, CredentialResolver, DeviceId, Hash, LamportSource,
        PrincipalId, Signer, Verifier, VerifyError,
    };
    use std::collections::{HashMap, HashSet};
    use std::sync::atomic::{AtomicU64, Ordering};

    // -----------------------------------------------------------------------
    // MockSigner / MockVerifier
    // -----------------------------------------------------------------------

    /// A deterministic signer backed by a fixed 32-byte key.
    ///
    /// Signing is performed by XOR-folding all message bytes into a 64-byte
    /// output that depends on both the key and the message, making it
    /// reproducible but distinct per (key, message) pair.
    ///
    /// `MockSigner` also implements [`Verifier`]: it re-signs the message with
    /// the same key and compares the result byte-for-byte, so it only accepts
    /// signatures it produced itself.
    #[derive(Clone)]
    pub struct MockSigner {
        key: [u8; 32],
    }

    impl MockSigner {
        /// Create a `MockSigner` from an explicit 32-byte key.
        pub fn new(key: [u8; 32]) -> Self {
            Self { key }
        }

        /// Create a `MockSigner` from a short seed (the seed is XOR-extended
        /// to fill 32 bytes for convenience in tests).
        pub fn from_seed(seed: u8) -> Self {
            Self { key: [seed; 32] }
        }

        fn compute_signature(&self, message: &[u8]) -> Vec<u8> {
            // Build a deterministic 64-byte signature.
            // The first 32 bytes are derived from the key XOR'd with message
            // bytes (wrapping); the second 32 bytes invert the first.
            let mut sig = [0u8; 64];
            for (i, b) in sig[..32].iter_mut().enumerate() {
                let msg_byte = if message.is_empty() {
                    0u8
                } else {
                    message[i % message.len()]
                };
                *b = self.key[i] ^ msg_byte ^ (i as u8);
            }
            for i in 0..32 {
                sig[32 + i] = !sig[i];
            }
            sig.to_vec()
        }
    }

    impl Signer for MockSigner {
        fn sign(&self, message: &[u8]) -> Vec<u8> {
            self.compute_signature(message)
        }

        fn device_id(&self) -> DeviceId {
            DeviceId(self.key)
        }
    }

    impl Verifier for MockSigner {
        fn verify(
            &self,
            device_id: &DeviceId,
            message: &[u8],
            signature: &[u8],
        ) -> Result<(), VerifyError> {
            if device_id.0 != self.key {
                return Err(VerifyError::UnknownDevice(*device_id));
            }
            let expected = self.compute_signature(message);
            if signature != expected.as_slice() {
                return Err(VerifyError::InvalidSignature {
                    device_id: *device_id,
                });
            }
            Ok(())
        }
    }

    // -----------------------------------------------------------------------
    // MockCredentialResolver
    // -----------------------------------------------------------------------

    /// A credential resolver backed by an in-memory allow-list.
    ///
    /// Pairs inserted with `register` return `Ok(())`; all others return
    /// [`CredentialError::NotFound`].
    #[derive(Clone)]
    pub struct MockCredentialResolver {
        allowed: HashMap<(DeviceId, PrincipalId), bool>,
    }

    impl MockCredentialResolver {
        pub fn new() -> Self {
            Self {
                allowed: HashMap::new(),
            }
        }

        /// Register a (device, principal) pair as valid.
        pub fn register(&mut self, device: DeviceId, principal: PrincipalId) {
            self.allowed.insert((device, principal), true);
        }
    }

    impl Default for MockCredentialResolver {
        fn default() -> Self {
            Self::new()
        }
    }

    impl CredentialResolver for MockCredentialResolver {
        fn resolve(
            &self,
            device: &DeviceId,
            principal: &PrincipalId,
        ) -> Result<(), CredentialError> {
            if self.allowed.contains_key(&(*device, *principal)) {
                Ok(())
            } else {
                Err(CredentialError::NotFound {
                    device: *device,
                    principal: *principal,
                })
            }
        }
    }

    // -----------------------------------------------------------------------
    // MockLamportSource
    // -----------------------------------------------------------------------

    /// A Lamport clock that starts at 1 and increments atomically on each call.
    pub struct MockLamportSource {
        counter: AtomicU64,
    }

    impl Clone for MockLamportSource {
        fn clone(&self) -> Self {
            Self {
                counter: AtomicU64::new(self.counter.load(Ordering::SeqCst)),
            }
        }
    }

    impl MockLamportSource {
        pub fn new() -> Self {
            Self {
                counter: AtomicU64::new(1),
            }
        }
    }

    impl Default for MockLamportSource {
        fn default() -> Self {
            Self::new()
        }
    }

    impl LamportSource for MockLamportSource {
        fn next_lamport(&self) -> u64 {
            self.counter.fetch_add(1, Ordering::SeqCst)
        }
    }

    // -----------------------------------------------------------------------
    // MockBlobPresence
    // -----------------------------------------------------------------------

    /// A blob-presence oracle backed by an in-memory `HashSet`.
    pub struct MockBlobPresence {
        present: HashSet<Hash>,
    }

    impl MockBlobPresence {
        pub fn new() -> Self {
            Self {
                present: HashSet::new(),
            }
        }

        /// Mark `hash` as present.
        pub fn insert(&mut self, hash: Hash) {
            self.present.insert(hash);
        }
    }

    impl Default for MockBlobPresence {
        fn default() -> Self {
            Self::new()
        }
    }

    impl BlobPresence for MockBlobPresence {
        fn is_present(&self, hash: &Hash) -> bool {
            self.present.contains(hash)
        }
    }

    // -----------------------------------------------------------------------
    // Unit tests for the mocks themselves
    // -----------------------------------------------------------------------

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn mock_signer_round_trip() {
            let signer = MockSigner::from_seed(0xAB);
            let msg = b"hello world";
            let sig = signer.sign(msg);
            let dev = signer.device_id();
            assert!(signer.verify(&dev, msg, &sig).is_ok());
        }

        #[test]
        fn mock_signer_wrong_message_fails() {
            let signer = MockSigner::from_seed(0x01);
            let sig = signer.sign(b"correct");
            let dev = signer.device_id();
            assert!(signer.verify(&dev, b"wrong", &sig).is_err());
        }

        #[test]
        fn mock_signer_unknown_device_fails() {
            let signer = MockSigner::from_seed(0x01);
            let other = MockSigner::from_seed(0x02);
            let sig = signer.sign(b"msg");
            let other_dev = other.device_id();
            assert!(signer.verify(&other_dev, b"msg", &sig).is_err());
        }

        #[test]
        fn mock_credential_resolver_allow_deny() {
            let mut resolver = MockCredentialResolver::new();
            let dev = DeviceId([1u8; 32]);
            let prin = PrincipalId([2u8; 32]);
            assert!(resolver.resolve(&dev, &prin).is_err());
            resolver.register(dev, prin);
            assert!(resolver.resolve(&dev, &prin).is_ok());
        }

        #[test]
        fn mock_lamport_monotonic() {
            let src = MockLamportSource::new();
            let a = src.next_lamport();
            let b = src.next_lamport();
            let c = src.next_lamport();
            assert!(a < b && b < c);
            assert_eq!(a, 1);
        }

        #[test]
        fn mock_blob_presence() {
            let mut bp = MockBlobPresence::new();
            let h = Hash([0xFFu8; 32]);
            assert!(!bp.is_present(&h));
            bp.insert(h);
            assert!(bp.is_present(&h));
        }
    }
}
