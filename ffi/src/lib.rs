//! D1 SPIKE — Phase 0 discovery, NOT production code.
//!
//! The question this answers: does a croft core type cross the uniffi boundary,
//! and does a *refusal* cross it as a typed error rather than a swallowed null?
//! The second half matters more than the first — an FFI layer that only proves
//! happy paths is where fail-loud goes to die, and the plan's S0 test specifics
//! call for the refusal paths explicitly.
//!
//! Disposition `promote`: S0 builds the real surface on this scaffold under TDD.

use social_tree_core::model::PrincipalId;

uniffi::setup_scaffolding!();

/// What can go wrong crossing the boundary. Typed, so Kotlin sees a real
/// exception rather than a null or a sentinel.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    /// A principal is exactly 32 bytes; anything else is refused with its length.
    #[error("a principal is 32 bytes, got {got}")]
    BadPrincipalLength {
        /// The length actually supplied.
        got: u32,
    },
}

/// Render a 32-byte principal as its canonical lowercase hex.
///
/// The round-trip under test: Kotlin hands over bytes, the core's own
/// `PrincipalId` is constructed and displayed, the string comes back.
#[uniffi::export]
pub fn principal_hex(bytes: Vec<u8>) -> Result<String, FfiError> {
    let raw: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| FfiError::BadPrincipalLength {
            got: bytes.len() as u32,
        })?;
    Ok(PrincipalId::new(raw).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_32_byte_principal_renders_as_hex() {
        let bytes = vec![0xab; 32];
        assert_eq!(principal_hex(bytes).unwrap(), "ab".repeat(32));
    }

    #[test]
    fn a_wrong_length_principal_is_refused_with_its_length() {
        // The edges, not one point: short, long, and empty all refuse.
        for len in [0usize, 31, 33] {
            let err = principal_hex(vec![0u8; len]).unwrap_err();
            match err {
                FfiError::BadPrincipalLength { got } => assert_eq!(got, len as u32),
            }
        }
    }
}
