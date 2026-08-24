//! The typed charter (O6 + O13, E117 P2): every governance dial the core knows
//! is **data in the fold**, never a compile-time constant — `GroupRules` is the
//! socket the E111 profile sheet plugs into, and this module is the sheet's
//! executable half. A core that baked a dial in as an assumption would make the
//! conformance declaration a fiction.
//!
//! The named constructors are the product postures the owner sketched (E121):
//! today only the dials the core actually carries (the five thresholds); the
//! §11.7 admission dials (door, issuance, lifetime, serve posture) join when
//! Phase 4 lands them as mechanism.

use crate::model::GroupRules;

/// A group charter as minted at genesis: the quorum dials, as data.
#[derive(Debug, Clone, PartialEq)]
pub struct Charter {
    /// The five governance thresholds this charter mints.
    pub rules: GroupRules,
}

impl Charter {
    /// The Croft reference posture — E121's **default mode**, the close-circle
    /// charter: anyone invites (add 1), **two to ban** ("so no one gets
    /// accidentally banned"), role and rule changes owner-gated by role but
    /// single-signature (1), and resolution at the owner's default **2** —
    /// never silently single-author.
    #[must_use]
    pub fn croft_default() -> Self {
        Charter {
            rules: GroupRules {
                add_member_threshold: 1,
                remove_member_threshold: 2,
                role_change_threshold: 1,
                rule_change_threshold: 1,
                resolution_threshold: 2,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn croft_default_is_the_e121_close_circle_posture() {
        let c = Charter::croft_default();
        assert_eq!(c.rules.add_member_threshold, 1, "anyone invites");
        assert_eq!(c.rules.remove_member_threshold, 2, "two to ban");
        assert_eq!(c.rules.resolution_threshold, 2, "no one-signature verdicts");
    }
}
