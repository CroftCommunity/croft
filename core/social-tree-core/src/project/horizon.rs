//! EXP-H1 (backlog §2b, RUN-07) — experiment-grade horizon-manifest determinism.
//!
//! This module implements the *minimal* horizon machinery as pure fold-side
//! functions: enough to establish the **manifest-determinism** claim and nothing
//! more. Two members that folded the same governance fact set produce a
//! **byte-identical** horizon manifest regardless of the arrival order of the
//! facts.
//!
//! Scope and non-claims (deliberate):
//!   * There is **no wire format, no persistence, and no networking** here. The
//!     `[gates-release]` horizon-checkpoint manifest encoding (Part 2 §7.6.9) is
//!     untouched and stays `Design`; this earns only the *manifest determinism*
//!     property, not the checkpoint's release-grade byte layout.
//!   * The one byte layout in this file — `frontier_digest` — is an **internal,
//!     experiment-grade** reduction used to name the frontier order-independently.
//!     It is explicitly **NOT** a wire/persistence format and **NOT** the
//!     `[gates-release]` manifest encoding. It exists only so the frontier of the
//!     manifest is a single order-independent value.
//!
//! Why a digest rather than `GroupState::computed_at_gov_head`: that field is the
//! hash of the **last-ingested** governance envelope, which is arrival-order
//! dependent (two members that fold the same set in opposite orders end on
//! different last facts). The order-independent content of the folded state is the
//! membership set, the rules, and the governance sequence — all of which converge
//! by construction (Part 2 §7.3.1/§7.3.2; and for a contradiction, the
//! `resolve_contradiction` replay is order-independent). The frontier digest hashes
//! exactly those, so it converges too.

use crate::model::{ForkStatus, GroupState};
use crate::model::{Hash, Role};

/// The deterministic horizon manifest: the order-independent governance frontier,
/// plus the sorted set of contradiction byte-heads currently open.
///
/// `PartialEq`/`Eq` compare the manifest structurally; the byte-identity the
/// experiment asserts is exercised in the test via a test-only serialization that
/// is explicitly commented as **not** the `[gates-release]` manifest encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HorizonManifest {
    /// An order-independent digest naming the governance frontier of the folded set
    /// (experiment-grade; see the module note — not a wire format).
    pub frontier_head: Hash,
    /// The byte-heads of every contradiction currently open, in ascending
    /// lexicographic order and deduplicated. A resolved or absent contradiction does
    /// not appear; an open one persists here across horizon boundaries (decay is a
    /// presentation concern, not a truth concern).
    pub open_contradictions: Vec<Hash>,
}

/// Map a `Role` to its canonical byte, matching `fold_derived::role_to_u8`. Kept
/// local so this module does not depend on that private helper.
fn role_byte(r: &Role) -> u8 {
    match r {
        Role::Owner => 0,
        Role::Admin => 1,
        Role::Member => 2,
        Role::Observer => 3,
    }
}

/// Order-independent digest of the folded state's frontier. Hashes only the parts of
/// the state that converge regardless of arrival order — the governance sequence, the
/// four rule thresholds, and the membership set (sorted by principal so the *set*, not
/// its stored order, is what is hashed). The last-ingested head and the fork label are
/// deliberately excluded (the head is order-dependent; the fork label is carried
/// separately as an open-contradiction byte-head).
///
/// NOTE: this byte layout is internal and experiment-grade. It is NOT a wire or
/// persistence format and NOT the `[gates-release]` horizon-manifest encoding.
fn frontier_digest(state: &GroupState) -> Hash {
    let mut members: Vec<&(crate::model::PrincipalId, Role, u64)> = state.members.iter().collect();
    members.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    let mut buf: Vec<u8> = Vec::with_capacity(8 + 16 + members.len() * 41);
    buf.extend_from_slice(&state.computed_at_gov_seq.to_be_bytes());
    buf.extend_from_slice(&state.rules.add_member_threshold.to_be_bytes());
    buf.extend_from_slice(&state.rules.remove_member_threshold.to_be_bytes());
    buf.extend_from_slice(&state.rules.role_change_threshold.to_be_bytes());
    buf.extend_from_slice(&state.rules.rule_change_threshold.to_be_bytes());
    for (pid, role, since) in members {
        buf.extend_from_slice(pid.as_bytes());
        buf.push(role_byte(role));
        buf.extend_from_slice(&since.to_be_bytes());
    }
    Hash::new(*blake3::hash(&buf).as_bytes())
}

/// The horizon manifest of a folded group state: `(frontier_head, sorted open
/// contradiction byte-heads)`. A **pure** function of the folded state, with fully
/// deterministic ordering — the same fact set yields the byte-identical manifest on
/// every honest member and under every arrival order.
///
/// Only `ForkStatus::Contested` contributes byte-heads: one per open pair — the
/// order-independent `min(H(F), H(G))`, which is exactly `pair.0` of the set-valued
/// entry (§7.3.2/E108), so the manifest semantics are unchanged while multiple
/// simultaneously open contradictions are now each named. A clean, forked-slot, or
/// under-determined state contributes no contradiction byte-head.
#[must_use]
pub fn horizon_manifest(state: &GroupState) -> HorizonManifest {
    let mut open_contradictions: Vec<Hash> = match &state.fork_status {
        ForkStatus::Contested(entries) => entries.iter().map(|e| e.pair.0).collect(),
        ForkStatus::Clean | ForkStatus::ForkedFrom(_) | ForkStatus::UnderDetermined => vec![],
    };
    open_contradictions.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    open_contradictions.dedup();
    HorizonManifest {
        frontier_head: frontier_digest(state),
        open_contradictions,
    }
}

/// A single event the reconciliation-horizon cadence observes (Part 2 §7.6.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizonEvent {
    /// One folded fact accumulated since the last horizon boundary.
    Fact,
    /// An epoch roll — the cadence's other trigger term. Fires a boundary immediately.
    EpochRoll,
}

/// The two cadence terms of the reconciliation-horizon trigger (Part 2 §7.6.9): a
/// horizon boundary fires on an **epoch-roll** event, and on **N facts** accumulated
/// since the last boundary, with the fact counter reset at every boundary. The
/// constant `facts_per_horizon` is seeded by the caller (in the test), standing in for
/// the genesis-seeded rule.
#[derive(Debug, Clone)]
pub struct HorizonCadence {
    facts_per_horizon: u64,
    facts_since_boundary: u64,
}

impl HorizonCadence {
    /// A cadence that fires every `facts_per_horizon` facts (in addition to every
    /// epoch roll). `facts_per_horizon` must be non-zero; a zero is clamped to one so
    /// the counter term always makes progress.
    #[must_use]
    pub fn new(facts_per_horizon: u64) -> Self {
        Self {
            facts_per_horizon: facts_per_horizon.max(1),
            facts_since_boundary: 0,
        }
    }

    /// Facts accumulated since the last horizon boundary (0 immediately after one).
    #[must_use]
    pub fn facts_since_boundary(&self) -> u64 {
        self.facts_since_boundary
    }

    /// Observe one event; return `true` iff it triggers a horizon boundary. A boundary
    /// (from either term) resets the fact counter. Deterministic: fed the same event
    /// stream, two independent cadences fire boundaries at the identical positions.
    pub fn observe(&mut self, event: HorizonEvent) -> bool {
        match event {
            HorizonEvent::EpochRoll => {
                self.facts_since_boundary = 0;
                true
            }
            HorizonEvent::Fact => {
                self.facts_since_boundary += 1;
                if self.facts_since_boundary >= self.facts_per_horizon {
                    self.facts_since_boundary = 0;
                    true
                } else {
                    false
                }
            }
        }
    }
}
