//! The read side, pinned by writing through the real fold first.
//!
//! Every test here authors assertions with [`store_redb::payload`]'s encoders,
//! signs them with real Ed25519, ingests them through [`DerivedFold`], and then
//! asks the read module what it sees. That round trip is the point: two of the
//! three encoders live in a different crate from their decoder, so nothing
//! short of driving the actual fold can catch the two halves drifting apart.
//! A codec test in `payload.rs` would only prove that module agrees with
//! itself.
//!
//! These reads are what the corpus's `surface.rs` did and did not bring with
//! it. They return substrate types, never view models — assembling a view was
//! precisely what made `surface.rs` impossible to split, and repeating that
//! here would rebuild the thing we deliberately left behind.

use std::sync::Arc;

use social_tree_core::model::{
    encode_message_payload, envelope_hash, AssertionEnvelope, AssertionType, GroupId, Hash,
    PrincipalId, Role, ENVELOPE_WIRE_VERSION,
};
use social_tree_core::ports::ed25519::{
    Ed25519Signer, Ed25519Verifier, RegistryCredentialResolver,
};
use social_tree_core::ports::{DeviceId as PortDeviceId, PrincipalId as PortPrincipalId, Signer};
use store_redb::fold_derived::DerivedFold;
use store_redb::payload::{encode_genesis_payload, encode_membership_add_payload, GenesisRules};
use store_redb::read;
use store_redb::tables::Db;

/// One local identity with a real signing key, and a fold that accepts it.
struct Fixture {
    db: Arc<Db>,
    fold: DerivedFold<Ed25519Verifier, RegistryCredentialResolver>,
    signer: Ed25519Signer,
    device: social_tree_core::model::DeviceId,
    principal: PrincipalId,
    lamport: u64,
}

impl Fixture {
    fn new(seed: u8) -> Self {
        let signer = Ed25519Signer::from_seed([seed; 32]);
        let device = social_tree_core::model::DeviceId::new(signer.device_id().0);
        // One device, one principal: S0's single local identity. The persona
        // layer (S3) is what makes these genuinely different things.
        let principal = PrincipalId::new(signer.device_id().0);

        let resolver = RegistryCredentialResolver::default();
        resolver.register(
            PortDeviceId(signer.device_id().0),
            PortPrincipalId(*principal.as_bytes()),
        );

        let db = Arc::new(Db::create_in_memory().expect("in-memory db"));
        let fold = DerivedFold::new(Arc::clone(&db), Ed25519Verifier, resolver);
        Self {
            db,
            fold,
            signer,
            device,
            principal,
            lamport: 0,
        }
    }

    fn sign_and_ingest(
        &mut self,
        assertion_type: AssertionType,
        group: GroupId,
        antecedents: Vec<Hash>,
        payload: Vec<u8>,
    ) -> Hash {
        let mut env = AssertionEnvelope {
            version: ENVELOPE_WIRE_VERSION,
            assertion_type,
            author_device: self.device,
            author_principal: self.principal,
            group,
            antecedents,
            lamport: self.lamport,
            payload,
            signature: vec![],
        };
        env.signature = self.signer.sign(&env.canonical_bytes());
        self.lamport += 1;
        self.fold.ingest(&env).expect("ingest must succeed");
        envelope_hash(&env)
    }

    /// Genesis plus the founder's own MembershipAdd — genesis seats the author
    /// as Owner in the folded state, but it is the MembershipAdd that writes
    /// the MEMBER_OF edge, and the edge is how "the groups I am in" is found.
    fn found_group(&mut self, id_seed: u8) -> GroupId {
        let group = GroupId::new([id_seed; 32]);
        let genesis = self.sign_and_ingest(
            AssertionType::GroupGenesis,
            group,
            vec![],
            encode_genesis_payload(GenesisRules::SOLO_FOUNDER, &self.device),
        );
        self.sign_and_ingest(
            AssertionType::MembershipAdd,
            group,
            vec![genesis],
            encode_membership_add_payload(&self.principal, Role::Owner),
        );
        group
    }

    fn say(&mut self, group: GroupId, body: &str) -> Hash {
        self.sign_and_ingest(
            AssertionType::Message,
            group,
            vec![],
            encode_message_payload(body, None, None),
        )
    }
}

// ---------------------------------------------------------------------------

#[test]
fn a_principal_in_no_groups_reads_as_an_empty_list_not_an_error() {
    let f = Fixture::new(0x01);
    assert_eq!(
        read::groups_for_principal(&f.db, &f.principal).expect("read"),
        Vec::<GroupId>::new()
    );
}

#[test]
fn a_founded_group_is_one_the_founder_is_a_member_of() {
    let mut f = Fixture::new(0x02);
    let group = f.found_group(0xAA);
    assert_eq!(
        read::groups_for_principal(&f.db, &f.principal).expect("read"),
        vec![group],
        "the genesis payload this crate encoded must fold into a membership the \
         read side can see — the drift check"
    );
}

#[test]
fn a_principal_sees_only_their_own_groups() {
    let mut a = Fixture::new(0x03);
    let group_a = a.found_group(0xA1);
    let mut b = Fixture::new(0x04);
    b.found_group(0xB1);

    assert_eq!(
        read::groups_for_principal(&a.db, &a.principal).expect("read"),
        vec![group_a]
    );
    assert_eq!(
        read::groups_for_principal(&a.db, &b.principal).expect("read"),
        Vec::<GroupId>::new(),
        "a principal with no membership in this store reads empty, not everything"
    );
}

#[test]
fn an_empty_group_has_an_empty_timeline() {
    let mut f = Fixture::new(0x05);
    let group = f.found_group(0xCC);
    assert!(read::messages_in_group(&f.db, &group)
        .expect("read")
        .is_empty());
}

#[test]
fn messages_come_back_in_lamport_order_whatever_order_they_were_written() {
    let mut f = Fixture::new(0x06);
    let group = f.found_group(0xDD);
    f.say(group, "first");
    f.say(group, "second");
    f.say(group, "third");

    let bodies: Vec<String> = read::messages_in_group(&f.db, &group)
        .expect("read")
        .into_iter()
        .map(|m| m.body)
        .collect();
    assert_eq!(bodies, vec!["first", "second", "third"]);
}

#[test]
fn a_message_carries_its_author_and_its_lamport_not_just_its_body() {
    let mut f = Fixture::new(0x07);
    let group = f.found_group(0xEE);
    f.say(group, "attributed");

    let msgs = read::messages_in_group(&f.db, &group).expect("read");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].author, f.principal, "an unattributed line is a lie");
    assert!(msgs[0].lamport > 0);
}

#[test]
fn a_groups_messages_do_not_leak_into_another_groups_timeline() {
    let mut f = Fixture::new(0x08);
    let one = f.found_group(0xF1);
    let two = f.found_group(0xF2);
    f.say(one, "for one");
    f.say(two, "for two");

    let in_one: Vec<String> = read::messages_in_group(&f.db, &one)
        .expect("read")
        .into_iter()
        .map(|m| m.body)
        .collect();
    assert_eq!(in_one, vec!["for one"]);
}

#[test]
fn the_founder_is_the_owner_in_the_membership_the_fold_derived() {
    let mut f = Fixture::new(0x09);
    let group = f.found_group(0xF3);

    let members = read::members_of_group(&f.db, &group).expect("read");
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].0, f.principal);
    assert_eq!(members[0].1, Role::Owner);
}

#[test]
fn a_group_with_no_folded_state_reads_as_no_members_rather_than_erroring() {
    let f = Fixture::new(0x0a);
    let ghost = GroupId::new([0x77; 32]);
    assert!(read::members_of_group(&f.db, &ghost)
        .expect("read")
        .is_empty());
}
