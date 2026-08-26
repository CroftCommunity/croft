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
use store_redb::tables::{Db, AUTH_ASSERTIONS};

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
