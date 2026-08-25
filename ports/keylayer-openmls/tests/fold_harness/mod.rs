//! The fold harness for the adapter e2e (a copy of the core suite's
//! tests/common — test modules do not cross crates): the minimal honest
//! adapter (hashmaps standing where redb stands in the corpus) plus the
//! envelope/payload builders. Fold behavior under test lives in the crate;
//! this module only assembles context, exactly as a real adapter would.

#![allow(dead_code)] // each test binary uses a different subset

use std::collections::HashMap;

use social_tree_core::metrics::NoopMetrics;
use social_tree_core::model::{
    envelope_hash, AssertionEnvelope, AssertionType, DeviceId, GroupId, GroupState, Hash,
    PrincipalId, ENVELOPE_WIRE_VERSION,
};
use social_tree_core::ports::ed25519::{Ed25519Signer, Ed25519Verifier};
use social_tree_core::ports::{DeviceId as PortDeviceId, Signer, Verifier};
use social_tree_core::update::{evaluate, is_governance, Evaluation, FoldContext, SlotOccupancy};

/// In-memory store driving `evaluate` the way the redb adapter does.
#[derive(Default)]
pub struct MemStore {
    states: HashMap<[u8; 32], GroupState>,
    logs: HashMap<[u8; 32], Vec<(Hash, AssertionEnvelope)>>,
    by_device: HashMap<[u8; 32], u64>,
    held: HashMap<Hash, AssertionEnvelope>,
}

impl MemStore {
    pub fn ingest(&mut self, env: &AssertionEnvelope) -> Result<Evaluation, String> {
        // Authorship on real crypto before the fold sees the fact.
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

    pub fn state(&self, group: &GroupId) -> &GroupState {
        self.states.get(group.as_bytes()).expect("state")
    }

    pub fn log(&self, group: &GroupId) -> &[(Hash, AssertionEnvelope)] {
        self.logs.get(group.as_bytes()).map_or(&[], Vec::as_slice)
    }
}

/// A signed envelope from device `device_seed` for principal `principal`.
pub fn env(
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

/// Genesis payload with explicit `[add, remove, role, rule]` thresholds.
pub fn genesis_payload_with(thresholds: [u32; 4]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&1u16.to_be_bytes()); // policy_version
    for t in thresholds {
        p.extend_from_slice(&t.to_be_bytes());
    }
    p.extend_from_slice(&[0x10u8; 32]); // founding device
    p
}

pub fn add_payload(subject: u8, role: u8) -> Vec<u8> {
    let mut p = PrincipalId::new([subject; 32]).as_bytes().to_vec();
    p.push(role);
    p
}

/// A `MembershipRemove` payload: subject ‖ removal-kind (§7.6.4 — the
/// artifact distinction is a MUST). 0x00 = departure (voluntary or
/// eviction/migration; standing intact), 0x01 = ban (the quorum-stamped
/// standing ceiling).
pub fn remove_payload(subject: u8, kind: u8) -> Vec<u8> {
    let mut p = PrincipalId::new([subject; 32]).as_bytes().to_vec();
    p.push(kind);
    p
}

pub fn approval_payload(ty: AssertionType, subject: PrincipalId) -> Vec<u8> {
    let mut p = ty.to_u16().to_be_bytes().to_vec();
    p.extend_from_slice(subject.as_bytes());
    p
}
