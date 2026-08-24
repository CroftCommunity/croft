//! EXP-C1 (backlog §2c, RUN-07) — experiment-grade completeness-ahead primitives.
//!
//! The completeness-ahead contract (Part 2 §7.3.3 corroboration dials, §7.4 freshness,
//! §7.4.3 generation stamp) states, in one line: a node acts on an irreversible /
//! dependent governance step only when it can corroborate it is current, and otherwise
//! **fail-closed stalls** while still serving reads on its best-known state. This module
//! carries the *minimal* pure fold-side helpers that make the four contract assertions
//! separately RED-able at loopback / fold grade.
//!
//! Scope and non-claims (deliberate):
//!   * **No wire format, no persistence, no networking.** The `[gates-release]` stamp and
//!     `(G, D)` cursor encodings (Part 2 §7.4) are untouched; the freshness threshold and
//!     generation stamp here are plain integers a test seeds, standing in for the real
//!     attested values.
//!   * This earns the contract at **loopback grade** only. The real-NAT / live-transport
//!     path stays X1.

/// Assertion 4 — the formula-valued freshness threshold `k = ceil(n/2)` over the folded
/// member set. A pure function of the member count, so every node computes the identical
/// `k` at the same act position: the member count converges by construction (Part 2
/// §7.3.1/§7.3.2), regardless of arrival order.
#[must_use]
pub fn quorum_k(member_count: u64) -> u64 {
    // ceil(n/2): a strict majority of distinct lineages.
    member_count.div_ceil(2)
}

/// Assertion 2 — the §7.4.3 generation-stamp gap. A data-plane entry carries the
/// governance **generation stamp** it was produced under; if that stamp is ahead of the
/// local governance frontier (`local_gov_seq`), there is a gap the node must fill before
/// acting on the entry (the "behind-via-traffic" case). Returns the **size** of the gap
/// (how many generations behind the node is), or `None` if the node is already current.
#[must_use]
pub fn detect_stamp_gap(local_gov_seq: u64, entry_gen_stamp: u64) -> Option<u64> {
    if entry_gen_stamp > local_gov_seq {
        Some(entry_gen_stamp - local_gov_seq)
    } else {
        None
    }
}

/// Assertion 1 — the fail-closed freshness gate on an irreversible/dependent act. A node
/// may enforce such an act only when it holds at least `k` distinct-lineage currency
/// attestations (`freshness >= k`). Below `k` it **STALLS** (fail-closed) rather than
/// acting on a possibly-stale frontier; crucially this gate governs *origination /
/// enforcement* only, never reads, so a stalled node keeps serving its best-known state.
#[must_use]
pub fn admits_irreversible(freshness: u64, k: u64) -> bool {
    freshness >= k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quorum_k_is_ceil_half() {
        assert_eq!(quorum_k(0), 0);
        assert_eq!(quorum_k(1), 1);
        assert_eq!(quorum_k(2), 1);
        assert_eq!(quorum_k(3), 2);
        assert_eq!(quorum_k(4), 2);
        assert_eq!(quorum_k(5), 3);
    }

    #[test]
    fn stamp_gap_detects_and_sizes() {
        assert_eq!(detect_stamp_gap(5, 5), None, "current: no gap");
        assert_eq!(detect_stamp_gap(5, 4), None, "ahead of the stamp: no gap");
        assert_eq!(detect_stamp_gap(5, 8), Some(3), "behind by 3 generations");
    }

    #[test]
    fn irreversible_gate_is_fail_closed_below_k() {
        assert!(!admits_irreversible(0, 2), "no attestations: stall");
        assert!(!admits_irreversible(1, 2), "below k: stall");
        assert!(admits_irreversible(2, 2), "at k: admit");
        assert!(admits_irreversible(3, 2), "above k: admit");
    }
}
