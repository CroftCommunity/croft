//! The E108/§7.3.2 pins, run PURE — the same five behaviors the croft-chat
//! adapter suite pins over redb, here proven over an in-memory context: the
//! fold is a function of data, not of a store (E117 P2's inversion made
//! testable). Plus the O3 standing property: order independence over full
//! permutations of the concurrent set.

use std::collections::HashMap;

use social_tree_core::metrics::NoopMetrics;
use social_tree_core::model::{
    envelope_hash, AssertionEnvelope, AssertionType, DeviceId, ForkStatus, GroupId, GroupState,
    Hash, MembershipView, PrincipalId, ENVELOPE_WIRE_VERSION,
};
use social_tree_core::ports::ed25519::{Ed25519Signer, Ed25519Verifier};
use social_tree_core::ports::{DeviceId as PortDeviceId, Signer, Verifier};
use social_tree_core::update::{evaluate, is_governance, Evaluation, FoldContext, SlotOccupancy};

/// The minimal honest adapter: hashmaps standing where redb stands in the
/// experiment corpus. Everything `evaluate` needs, nothing it must not have.
#[derive(Default)]
struct MemStore {
    states: HashMap<[u8; 32], GroupState>,
    logs: HashMap<[u8; 32], Vec<(Hash, AssertionEnvelope)>>,
    by_device: HashMap<[u8; 32], u64>,
    held: HashMap<Hash, AssertionEnvelope>,
}

impl MemStore {
    fn ingest(&mut self, env: &AssertionEnvelope) -> Result<Evaluation, String> {
        // Authorship on real crypto: every fact's signature verifies against the
        // author device's ed25519 key before the fold sees it (the adapter's
        // step 2, reproduced here so the pins pin the composed check).
        Ed25519Verifier
            .verify(
                &PortDeviceId(*env.author_device.as_bytes()),
                &env.canonical_bytes(),
                &env.signature,
            )
            .map_err(|e| format!("signature: {e:?}"))?;
        let g = *env.group.as_bytes();
        let log = self.logs.entry(g).or_default().clone();
        let gov = is_governance(&env.assertion_type);
        let target_seq = if env.assertion_type == AssertionType::GroupGenesis {
            0
        } else {
            log.len() as u64
        };
        let existing = if gov {
            log.iter()
                .enumerate()
                .find(|(i, _)| *i as u64 == target_seq)
                .map(|(_, (h, _))| *h)
        } else {
            None
        };
        let ctx = FoldContext {
            current_state: self.states.get(&g).cloned(),
            governance_log: &log,
            last_device_lamport: self.by_device.get(env.author_device.as_bytes()).copied(),
            antecedents_present: env
                .antecedents
                .iter()
                .filter(|h| self.held.contains_key(h))
                .count(),
            antecedent_envelopes: env
                .antecedents
                .iter()
                .filter_map(|h| self.held.get(h).cloned())
                .collect(),
            gov_slot: gov.then_some(SlotOccupancy {
                target_seq,
                existing,
            }),
        };
        let out = evaluate(env, &ctx, &NoopMetrics).map_err(|e| e.to_string())?;
        let hash = envelope_hash(env);
        self.held.insert(hash, env.clone());
        let dl = self
            .by_device
            .entry(*env.author_device.as_bytes())
            .or_insert(0);
        if env.lamport > *dl {
            *dl = env.lamport;
        }
        if let Evaluation::Governance { ref next_state, .. } = out {
            self.logs.entry(g).or_default().push((hash, env.clone()));
            self.states.insert(g, next_state.clone());
        }
        Ok(out)
    }

    fn state(&self, group: &GroupId) -> &GroupState {
        self.states.get(group.as_bytes()).expect("state")
    }
}

fn env(
    device_seed: u8,
    principal: u8,
    ty: AssertionType,
    lamport: u64,
    antecedents: Vec<Hash>,
    payload: Vec<u8>,
) -> AssertionEnvelope {
    let signer = Ed25519Signer::from_seed([device_seed; 32]);
    let mut e = AssertionEnvelope {
        version: ENVELOPE_WIRE_VERSION,
        assertion_type: ty,
        author_device: DeviceId::new(signer.device_id().0),
        author_principal: PrincipalId::new([principal; 32]),
        group: GroupId::new([0xC7; 32]),
        antecedents,
        lamport,
        payload,
        signature: vec![],
    };
    e.signature = signer.sign(&e.canonical_bytes());
    e
}

fn genesis_payload() -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&1u16.to_be_bytes()); // policy_version
    for t in [1u32, 1, 1, 1] {
        p.extend_from_slice(&t.to_be_bytes());
    }
    p.extend_from_slice(&[0x10u8; 32]); // founding device
    p
}

fn add_payload(subject: u8, role: u8) -> Vec<u8> {
    let mut p = PrincipalId::new([subject; 32]).as_bytes().to_vec();
    p.push(role);
    p
}

fn remove_payload(subject: u8) -> Vec<u8> {
    PrincipalId::new([subject; 32]).as_bytes().to_vec()
}

fn resolution_payload(a: Hash, b: Hash) -> Vec<u8> {
    let (lo, hi) = if a.as_bytes() <= b.as_bytes() {
        (a, b)
    } else {
        (b, a)
    };
    let mut p = lo.as_bytes().to_vec();
    p.extend_from_slice(hi.as_bytes());
    p
}

fn approval_payload(ty: AssertionType, subject: PrincipalId) -> Vec<u8> {
    let mut p = ty.to_u16().to_be_bytes().to_vec();
    p.extend_from_slice(subject.as_bytes());
    p
}

/// O(0x10)/O(0x20) owner; A(0x21) B(0x22) C(0x23) admins; D(0x24) member.
/// Devices mirror principals (0x1n). Same shape as the adapter-side cast.
struct Cast {
    group: GroupId,
    genesis: AssertionEnvelope,
    adds: Vec<AssertionEnvelope>,
    a_removes_b: AssertionEnvelope,
    b_removes_a: AssertionEnvelope,
    c_removes_d: AssertionEnvelope,
    o_readds_d: AssertionEnvelope,
}

fn cast() -> Cast {
    let genesis = env(
        0x10,
        0x20,
        AssertionType::GroupGenesis,
        1,
        vec![],
        genesis_payload(),
    );
    let add_a = env(
        0x10,
        0x20,
        AssertionType::MembershipAdd,
        2,
        vec![],
        add_payload(0x21, 1),
    );
    let add_b = env(
        0x10,
        0x20,
        AssertionType::MembershipAdd,
        3,
        vec![],
        add_payload(0x22, 1),
    );
    let add_c = env(
        0x10,
        0x20,
        AssertionType::MembershipAdd,
        4,
        vec![],
        add_payload(0x23, 1),
    );
    let add_d = env(
        0x10,
        0x20,
        AssertionType::MembershipAdd,
        5,
        vec![],
        add_payload(0x24, 2),
    );
    let a_removes_b = env(
        0x11,
        0x21,
        AssertionType::MembershipRemove,
        10,
        vec![envelope_hash(&add_b)],
        remove_payload(0x22),
    );
    let b_removes_a = env(
        0x12,
        0x22,
        AssertionType::MembershipRemove,
        10,
        vec![envelope_hash(&add_a)],
        remove_payload(0x21),
    );
    let c_removes_d = env(
        0x13,
        0x23,
        AssertionType::MembershipRemove,
        12,
        vec![envelope_hash(&add_d)],
        remove_payload(0x24),
    );
    let o_readds_d = env(
        0x10,
        0x20,
        AssertionType::MembershipAdd,
        12,
        vec![envelope_hash(&add_d)],
        add_payload(0x24, 2),
    );
    Cast {
        group: GroupId::new([0xC7; 32]),
        genesis,
        adds: vec![add_a, add_b, add_c, add_d],
        a_removes_b,
        b_removes_a,
        c_removes_d,
        o_readds_d,
    }
}

fn boot(store: &mut MemStore, k: &Cast) {
    store.ingest(&k.genesis).expect("genesis");
    for a in &k.adds {
        store.ingest(a).expect("add");
    }
}

fn normalized(s: &GroupState) -> Vec<u8> {
    let mut c = s.clone();
    c.computed_at_gov_head = Hash::new([0u8; 32]);
    c.to_bytes()
}

fn contested(v: &MembershipView) -> bool {
    matches!(v, MembershipView::Contested(_))
}

fn p(byte: u8) -> PrincipalId {
    PrincipalId::new([byte; 32])
}

#[test]
fn pin1_mutual_expulsion_projects_contested_both_orders() {
    let k = cast();
    let mut s1 = MemStore::default();
    boot(&mut s1, &k);
    s1.ingest(&k.a_removes_b).expect("r1");
    s1.ingest(&k.b_removes_a).expect("r2");
    let mut s2 = MemStore::default();
    boot(&mut s2, &k);
    s2.ingest(&k.b_removes_a).expect("r2");
    s2.ingest(&k.a_removes_b).expect("r1");
    for st in [s1.state(&k.group), s2.state(&k.group)] {
        assert!(contested(&st.membership(&p(0x21))), "A contested");
        assert!(contested(&st.membership(&p(0x22))), "B contested");
        assert!(
            matches!(st.membership(&p(0x23)), MembershipView::Member(_)),
            "C member"
        );
        assert!(
            matches!(st.membership(&p(0x55)), MembershipView::NotMember),
            "stranger"
        );
    }
    assert_eq!(
        normalized(s1.state(&k.group)),
        normalized(s2.state(&k.group))
    );
}

#[test]
fn pin2_two_open_contradictions_representable() {
    let k = cast();
    let mut s = MemStore::default();
    boot(&mut s, &k);
    for e in [
        &k.a_removes_b,
        &k.b_removes_a,
        &k.c_removes_d,
        &k.o_readds_d,
    ] {
        s.ingest(e).expect("concurrent set");
    }
    let st = s.state(&k.group);
    match &st.fork_status {
        ForkStatus::Contested(entries) => assert_eq!(entries.len(), 2, "both pairs open"),
        other => panic!("expected Contested, got {other:?}"),
    }
    assert!(contested(&st.membership(&p(0x21))));
    assert!(contested(&st.membership(&p(0x22))));
    assert!(contested(&st.membership(&p(0x24))));
    assert!(matches!(st.membership(&p(0x23)), MembershipView::Member(_)));
}

#[test]
fn pin3_single_author_resolution_refused() {
    let k = cast();
    let mut s = MemStore::default();
    boot(&mut s, &k);
    s.ingest(&k.a_removes_b).expect("r1");
    s.ingest(&k.b_removes_a).expect("r2");
    let pair = resolution_payload(envelope_hash(&k.a_removes_b), envelope_hash(&k.b_removes_a));
    let solo = env(0x10, 0x20, AssertionType::Resolution, 13, vec![], pair);
    let err = s
        .ingest(&solo)
        .expect_err("single-author resolution must be refused");
    assert!(err.contains("threshold"), "got: {err}");
    assert!(
        contested(&s.state(&k.group).membership(&p(0x21))),
        "pair stays open"
    );
}

fn resolve(
    store: &mut MemStore,
    pair_bytes: Vec<u8>,
    approver: (u8, u8),
    author_lam: u64,
    appr_lam: u64,
) {
    let subject = PrincipalId::new(social_tree_core::update::rule_change_approval_subject(
        &pair_bytes,
    ));
    let approve = env(
        approver.0,
        approver.1,
        AssertionType::Approval,
        appr_lam,
        vec![],
        approval_payload(AssertionType::Resolution, subject),
    );
    store.ingest(&approve).expect("approval");
    let res = env(
        0x10,
        0x20,
        AssertionType::Resolution,
        author_lam,
        vec![envelope_hash(&approve)],
        pair_bytes,
    );
    store.ingest(&res).expect("resolution");
}

#[test]
fn pin4_quorum_resolution_closes_named_pair_only() {
    let k = cast();
    let mut s = MemStore::default();
    boot(&mut s, &k);
    for e in [
        &k.a_removes_b,
        &k.b_removes_a,
        &k.c_removes_d,
        &k.o_readds_d,
    ] {
        s.ingest(e).expect("concurrent set");
    }
    let pair1 = resolution_payload(envelope_hash(&k.a_removes_b), envelope_hash(&k.b_removes_a));
    resolve(&mut s, pair1, (0x13, 0x23), 13, 13);
    let st = s.state(&k.group);
    assert!(
        matches!(st.membership(&p(0x21)), MembershipView::Member(_)),
        "A back to Member"
    );
    assert!(
        matches!(st.membership(&p(0x22)), MembershipView::Member(_)),
        "B back to Member"
    );
    assert!(
        contested(&st.membership(&p(0x24))),
        "D stays contested — one pair named"
    );
    match &st.fork_status {
        ForkStatus::Contested(entries) => assert_eq!(entries.len(), 1),
        other => panic!("expected one open entry, got {other:?}"),
    }
}

#[test]
fn pin5_resolved_exclusions_persist_through_later_replays() {
    let k = cast();
    let mut s = MemStore::default();
    boot(&mut s, &k);
    for e in [
        &k.a_removes_b,
        &k.b_removes_a,
        &k.c_removes_d,
        &k.o_readds_d,
    ] {
        s.ingest(e).expect("concurrent set");
    }
    let pair1 = resolution_payload(envelope_hash(&k.a_removes_b), envelope_hash(&k.b_removes_a));
    resolve(&mut s, pair1, (0x13, 0x23), 13, 13);
    let pair2 = resolution_payload(envelope_hash(&k.c_removes_d), envelope_hash(&k.o_readds_d));
    resolve(&mut s, pair2, (0x13, 0x23), 14, 14);
    let st = s.state(&k.group);
    assert!(
        matches!(st.fork_status, ForkStatus::Clean),
        "both closed, got {:?}",
        st.fork_status
    );
    for who in [0x21u8, 0x22, 0x23, 0x24] {
        assert!(
            matches!(st.membership(&p(who)), MembershipView::Member(_)),
            "0x{who:02x} must be a member — resolved removes stay withheld"
        );
    }
}

mod order_independence {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// O3 (E117 P2): the standing order-independence property. Every
        /// permutation of the four concurrent facts converges to the same
        /// normalized state — the §7.3.1 keys, permutation-tested by default
        /// on every future fold change.
        #[test]
        fn all_orders_of_the_concurrent_set_converge(perm in proptest::sample::subsequence(vec![0usize,1,2,3], 4).prop_shuffle()) {
            let k = cast();
            let facts = [&k.a_removes_b, &k.b_removes_a, &k.c_removes_d, &k.o_readds_d];
            let mut reference = MemStore::default();
            boot(&mut reference, &k);
            for f in facts {
                reference.ingest(f).expect("reference order");
            }
            let want = normalized(reference.state(&k.group));

            let mut s = MemStore::default();
            boot(&mut s, &k);
            for i in perm {
                s.ingest(facts[i]).expect("permuted order");
            }
            prop_assert_eq!(normalized(s.state(&k.group)), want);
        }
    }
}
