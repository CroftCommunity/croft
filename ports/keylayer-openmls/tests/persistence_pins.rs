//! MLS state persistence: the boundary behaviours, not "it round-trips".
//!
//! The plan names these edges explicitly, and the reason is worth restating:
//! **a single round-trip assertion would survive almost any mutation to the
//! persistence path.** Write-then-read in one process passes even if the data
//! never reaches the disk, even if the version guard is absent, and even if
//! two groups share a key. Each of those is a real way this module can be
//! wrong, so each gets its own test.
//!
//! What is being protected: MLS state corruption across restarts. The failure
//! is not "the app forgets a message" — it is a group that cannot decrypt,
//! because an epoch advanced in memory and did not survive. That is
//! unrecoverable for the user and silent until someone speaks.

use keylayer_openmls::store::{RedbStorage, StorageError};
use openmls_traits::storage::StorageProvider;

fn temp_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("croft-mls-persist-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tempdir");
    dir.join("mls.redb")
}

// ---------------------------------------------------------------------------
// The version guard — P0-2's "fails loud"
// ---------------------------------------------------------------------------

#[test]
fn a_fresh_store_stamps_its_version() {
    let path = temp_path("stamp");
    let _ = std::fs::remove_file(&path);
    let store = RedbStorage::open(&path).expect("open");
    assert_eq!(
        store.stored_version().expect("read version"),
        Some(RedbStorage::VERSION),
    );
}

#[test]
fn a_store_from_a_future_version_is_refused_by_name_not_read_as_empty() {
    // The trap D2 found: `CURRENT_VERSION` is baked into every openmls key, so
    // a version bump turns every read into `Ok(None)` — which openmls reads as
    // "this group does not exist". A silent miss is the worst possible shape
    // for this failure: the group does not fail to load, it loads as nothing.
    // The guard exists so the store refuses to open at all.
    let path = temp_path("future");
    let _ = std::fs::remove_file(&path);
    RedbStorage::open(&path)
        .expect("open")
        .stamp_version_for_test(RedbStorage::VERSION + 1);

    match RedbStorage::open(&path) {
        Err(StorageError::VersionMismatch { found, expected }) => {
            assert_eq!(found, RedbStorage::VERSION + 1);
            assert_eq!(expected, RedbStorage::VERSION);
        }
        other => panic!("a future-version store must be refused by name, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// The six helpers — where every one of the 57 methods actually lives
// ---------------------------------------------------------------------------
//
// All 57 trait methods delegate to six helpers, so the helpers are where a
// mutation would survive unnoticed. Testing them through the trait's own
// methods (rather than directly) keeps the tests honest about the key layout,
// which is the part openmls cares about.

#[test]
fn a_written_value_reads_back_within_the_same_process() {
    let path = temp_path("same-process");
    let _ = std::fs::remove_file(&path);
    let store = RedbStorage::open(&path).expect("open");

    let group = TestKey(b"group-1".to_vec());
    let config = TestValue(b"join-config".to_vec());
    store.write_mls_join_config(&group, &config).expect("write");

    let read: Option<TestValue> = store.mls_group_join_config(&group).expect("read");
    assert_eq!(read, Some(config));
}

#[test]
fn an_absent_key_reads_as_none_rather_than_erroring() {
    let path = temp_path("absent");
    let _ = std::fs::remove_file(&path);
    let store = RedbStorage::open(&path).expect("open");
    let read: Option<TestValue> = store
        .mls_group_join_config(&TestKey(b"never-written".to_vec()))
        .expect("read");
    assert_eq!(read, None);
}

#[test]
fn two_groups_do_not_share_a_key() {
    // A key layout that dropped the group id would pass every single-group
    // test in this file and corrupt every multi-group install.
    let path = temp_path("two-groups");
    let _ = std::fs::remove_file(&path);
    let store = RedbStorage::open(&path).expect("open");

    store
        .write_mls_join_config(&TestKey(b"a".to_vec()), &TestValue(b"config-a".to_vec()))
        .expect("write a");
    store
        .write_mls_join_config(&TestKey(b"b".to_vec()), &TestValue(b"config-b".to_vec()))
        .expect("write b");

    let a: Option<TestValue> = store
        .mls_group_join_config(&TestKey(b"a".to_vec()))
        .expect("read a");
    let b: Option<TestValue> = store
        .mls_group_join_config(&TestKey(b"b".to_vec()))
        .expect("read b");
    assert_eq!(a, Some(TestValue(b"config-a".to_vec())));
    assert_eq!(b, Some(TestValue(b"config-b".to_vec())));
}

#[test]
fn two_different_kinds_of_value_do_not_share_a_key() {
    // The label is what separates "the tree for group X" from "the context for
    // group X". Drop it and one silently overwrites the other.
    let path = temp_path("two-labels");
    let _ = std::fs::remove_file(&path);
    let store = RedbStorage::open(&path).expect("open");
    let group = TestKey(b"g".to_vec());

    store
        .write_tree(&group, &TestValue(b"the-tree".to_vec()))
        .expect("tree");
    store
        .write_context(&group, &TestValue(b"the-context".to_vec()))
        .expect("ctx");

    let tree: Option<TestValue> = store.tree(&group).expect("read tree");
    let ctx: Option<TestValue> = store.group_context(&group).expect("read ctx");
    assert_eq!(tree, Some(TestValue(b"the-tree".to_vec())));
    assert_eq!(ctx, Some(TestValue(b"the-context".to_vec())));
}

#[test]
fn an_appended_list_keeps_every_item_in_order() {
    let path = temp_path("append");
    let _ = std::fs::remove_file(&path);
    let store = RedbStorage::open(&path).expect("open");
    let group = TestKey(b"g".to_vec());

    for n in [b"one", b"two"] {
        store
            .append_own_leaf_node(&group, &TestValue(n.to_vec()))
            .expect("append");
    }
    let nodes: Vec<TestValue> = store.own_leaf_nodes(&group).expect("read");
    assert_eq!(
        nodes,
        vec![TestValue(b"one".to_vec()), TestValue(b"two".to_vec())],
    );
}

#[test]
fn an_empty_list_reads_as_empty_rather_than_erroring() {
    let path = temp_path("empty-list");
    let _ = std::fs::remove_file(&path);
    let store = RedbStorage::open(&path).expect("open");
    let nodes: Vec<TestValue> = store
        .own_leaf_nodes(&TestKey(b"never".to_vec()))
        .expect("read");
    assert!(nodes.is_empty());
}

#[test]
fn a_deleted_value_is_gone_and_says_so() {
    let path = temp_path("delete");
    let _ = std::fs::remove_file(&path);
    let store = RedbStorage::open(&path).expect("open");
    let group = TestKey(b"g".to_vec());

    store
        .write_tree(&group, &TestValue(b"t".to_vec()))
        .expect("write");
    store.delete_tree(&group).expect("delete");
    let read: Option<TestValue> = store.tree(&group).expect("read");
    assert_eq!(read, None, "a deleted value reads absent, not stale");
}

// ---------------------------------------------------------------------------
// Across a process boundary — the whole point
// ---------------------------------------------------------------------------

#[test]
fn state_survives_a_clean_restart() {
    let path = temp_path("clean-restart");
    let _ = std::fs::remove_file(&path);
    let group = TestKey(b"g".to_vec());

    {
        let store = RedbStorage::open(&path).expect("open");
        store
            .write_tree(&group, &TestValue(b"epoch-3-tree".to_vec()))
            .expect("write");
        store
            .append_own_leaf_node(&group, &TestValue(b"leaf".to_vec()))
            .expect("append");
    } // dropped — the handle is gone, as it is when the app closes

    let reopened = RedbStorage::open(&path).expect("reopen");
    let tree: Option<TestValue> = reopened.tree(&group).expect("read");
    let nodes: Vec<TestValue> = reopened.own_leaf_nodes(&group).expect("read");
    assert_eq!(tree, Some(TestValue(b"epoch-3-tree".to_vec())));
    assert_eq!(
        nodes,
        vec![TestValue(b"leaf".to_vec())],
        "lists survive too"
    );
}

#[test]
fn a_store_refuses_a_second_handle_rather_than_corrupting_quietly() {
    // Discovered by writing the durability test the wrong way. redb takes an
    // EXCLUSIVE file lock, so two handles on one path cannot coexist — which
    // means the shell must hold exactly one store per identity and cannot
    // casually open a second to peek. Pinned because it constrains callers,
    // and because a refusal here is the good outcome: two writers on one MLS
    // store would be corruption, and a lock is how that is prevented rather
    // than detected afterwards.
    let path = temp_path("exclusive");
    let _ = std::fs::remove_file(&path);
    let _first = RedbStorage::open(&path).expect("first handle");
    match RedbStorage::open(&path) {
        Err(StorageError::Storage(msg)) => assert!(
            msg.to_lowercase().contains("lock") || msg.to_lowercase().contains("already open"),
            "the refusal should name the lock, got: {msg}",
        ),
        other => panic!("a second handle must be refused, got {other:?}"),
    }
}

#[test]
fn a_write_survives_a_process_that_never_gets_to_clean_up() {
    // The kill-mid-epoch case, for real. A child process writes and then
    // `abort()`s: no destructors, no drop, no flush-on-close. If the provider
    // batched writes and committed at drop, the value would be gone — and that
    // provider would pass every other test in this file, including the clean
    // restart, while losing an epoch to any real crash. Losing an epoch is not
    // a lost message; it is a group that can no longer decrypt.
    let path = temp_path("abort");
    let _ = std::fs::remove_file(&path);

    let exe = std::env::current_exe().expect("test binary");
    let status = std::process::Command::new(exe)
        .env("CROFT_ABORT_WRITE_TO", &path)
        .args(["--exact", "the_child_writes_then_aborts", "--nocapture"])
        .status()
        .expect("spawn child");
    assert!(
        !status.success(),
        "the child is meant to abort, not exit cleanly"
    );

    let reopened = RedbStorage::open(&path).expect("reopen after the abort");
    let tree: Option<TestValue> = reopened.tree(&TestKey(b"g".to_vec())).expect("read");
    assert_eq!(
        tree,
        Some(TestValue(b"written-then-killed".to_vec())),
        "a write must be committed by the time it returns, not at drop",
    );
}

/// The child half of the abort test. Inert unless the env var is set, so it is
/// a no-op in an ordinary run and does not need to be filtered out.
#[test]
fn the_child_writes_then_aborts() {
    let Ok(path) = std::env::var("CROFT_ABORT_WRITE_TO") else {
        return;
    };
    let store = RedbStorage::open(std::path::Path::new(&path)).expect("child open");
    store
        .write_tree(
            &TestKey(b"g".to_vec()),
            &TestValue(b"written-then-killed".to_vec()),
        )
        .expect("child write");
    // No unwinding, no destructors, no flush. Exactly what a kill -9 does.
    std::process::abort();
}

#[test]
fn an_overwrite_after_a_restart_replaces_rather_than_accumulating() {
    let path = temp_path("overwrite");
    let _ = std::fs::remove_file(&path);
    let group = TestKey(b"g".to_vec());

    {
        let store = RedbStorage::open(&path).expect("open");
        store
            .write_tree(&group, &TestValue(b"epoch-1".to_vec()))
            .expect("write");
    }
    {
        let store = RedbStorage::open(&path).expect("reopen");
        store
            .write_tree(&group, &TestValue(b"epoch-2".to_vec()))
            .expect("write");
    }

    let final_store = RedbStorage::open(&path).expect("reopen");
    let tree: Option<TestValue> = final_store.tree(&group).expect("read");
    assert_eq!(
        tree,
        Some(TestValue(b"epoch-2".to_vec())),
        "an epoch advance replaces the tree; keeping both would be a fork nobody asked for",
    );
}

// ---------------------------------------------------------------------------
// Test entities
// ---------------------------------------------------------------------------
//
// openmls's storage traits are marker traits over `Serialize`/`DeserializeOwned`
// — the provider never inspects an MLS value, which is exactly why a provider
// can be tested with stand-in types. Using real MLS types here would test
// openmls, not this module.

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct TestKey(Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct TestValue(Vec<u8>);

const V: u16 = openmls_traits::storage::CURRENT_VERSION;

impl openmls_traits::storage::Key<V> for TestKey {}
impl openmls_traits::storage::traits::GroupId<V> for TestKey {}

impl openmls_traits::storage::Entity<V> for TestValue {}
impl openmls_traits::storage::traits::MlsGroupJoinConfig<V> for TestValue {}
impl openmls_traits::storage::traits::TreeSync<V> for TestValue {}
impl openmls_traits::storage::traits::GroupContext<V> for TestValue {}
impl openmls_traits::storage::traits::LeafNode<V> for TestValue {}
impl openmls_traits::storage::traits::QueuedProposal<V> for TestValue {}
impl openmls_traits::storage::traits::ProposalRef<V> for TestKey {}
impl openmls_traits::storage::Entity<V> for TestKey {}

// ---------------------------------------------------------------------------
// Closing the two gaps a mutation audit found (2026-08-27)
// ---------------------------------------------------------------------------
//
// `cargo mutants` over the six helpers: 11 caught, 1 unviable, **2 missed** —
// both in `remove_from_list`, and both real gaps rather than equivalent
// mutants. Replacing the whole function with `Ok(())` survived, so nothing
// tested that removal removes; and flipping `==` to `!=` in its position
// search survived, so nothing tested that it removes the RIGHT item.
//
// That function is what `remove_proposal` uses to take a proposal off a
// group's queue. Un-tested, a proposal could stay queued after being removed
// (openmls would re-process it) or the wrong one could vanish (openmls would
// process a proposal the caller meant to drop). Neither is loud.

#[test]
fn removing_a_queued_proposal_actually_takes_it_off_the_queue() {
    let path = temp_path("remove-proposal");
    let _ = std::fs::remove_file(&path);
    let store = RedbStorage::open(&path).expect("open");
    let group = TestKey(b"g".to_vec());

    store
        .queue_proposal(&group, &TestKey(b"ref-1".to_vec()), &TestValue(b"p1".to_vec()))
        .expect("queue");
    store.remove_proposal(&group, &TestKey(b"ref-1".to_vec())).expect("remove");

    let refs: Vec<TestKey> = store.queued_proposal_refs(&group).expect("read refs");
    assert!(
        refs.is_empty(),
        "a removed proposal must leave the queue — left on it, openmls \
         re-processes a proposal the caller meant to drop",
    );
}

#[test]
fn removing_one_queued_proposal_leaves_the_others_alone() {
    // The `==` -> `!=` mutant: a position search that matches the wrong item
    // removes the wrong proposal, and every single-item test still passes.
    let path = temp_path("remove-right-one");
    let _ = std::fs::remove_file(&path);
    let store = RedbStorage::open(&path).expect("open");
    let group = TestKey(b"g".to_vec());

    for name in [b"ref-1", b"ref-2", b"ref-3"] {
        store
            .queue_proposal(
                &group,
                &TestKey(name.to_vec()),
                &TestValue(name.to_vec()),
            )
            .expect("queue");
    }
    store.remove_proposal(&group, &TestKey(b"ref-2".to_vec())).expect("remove");

    let refs: Vec<TestKey> = store.queued_proposal_refs(&group).expect("read refs");
    assert_eq!(
        refs,
        vec![TestKey(b"ref-1".to_vec()), TestKey(b"ref-3".to_vec())],
        "exactly the named proposal goes, and the order of the rest is kept",
    );
}

#[test]
fn clearing_the_queue_removes_every_proposal_and_its_refs() {
    let path = temp_path("clear-queue");
    let _ = std::fs::remove_file(&path);
    let store = RedbStorage::open(&path).expect("open");
    let group = TestKey(b"g".to_vec());

    for name in [b"ref-1", b"ref-2"] {
        store
            .queue_proposal(&group, &TestKey(name.to_vec()), &TestValue(name.to_vec()))
            .expect("queue");
    }
    StorageProvider::<{ openmls_traits::storage::CURRENT_VERSION }>::clear_proposal_queue::<
        TestKey,
        TestKey,
    >(&store, &group)
    .expect("clear");

    let refs: Vec<TestKey> = store.queued_proposal_refs(&group).expect("read refs");
    assert!(refs.is_empty(), "the queue is empty after clearing it");
}
