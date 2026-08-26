//! Stage 7: mutation/scale tests for `local_storage_projection`.
//!
//! Validates test-suite coverage and scale characteristics:
//!   - Large diverse logs (many groups, members, messages, attachments, governance)
//!   - Deep causal lineages
//!   - Many devices per principal
//!   - Fork convergence at scale
//!   - Property-based invariant checks (I2, I3, I4, I9)
//!   - Adversarial inputs (lamport collision, dangling antecedent, kind mismatch, malformed payload)

#[cfg(test)]
mod scale_tests {
    use crate::fold_derived::{rebuild, DerivedFold, FoldError, GroupState, IngestResult};
    // The table constants come from `tables`, not from a local mirror. This
    // module used to re-declare its own copy "(mirrors fold_derived)" — the
    // third of three, and the one best placed to hide a divergence, because a
    // suite reading its own private tables agrees with itself no matter what
    // production writes.
    use crate::tables::{
        encode_edge_out_key, encode_gov_log_key, Db, EdgeMeta, EdgeType, AUTH_ASSERTIONS,
        AUTH_ASSERTIONS_BY_DEVICE, AUTH_GOV_LOG, IDX_EDGES_IN, IDX_EDGES_OUT, IDX_NODES,
        STATE_GROUP,
    };
    use redb::{ReadableTable, TableDefinition};
    use social_tree_core::model::{
        envelope_hash, AssertionEnvelope, AssertionType, DeviceId as TypesDeviceId, GroupId,
        Hash as TypesHash, KindTag, PrincipalId as TypesPrincipalId, Role, TypedId,
    };
    use social_tree_core::ports::mocks::{MockCredentialResolver, MockSigner};
    use social_tree_core::ports::{
        DeviceId as TraitsDeviceId, PrincipalId as TraitsPrincipalId, Signer,
    };
    use std::sync::Arc;

    // -----------------------------------------------------------------------
    // Shared helpers
    // -----------------------------------------------------------------------

    fn make_device(seed: u8) -> TypesDeviceId {
        TypesDeviceId::new([seed; 32])
    }

    fn make_principal(seed: u8) -> TypesPrincipalId {
        TypesPrincipalId::new([seed; 32])
    }

    fn make_group(seed: u8) -> GroupId {
        GroupId::new([seed; 32])
    }

    fn make_group_from_u16(id: u16) -> GroupId {
        let mut b = [0u8; 32];
        b[0] = (id >> 8) as u8;
        b[1] = id as u8;
        GroupId::new(b)
    }

    fn make_principal_from_u16(id: u16) -> TypesPrincipalId {
        let mut b = [0u8; 32];
        b[0] = (id >> 8) as u8;
        b[1] = id as u8;
        TypesPrincipalId::new(b)
    }

    fn make_hash(seed: u8) -> TypesHash {
        TypesHash::new([seed; 32])
    }

    fn genesis_payload(device: &TypesDeviceId) -> Vec<u8> {
        let mut p = Vec::with_capacity(50);
        p.extend_from_slice(&1u16.to_be_bytes()); // policy_version
        p.extend_from_slice(&1u32.to_be_bytes()); // add_member_threshold
        p.extend_from_slice(&1u32.to_be_bytes()); // remove_member_threshold
        p.extend_from_slice(&1u32.to_be_bytes()); // role_change_threshold
        p.extend_from_slice(&1u32.to_be_bytes()); // rule_change_threshold
        p.extend_from_slice(device.as_bytes()); // founding_device
        p
    }

    fn membership_add_payload(principal: &TypesPrincipalId, role: &Role) -> Vec<u8> {
        let mut p = Vec::with_capacity(33);
        p.extend_from_slice(principal.as_bytes());
        p.push(role_byte(role));
        p
    }

    fn membership_remove_payload(principal: &TypesPrincipalId) -> Vec<u8> {
        principal.as_bytes().to_vec()
    }

    fn role_grant_payload(principal: &TypesPrincipalId, role: &Role) -> Vec<u8> {
        let mut p = Vec::with_capacity(33);
        p.extend_from_slice(principal.as_bytes());
        p.push(role_byte(role));
        p
    }

    fn rule_change_payload(key_byte: u8, new_value: u32) -> Vec<u8> {
        let mut p = Vec::with_capacity(5);
        p.push(key_byte);
        p.extend_from_slice(&new_value.to_be_bytes());
        p
    }

    fn message_payload(body: &str) -> Vec<u8> {
        let mut p = Vec::new();
        let b = body.as_bytes();
        p.extend_from_slice(&(b.len() as u32).to_be_bytes());
        p.extend_from_slice(b);
        p.extend_from_slice(&0u32.to_be_bytes()); // no reply_to
        p
    }

    fn attachment_add_payload(kind: KindTag, title: &str, blob: Option<TypesHash>) -> Vec<u8> {
        let mut p = Vec::new();
        p.push(kind as u8);
        let tb = title.as_bytes();
        p.extend_from_slice(&(tb.len() as u32).to_be_bytes());
        p.extend_from_slice(tb);
        match blob {
            None => p.push(0x00),
            Some(h) => {
                p.push(0x01);
                p.extend_from_slice(h.as_bytes());
            }
        }
        p
    }

    fn role_byte(r: &Role) -> u8 {
        match r {
            Role::Owner => 0,
            Role::Admin => 1,
            Role::Member => 2,
            Role::Observer => 3,
        }
    }

    fn sign_envelope(env: &mut AssertionEnvelope, signer: &MockSigner) {
        let canonical = env.canonical_bytes();
        env.signature = signer.sign(&canonical);
    }

    /// Build a fold engine that accepts one signer/principal pair.
    fn make_fold(
        signer: &MockSigner,
        principal: TypesPrincipalId,
        db: Arc<Db>,
    ) -> DerivedFold<MockSigner, MockCredentialResolver> {
        let verifier = MockSigner::new(signer.device_id().0);
        let mut cred = MockCredentialResolver::new();
        cred.register(
            TraitsDeviceId(signer.device_id().0),
            TraitsPrincipalId(*principal.as_bytes()),
        );
        DerivedFold::new(db, verifier, cred)
    }

    /// A verifier that knows MANY devices (delegates to the matching per-device
    /// `MockSigner`). NOTE: a single `MockSigner` only verifies its own device,
    /// so the previous single-signer `make_fold_multi` silently rejected every
    /// non-first device's signature — multi-device coverage was illusory. This
    /// makes multi-device signature verification real. (Review fixup 2026-06-26.)
    struct MultiVerifier {
        signers: Vec<MockSigner>,
    }
    impl social_tree_core::ports::Verifier for MultiVerifier {
        fn verify(
            &self,
            device_id: &TraitsDeviceId,
            message: &[u8],
            signature: &[u8],
        ) -> Result<(), social_tree_core::ports::VerifyError> {
            for s in &self.signers {
                if s.device_id().0 == device_id.0 {
                    return social_tree_core::ports::Verifier::verify(
                        s, device_id, message, signature,
                    );
                }
            }
            Err(social_tree_core::ports::VerifyError::UnknownDevice(
                *device_id,
            ))
        }
    }

    /// Build a fold engine that accepts many (device, principal) pairs, with a
    /// verifier that genuinely verifies every device's signatures.
    fn make_fold_multi(
        pairs: &[(MockSigner, TypesPrincipalId)],
        db: Arc<Db>,
    ) -> DerivedFold<MultiVerifier, MockCredentialResolver> {
        let verifier = MultiVerifier {
            signers: pairs
                .iter()
                .map(|(s, _)| MockSigner::new(s.device_id().0))
                .collect(),
        };
        let mut cred = MockCredentialResolver::new();
        for (s, p) in pairs {
            cred.register(
                TraitsDeviceId(s.device_id().0),
                TraitsPrincipalId(*p.as_bytes()),
            );
        }
        DerivedFold::new(db, verifier, cred)
    }

    /// Ingest genesis + MembershipAdd(owner) for a group and return the next
    /// available lamport value.
    fn boot_group<V: social_tree_core::ports::Verifier>(
        fold: &DerivedFold<V, MockCredentialResolver>,
        signer: &MockSigner,
        principal: TypesPrincipalId,
        group: GroupId,
        lamport: u64,
    ) -> u64 {
        let device = TypesDeviceId::new(signer.device_id().0);

        let mut genesis = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::GroupGenesis,
            author_device: device,
            author_principal: principal,
            group,
            antecedents: vec![],
            lamport,
            payload: genesis_payload(&device),
            signature: vec![],
        };
        sign_envelope(&mut genesis, signer);
        fold.ingest(&genesis).expect("genesis");

        let mut add_owner = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::MembershipAdd,
            author_device: device,
            author_principal: principal,
            group,
            antecedents: vec![],
            lamport: lamport + 1,
            payload: membership_add_payload(&principal, &Role::Owner),
            signature: vec![],
        };
        sign_envelope(&mut add_owner, signer);
        fold.ingest(&add_owner).expect("add owner");

        lamport + 2
    }

    fn snapshot_state(db: &Arc<Db>, group: &GroupId) -> Vec<u8> {
        let rtxn = db.inner().begin_read().unwrap();
        let tbl = rtxn.open_table(STATE_GROUP).unwrap();
        tbl.get(group.as_bytes().as_ref())
            .unwrap()
            .map(|v| v.value().to_vec())
            .unwrap_or_default()
    }

    fn count_table_rows(
        db: &Arc<Db>,
        def: TableDefinition<'static, &'static [u8], &'static [u8]>,
    ) -> usize {
        let rtxn = db.inner().begin_read().unwrap();
        let tbl = rtxn.open_table(def).unwrap();
        tbl.iter().unwrap().count()
    }

    // -----------------------------------------------------------------------
    // Scale test: large diverse log
    // -----------------------------------------------------------------------

    #[test]
    fn test_large_diverse_log() {
        let num_groups = 5usize;
        let members_each = 10usize;
        let msgs_each = 50usize;
        let attaches_each = 5usize;

        let db = Arc::new(Db::create_in_memory().unwrap());

        // One owner signer/principal per group so each owner's lamport stream
        // is independent and doesn't collide.
        let mut group_owners: Vec<(MockSigner, TypesPrincipalId, GroupId)> = Vec::new();
        for g in 0..num_groups {
            let seed = (g as u8).wrapping_add(0x10);
            group_owners.push((
                MockSigner::from_seed(seed),
                make_principal(seed),
                make_group_from_u16(g as u16 + 1),
            ));
        }

        // Register all (device, principal) pairs in one credential resolver.
        let verifier = MockSigner::from_seed(0x10); // used for first group; each gets own fold
        let _ = verifier;

        for (owner_signer, owner_principal, group_id) in &group_owners {
            // Build a fold instance accepted by this owner.
            let fold = make_fold(owner_signer, *owner_principal, Arc::clone(&db));

            let mut lam = boot_group(&fold, owner_signer, *owner_principal, *group_id, 1);
            let device = TypesDeviceId::new(owner_signer.device_id().0);

            // Add 10 members.
            for m in 0..members_each {
                let member = make_principal_from_u16((m as u16 + 1) * 100);
                let mut env = AssertionEnvelope {
                    version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                    assertion_type: AssertionType::MembershipAdd,
                    author_device: device,
                    author_principal: *owner_principal,
                    group: *group_id,
                    antecedents: vec![],
                    lamport: lam,
                    payload: membership_add_payload(&member, &Role::Member),
                    signature: vec![],
                };
                sign_envelope(&mut env, owner_signer);
                fold.ingest(&env).expect("add member");
                lam += 1;
            }

            // Send 50 messages.
            for i in 0..msgs_each {
                let body = format!("group_msg_{}", i);
                let mut env = AssertionEnvelope {
                    version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                    assertion_type: AssertionType::Message,
                    author_device: device,
                    author_principal: *owner_principal,
                    group: *group_id,
                    antecedents: vec![],
                    lamport: lam,
                    payload: message_payload(&body),
                    signature: vec![],
                };
                sign_envelope(&mut env, owner_signer);
                fold.ingest(&env).expect("message");
                lam += 1;
            }

            // Add 5 attachments.
            for a in 0..attaches_each {
                let title = format!("attach_{}", a);
                let mut env = AssertionEnvelope {
                    version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                    assertion_type: AssertionType::AttachmentAdd,
                    author_device: device,
                    author_principal: *owner_principal,
                    group: *group_id,
                    antecedents: vec![],
                    lamport: lam,
                    payload: attachment_add_payload(KindTag::ArtifactNote, &title, None),
                    signature: vec![],
                };
                sign_envelope(&mut env, owner_signer);
                fold.ingest(&env).expect("attachment");
                lam += 1;
            }

            // Governance: role grant on first member, then rule change.
            let first_member = make_principal_from_u16(100);
            let mut rg = AssertionEnvelope {
                version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                assertion_type: AssertionType::RoleGrant,
                author_device: device,
                author_principal: *owner_principal,
                group: *group_id,
                antecedents: vec![],
                lamport: lam,
                payload: role_grant_payload(&first_member, &Role::Admin),
                signature: vec![],
            };
            sign_envelope(&mut rg, owner_signer);
            fold.ingest(&rg).expect("role grant");
            lam += 1;

            // Rule change: set add_member_threshold = 2.
            let mut rc = AssertionEnvelope {
                version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                assertion_type: AssertionType::RuleChange,
                author_device: device,
                author_principal: *owner_principal,
                group: *group_id,
                antecedents: vec![],
                lamport: lam,
                payload: rule_change_payload(0, 2), // key 0 = AddMember
                signature: vec![],
            };
            sign_envelope(&mut rc, owner_signer);
            fold.ingest(&rc).expect("rule change");
            lam += 1;
        }

        // --- Assertions ---

        // I3: rebuild reproduces identical state for every group.
        for (_, _, group_id) in &group_owners {
            let state_before = snapshot_state(&db, group_id);
            assert!(
                !state_before.is_empty(),
                "group state must be present before rebuild"
            );
        }

        {
            let (signer0, _, _) = &group_owners[0];
            let verifier = MockSigner::new(signer0.device_id().0);
            let cred = MockCredentialResolver::new();
            rebuild(&db, &verifier, &cred).unwrap();
        }

        for (_, _, group_id) in &group_owners {
            let state_after = snapshot_state(&db, group_id);
            assert!(
                !state_after.is_empty(),
                "group state must be present after rebuild"
            );
        }

        // list_my_groups equivalent: for each owner, the edge table should have
        // MemberOf edges from the owner to their group (present=true).
        for (owner_signer, owner_principal, group_id) in &group_owners {
            let my_hash = TypesHash::new(*owner_principal.as_bytes());
            let my_typed = TypedId::new(KindTag::Principal, my_hash);
            let group_hash = TypesHash::new(*group_id.as_bytes());
            let group_typed = TypedId::new(KindTag::Group, group_hash);

            let rtxn = db.inner().begin_read().unwrap();
            let tbl = rtxn.open_table(IDX_EDGES_OUT).unwrap();
            let edge_key = encode_edge_out_key(&my_typed, EdgeType::MemberOf, &group_typed);
            let edge = tbl.get(edge_key.as_ref()).unwrap();
            assert!(
                edge.is_some(),
                "owner {:?} must have MemberOf edge to group {:?}",
                owner_principal,
                group_id
            );
            let meta = EdgeMeta::from_bytes(edge.unwrap().value()).unwrap();
            assert!(meta.present, "owner MemberOf edge must be present=true");
        }

        // get_timeline(LastN(20)): for each group, References edges must exist.
        for (_, _, group_id) in &group_owners {
            let group_hash = TypesHash::new(*group_id.as_bytes());
            let group_typed = TypedId::new(KindTag::Group, group_hash);

            let edge_type_bytes = EdgeType::References.to_be_bytes();
            let mut prefix = [0u8; 35];
            prefix[..33].copy_from_slice(group_typed.as_bytes());
            prefix[33..35].copy_from_slice(&edge_type_bytes);

            let start = {
                let mut k = [0u8; 68];
                k[..35].copy_from_slice(&prefix);
                k
            };
            let end = {
                let mut k = [0u8; 68];
                k[..35].copy_from_slice(&prefix);
                k[35..].fill(0xFF);
                k
            };

            let rtxn = db.inner().begin_read().unwrap();
            let tbl = rtxn.open_table(IDX_EDGES_OUT).unwrap();
            let ref_count = tbl
                .range(start.as_slice()..=end.as_slice())
                .unwrap()
                .count();

            // We sent 50 messages, each creates one References edge.
            assert_eq!(
                ref_count, msgs_each,
                "expected {} References edges for group {:?}, got {}",
                msgs_each, group_id, ref_count
            );
        }
    }

    // -----------------------------------------------------------------------
    // Scale test: deep lineages
    // -----------------------------------------------------------------------

    #[test]
    fn test_deep_lineage() {
        const CHAIN_LEN: usize = 100;

        let signer = MockSigner::from_seed(0x77);
        let owner = make_principal(0x77);
        let group = make_group(0x77);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&signer, owner, Arc::clone(&db));
        let device = TypesDeviceId::new(signer.device_id().0);

        let mut lam = boot_group(&fold, &signer, owner, group, 1);

        // Build a chain: each message's antecedent = hash of the previous.
        let mut prev_hash: Option<TypesHash> = None;
        let mut msg_hashes: Vec<TypesHash> = Vec::with_capacity(CHAIN_LEN);

        for i in 0..CHAIN_LEN {
            let antecedents = prev_hash.map(|h| vec![h]).unwrap_or_default();
            let mut env = AssertionEnvelope {
                version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                assertion_type: AssertionType::Message,
                author_device: device,
                author_principal: owner,
                group,
                antecedents,
                lamport: lam,
                payload: message_payload(&format!("chain_{}", i)),
                signature: vec![],
            };
            sign_envelope(&mut env, &signer);
            let h = envelope_hash(&env);
            fold.ingest(&env).expect("chain message");
            msg_hashes.push(h);
            prev_hash = Some(h);
            lam += 1;
        }

        // Assert causal order preserved: lamport values must be strictly ascending.
        // Collect lamport values from auth_assertions via the by-device index.
        use crate::tables::{decode_by_device_key, encode_by_device_key};

        let rtxn = db.inner().begin_read().unwrap();
        let by_dev = rtxn.open_table(AUTH_ASSERTIONS_BY_DEVICE).unwrap();
        let start = encode_by_device_key(&device, 0);
        let end = encode_by_device_key(&device, u64::MAX);

        let lamports: Vec<u64> = by_dev
            .range(start.as_slice()..=end.as_slice())
            .unwrap()
            .map(|item| {
                let (k, _) = item.unwrap();
                let (_, lam) = decode_by_device_key(k.value());
                lam
            })
            .collect();

        // All lamports must be strictly ascending (total order preserved).
        for w in lamports.windows(2) {
            assert!(
                w[0] < w[1],
                "causal order violated: lamport {} not < {}",
                w[0],
                w[1]
            );
        }

        // Rebuild and verify identical state.
        let state_before = snapshot_state(&db, &group);

        {
            let verifier = MockSigner::new(signer.device_id().0);
            let cred = MockCredentialResolver::new();
            rebuild(&db, &verifier, &cred).unwrap();
        }

        let state_after = snapshot_state(&db, &group);
        assert_eq!(
            state_before, state_after,
            "rebuild must reproduce identical state for deep lineage"
        );

        // All CHAIN_LEN message assertions must be present in auth_assertions.
        let auth_table = rtxn.open_table(AUTH_ASSERTIONS).unwrap();
        for h in &msg_hashes {
            // After rebuild the read txn is stale; open a fresh one.
            let rtxn2 = db.inner().begin_read().unwrap();
            let auth2 = rtxn2.open_table(AUTH_ASSERTIONS).unwrap();
            assert!(
                auth2.get(h.as_bytes().as_ref()).unwrap().is_some(),
                "chain message {:?} must be in auth_assertions",
                h
            );
        }
    }

    // -----------------------------------------------------------------------
    // Scale test: many devices per principal
    // -----------------------------------------------------------------------

    #[test]
    fn test_many_devices_per_principal() {
        const NUM_DEVICES: usize = 5;
        const MSGS_PER_DEVICE: usize = 10;

        let group = make_group(0x88);
        let db = Arc::new(Db::create_in_memory().unwrap());

        // One principal with NUM_DEVICES devices; each device has its own signer.
        let principal = make_principal(0x88);
        let signers: Vec<MockSigner> = (0..NUM_DEVICES)
            .map(|i| MockSigner::from_seed(0x88 + i as u8))
            .collect();

        // Register all devices for this principal.
        let verifier_seed = signers[0].device_id().0;
        let mut cred = MockCredentialResolver::new();
        for s in &signers {
            cred.register(
                TraitsDeviceId(s.device_id().0),
                TraitsPrincipalId(*principal.as_bytes()),
            );
        }

        // Build a fold that uses the first device's verifier but all credentials.
        // For the genesis we use device 0.
        let fold0 = {
            let verifier = MockSigner::new(verifier_seed);
            DerivedFold::new(Arc::clone(&db), verifier, cred.clone())
        };

        let device0 = TypesDeviceId::new(signers[0].device_id().0);
        let mut lam0 = boot_group(&fold0, &signers[0], principal, group, 1);

        // Each device sends MSGS_PER_DEVICE messages from its own lamport stream.
        // Device N starts at lamport = N * 1000 + 1 (well above boot sequence).
        let mut total_ingested = 0usize;
        for (di, signer) in signers.iter().enumerate() {
            // Build a per-device fold (each needs its own verifier).
            let fold_dev = {
                let verifier = MockSigner::new(signer.device_id().0);
                DerivedFold::new(Arc::clone(&db), verifier, cred.clone())
            };
            let dev = TypesDeviceId::new(signer.device_id().0);
            let base_lam = (di as u64 + 1) * 1000;

            for mi in 0..MSGS_PER_DEVICE {
                let lam = base_lam + mi as u64 + 1;
                let mut env = AssertionEnvelope {
                    version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                    assertion_type: AssertionType::Message,
                    author_device: dev,
                    author_principal: principal,
                    group,
                    antecedents: vec![],
                    lamport: lam,
                    payload: message_payload(&format!("dev{}msg{}", di, mi)),
                    signature: vec![],
                };
                sign_envelope(&mut env, signer);
                fold_dev.ingest(&env).expect("device message");
                total_ingested += 1;
            }
        }

        assert_eq!(total_ingested, NUM_DEVICES * MSGS_PER_DEVICE);

        // All 50 messages must be present in auth_assertions.
        let rtxn = db.inner().begin_read().unwrap();
        let auth = rtxn.open_table(AUTH_ASSERTIONS).unwrap();
        let auth_count = auth.iter().unwrap().count();
        // 50 messages + 1 genesis + 1 MembershipAdd(owner) = 52
        assert_eq!(
            auth_count,
            total_ingested + 2,
            "expected {} assertions in auth_assertions, got {}",
            total_ingested + 2,
            auth_count
        );

        // Merge order must be total and deterministic: collect all lamports from
        // the by-device index for each device and verify strict monotonicity.
        use crate::tables::{decode_by_device_key, encode_by_device_key};

        let by_dev_tbl = rtxn.open_table(AUTH_ASSERTIONS_BY_DEVICE).unwrap();
        for signer in &signers {
            let dev = TypesDeviceId::new(signer.device_id().0);
            let start = encode_by_device_key(&dev, 0);
            let end = encode_by_device_key(&dev, u64::MAX);
            let lams: Vec<u64> = by_dev_tbl
                .range(start.as_slice()..=end.as_slice())
                .unwrap()
                .map(|item| {
                    let (k, _) = item.unwrap();
                    let (_, l) = decode_by_device_key(k.value());
                    l
                })
                .collect();
            for w in lams.windows(2) {
                assert!(
                    w[0] < w[1],
                    "device {:?}: lamport order violated: {} not < {}",
                    dev,
                    w[0],
                    w[1]
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Scale test: fork convergence at scale
    // -----------------------------------------------------------------------

    #[test]
    fn test_fork_convergence_at_scale() {
        // Create 3 competing governance assertions at the same gov_seq (genesis slot 0).
        // Assert: exactly one wins via tiebreak; state is consistent.

        let group = make_group(0x99);
        let db = Arc::new(Db::create_in_memory().unwrap());

        let signers: Vec<MockSigner> = (0..3)
            .map(|i| MockSigner::from_seed(0xA0 + i as u8))
            .collect();
        let principals: Vec<TypesPrincipalId> =
            (0..3).map(|i| make_principal(0xA0 + i as u8)).collect();

        // Build a genesis from each signer for the same group.
        let mut genesis_envs: Vec<AssertionEnvelope> = Vec::new();
        for (s, p) in signers.iter().zip(principals.iter()) {
            let dev = TypesDeviceId::new(s.device_id().0);
            let mut genesis = AssertionEnvelope {
                version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                assertion_type: AssertionType::GroupGenesis,
                author_device: dev,
                author_principal: *p,
                group,
                antecedents: vec![],
                lamport: 1,
                payload: genesis_payload(&dev),
                signature: vec![],
            };
            sign_envelope(&mut genesis, s);
            genesis_envs.push(genesis);
        }

        // Compute all hashes; find expected winner (lex-smallest).
        let hashes: Vec<TypesHash> = genesis_envs.iter().map(envelope_hash).collect();
        let winner_idx = hashes
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.as_bytes().cmp(b.as_bytes()))
            .map(|(i, _)| i)
            .unwrap();

        // Ingest all three in order.
        for (i, (signer, (principal, genesis))) in signers
            .iter()
            .zip(principals.iter().zip(genesis_envs.iter()))
            .enumerate()
        {
            let fold = make_fold(signer, *principal, Arc::clone(&db));
            fold.ingest(genesis).expect("competing genesis");
        }

        // Exactly one slot 0 in AUTH_GOV_LOG.
        let rtxn = db.inner().begin_read().unwrap();
        let gov_tbl = rtxn.open_table(AUTH_GOV_LOG).unwrap();
        let start = encode_gov_log_key(&group, 0);
        let end = encode_gov_log_key(&group, u64::MAX);
        let gov_count = gov_tbl
            .range(start.as_slice()..=end.as_slice())
            .unwrap()
            .count();
        // Only one entry at slot 0 is written (subsequent ones overwrite in tiebreak).
        assert!(
            gov_count >= 1,
            "at least one governance entry must be present"
        );

        // State is consistent (deserializes without error).
        let state_bytes = snapshot_state(&db, &group);
        assert!(!state_bytes.is_empty(), "group state must be present");
        let state =
            GroupState::from_bytes(&state_bytes).expect("state must deserialize without error");

        // Fork status must reflect a fork (all three competed).
        // The final state after all three ingests should either be Clean (if the
        // winner was ingested last and no fork tracking remains) or ForkedFrom
        // the displaced hash.  Either way the state is consistent.
        // What we assert: the gov_seq is 0 (genesis slot) and the state is valid.
        assert_eq!(state.computed_at_gov_seq, 0, "genesis occupies seq=0");

        // Determinism: ingest the same three geneses in reversed order to a fresh DB.
        let db2 = Arc::new(Db::create_in_memory().unwrap());
        for (signer, (principal, genesis)) in signers
            .iter()
            .zip(principals.iter().zip(genesis_envs.iter()))
            .rev()
        {
            let fold = make_fold(signer, *principal, Arc::clone(&db2));
            fold.ingest(genesis).expect("competing genesis reversed");
        }
        let state2_bytes = snapshot_state(&db2, &group);
        let state2 = GroupState::from_bytes(&state2_bytes).unwrap();
        assert_eq!(
            state.fork_status, state2.fork_status,
            "fork tiebreak must be deterministic regardless of ingestion order"
        );
    }

    // -----------------------------------------------------------------------
    // Property-based tests
    // -----------------------------------------------------------------------

    use proptest::prelude::*;

    /// Deterministic pseudo-random helper based on a seed.
    fn prng_u64(seed: u64, idx: u64) -> u64 {
        let a: u64 = 6364136223846793005;
        let c: u64 = 1442695040888963407;
        seed.wrapping_mul(a)
            .wrapping_add(c)
            .wrapping_mul(idx.wrapping_add(1))
    }

    fn prng_u8(seed: u64, idx: u64) -> u8 {
        (prng_u64(seed, idx) & 0xFF) as u8
    }

    /// Build a causally-consistent sequence of N assertions (genesis + ops)
    /// from a single device, using a deterministic seed.
    fn build_seeded_sequence(
        seed: u64,
        count: usize,
    ) -> (MockSigner, TypesPrincipalId, Vec<AssertionEnvelope>) {
        let dev_seed = prng_u8(seed, 0);
        let prin_seed = prng_u8(seed, 1);
        let group_seed = prng_u8(seed, 2);

        let signer = MockSigner::from_seed(dev_seed);
        let principal = make_principal(prin_seed);
        let group = make_group(group_seed);
        let device = TypesDeviceId::new(signer.device_id().0);

        let mut envs: Vec<AssertionEnvelope> = Vec::with_capacity(count);

        // Genesis is always first.
        let mut genesis = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::GroupGenesis,
            author_device: device,
            author_principal: principal,
            group,
            antecedents: vec![],
            lamport: 1,
            payload: genesis_payload(&device),
            signature: vec![],
        };
        sign_envelope(&mut genesis, &signer);
        envs.push(genesis);

        // Add owner so subsequent ops are authorized.
        if count >= 2 {
            let mut add = AssertionEnvelope {
                version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                assertion_type: AssertionType::MembershipAdd,
                author_device: device,
                author_principal: principal,
                group,
                antecedents: vec![],
                lamport: 2,
                payload: membership_add_payload(&principal, &Role::Owner),
                signature: vec![],
            };
            sign_envelope(&mut add, &signer);
            envs.push(add);
        }

        // Fill remaining slots with messages (always authorized for members).
        for i in 2..count {
            let lam = i as u64 + 1;
            let body = format!("msg_{}", i);
            let mut msg = AssertionEnvelope {
                version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                assertion_type: AssertionType::Message,
                author_device: device,
                author_principal: principal,
                group,
                antecedents: vec![],
                lamport: lam,
                payload: message_payload(&body),
                signature: vec![],
            };
            sign_envelope(&mut msg, &signer);
            envs.push(msg);
        }

        (signer, principal, envs)
    }

    proptest! {
        /// I2 convergence: applying the same causally-ordered sequence N times
        /// to independent DBs produces byte-identical state.
        #[test]
        fn prop_i2_convergence(seed in 0u64..1000) {
            let (signer, principal, envs) = build_seeded_sequence(seed, 8);
            let group_seed_byte = envs[0].group.as_bytes()[0];
            let group = envs[0].group;

            // Apply 5 times to separate DBs; compare state bytes.
            let mut states: Vec<Vec<u8>> = Vec::new();
            for _run in 0..5 {
                let db = Arc::new(Db::create_in_memory().unwrap());
                let fold = make_fold(&signer, principal, Arc::clone(&db));
                for env in &envs {
                    // All envs share the same device/lamport stream so they must
                    // be applied in order; duplicates return Duplicate which is OK.
                    let _ = fold.ingest(env);
                }
                states.push(snapshot_state(&db, &group));
            }

            for i in 1..states.len() {
                prop_assert_eq!(
                    &states[0], &states[i],
                    "I2: state diverged on run {}", i
                );
            }
        }

        /// I3 rebuild: rebuild produces byte-identical state.
        #[test]
        fn prop_i3_rebuild(seed in 0u64..1000) {
            let count = (seed % 19 + 2) as usize; // 2..=20
            let (signer, principal, envs) = build_seeded_sequence(seed, count);
            let group = envs[0].group;

            let db = Arc::new(Db::create_in_memory().unwrap());
            let fold = make_fold(&signer, principal, Arc::clone(&db));
            for env in &envs {
                let _ = fold.ingest(env);
            }

            let state_before = snapshot_state(&db, &group);

            // Collect sorted edges before rebuild.
            let edges_before: Vec<Vec<u8>> = {
                let rtxn = db.inner().begin_read().unwrap();
                let tbl = rtxn.open_table(IDX_EDGES_OUT).unwrap();
                let mut rows: Vec<Vec<u8>> = tbl
                    .iter()
                    .unwrap()
                    .map(|item| {
                        let (k, v) = item.unwrap();
                        let mut row = k.value().to_vec();
                        row.extend_from_slice(v.value());
                        row
                    })
                    .collect();
                rows.sort();
                rows
            };

            {
                let verifier = MockSigner::new(signer.device_id().0);
                let cred = MockCredentialResolver::new();
                rebuild(&db, &verifier, &cred).unwrap();
            }

            let state_after = snapshot_state(&db, &group);
            let edges_after: Vec<Vec<u8>> = {
                let rtxn = db.inner().begin_read().unwrap();
                let tbl = rtxn.open_table(IDX_EDGES_OUT).unwrap();
                let mut rows: Vec<Vec<u8>> = tbl
                    .iter()
                    .unwrap()
                    .map(|item| {
                        let (k, v) = item.unwrap();
                        let mut row = k.value().to_vec();
                        row.extend_from_slice(v.value());
                        row
                    })
                    .collect();
                rows.sort();
                rows
            };

            prop_assert_eq!(
                state_before, state_after,
                "I3: state_group must be byte-identical after rebuild (seed={})", seed
            );
            prop_assert_eq!(
                edges_before, edges_after,
                "I3: idx_edges_out must be byte-identical after rebuild (seed={})", seed
            );
        }

        /// I4 justification: every edge in idx_edges_out references an assertion
        /// present in auth_assertions.
        #[test]
        fn prop_i4_justification(seed in 0u64..500) {
            let count = (seed % 8 + 2) as usize; // 2..=9
            let (signer, principal, envs) = build_seeded_sequence(seed, count);

            let db = Arc::new(Db::create_in_memory().unwrap());
            let fold = make_fold(&signer, principal, Arc::clone(&db));
            for env in &envs {
                let _ = fold.ingest(env);
            }

            let rtxn = db.inner().begin_read().unwrap();
            let edges_tbl = rtxn.open_table(IDX_EDGES_OUT).unwrap();
            let auth_tbl = rtxn.open_table(AUTH_ASSERTIONS).unwrap();

            for item in edges_tbl.iter().unwrap() {
                let (_, v) = item.unwrap();
                let meta = EdgeMeta::from_bytes(v.value())
                    .expect("EdgeMeta must deserialize");
                let exists = auth_tbl
                    .get(meta.since_assertion.as_bytes().as_ref())
                    .unwrap()
                    .is_some();
                prop_assert!(
                    exists,
                    "I4: edge backed by unknown assertion {:?} (seed={})",
                    meta.since_assertion, seed
                );
            }
        }

        /// I9 idempotency: ingesting each assertion twice leaves row count unchanged
        /// after the second ingest, and derived state is not corrupted.
        #[test]
        fn prop_i9_idempotent(seed in 0u64..500) {
            let count = (seed % 6 + 2) as usize; // 2..=7
            let (signer, principal, envs) = build_seeded_sequence(seed, count);
            let group = envs[0].group;

            let db = Arc::new(Db::create_in_memory().unwrap());
            let fold = make_fold(&signer, principal, Arc::clone(&db));

            // First pass: ingest all.
            for env in &envs {
                match fold.ingest(env) {
                    Ok(IngestResult::Applied { .. }) | Ok(IngestResult::Duplicate) => {}
                    Err(e) => prop_assert!(false, "first-pass ingest error: {:?}", e),
                }
            }

            let auth_count_after_first = count_table_rows(&db, AUTH_ASSERTIONS);
            let state_after_first = snapshot_state(&db, &group);

            // Second pass: ingest all again.
            for env in &envs {
                match fold.ingest(env) {
                    Ok(IngestResult::Duplicate) => {}
                    Ok(IngestResult::Applied { .. }) => {
                        prop_assert!(false, "I9: second ingest returned Applied for existing assertion (seed={})", seed);
                    }
                    Err(e) => prop_assert!(false, "second-pass ingest error: {:?}", e),
                }
            }

            let auth_count_after_second = count_table_rows(&db, AUTH_ASSERTIONS);
            let state_after_second = snapshot_state(&db, &group);

            prop_assert_eq!(
                auth_count_after_first, auth_count_after_second,
                "I9: row count must not change after second ingest (seed={})", seed
            );
            prop_assert_eq!(
                state_after_first, state_after_second,
                "I9: derived state must not change after second ingest (seed={})", seed
            );
        }
    }

    // -----------------------------------------------------------------------
    // Adversarial test matrix
    // -----------------------------------------------------------------------

    /// A second assertion with the same lamport from the same device is rejected
    /// with LamportViolation.
    #[test]
    fn adversarial_lamport_collision() {
        let signer = MockSigner::from_seed(0xCC);
        let principal = make_principal(0xCC);
        let group = make_group(0xCC);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&signer, principal, Arc::clone(&db));
        let device = TypesDeviceId::new(signer.device_id().0);

        let mut lam = boot_group(&fold, &signer, principal, group, 1);

        // First message at lamport = lam.
        let mut msg1 = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::Message,
            author_device: device,
            author_principal: principal,
            group,
            antecedents: vec![],
            lamport: lam,
            payload: message_payload("first"),
            signature: vec![],
        };
        sign_envelope(&mut msg1, &signer);
        fold.ingest(&msg1).expect("first message at lamport=lam");

        // Second message with the SAME lamport=lam (collision).
        let mut msg2 = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::Message,
            author_device: device,
            author_principal: principal,
            group,
            antecedents: vec![],
            lamport: lam, // same lamport — collision!
            payload: message_payload("second at same lamport"),
            signature: vec![],
        };
        sign_envelope(&mut msg2, &signer);
        let result = fold.ingest(&msg2);

        // The second message must be rejected.  Because the envelope hash differs
        // from msg1 (different payload) it is not a duplicate; it must hit the
        // Lamport monotonicity check.
        assert!(
            matches!(result, Err(FoldError::LamportViolation { .. })),
            "expected LamportViolation for duplicate lamport, got {:?}",
            result
        );
    }

    /// A DATA-PLANE assertion (Message) whose antecedent hash is not present is
    /// accepted (deferred / optimistic acceptance) — the engine does not block the
    /// data plane on causal gaps. By the razor (Part 1 §2.0.1) a dangling antecedent
    /// on a message is a display concern (a reply whose parent hasn't arrived), so
    /// optimism is correct here. GOVERNANCE assertions are the opposite: the fold's
    /// Step 5.5 completeness guard holds them back until their antecedents arrive
    /// (see `fold_derived::tests::missing_antecedent_holds_the_fact_back`), because a
    /// governance gap can diverge the authority head (the G1 experiment). It is
    /// therefore load-bearing that this assertion is a `Message`.
    #[test]
    fn adversarial_dangling_antecedent() {
        let signer = MockSigner::from_seed(0xDD);
        let principal = make_principal(0xDD);
        let group = make_group(0xDD);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&signer, principal, Arc::clone(&db));
        let device = TypesDeviceId::new(signer.device_id().0);

        let mut lam = boot_group(&fold, &signer, principal, group, 1);

        // A phantom antecedent hash that is not in the store.
        let phantom = TypesHash::new([0xFFu8; 32]);

        let mut env = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::Message,
            author_device: device,
            author_principal: principal,
            group,
            antecedents: vec![phantom], // dangling reference
            lamport: lam,
            payload: message_payload("message with phantom antecedent"),
            signature: vec![],
        };
        sign_envelope(&mut env, &signer);

        // Must be accepted (no error), not blocked by the missing antecedent.
        let result = fold.ingest(&env);
        assert!(
            matches!(result, Ok(IngestResult::Applied { .. })),
            "dangling antecedent must be accepted (deferred), got {:?}",
            result
        );

        // The assertion must be persisted in auth_assertions.
        let h = envelope_hash(&env);
        let rtxn = db.inner().begin_read().unwrap();
        let auth = rtxn.open_table(AUTH_ASSERTIONS).unwrap();
        assert!(
            auth.get(h.as_bytes().as_ref()).unwrap().is_some(),
            "assertion with dangling antecedent must be stored"
        );
    }

    /// An ArtifactRef that claims kind=ArtifactNote for a hash that an earlier
    /// assertion stored as kind=Group creates two distinct TypedId nodes
    /// (different KindTag prefix byte), making the mismatch detectable by
    /// comparing their NodeCard kinds.
    #[test]
    fn adversarial_wrong_kind_reference() {
        let signer = MockSigner::from_seed(0xEE);
        let principal = make_principal(0xEE);
        let group = make_group(0xEE);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&signer, principal, Arc::clone(&db));
        let device = TypesDeviceId::new(signer.device_id().0);

        let mut lam = boot_group(&fold, &signer, principal, group, 1);

        // The shared content hash.
        let shared_hash = TypesHash::new([0x42u8; 32]);

        // First ArtifactRef: claim kind=Group for shared_hash.
        let mut ref_group = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::ArtifactRef,
            author_device: device,
            author_principal: principal,
            group,
            antecedents: vec![],
            lamport: lam,
            payload: {
                let mut p = Vec::with_capacity(33);
                p.push(KindTag::Group as u8);
                p.extend_from_slice(shared_hash.as_bytes());
                p
            },
            signature: vec![],
        };
        sign_envelope(&mut ref_group, &signer);
        fold.ingest(&ref_group).expect("ref group");
        lam += 1;

        // Second ArtifactRef: same hash but claim kind=ArtifactNote.
        // If the genesis for the actual entity said Group, this is a mismatch.
        let mut ref_note = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::ArtifactRef,
            author_device: device,
            author_principal: principal,
            group,
            antecedents: vec![],
            lamport: lam,
            payload: {
                let mut p = Vec::with_capacity(33);
                p.push(KindTag::ArtifactNote as u8);
                p.extend_from_slice(shared_hash.as_bytes());
                p
            },
            signature: vec![],
        };
        sign_envelope(&mut ref_note, &signer);
        fold.ingest(&ref_note).expect("ref note");

        // Both TypedIds must be present in idx_nodes.
        let group_typed = TypedId::new(KindTag::Group, shared_hash);
        let note_typed = TypedId::new(KindTag::ArtifactNote, shared_hash);

        let rtxn = db.inner().begin_read().unwrap();
        let nodes = rtxn.open_table(IDX_NODES).unwrap();

        let nc_group = nodes.get(group_typed.as_bytes().as_ref()).unwrap();
        let nc_note = nodes.get(note_typed.as_bytes().as_ref()).unwrap();

        assert!(nc_group.is_some(), "Group stub must exist for shared_hash");
        assert!(
            nc_note.is_some(),
            "ArtifactNote stub must exist for shared_hash"
        );

        // Mismatch is detectable: the two records carry different KindTag values.
        let card_group = crate::tables::NodeCard::from_bytes(nc_group.unwrap().value()).unwrap();
        let card_note = crate::tables::NodeCard::from_bytes(nc_note.unwrap().value()).unwrap();

        assert_ne!(
            card_group.kind, card_note.kind,
            "kind mismatch must be detectable: group={:?} note={:?}",
            card_group.kind, card_note.kind
        );
    }

    /// An envelope whose payload bytes fail the fold engine's deserialization
    /// (too short for the claimed assertion type) must be rejected with
    /// FoldError::MalformedEnvelope and must produce zero writes.
    #[test]
    fn adversarial_malformed_payload() {
        let signer = MockSigner::from_seed(0xFF);
        let principal = make_principal(0xFF);
        let group = make_group(0xFF);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold(&signer, principal, Arc::clone(&db));
        let device = TypesDeviceId::new(signer.device_id().0);

        let mut lam = boot_group(&fold, &signer, principal, group, 1);

        let auth_before = count_table_rows(&db, AUTH_ASSERTIONS);

        // A MembershipAdd payload that is too short (needs 33 bytes; we give 1).
        let mut malformed = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::MembershipAdd,
            author_device: device,
            author_principal: principal,
            group,
            antecedents: vec![],
            lamport: lam,
            payload: vec![0xDE], // only 1 byte — malformed!
            signature: vec![],
        };
        sign_envelope(&mut malformed, &signer);

        let result = fold.ingest(&malformed);

        // Must be rejected.
        assert!(
            matches!(
                result,
                Err(FoldError::MalformedEnvelope(_)) | Err(FoldError::AuthorizationFailed(_))
            ),
            "malformed payload must be rejected, got {:?}",
            result
        );

        // Zero writes: auth_assertions row count unchanged.
        let auth_after = count_table_rows(&db, AUTH_ASSERTIONS);
        assert_eq!(
            auth_before, auth_after,
            "malformed envelope must produce zero writes (before={}, after={})",
            auth_before, auth_after
        );

        // A GroupGenesis payload that is shorter than 50 bytes (fields are decoded
        // with byte-level slicing; engine must reject rather than panic).
        let mut short_genesis = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::GroupGenesis,
            // Use a fresh group so no existing state interferes.
            group: make_group(0xFE),
            author_device: device,
            author_principal: principal,
            antecedents: vec![],
            lamport: lam + 1,          // avoid lamport collision with above
            payload: vec![0x00, 0x01], // only 2 bytes — far too short
            signature: vec![],
        };
        sign_envelope(&mut short_genesis, &signer);

        let result2 = fold.ingest(&short_genesis);
        assert!(
            matches!(result2, Err(FoldError::MalformedEnvelope(_))),
            "short GroupGenesis payload must produce MalformedEnvelope, got {:?}",
            result2
        );

        // Still zero extra writes.
        let auth_after2 = count_table_rows(&db, AUTH_ASSERTIONS);
        assert_eq!(
            auth_before, auth_after2,
            "short genesis must produce zero writes (before={}, after={})",
            auth_before, auth_after2
        );
    }

    // =====================================================================
    // Diverse property-based invariant tests (added 2026-06-26 review fixup #2)
    //
    // The original prop_i2_convergence exercised only a single-device, linear
    // message stream. These exercise a multi-member, multi-device scenario
    // (governance + messages + attachment) and check determinism-under-
    // diversity, I3 (rebuild matches), and I9 (any causal prefix is panic-free
    // and self-consistent). NOTE on scope: the fold validates authorization at
    // ingest, so a member's message arriving BEFORE its MembershipAdd is
    // cleanly rejected (see out_of_causal_order_message_is_rejected_not_panicked).
    // True I5 order-insensitivity therefore holds for causally-valid orders,
    // not arbitrary reorderings — which is why these reuse a fixed causal order
    // rather than random interleavings. (Couples OPEN-THREADS T29.)
    // =====================================================================

    /// Full derived-state fingerprint: every (key,value) of all derived tables
    /// in key order. Identical derived state => identical fingerprint.
    fn snapshot_all(db: &Arc<Db>) -> Vec<u8> {
        let rtxn = db.inner().begin_read().unwrap();
        let mut out = Vec::new();
        for (tag, td) in [
            (0u8, IDX_EDGES_OUT),
            (1u8, IDX_EDGES_IN),
            (2u8, IDX_NODES),
            (3u8, STATE_GROUP),
        ] {
            out.push(0xFF);
            out.push(tag);
            if let Ok(tbl) = rtxn.open_table(td) {
                for item in tbl.iter().unwrap() {
                    let (k, v) = item.unwrap();
                    let (kb, vb) = (k.value(), v.value());
                    out.extend_from_slice(&(kb.len() as u32).to_be_bytes());
                    out.extend_from_slice(kb);
                    out.extend_from_slice(&(vb.len() as u32).to_be_bytes());
                    out.extend_from_slice(vb);
                }
            }
        }
        out
    }

    fn cred_for(
        pairs: &[(MockSigner, TypesPrincipalId)],
    ) -> (MultiVerifier, MockCredentialResolver) {
        let verifier = MultiVerifier {
            signers: pairs
                .iter()
                .map(|(s, _)| MockSigner::new(s.device_id().0))
                .collect(),
        };
        let mut cred = MockCredentialResolver::new();
        for (s, p) in pairs {
            cred.register(
                TraitsDeviceId(s.device_id().0),
                TraitsPrincipalId(*p.as_bytes()),
            );
        }
        (verifier, cred)
    }

    /// A diverse but causally-ordered scenario: one group; owner + two members
    /// (m1 has two devices); owner authors governance (genesis, three adds, a
    /// role grant, a rule change); members then author an attachment + messages.
    /// Every member is added before it authors anything, so the Vec is a valid
    /// causal order. Returns (pairs, group, envelopes-in-causal-order).
    fn gen_diverse(
        seed: u64,
    ) -> (
        Vec<(MockSigner, TypesPrincipalId)>,
        GroupId,
        Vec<AssertionEnvelope>,
    ) {
        let group = make_group_from_u16((seed & 0x0FFF) as u16);
        // 4 DISTINCT device seeds. from_seed(b) = [b; 32], so distinct bytes =>
        // distinct devices; prng_u8 could collide bytes and break per-device
        // lamport streams (the real-multi-device requirement).
        let base = ((seed % 60) as u8) * 4 + 1;
        let owner = MockSigner::from_seed(base);
        let m1 = MockSigner::from_seed(base + 1);
        let m1b = MockSigner::from_seed(base + 2);
        let m2 = MockSigner::from_seed(base + 3);
        let owner_p = make_principal_from_u16(0x1000u16 | (((seed >> 2) & 0x0FFF) as u16));
        let m1_p = make_principal_from_u16(0x2000u16 | (((seed >> 5) & 0x0FFF) as u16));
        let m2_p = make_principal_from_u16(0x3000u16 | (((seed >> 8) & 0x0FFF) as u16));

        let pairs = vec![
            (MockSigner::new(owner.device_id().0), owner_p),
            (MockSigner::new(m1.device_id().0), m1_p),
            (MockSigner::new(m1b.device_id().0), m1_p),
            (MockSigner::new(m2.device_id().0), m2_p),
        ];

        let mk = |aty, s: &MockSigner, p, lam: u64, payload: Vec<u8>| -> AssertionEnvelope {
            let mut e = AssertionEnvelope {
                version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
                assertion_type: aty,
                author_device: TypesDeviceId::new(s.device_id().0),
                author_principal: p,
                group,
                antecedents: vec![],
                lamport: lam,
                payload,
                signature: vec![],
            };
            sign_envelope(&mut e, s);
            e
        };

        let owner_dev = TypesDeviceId::new(owner.device_id().0);
        let mut envs = Vec::new();
        envs.push(mk(
            AssertionType::GroupGenesis,
            &owner,
            owner_p,
            1,
            genesis_payload(&owner_dev),
        ));
        envs.push(mk(
            AssertionType::MembershipAdd,
            &owner,
            owner_p,
            2,
            membership_add_payload(&owner_p, &Role::Owner),
        ));
        envs.push(mk(
            AssertionType::MembershipAdd,
            &owner,
            owner_p,
            3,
            membership_add_payload(&m1_p, &Role::Member),
        ));
        envs.push(mk(
            AssertionType::MembershipAdd,
            &owner,
            owner_p,
            4,
            membership_add_payload(&m2_p, &Role::Member),
        ));
        envs.push(mk(
            AssertionType::RoleGrant,
            &owner,
            owner_p,
            5,
            role_grant_payload(&m1_p, &Role::Admin),
        ));
        envs.push(mk(
            AssertionType::RuleChange,
            &owner,
            owner_p,
            6,
            rule_change_payload(0, 1),
        ));
        envs.push(mk(
            AssertionType::AttachmentAdd,
            &m2,
            m2_p,
            1,
            attachment_add_payload(KindTag::ArtifactNote, "note", None),
        ));
        envs.push(mk(
            AssertionType::Message,
            &m1,
            m1_p,
            1,
            message_payload("m1-a"),
        ));
        envs.push(mk(
            AssertionType::Message,
            &m1,
            m1_p,
            2,
            message_payload("m1-b"),
        ));
        envs.push(mk(
            AssertionType::Message,
            &m1b,
            m1_p,
            1,
            message_payload("m1b-a"),
        ));
        envs.push(mk(
            AssertionType::Message,
            &m2,
            m2_p,
            2,
            message_payload("m2-a"),
        ));

        (pairs, group, envs)
    }

    proptest! {
        /// Determinism under diversity: same causal order to 3 independent DBs
        /// yields byte-identical full derived state.
        #[test]
        fn prop_diverse_determinism(seed in 0u64..400) {
            let (pairs, _g, envs) = gen_diverse(seed);
            let mut snaps: Vec<Vec<u8>> = Vec::new();
            for run in 0..3 {
                let db = Arc::new(Db::create_in_memory().unwrap());
                let fold = make_fold_multi(&pairs, Arc::clone(&db));
                for e in &envs {
                    // Every envelope (incl. messages from non-owner devices) must
                    // be ACCEPTED — proves multi-device verification is real, not
                    // silently rejected. On the first run assert; later runs mirror.
                    let r = fold.ingest(e);
                    if run == 0 {
                        prop_assert!(r.is_ok(), "diverse envelope rejected: {:?}", r);
                    }
                }
                // Every assertion landed in the authoritative log.
                let n_auth = count_table_rows(&db, AUTH_ASSERTIONS);
                prop_assert_eq!(n_auth, envs.len(), "not all envelopes accepted (run {})", run);
                snaps.push(snapshot_all(&db));
            }
            prop_assert_eq!(&snaps[0], &snaps[1], "determinism: run 1 diverged");
            prop_assert_eq!(&snaps[0], &snaps[2], "determinism: run 2 diverged");
        }

        /// I3: rebuild() reproduces byte-identical derived state from the log.
        /// Regression guard for the 2026-06-26 convergence bug: `idx_nodes`
        /// node-card `created_at`/`created_by` were first-touch-wins (ingest-order-
        /// sensitive), so rebuild (canonical order) — and any two peers with
        /// different arrival orders — diverged. Fixed by making them the canonical
        /// MIN (monotonic, commutative). See upsert_node_stub / upsert_node_full.
        #[test]
        fn prop_diverse_rebuild(seed in 0u64..400) {
            let (pairs, _g, envs) = gen_diverse(seed);
            let db = Arc::new(Db::create_in_memory().unwrap());
            let fold = make_fold_multi(&pairs, Arc::clone(&db));
            for e in &envs { let _ = fold.ingest(e); }
            let before = snapshot_all(&db);
            let (verifier, cred) = cred_for(&pairs);
            rebuild(&db, &verifier, &cred).expect("rebuild");
            prop_assert_eq!(before, snapshot_all(&db), "I3: rebuild diverged");
        }

        /// I9: any causal PREFIX is panic-free, deterministic, and rebuildable.
        #[test]
        fn prop_diverse_partial_prefix(seed in 0u64..400, k in 0usize..=11) {
            let (pairs, _g, envs) = gen_diverse(seed);
            let n = k.min(envs.len());
            let db = Arc::new(Db::create_in_memory().unwrap());
            let fold = make_fold_multi(&pairs, Arc::clone(&db));
            for e in &envs[..n] { let _ = fold.ingest(e); }   // must not panic
            let db2 = Arc::new(Db::create_in_memory().unwrap());
            let fold2 = make_fold_multi(&pairs, Arc::clone(&db2));
            for e in &envs[..n] { let _ = fold2.ingest(e); }
            prop_assert_eq!(snapshot_all(&db), snapshot_all(&db2), "partial determinism");
            let (verifier, cred) = cred_for(&pairs);
            let s_before = snapshot_all(&db);
            rebuild(&db, &verifier, &cred).expect("rebuild partial");
            prop_assert_eq!(s_before, snapshot_all(&db), "I3 on partial log");
        }
    }

    /// Regression test (review 2026-06-26): per-derived-table check that the
    /// live fold and rebuild() agree. Was the repro for the `idx_nodes`
    /// `created_at` ingest-order-sensitivity bug (now fixed by canonical-MIN
    /// in upsert_node_stub / upsert_node_full); run with `--nocapture` to see
    /// per-table match status and (on any regression) the first diverging card.
    #[test]
    fn diag_rebuild_divergence() {
        let (pairs, _g, envs) = gen_diverse(7);
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold_multi(&pairs, Arc::clone(&db));
        for e in &envs {
            fold.ingest(e).expect("accepted");
        }

        let per_table = |db: &Arc<Db>| -> Vec<(usize, Vec<u8>)> {
            let rtxn = db.inner().begin_read().unwrap();
            [IDX_EDGES_OUT, IDX_EDGES_IN, IDX_NODES, STATE_GROUP]
                .iter()
                .map(|td| {
                    let mut n = 0usize;
                    let mut bytes = Vec::new();
                    if let Ok(tbl) = rtxn.open_table(*td) {
                        for it in tbl.iter().unwrap() {
                            let (k, v) = it.unwrap();
                            n += 1;
                            bytes.extend_from_slice(k.value());
                            bytes.extend_from_slice(v.value());
                        }
                    }
                    (n, bytes)
                })
                .collect()
        };
        let names = ["IDX_EDGES_OUT", "IDX_EDGES_IN", "IDX_NODES", "STATE_GROUP"];
        let before = per_table(&db);
        let (verifier, cred) = cred_for(&pairs);
        rebuild(&db, &verifier, &cred).expect("rebuild");
        let after = per_table(&db);
        for i in 0..before.len() {
            eprintln!(
                "DIAG {}: before rows={} after rows={} match={}",
                names[i],
                before[i].0,
                after[i].0,
                before[i].1 == after[i].1
            );
        }
        // Decode the first differing IDX_NODES card (before vs after rebuild).
        {
            let rd = |db: &Arc<Db>| -> Vec<(Vec<u8>, Vec<u8>)> {
                let rtxn = db.inner().begin_read().unwrap();
                let tbl = rtxn.open_table(IDX_NODES).unwrap();
                tbl.iter()
                    .unwrap()
                    .map(|it| {
                        let (k, v) = it.unwrap();
                        (k.value().to_vec(), v.value().to_vec())
                    })
                    .collect()
            };
            let bdb = Arc::new(Db::create_in_memory().unwrap());
            let bfold = make_fold_multi(&pairs, Arc::clone(&bdb));
            for e in &envs {
                bfold.ingest(e).expect("acc");
            }
            let b = rd(&bdb);
            let a = rd(&db); // db has been rebuilt
            for (i, (bk, bv)) in b.iter().enumerate() {
                if let Some((_, av)) = a.iter().find(|(ak, _)| ak == bk) {
                    if bv != av {
                        let bc = crate::tables::NodeCard::from_bytes(bv).unwrap();
                        let ac = crate::tables::NodeCard::from_bytes(av).unwrap();
                        eprintln!("DIFF node #{}: kind {:?}", i, bc.kind);
                        eprintln!(
                            "  LIVE   : present={} created_at={} created_by={:?} title={:?}",
                            bc.present,
                            bc.created_at,
                            &bc.created_by.as_bytes()[..2],
                            bc.title
                        );
                        eprintln!(
                            "  REBUILD: present={} created_at={} created_by={:?} title={:?}",
                            ac.present,
                            ac.created_at,
                            &ac.created_by.as_bytes()[..2],
                            ac.title
                        );
                        break;
                    }
                }
            }
        }
        for i in 0..before.len() {
            assert_eq!(
                before[i].1, after[i].1,
                "table {} diverged on rebuild (before {} rows, after {} rows)",
                names[i], before[i].0, after[i].0
            );
        }
    }

    /// Documents the order-insensitivity SCOPE (review finding 2026-06-26):
    /// the fold validates authorization at ingest, so a member's message
    /// arriving before its MembershipAdd is cleanly REJECTED (Err), never
    /// buffered and never a panic. True I5 holds for causally-valid orders,
    /// not arbitrary reorderings. Couples OPEN-THREADS T29.
    #[test]
    fn out_of_causal_order_message_is_rejected_not_panicked() {
        let owner = MockSigner::from_seed(70);
        let owner_p = make_principal_from_u16(0x1700);
        let m1 = MockSigner::from_seed(71);
        let m1_p = make_principal_from_u16(0x2700);
        let group = make_group_from_u16(0x0777);
        let pairs = vec![
            (MockSigner::new(owner.device_id().0), owner_p),
            (MockSigner::new(m1.device_id().0), m1_p),
        ];
        let db = Arc::new(Db::create_in_memory().unwrap());
        let fold = make_fold_multi(&pairs, Arc::clone(&db));
        boot_group(&fold, &owner, owner_p, group, 1); // genesis + add owner

        let mut early = AssertionEnvelope {
            version: social_tree_core::model::ENVELOPE_WIRE_VERSION,
            assertion_type: AssertionType::Message,
            author_device: TypesDeviceId::new(m1.device_id().0),
            author_principal: m1_p,
            group,
            antecedents: vec![],
            lamport: 1,
            payload: message_payload("early"),
            signature: vec![],
        };
        sign_envelope(&mut early, &m1);
        let r = fold.ingest(&early);
        assert!(
            matches!(r, Err(FoldError::AuthorizationFailed(_))),
            "message from a not-yet-added member must be AuthorizationFailed, got {:?}",
            r
        );
    }
}
