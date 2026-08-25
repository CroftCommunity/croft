//! **The §7.6.4 removal-kind pins (E117 P4, prerequisite for the admission facts).**
//!
//! A ban and a departure are one primitive with **distinct artifacts**, and
//! the distinction MUST be preserved (§7.6.4 — it is the only thing that
//! lets a third party read the provenance of a departure). The fold
//! previously conflated them: every `MembershipRemove` carried only the
//! subject, so a dormancy migration was indistinguishable from a
//! quorum-stamped ceiling — and the admission machinery cannot be built on
//! that conflation, because every legitimate returner has a removal in
//! their history and only a *ban* may block re-entry.
//!
//! The wire grows a removal-kind byte: subject(32) ‖ kind(1), 0x00 =
//! departure (voluntary, eviction, migration — standing intact), 0x01 = ban
//! (the standing ceiling). Kindless 32-byte payloads are refused loudly
//! (experiment-grade wire, rebuild posture — WIRE-REGISTER).
//!
//! Two consequences pinned here beyond the byte itself:
//! - **The exit floor (Part 1 §2.5).** The right to leave cannot be
//!   configured away: a self-departure (author == subject, kind departure)
//!   requires no role and no quorum. The fold previously demanded Admin
//!   role AND the remove threshold for every removal — a member of a
//!   two-to-ban group literally could not leave alone.
//! - **Only a ban contests.** §7.3.2's pair is decision-vs-decision on the
//!   standing slot. A re-add racing a *departure* is an ordinary re-invite,
//!   not a contradiction; a re-add racing a *ban* hard-stops CONTESTED as
//!   before.

mod common;

use common::{add_payload, env, genesis_payload_with, remove_payload, MemStore};
use social_tree_core::model::{
    envelope_hash, AssertionType, ForkStatus, GroupId, MembershipView, PrincipalId,
};

const GROUP: [u8; 32] = [0xC7; 32];

fn group() -> GroupId {
    GroupId::new(GROUP)
}

fn pid(seed: u8) -> PrincipalId {
    PrincipalId::new([seed; 32])
}

/// O(0x20, dev 0x10) owner; C(0x23, dev 0x13) admin; D(0x24, dev 0x14) member.
fn boot(store: &mut MemStore, thresholds: [u32; 4]) {
    let genesis = env(
        0x10,
        0x20,
        AssertionType::GroupGenesis,
        1,
        vec![],
        genesis_payload_with(thresholds),
    );
    store.ingest(&genesis).expect("genesis");
    let add_c = env(
        0x10,
        0x20,
        AssertionType::MembershipAdd,
        2,
        vec![],
        add_payload(0x23, 1),
    );
    store.ingest(&add_c).expect("add admin C");
    let add_d = env(
        0x10,
        0x20,
        AssertionType::MembershipAdd,
        3,
        vec![],
        add_payload(0x24, 2),
    );
    store.ingest(&add_d).expect("add member D");
}

/// **Pin 1 — a kindless removal payload is refused loudly.**
///
/// 32 bytes was the pre-§7.6.4 wire; accepting it would silently default a
/// removal's provenance, and a ban that decays to "unspecified" on old
/// bytes is exactly the illegible record the spec forbids. Rebuild, never
/// reinterpret.
#[test]
fn a_kindless_remove_payload_is_refused() {
    let mut store = MemStore::default();
    boot(&mut store, [1, 1, 1, 1]);

    let kindless = env(
        0x10,
        0x20,
        AssertionType::MembershipRemove,
        10,
        vec![],
        pid(0x24).as_bytes().to_vec(), // 32 bytes, no kind
    );
    let err = store
        .ingest(&kindless)
        .expect_err("a removal without its kind byte is not a §7.6.4 artifact");
    assert!(
        err.contains("kind"),
        "the refusal names the missing kind byte: {err}"
    );
}

/// **Pin 2 — the exit floor: a plain member self-departs with no role and
/// no quorum.**
///
/// The group's remove threshold is 2 ("two to ban") and D holds no Admin
/// role — and none of that binds a voluntary departure, because the right
/// to leave is inherent (Part 1 §2.5) and a threshold on it would be the
/// group configuring away the exit floor.
#[test]
fn a_member_departs_alone_despite_role_and_threshold() {
    let mut store = MemStore::default();
    boot(&mut store, [1, 2, 1, 1]); // two-to-ban charter

    let d_departs = env(
        0x14,
        0x24, // author == subject
        AssertionType::MembershipRemove,
        10,
        vec![],
        remove_payload(0x24, 0x00), // departure
    );
    store
        .ingest(&d_departs)
        .expect("a self-departure needs neither Admin role nor the remove quorum");

    let state = store.state(&group());
    assert_eq!(
        state.membership(&pid(0x24)),
        MembershipView::NotMember,
        "D left"
    );
    assert!(matches!(state.fork_status, ForkStatus::Clean));
}

/// **Pin 3 — a self-authored ban is refused: no one stamps the group's
/// ceiling on themselves.**
///
/// The ban artifact carries group authority (§7.6.4's quorum-stamped
/// ceiling). A self-removal is always a departure; kind = ban with
/// author == subject is a malformed claim of group authority and refuses
/// loudly rather than folding as either kind.
#[test]
fn a_self_authored_ban_is_refused() {
    let mut store = MemStore::default();
    boot(&mut store, [1, 2, 1, 1]);

    let d_bans_self = env(
        0x14,
        0x24,
        AssertionType::MembershipRemove,
        10,
        vec![],
        remove_payload(0x24, 0x01), // ban, self-authored
    );
    let err = store
        .ingest(&d_bans_self)
        .expect_err("a ban is group authority; a self-ban is not a thing");
    assert!(
        err.contains("self"),
        "the refusal names the self-ban shape: {err}"
    );
}

/// **Pin 4 — a departure racing a re-add is a benign race, never a
/// contradiction — and it CONVERGES.**
///
/// C evicts D (departure kind — a liveness migration, standing intact)
/// concurrently with O re-adding D. §7.3.2's pair is two *decisions* on
/// the standing slot; an eviction is an enactment with standing intact, so
/// this is §7.4.1's provably-benign case: auto-reconciled by the canonical
/// fold, never escalated. The load-bearing half is convergence — the old
/// fold achieved convergence for every non-commutative race by contesting
/// it, so the kind-narrowing must supply the replacement: both arrival
/// orders land byte-identically, with nothing withheld and no hard-stop.
#[test]
fn a_departure_racing_a_readd_reconciles_identically_in_both_orders() {
    let add_d_hash = envelope_hash(&env(
        0x10,
        0x20,
        AssertionType::MembershipAdd,
        3,
        vec![],
        add_payload(0x24, 2),
    ));

    let c_evicts_d = env(
        0x13,
        0x23,
        AssertionType::MembershipRemove,
        12,
        vec![add_d_hash],
        remove_payload(0x24, 0x00), // departure
    );
    let o_readds_d = env(
        0x10,
        0x20,
        AssertionType::MembershipAdd,
        12,
        vec![add_d_hash],
        add_payload(0x24, 2),
    );

    let mut evict_first = MemStore::default();
    boot(&mut evict_first, [1, 1, 1, 1]);
    evict_first.ingest(&c_evicts_d).expect("eviction folds");
    evict_first
        .ingest(&o_readds_d)
        .expect("concurrent re-add folds");

    let mut readd_first = MemStore::default();
    boot(&mut readd_first, [1, 1, 1, 1]);
    readd_first.ingest(&o_readds_d).expect("re-add folds");
    readd_first
        .ingest(&c_evicts_d)
        .expect("concurrent eviction folds");

    let a = evict_first.state(&group());
    let b = readd_first.state(&group());
    assert!(
        matches!(a.fork_status, ForkStatus::Clean),
        "a departure is not a standing decision; nothing contests: {:?}",
        a.fork_status
    );
    assert_eq!(
        a.membership(&pid(0x24)),
        b.membership(&pid(0x24)),
        "both arrival orders converge on D's membership"
    );
    // The head stamp legitimately differs by arrival order (a locator, not
    // content — P1 pin 1's normalization); the semantic state must not.
    assert_eq!(a.members, b.members, "identical rosters");
    assert_eq!(a.fork_status, b.fork_status, "identical fork status");
}

/// **Pin 5 — a ban racing a re-add still hard-stops CONTESTED (the E108
/// behavior, now kind-scoped).**
///
/// Identical race, kind = ban: two rival decisions on D's standing slot.
/// The narrowing in pin 4 must not have loosened this.
#[test]
fn a_ban_racing_a_readd_still_contests() {
    let mut store = MemStore::default();
    boot(&mut store, [1, 1, 1, 1]);

    let add_d_hash = envelope_hash(&env(
        0x10,
        0x20,
        AssertionType::MembershipAdd,
        3,
        vec![],
        add_payload(0x24, 2),
    ));

    let c_bans_d = env(
        0x13,
        0x23,
        AssertionType::MembershipRemove,
        12,
        vec![add_d_hash],
        remove_payload(0x24, 0x01), // ban
    );
    let o_readds_d = env(
        0x10,
        0x20,
        AssertionType::MembershipAdd,
        12,
        vec![add_d_hash],
        add_payload(0x24, 2),
    );

    store.ingest(&c_bans_d).expect("ban folds");
    store
        .ingest(&o_readds_d)
        .expect("the second half is the hard-stop");

    let state = store.state(&group());
    assert!(
        matches!(state.fork_status, ForkStatus::Contested(_)),
        "ban-vs-readd is two decisions on the standing slot: {:?}",
        state.fork_status
    );
    assert!(
        matches!(state.membership(&pid(0x24)), MembershipView::Contested(_)),
        "the subject of an open contradiction projects CONTESTED"
    );
}
