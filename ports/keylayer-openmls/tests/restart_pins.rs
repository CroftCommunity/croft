//! The property S2 actually promises: a group survives the app closing.
//!
//! The persistence module's own tests prove the *store* works. These prove the
//! **key layer** works over it, which is a different claim: openmls has to have
//! written everything it needs, our provider has to have kept it, and the group
//! has to be reloadable from nothing but the store and a group id.
//!
//! P0-3 is what makes the last part non-obvious. openmls has **no enumeration
//! API** — nothing lists the groups in a store — so `MlsGroup::load` cannot be
//! called without a `GroupId` the shell kept itself. A store that held every
//! byte of a group and not its id would be a store nobody can open.

use keylayer_openmls::store::RedbStorage;
mod fold_harness;

use fold_harness::{add_payload, env, genesis_payload_with, MemStore};
use keylayer_openmls::store::PersistentProvider;
use keylayer_openmls::OpenMlsKeyLayer;
use social_tree_core::admission::authorize_invite_enactment;
use social_tree_core::model::{AssertionType, GroupId, PrincipalId};
use social_tree_core::ports::keylayer::KeyLayer;

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("croft-keylayer-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tempdir");
    dir
}

/// Functions rather than consts: `PrincipalId::new` is not `const`.
/// Matches the fold harness's author principal, so a slip minted from the
/// folded state and the MLS identity that enacts it are the same person.
fn alice_id() -> PrincipalId {
    PrincipalId::new([0x20; 32])
}

fn bob_id() -> PrincipalId {
    PrincipalId::new([0x24; 32])
}

#[test]
fn a_group_created_before_a_restart_is_still_there_after_one() {
    let dir = temp_dir("restart");
    let path = dir.join("mls.redb");

    let group_id = {
        let mut alice = OpenMlsKeyLayer::persistent(alice_id(), &path).expect("open");
        alice.create_group().expect("create");
        alice.group_id().expect("a created group has an id")
    }; // dropped — the app closed, the exclusive lock released

    let reopened = OpenMlsKeyLayer::persistent(alice_id(), &path).expect("reopen");
    assert_eq!(
        reopened.stored_group_ids().expect("read ids"),
        vec![group_id],
        "the group id must be in the store — openmls has no way to enumerate \
         groups, so an id we did not keep is a group nobody can open",
    );
}

#[test]
fn a_group_reloaded_after_a_restart_is_at_the_epoch_it_left_off_at() {
    // NOT "seal then open on the same layer" — MLS refuses to decrypt your own
    // application messages ("Cannot decrypt own messages"), so that test would
    // have been asserting something MLS does not do. Found by writing it that
    // way first.
    //
    // What a single identity CAN prove is that the reloaded group is the same
    // group at the same epoch, and is still able to seal. A half-survived
    // epoch fails here. The cross-member open — seal on one substrate, open on
    // another — needs two seated members and is the next test.
    let dir = temp_dir("epoch");
    let path = dir.join("mls.redb");

    let (group_id, epoch) = {
        let mut alice = OpenMlsKeyLayer::persistent(alice_id(), &path).expect("open");
        alice.create_group().expect("create");
        (alice.group_id().expect("id"), alice.epoch().expect("epoch"))
    };

    let mut reopened = OpenMlsKeyLayer::persistent(alice_id(), &path).expect("reopen");
    assert!(
        reopened.load_group().expect("load"),
        "the group must come back"
    );
    assert_eq!(reopened.group_id().expect("id"), group_id, "the same group");
    assert_eq!(reopened.epoch().expect("epoch"), epoch, "at the same epoch");
    reopened
        .seal(b"still working after the restart")
        .expect("a reloaded group must still be able to seal");
}

#[test]
fn one_member_restarts_and_the_other_can_still_open_what_they_seal() {
    // The plan's wiring-test shape at Rust grade: seal on one substrate, open
    // on another, with a process boundary in the middle. This is the property
    // the whole phase exists for — if an epoch does not survive a restart, the
    // two members are on different epochs and the conversation is over, with
    // no error until someone speaks.
    let dir = temp_dir("two-members");
    let alice_path = dir.join("alice.redb");
    let bob_path = dir.join("bob.redb");

    let fold = seated_fold();
    let mut bob = OpenMlsKeyLayer::persistent(bob_id(), &bob_path).expect("bob");
    let bob_kp = bob.key_package_bytes().expect("bob's key package");

    let welcome = {
        let mut alice = OpenMlsKeyLayer::persistent(alice_id(), &alice_path).expect("alice");
        alice.create_group().expect("create");
        alice
            .deposit_key_package(bob_id(), &bob_kp)
            .expect("deposit");
        // The real admission path, not a shortcut: the governance decision
        // folds first, then the slip is minted from the folded state, then the
        // port enacts. Reusing loopback_e2e's harness rather than adding a
        // test-only constructor to production — the S0 lesson about
        // `#[cfg(test)]` constructors cuts both ways.
        let slip = authorize_invite_enactment(&bob_id(), fold.state(&the_group()))
            .expect("the folded decision mints the slip");
        alice.add_with_welcome(slip).expect("enact").welcome
    }; // alice's process ends here, mid-conversation

    bob.join_from_welcome(&welcome).expect("bob seats");

    let mut alice = OpenMlsKeyLayer::persistent(alice_id(), &alice_path).expect("alice restarts");
    assert!(
        alice.load_group().expect("load"),
        "alice's group must come back"
    );

    let sealed = alice.seal(b"after my restart").expect("alice seals");
    let opened = bob
        .open(&sealed)
        .expect("bob opens what alice sealed after restarting");
    assert_eq!(&opened, b"after my restart");
}

#[test]
fn a_second_identity_gets_its_own_store_and_sees_no_groups() {
    // Two personas on one device must not see each other's MLS state. The
    // store is per-identity; a shared one would be a cross-persona leak of
    // exactly the material that must not leak.
    let dir = temp_dir("two-identities");
    let alice_path = dir.join("alice.redb");
    let bob_path = dir.join("bob.redb");

    {
        let mut alice = OpenMlsKeyLayer::persistent(alice_id(), &alice_path).expect("alice");
        alice.create_group().expect("create");
    }

    let bob = OpenMlsKeyLayer::persistent(bob_id(), &bob_path).expect("bob");
    assert!(
        bob.stored_group_ids().expect("read").is_empty(),
        "a fresh identity sees no groups, whatever else is on the device",
    );
}

#[test]
fn a_pending_key_package_survives_a_restart_and_staged_state_does_not() {
    // P0-3, split on PROVENANCE. A key package was handed over by another
    // person and cannot be regenerated locally — dropping it charges the
    // JOINER for our crash. Staged commits can be replayed from the log,
    // because the fold is the source of truth for those.
    //
    // Asserting both halves in one test on purpose: the decision is the split,
    // and a test of only the surviving half would pass just as well if
    // everything persisted, which is the outcome P0-3 rejected.
    let dir = temp_dir("provenance");
    let path = dir.join("mls.redb");

    let bob_kp = {
        let bob_dir = temp_dir("provenance-bob");
        let bob =
            OpenMlsKeyLayer::persistent(bob_id(), &bob_dir.join("mls.redb")).expect("bob");
        bob.key_package_bytes().expect("bob's key package")
    };

    {
        let mut alice = OpenMlsKeyLayer::persistent(alice_id(), &path).expect("alice");
        alice.create_group().expect("create");
        alice
            .deposit_key_package(bob_id(), &bob_kp)
            .expect("deposit");
        assert!(
            alice.has_pending_key_package(&bob_id()),
            "deposited before the restart"
        );
    }

    let reopened = OpenMlsKeyLayer::persistent(alice_id(), &path).expect("reopen");
    assert!(
        reopened.has_pending_key_package(&bob_id()),
        "a key package handed over by someone else must survive — it cannot be \
         regenerated on this side, so losing it makes our crash their problem",
    );
    assert!(
        reopened.staged_is_empty(),
        "staged commits replay from the log and must NOT be restored from the \
         store; two sources for the same fact is how they disagree",
    );
}

#[test]
fn a_store_written_by_a_future_version_refuses_the_whole_key_layer() {
    // The version guard has to reach the caller, not stop at the store. A key
    // layer that opened onto an empty-looking store would look like a fresh
    // install and would let someone start a second group with the same
    // identity.
    let dir = temp_dir("version");
    let path = dir.join("mls.redb");
    RedbStorage::open(&path)
        .expect("open")
        .stamp_version_for_test(RedbStorage::VERSION + 1);

    assert!(
        OpenMlsKeyLayer::persistent(alice_id(), &path).is_err(),
        "a future-version store must refuse, not open empty",
    );
}

#[test]
fn the_persistent_provider_is_the_one_the_key_layer_uses() {
    // Guards the wiring itself. If `persistent()` quietly fell back to the
    // in-memory provider, every test above still passes on a fresh store and
    // nothing survives a restart in production.
    let dir = temp_dir("wiring");
    let path = dir.join("mls.redb");
    {
        let mut alice = OpenMlsKeyLayer::persistent(alice_id(), &path).expect("open");
        alice.create_group().expect("create");
    }
    let size = std::fs::metadata(&path)
        .expect("the store file exists")
        .len();
    assert!(
        size > 0,
        "creating a group must have written to the store file"
    );

    // And the type is genuinely the persistent one, not a look-alike.
    fn assert_persistent(_: &PersistentProvider) {}
    let alice = OpenMlsKeyLayer::persistent(alice_id(), &dir.join("other.redb")).expect("open");
    assert_persistent(alice.provider());
}

/// The group id the harness envelopes name.
fn the_group() -> GroupId {
    // The harness stamps every envelope with this group.
    GroupId::new([0xC7; 32])
}

/// A fold with alice seated as owner and bob's membership decided — the state
/// `authorize_invite_enactment` reads to mint an invite slip.
fn seated_fold() -> MemStore {
    let mut store = MemStore::default();
    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::GroupGenesis,
            1,
            vec![],
            genesis_payload_with([1, 1, 1, 1]),
        ))
        .expect("genesis");
    store
        .ingest(&env(
            0x10,
            0x20,
            AssertionType::MembershipAdd,
            2,
            vec![],
            add_payload(0x24, 2),
        ))
        .expect("the governance decision folds first");
    store
}
