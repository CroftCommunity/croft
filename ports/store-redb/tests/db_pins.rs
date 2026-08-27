//! The store's edges, pinned from *outside* the crate.
//!
//! Outside is the point. Two of the frictions the promotion inherited are
//! invisible from an inline `#[cfg(test)] mod tests`: a `#[cfg(test)]`
//! constructor is not reachable across a crate boundary (the core documents
//! this same footgun at `ports/mod.rs:118-120`), and a table constant that is
//! only ever named by string literal elsewhere can drift from its canonical
//! declaration without any inline test noticing.
//!
//! These are the plan's named store edges (P7 S0 § Test specifics): an empty
//! store's reads, a key absent vs present, a round-trip that survives reopen,
//! and a write that overwrites where overwriting is the contract.

use redb::ReadableTableMetadata;
use store_redb::tables::{Db, EdgeType, AUTH_ASSERTIONS};

/// The canonical constants must be the ones a caller can actually read
/// through. Anything that only works via a re-declared string literal is the
/// silent-wrong-table bug this pin exists to prevent.
fn write_assertion(db: &Db, key: &[u8], value: &[u8]) {
    let txn = db.inner().begin_write().expect("begin_write");
    {
        let mut table = txn.open_table(AUTH_ASSERTIONS).expect("open_table");
        table.insert(key, value).expect("insert");
    }
    txn.commit().expect("commit");
}

fn read_assertion(db: &Db, key: &[u8]) -> Option<Vec<u8>> {
    let txn = db.inner().begin_read().expect("begin_read");
    let table = txn.open_table(AUTH_ASSERTIONS).expect("open_table");
    table.get(key).expect("get").map(|v| v.value().to_vec())
}

#[test]
fn an_in_memory_db_is_constructible_from_outside_the_crate() {
    // The promotion's first obligation: the constructor the fold's own callers
    // need is not gated behind `#[cfg(test)]`.
    let db = Db::create_in_memory().expect("create_in_memory");
    let txn = db.inner().begin_write().expect("begin_write");
    txn.open_table(AUTH_ASSERTIONS).expect("open_table");
    txn.commit().expect("commit");
}

#[test]
fn an_empty_store_reads_as_absent_not_as_an_error() {
    let db = Db::create_in_memory().expect("create_in_memory");
    // No write of any kind has happened. A table that has never been written
    // is not an error condition; it is empty — and redb only agrees once the
    // schema is created up front, which is why `Db` creates it rather than
    // leaving it to whichever component happens to construct first.
    assert_eq!(read_assertion(&db, b"never-written"), None);
}

#[test]
fn a_key_reads_absent_before_its_write_and_present_after() {
    let db = Db::create_in_memory().expect("create_in_memory");
    assert_eq!(read_assertion(&db, b"k"), None, "absent before the write");
    write_assertion(&db, b"k", b"v");
    assert_eq!(
        read_assertion(&db, b"k").as_deref(),
        Some(&b"v"[..]),
        "present after the write"
    );
}

#[test]
fn a_write_survives_closing_and_reopening_the_file() {
    // The property that makes this crate worth promoting at all: durability
    // across a process boundary. An in-memory backend cannot answer it, so
    // this pin uses a real file and drops the handle in between.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("store.redb");

    {
        let db = Db::open(&path).expect("open");
        write_assertion(&db, b"durable", b"still here");
    }

    let reopened = Db::open(&path).expect("reopen");
    assert_eq!(
        read_assertion(&reopened, b"durable").as_deref(),
        Some(&b"still here"[..]),
        "the value must survive the handle being dropped"
    );
}

#[test]
fn a_second_write_to_one_key_overwrites_rather_than_accumulating() {
    // Overwrite is the contract for the content-addressed tables: the same key
    // is the same fact, so a re-ingest must not double the store. The mirror
    // property — that the *by-device* index appends under distinct keys — is
    // pinned by the fold's own suite, which owns the key layout.
    let db = Db::create_in_memory().expect("create_in_memory");
    write_assertion(&db, b"k", b"first");
    write_assertion(&db, b"k", b"second");

    assert_eq!(read_assertion(&db, b"k").as_deref(), Some(&b"second"[..]));

    let txn = db.inner().begin_read().expect("begin_read");
    let table = txn.open_table(AUTH_ASSERTIONS).expect("open_table");
    assert_eq!(table.len().expect("len"), 1, "one key, one row");
}

// ---------------------------------------------------------------------------
// The key decoders refuse rather than panic
// ---------------------------------------------------------------------------
//
// These decode *stored bytes*. An unknown discriminant is not a programming
// error — it is what a store written by a future version, or a corrupted file,
// looks like. Every value decoder in `tables` already returns `Result`
// (`EdgeMeta::from_bytes`, `NodeCard::from_bytes`), and `DbError::KeyLength`
// exists for exactly this and is constructed nowhere. The key decoders were
// the odd ones out: they panicked.

#[test]
fn an_edge_key_of_the_wrong_width_is_refused_not_panicked() {
    for len in [0usize, 67, 69] {
        let err = store_redb::tables::decode_edge_out_key(&vec![0u8; len])
            .expect_err("a wrong-width key must be refused");
        match err {
            store_redb::tables::DbError::KeyLength { expected, got } => {
                assert_eq!(expected, 68);
                assert_eq!(got, len);
            }
            other => panic!("expected KeyLength, got {other:?}"),
        }
    }
}

#[test]
fn an_edge_key_with_an_unknown_edge_type_is_refused() {
    // A discriminant this version does not know — what a forward-version store
    // hands back. Refusing names the value; panicking would take the process
    // down on a read.
    let mut key = [0u8; 68];
    key[0] = 0x01; // a valid source KindTag
    key[35] = 0x01; // a valid target KindTag
    key[33..35].copy_from_slice(&0xBEEFu16.to_be_bytes());
    let err = store_redb::tables::decode_edge_out_key(&key).expect_err("unknown edge type");
    assert!(
        matches!(err, store_redb::tables::DbError::Deserialize(ref m) if m.contains("48879")),
        "the refusal must name the discriminant it did not recognise, got: {err}"
    );
}

#[test]
fn an_edge_key_with_an_invalid_kind_tag_is_refused() {
    let mut key = [0u8; 68];
    key[0] = 0xFF; // not a KindTag
    key[35] = 0x01;
    key[33..35].copy_from_slice(&EdgeType::MemberOf.to_be_bytes());
    let err = store_redb::tables::decode_edge_out_key(&key).expect_err("invalid source kind");
    assert!(
        matches!(err, store_redb::tables::DbError::Deserialize(ref m) if m.contains("source")),
        "the refusal must say which end was bad, got: {err}"
    );
}

#[test]
fn a_by_device_key_of_the_wrong_width_is_refused() {
    let err = store_redb::tables::decode_by_device_key(&[0u8; 39])
        .expect_err("a wrong-width key must be refused");
    assert!(matches!(
        err,
        store_redb::tables::DbError::KeyLength {
            expected: 40,
            got: 39
        }
    ));
}

#[test]
fn a_gov_log_key_of_the_wrong_width_is_refused() {
    let err = store_redb::tables::decode_gov_log_key(&[0u8; 41])
        .expect_err("a wrong-width key must be refused");
    assert!(matches!(
        err,
        store_redb::tables::DbError::KeyLength {
            expected: 40,
            got: 41
        }
    ));
}
