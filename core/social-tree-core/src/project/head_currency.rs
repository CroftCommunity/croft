//! EXP-C2/C3 (E112) — head-currency: the §7.4 freshness precondition, wired to the fold.
//!
//! §7.4 requires that a node originating or co-signing a membership act be **corroborated-fresh**
//! (it can show its head is current) before acting on an irreversible/dependent governance step;
//! otherwise it **fail-closed stalls** while still serving reads on its best-known state. The pure
//! threshold arithmetic lives in [`crate::project::completeness_ahead`] (`quorum_k`, `admits_irreversible`,
//! `detect_stamp_gap`). This module carries the *state* that makes the precondition real at the
//! fold, and the origination gate a would-be originator MUST consult.
//!
//! Two ingredients, from two directions:
//!   * **Behind-via-traffic (the negative signal, C2).** When the fold holds a fact back with
//!     [`FoldError::MissingAntecedents`], an incoming fact named a head this node has not folded —
//!     so the node knows, *from traffic alone*, that it is behind. This is a detection, not a
//!     corroboration: absence of such traffic proves nothing (the quiet-group case).
//!   * **Corroborated-fresh (the positive signal, C3).** A count of distinct **lineages** (never
//!     clients, §5.7) attesting the node's current head — the freshness the gate needs to *admit*.
//!     Supplied by the HeadAck primitive; seeded as an integer here until C3 wires it.
//!
//! Fidelity: **Modeled / loopback grade.** No wire format and no networking on this path; the
//! freshness count is a plain integer a test seeds, standing in for real attested values.

use crate::project::completeness_ahead::{admits_irreversible, detect_stamp_gap, quorum_k};
use crate::update::{FoldError, IngestResult};

/// Why a membership-op origination or co-signature was stalled (fail-closed, §7.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stalled {
    /// Traffic named a head this node has not folded — behind-via-traffic (C2's detection).
    BehindViaTraffic,
    /// Not enough distinct-lineage currency attestations to corroborate the head is current
    /// (§7.4): `have < need`, where `need = ceil(member_count / 2)`.
    NotCorroboratedFresh { have: u64, need: u64 },
}

/// A node's head-currency state for one group. Reads are never gated by this; only origination
/// and co-signature of membership/governance acts are.
#[derive(Debug, Default, Clone)]
pub struct HeadCurrency {
    behind: bool,
}

impl HeadCurrency {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one ingest outcome into currency state. A [`FoldError::MissingAntecedents`] outcome
    /// means an incoming fact referenced a head this node has not folded — the behind-via-traffic
    /// signal. Any other outcome (applied, duplicate, a real rejection) leaves the flag untouched:
    /// only *unseen-head* references imply the node is behind.
    pub fn observe_ingest<E: BehindSignal>(&mut self, outcome: &Result<IngestResult, E>) {
        if matches!(outcome, Err(e) if e.is_missing_antecedents()) {
            self.behind = true;
        }
    }

    /// Fold one *data-plane* entry that carries the governance generation stamp it was produced
    /// under. If that stamp is ahead of the node's own governance frontier (`local_gov_seq`), the
    /// node is behind (the [`detect_stamp_gap`] "behind-via-traffic" case). NB: the fold itself
    /// does not enforce this — data-plane facts are optimistically accepted (§2.0.1 razor) — so a
    /// node only learns of a gap this way if it *actively reads* the carried stamp. This is the
    /// modeled data-plane channel; the governance channel ([`observe_ingest`]) is the fold-native
    /// one.
    pub fn observe_stamped_entry(&mut self, local_gov_seq: u64, entry_gen_stamp: u64) {
        if detect_stamp_gap(local_gov_seq, entry_gen_stamp).is_some() {
            self.behind = true;
        }
    }

    #[must_use]
    pub fn is_behind(&self) -> bool {
        self.behind
    }

    /// A behind node must not present its projection as current (§7.4 — it may serve, but must
    /// not claim currency).
    #[must_use]
    pub fn may_render_current(&self) -> bool {
        !self.behind
    }

    /// Clear the behind flag once the node has folded through the referenced head.
    pub fn note_caught_up(&mut self) {
        self.behind = false;
    }
}

/// The §7.4 gate on **originating or co-signing** a membership/governance op. Fail-closed: refuses
/// if the node is behind-via-traffic, or if its freshness (distinct-lineage head attestations) is
/// below `k = ceil(member_count / 2)`. Reads are never gated — a stalled node keeps serving its
/// best-known state.
///
/// # Errors
/// [`Stalled::BehindViaTraffic`] if traffic revealed an unseen head; [`Stalled::NotCorroboratedFresh`]
/// if freshness is below the quorum threshold.
pub fn admits_membership_origination(
    currency: &HeadCurrency,
    freshness: u64,
    member_count: u64,
) -> Result<(), Stalled> {
    // Behind-via-traffic: a fact named a head we have not folded. Fail closed regardless of
    // freshness — we know for certain we are missing history.
    if currency.is_behind() {
        return Err(Stalled::BehindViaTraffic);
    }
    // Corroborated-fresh: at least k = ceil(n/2) distinct-lineage head attestations. Below k we
    // stall (fail-closed) rather than act on a possibly-stale frontier.
    let need = quorum_k(member_count);
    if !admits_irreversible(freshness, need) {
        return Err(Stalled::NotCorroboratedFresh {
            have: freshness,
            need,
        });
    }
    Ok(())
}

/// The one signal currency listens for in an ingest outcome: an incoming fact
/// referenced a head this node has not folded. Adapters implement this for
/// their own error types so their outcomes feed currency without conversion.
pub trait BehindSignal {
    /// Was this outcome a missing-antecedents refusal?
    fn is_missing_antecedents(&self) -> bool;
}

impl BehindSignal for FoldError {
    fn is_missing_antecedents(&self) -> bool {
        matches!(self, FoldError::MissingAntecedents { .. })
    }
}
