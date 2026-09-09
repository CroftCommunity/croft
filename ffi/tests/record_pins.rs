//! Reading a record, judging it, and only then folding it.
//!
//! The shape here is the owner's (2026-09-04): accepting a group's record is
//! not something a scan does to you. It is a **bounded exchange** — scan, see
//! who this is and what they claim, accept — whose output is a trust judgment
//! in a relationship. So the session offers two calls with the judgment
//! between them, and `read_record` is required to touch nothing.
//!
//! The nearest familiar thing is accepting an SSH host key the first time. The
//! system is not computing that the other party is trustworthy; it is recording
//! that a person decided to proceed. That is the same shape S3 commits to for
//! the DID-to-persona binding — "a vouch-shaped fact, a human judgment the
//! system records, never computes" — arriving a phase early.

use chat_core::model::Intent;
use croft_ffi::session::Session;
use std::path::PathBuf;
use transport_iroh::record::{encode_elsewhere, encode_inline};

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

fn group_of(s: &Session) -> social_tree_core::model::GroupId {
    match s.view().tree.rows.first() {
        Some(chat_core::view::TreeRow::Group(g)) => {
            let mut raw = [0u8; 32];
            raw.copy_from_slice(&g.id.as_bytes()[..32]);
            social_tree_core::model::GroupId::new(raw)
        }
        other => panic!("expected a group row, got {other:?}"),
    }
}

/// Alice founds a group and invites Bob: MLS seats him, and she offers her
/// record. Bob has not accepted anything yet.
fn alice_and_bob_at(alice_path: &std::path::Path) -> (Session, Session, Vec<u8>) {
    let mut alice = Session::open(alice_path, &[1u8; 32]).expect("Alice opens");
    let mut bob = Session::open(&temp("bob"), &[2u8; 32]).expect("Bob opens");

    alice.create_group("supper club").expect("Alice founds it");
    let kp = bob.mls_key_package().expect("Bob's key package");
    let welcome = alice.invite(&kp).expect("Alice invites Bob");
    bob.accept_invite(&welcome).expect("MLS seats Bob");

    let offer = encode_inline(&alice.group_record().expect("Alice's record")).expect("it encodes");
    (alice, bob, offer)
}

fn alice_and_bob() -> (Session, Session, Vec<u8>) {
    alice_and_bob_at(&temp("alice"))
}

#[test]
fn reading_a_record_says_what_it_claims_and_changes_nothing() {
    let (_alice, bob, offer) = alice_and_bob();

    let claims = bob
        .read_record(&offer)
        .expect("Bob can read what is offered");

    assert!(claims.assertion_count >= 2, "genesis and a membership add");
    assert!(
        claims.would_seat_me,
        "Bob should be able to see that this record seats HIM before accepting it"
    );
    assert!(
        !claims.seats.is_empty(),
        "a record that seats nobody is not worth accepting"
    );

    // The obligation that makes this two calls rather than one.
    assert!(
        bob.view().tree.rows.is_empty(),
        "reading a record must fold nothing: Bob's tree is still empty"
    );
}

/// The payoff. Once Bob has judged and accepted, Alice's messages stop being
/// undifferentiated bytes and become lines with an author.
#[test]
fn after_accepting_the_record_a_sealed_message_lands_in_the_timeline() {
    let (mut alice, mut bob, offer) = alice_and_bob();

    bob.accept_record(&offer).expect("Bob accepts");

    alice
        .dispatch(Intent::SelectGroup(group_of(&alice)))
        .unwrap();
    for c in "bring bread".chars() {
        alice.dispatch(Intent::TypeChar(c)).unwrap();
    }
    let wire = alice.send_sealed().expect("Alice sends");

    let accepted = bob.receive_sealed(&wire).expect("Bob opens it");
    assert!(accepted, "the fold took it now that the record is in place");

    bob.dispatch(Intent::SelectGroup(group_of(&bob))).unwrap();
    let line = bob
        .view()
        .timeline
        .lines
        .into_iter()
        .find(|l| l.body == "bring bread")
        .expect("Bob's timeline carries Alice's message");

    assert!(!line.author.is_empty(), "and it knows who said it");
    assert!(!line.pending, "it is folded, not an optimistic local echo");
}

#[test]
fn accepting_a_record_seats_the_group_on_the_joining_device() {
    let (_alice, mut bob, offer) = alice_and_bob();

    bob.accept_record(&offer).expect("Bob accepts");

    assert!(
        !bob.view().tree.rows.is_empty(),
        "the group should now exist on Bob's device"
    );
}

/// The form exists on the wire so this day is not a breaking change. Nothing
/// fetches yet, and the refusal has to say THAT rather than look like damage.
#[test]
fn a_record_offered_elsewhere_is_refused_in_words_that_name_why() {
    let (_alice, bob, _offer) = alice_and_bob();
    let pointer = encode_elsewhere("croft://record/somewhere").unwrap();

    let err = bob
        .read_record(&pointer)
        .expect_err("this build cannot fetch");

    let said = err.to_string().to_lowercase();
    assert!(
        said.contains("fetch") || said.contains("elsewhere") || said.contains("points"),
        "the refusal should say it points elsewhere, got {said:?}"
    );
}

#[test]
fn a_record_whose_signature_does_not_hold_is_refused() {
    let (alice, bob, _offer) = alice_and_bob();
    let mut envelopes = alice.group_record().unwrap();

    // Corrupt the last byte of the first envelope's signature.
    let last = envelopes[0].len() - 1;
    envelopes[0][last] ^= 0xFF;
    let tampered = encode_inline(&envelopes).unwrap();

    let err = bob
        .read_record(&tampered)
        .expect_err("a broken signature is refused before anything is folded");

    assert!(!err.to_string().is_empty(), "and it says so");
}

#[test]
fn bytes_that_are_not_a_record_are_refused() {
    let (_alice, bob, _offer) = alice_and_bob();

    assert!(bob.read_record(b"not a record").is_err());
}

/// Gossip delivers a message to a member more than once whenever the swarm has
/// more than one path, so a second accept is a normal event and must not be a
/// failure.
#[test]
fn accepting_the_same_record_twice_is_not_a_failure() {
    let (_alice, mut bob, offer) = alice_and_bob();

    bob.accept_record(&offer).expect("first accept");
    let again = bob.accept_record(&offer);

    assert!(
        again.is_ok(),
        "a repeated record must not be an error: {again:?}"
    );
}

/// **A repeat accept must be DISTINGUISHABLE, not merely survivable.**
///
/// The test above pins that a second accept does not fail. That is necessary
/// and not sufficient: "did not fail" is what a surface reads when it renders
/// an Accept that folded nothing exactly like an Accept that folded everything,
/// which is the no-op Accept the S2 device run flagged as an open UI question.
///
/// The count is the live signal (CLAUDE.md, "a surface never claims a
/// capability it has not confirmed"): the first accept folds the record's
/// assertions, the second folds none of them because every one is already in
/// the fold. A shell that reads the count can say "you were already in this
/// group" and mean it; a shell that reads only `is_ok()` cannot.
///
/// This pins an existing property rather than driving new behaviour — the
/// counting is already right, and it is the SHELL that discards it. It is here
/// so the shell may rely on it and so a later refactor of the duplicate branch
/// cannot quietly turn 0 into N.
#[test]
fn a_repeat_accept_folds_nothing_and_says_so_in_the_count() {
    let (_alice, mut bob, offer) = alice_and_bob();

    let first = bob.accept_record(&offer).expect("first accept");
    let second = bob.accept_record(&offer).expect("second accept");

    assert!(
        first > 0,
        "the first accept must fold the record's assertions, folded {first}"
    );
    assert_eq!(
        0, second,
        "a repeat accept folds nothing new; anything else means duplicates \
         are being re-folded"
    );
}

/// **The reverse direction, which the device run found missing.**
///
/// The host learns nothing about the joiner from the record — it wrote that
/// record itself. So when the joiner authors its own first message, the host
/// has no credential for the joiner's device and the fold refuses it. On two
/// phones this is perfectly asymmetric and perfectly silent: the joiner sees
/// both messages, the host sees only its own, and neither device reports
/// anything wrong.
///
/// The credential belongs at the moment of the invite. The host already
/// extracts the invitee's principal from their key package in order to seat
/// them; registering it there is the same act, and it means the host trusts
/// exactly the person it just decided to admit.
#[test]
fn what_the_joiner_sends_lands_in_the_hosts_timeline_too() {
    let (mut alice, mut bob, offer) = alice_and_bob();
    bob.accept_record(&offer).expect("Bob accepts");

    bob.dispatch(Intent::SelectGroup(group_of(&bob))).unwrap();
    for c in "and cheese".chars() {
        bob.dispatch(Intent::TypeChar(c)).unwrap();
    }
    let wire = bob.send_sealed().expect("Bob sends");

    let accepted = alice
        .receive_sealed(&wire)
        .expect("Alice opens what Bob sealed");
    assert!(
        accepted,
        "the host must fold the joiner's message — it admitted them, so it \
         knows who they are"
    );

    alice
        .dispatch(Intent::SelectGroup(group_of(&alice)))
        .unwrap();
    let line = alice
        .view()
        .timeline
        .lines
        .into_iter()
        .find(|l| l.body == "and cheese")
        .expect("the host's timeline carries the joiner's message");
    assert!(!line.author.is_empty(), "and knows who said it");
}

/// **Rung 6's shape at Rust grade: credentials have to survive a restart too.**
///
/// Registering the invitee at invite time fixes the live case and nothing else:
/// the registry is in memory, and `Session::open` starts a fresh one holding
/// only this device's own credential. So a host that restarts forgets everyone
/// it ever admitted, and goes silent on them with no local symptom whatsoever —
/// the same failure as before, one process boundary later.
///
/// The answer is not to persist the registry. The fold already knows who is
/// seated, because that is what a MembershipAdd IS; opening a session can read
/// its own record back and re-register from it. That keeps exactly one source
/// of truth, and it means a credential can never outlive the membership that
/// justified it.
#[test]
fn a_host_that_restarts_can_still_fold_what_its_member_sends() {
    let alice_path = temp("alice-restart");
    let (alice, mut bob, offer) = alice_and_bob_at(&alice_path);
    bob.accept_record(&offer).expect("Bob accepts");

    drop(alice); // the app closed, mid-conversation

    bob.dispatch(Intent::SelectGroup(group_of(&bob))).unwrap();
    for c in "still here".chars() {
        bob.dispatch(Intent::TypeChar(c)).unwrap();
    }
    let wire = bob.send_sealed().expect("Bob sends");

    let mut alice = Session::open(&alice_path, &[1u8; 32]).expect("Alice restarts");
    let accepted = alice
        .receive_sealed(&wire)
        .expect("Alice opens what Bob sealed after she restarted");

    assert!(
        accepted,
        "a restarted host must still know the people its own record seats",
    );
}
