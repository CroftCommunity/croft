//! Assertion payload encoders.
//!
//! Until now these existed **only as `#[cfg(test)]` helpers** in the corpus's
//! suites. The wire format had a production reader and a test-only writer, so
//! nothing outside a test could legally author an assertion — which is why the
//! first thing a shell needed was the half that was missing.
//!
//! `MembershipAdd` is decoded by [`crate::fold_derived`], next door.
//! `GroupGenesis` is decoded by `social_tree_core::update` (privately), and
//! `Message` has both halves in `social_tree_core::model` — which is the shape
//! all three should have. Because two of these encoders sit in a different
//! crate from their decoder, a codec round-trip inside this module would prove
//! only that the module agrees with itself. The pins therefore ingest what
//! these functions produce **through the real fold** and assert the state it
//! derived, which is the check that fails when the two halves disagree.

use social_tree_core::model::{DeviceId, PrincipalId, Role};

/// Encode a `GroupGenesis` payload.
///
/// Layout: `policy_version(2) || add(4) || remove(4) || role_change(4) ||
/// rule_change(4) || founding_device(32)` = 50 bytes. The reader
/// (`genesis_initial_state`) refuses anything shorter, and mints
/// `resolution_threshold` and `readmission_threshold` itself rather than
/// reading them from here — they are product defaults, dialed afterwards by
/// governed `RuleChange` like every other threshold.
#[must_use]
pub fn encode_genesis_payload(rules: GenesisRules, founding_device: &DeviceId) -> Vec<u8> {
    let mut p = Vec::with_capacity(50);
    p.extend_from_slice(&1u16.to_be_bytes()); // policy_version
    p.extend_from_slice(&rules.add_member.to_be_bytes());
    p.extend_from_slice(&rules.remove_member.to_be_bytes());
    p.extend_from_slice(&rules.role_change.to_be_bytes());
    p.extend_from_slice(&rules.rule_change.to_be_bytes());
    p.extend_from_slice(founding_device.as_bytes());
    p
}

/// The four thresholds a genesis payload carries.
///
/// A struct rather than four bare `u32` arguments because they are all the same
/// type and adjacent: transposing two of them compiles, passes every type
/// check, and silently mints a group whose rules are not the ones asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenesisRules {
    /// Approvals required to add a member.
    pub add_member: u32,
    /// Approvals required to remove a member.
    pub remove_member: u32,
    /// Approvals required to change a role.
    pub role_change: u32,
    /// Approvals required to change a rule.
    pub rule_change: u32,
}

impl GenesisRules {
    /// A group founded by one person: the founder's own signature carries every
    /// governance act until they seat someone else.
    pub const SOLO_FOUNDER: Self = Self {
        add_member: 1,
        remove_member: 1,
        role_change: 1,
        rule_change: 1,
    };
}

/// Encode a `MembershipAdd` payload.
///
/// Layout: `principal(32) || role(1)` = 33 bytes; the fold refuses anything
/// shorter.
#[must_use]
pub fn encode_membership_add_payload(principal: &PrincipalId, role: Role) -> Vec<u8> {
    let mut p = Vec::with_capacity(33);
    p.extend_from_slice(principal.as_bytes());
    p.push(role_byte(role));
    p
}

/// The wire byte for a role.
///
/// Written as a total `match` rather than a cast so that adding a role to the
/// core makes this fail to compile instead of silently encoding the wrong
/// number.
#[must_use]
fn role_byte(role: Role) -> u8 {
    match role {
        Role::Owner => 0,
        Role::Admin => 1,
        Role::Member => 2,
        Role::Observer => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_genesis_payload_is_the_fifty_bytes_the_reader_requires() {
        let p = encode_genesis_payload(GenesisRules::SOLO_FOUNDER, &DeviceId::new([0x11; 32]));
        assert_eq!(p.len(), 50, "the reader refuses anything shorter than 50");
    }

    #[test]
    fn a_membership_add_payload_is_the_thirty_three_bytes_the_fold_requires() {
        let p = encode_membership_add_payload(&PrincipalId::new([0x22; 32]), Role::Owner);
        assert_eq!(p.len(), 33);
        assert_eq!(&p[..32], &[0x22u8; 32]);
        assert_eq!(p[32], 0, "Owner is role byte 0");
    }

    #[test]
    fn every_role_encodes_to_a_distinct_byte() {
        // Distinctness, not a table of expected values: a test that restates
        // the mapping is a copy of the implementation and cannot disagree
        // with it. What matters is that no two roles collide.
        let bytes: Vec<u8> = [Role::Owner, Role::Admin, Role::Member, Role::Observer]
            .into_iter()
            .map(role_byte)
            .collect();
        let mut sorted = bytes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), bytes.len(), "two roles share a wire byte");
    }
}
