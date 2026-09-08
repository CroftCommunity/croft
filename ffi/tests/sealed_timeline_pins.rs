//! Sealed chat that reaches the TIMELINE, not just a byte array.
//!
//! `seal`/`open_sealed` move bytes. That is enough to prove AEAD grade and not
//! enough to hold a conversation: the S2 runbook's rung 5 asks that each
//! message appear on the other device *with the sender's short principal*, and
//! a plaintext `Vec<u8>` has no author, no lamport and no place in the fold.
//!
//! So the thing sealed is the **assertion envelope**. The sender authors and
//! folds it locally (so it appears in their own timeline immediately), seals
//! its canonical-with-signature bytes, and the receiver opens, decodes, folds
//! and projects. The author travels inside the envelope, which is why the far
//! device can render it without being told separately who sent it.

use croft_ffi::session::Session;
use std::path::PathBuf;

use chat_core::model::Intent;

fn temp(name: &str) -> PathBuf {
    // A process-wide counter, not just a timestamp. `SystemTime::now()` does
    // not have nanosecond resolution on macOS, so two tests running in parallel
    // can derive the SAME path — and redb then refuses with "Database already
    // open. Cannot acquire lock." That refusal is the exclusive file lock doing
    // exactly its job (one store per identity; two writers is corruption), so
    // the fix belongs here rather than anywhere near the store.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "croft-{}-{name}-{}-{n}",
        env!("CARGO_CRATE_NAME"),
        std::process::id(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("store.redb")
}

/// A founds a group and seats B **in both halves**: the lockbox and the record.
///
/// This helper originally did only the MLS invite, and every test built on it
/// that needed the timeline failed — correctly. A Welcome hands over key
/// material and nothing else; without the record B has no group, no credential
/// for A, and folds nothing it receives. "Seated" means both, and the helper
/// says so now rather than quietly meaning half of it.
fn seated_pair() -> (Session, Session) {
    let mut a = Session::open(&temp("a"), &[1u8; 32]).expect("A opens");
    let mut b = Session::open(&temp("b"), &[2u8; 32]).expect("B opens");

    a.create_group("supper club").expect("A founds a group");
    let kp = b.mls_key_package().expect("B offers a key package");
    let welcome = a.invite(&kp).expect("A invites B");
    b.accept_invite(&welcome)
        .expect("B is seated in the lockbox");

    let offer = transport_iroh::record::encode_inline(&a.group_record().expect("A's record"))
        .expect("the record encodes");
    b.accept_record(&offer).expect("B is seated in the record");

    (a, b)
}

#[test]
fn what_one_member_sends_sealed_lands_in_the_others_timeline() {
    let (mut a, mut b) = seated_pair();

    let group = first_group(&a);
    a.dispatch(Intent::SelectGroup(group))
        .expect("A selects its group");
    for c in "hello".chars() {
        a.dispatch(Intent::TypeChar(c)).unwrap();
    }

    let wire = a.send_sealed().expect("A sends sealed");

    // The sender sees their own line immediately — the local fold happened
    // before the seal, not after an acknowledgement that will never come.
    let on_a = a.view();
    assert!(
        on_a.timeline.lines.iter().any(|l| l.body == "hello"),
        "the sender's own timeline should carry the line: {:?}",
        on_a.timeline.lines
    );

    let accepted = b.receive_sealed(&wire).expect("B opens and folds it");
    assert!(accepted, "the envelope was folded, not dropped");

    b.dispatch(Intent::SelectGroup(first_group(&b))).unwrap();
    let on_b = b.view();
    let line = on_b
        .timeline
        .lines
        .iter()
        .find(|l| l.body == "hello")
        .unwrap_or_else(|| panic!("B's timeline has no such line: {:?}", on_b.timeline.lines));

    // Rung 5's actual obligation: the far device knows WHO sent it, and learns
    // that from inside the envelope rather than from the transport.
    assert!(
        !line.author.is_empty(),
        "the line arrived with no author at all"
    );
    assert!(
        !line.pending,
        "a folded line is confirmed, not an optimistic local one"
    );
}

fn first_group(s: &Session) -> social_tree_core::model::GroupId {
    match s.view().tree.rows.first() {
        Some(chat_core::view::TreeRow::Group(g)) => {
            let mut raw = [0u8; 32];
            raw.copy_from_slice(&g.id.as_bytes()[..32]);
            social_tree_core::model::GroupId::new(raw)
        }
        other => panic!("expected a group row, got {other:?}"),
    }
}

#[test]
fn a_sealed_send_with_an_empty_draft_is_refused() {
    let (mut a, _b) = seated_pair();
    a.dispatch(Intent::SelectGroup(first_group(&a))).unwrap();

    let err = a.send_sealed().expect_err("an empty draft sends nothing");

    assert!(
        err.to_string().contains("draft"),
        "the refusal should name the draft, got {err}"
    );
}

#[test]
fn a_sealed_message_from_a_group_this_device_is_not_in_is_refused_with_words() {
    let (mut a, _b) = seated_pair();
    a.dispatch(Intent::SelectGroup(first_group(&a))).unwrap();
    for c in "hi".chars() {
        a.dispatch(Intent::TypeChar(c)).unwrap();
    }
    let wire = a.send_sealed().unwrap();

    // A stranger with its own store and no seat in the group.
    let mut stranger = Session::open(&temp("stranger"), &[9u8; 32]).expect("stranger opens");

    let err = stranger
        .receive_sealed(&wire)
        .expect_err("a stranger cannot open the group's traffic");

    assert!(
        !err.to_string().is_empty(),
        "the refusal must carry words for logcat"
    );
}

#[test]
fn bytes_that_are_not_a_sealed_envelope_are_refused_rather_than_folded() {
    let (_a, mut b) = seated_pair();

    let err = b
        .receive_sealed(b"not a sealed anything")
        .expect_err("garbage is refused");

    assert!(!err.to_string().is_empty());
}
