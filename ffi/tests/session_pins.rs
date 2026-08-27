//! The session's behaviour, tested in Rust.
//!
//! Everything interesting about the FFI layer is in `Session` — the update /
//! perform / project loop, the refusals, the lamport bookkeeping — and none of
//! it needs a generated binding or a JVM to exercise. Testing it here means the
//! Kotlin wiring test can be what it should be: a small number of cases that
//! prove the *boundary* works, rather than the only place any of this is
//! checked. A suite that can only run behind a gradle invocation is a suite
//! that gets run less.

use chat_core::model::Intent;
use croft_ffi::error::SessionError;
use croft_ffi::session::Session;

const KEY: [u8; 32] = [0x42; 32];

fn open_temp() -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().expect("tempdir");
    let session = Session::open(&dir.path().join("store.redb"), &KEY).expect("open");
    (dir, session)
}

fn type_into(session: &mut Session, text: &str) {
    for c in text.chars() {
        session.dispatch(Intent::TypeChar(c)).expect("typing");
    }
}

// ---- the loop ------------------------------------------------------------

#[test]
fn a_new_session_opens_onto_an_empty_world_rather_than_failing() {
    let (_dir, session) = open_temp();
    let view = session.view();
    assert!(view.tree.rows.is_empty());
    assert!(view.timeline.lines.is_empty());
}

#[test]
fn founding_a_group_puts_it_in_the_tree_with_its_local_title() {
    let (_dir, mut session) = open_temp();
    session.create_group("the kitchen table").expect("create");

    let view = session.view();
    assert_eq!(view.tree.rows.len(), 1, "one group, one row");
    match &view.tree.rows[0] {
        chat_core::view::TreeRow::Group(g) => {
            assert_eq!(g.title, "the kitchen table");
            assert_eq!(g.member_count, 1, "the founder is seated");
        }
        other => panic!("expected a group row, got {other:?}"),
    }
}

#[test]
fn a_sent_message_reaches_the_projection_with_its_body_and_clears_the_draft() {
    let (_dir, mut session) = open_temp();
    let group = session.create_group("g").expect("create");
    session
        .dispatch(Intent::SelectGroup(group))
        .expect("select");
    type_into(&mut session, "hello");

    let view = session.dispatch(Intent::SendMessage).expect("send");
    assert_eq!(view.timeline.lines.len(), 1);
    assert_eq!(view.timeline.lines[0].body, "hello");
    assert_eq!(view.draft, "", "sending clears the draft");
}

#[test]
fn the_projected_line_is_the_confirmed_one_not_the_optimistic_one() {
    // The pond appends an optimistic line at `MessageLine::OPTIMISTIC` so the
    // sender sees their text immediately; the session then reloads so the
    // confirmed line replaces it. If the reload were dropped the line would
    // still be there and still say "hello" — and would be marked pending
    // forever. `pending` is the only thing that tells the two apart, which is
    // why this asserts on it rather than on the body.
    let (_dir, mut session) = open_temp();
    let group = session.create_group("g").expect("create");
    session
        .dispatch(Intent::SelectGroup(group))
        .expect("select");
    type_into(&mut session, "confirmed");

    let view = session.dispatch(Intent::SendMessage).expect("send");
    assert!(
        !view.timeline.lines[0].pending,
        "the line must be the one the store confirmed"
    );
}

#[test]
fn messages_come_back_in_the_order_they_were_sent() {
    let (_dir, mut session) = open_temp();
    let group = session.create_group("g").expect("create");
    session
        .dispatch(Intent::SelectGroup(group))
        .expect("select");

    for body in ["first", "second", "third"] {
        type_into(&mut session, body);
        session.dispatch(Intent::SendMessage).expect("send");
    }

    let view = session.view();
    let bodies: Vec<&str> = view
        .timeline
        .lines
        .iter()
        .map(|l| l.body.as_str())
        .collect();
    assert_eq!(bodies, vec!["first", "second", "third"]);
}

#[test]
fn selecting_a_second_group_shows_that_groups_timeline_and_not_the_firsts() {
    let (_dir, mut session) = open_temp();
    let one = session.create_group("one").expect("create");
    let two = session.create_group("two").expect("create");

    session.dispatch(Intent::SelectGroup(one)).expect("select");
    type_into(&mut session, "in one");
    session.dispatch(Intent::SendMessage).expect("send");

    let view = session.dispatch(Intent::SelectGroup(two)).expect("select");
    assert!(
        view.timeline.lines.is_empty(),
        "the second group has no messages, so its timeline is empty"
    );

    let back = session.dispatch(Intent::SelectGroup(one)).expect("select");
    assert_eq!(back.timeline.lines.len(), 1, "the first group kept its own");
}

// ---- durability ----------------------------------------------------------

#[test]
fn everything_survives_closing_and_reopening_the_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("store.redb");

    let group = {
        let mut session = Session::open(&path, &KEY).expect("open");
        let group = session.create_group("durable").expect("create");
        session
            .dispatch(Intent::SelectGroup(group))
            .expect("select");
        type_into(&mut session, "still here");
        session.dispatch(Intent::SendMessage).expect("send");
        group
    };

    let mut reopened = Session::open(&path, &KEY).expect("reopen");
    let view = reopened
        .dispatch(Intent::SelectGroup(group))
        .expect("select");
    assert_eq!(view.timeline.lines.len(), 1);
    assert_eq!(view.timeline.lines[0].body, "still here");
}

#[test]
fn a_reopened_session_does_not_reuse_a_lamport_it_already_spent() {
    // The bug this guards: a session that restarts its lamport at zero authors
    // an assertion the fold has already seen, which is silently a duplicate —
    // the message simply does not appear, and nothing errors.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("store.redb");

    let group = {
        let mut session = Session::open(&path, &KEY).expect("open");
        let group = session.create_group("g").expect("create");
        session
            .dispatch(Intent::SelectGroup(group))
            .expect("select");
        type_into(&mut session, "before");
        session.dispatch(Intent::SendMessage).expect("send");
        group
    };

    let mut reopened = Session::open(&path, &KEY).expect("reopen");
    reopened
        .dispatch(Intent::SelectGroup(group))
        .expect("select");
    type_into(&mut reopened, "after");
    let view = reopened.dispatch(Intent::SendMessage).expect("send");

    let bodies: Vec<&str> = view
        .timeline
        .lines
        .iter()
        .map(|l| l.body.as_str())
        .collect();
    assert_eq!(
        bodies,
        vec!["before", "after"],
        "the second run's message must land beside the first run's, not vanish"
    );
}

#[test]
fn two_groups_founded_in_one_session_get_distinct_ids() {
    // The id is derived from device, lamport and title. Two groups with the
    // SAME title must still differ, because the lamport differs — a derivation
    // that leaned only on the title would collide here and silently fold the
    // second genesis into the first group.
    let (_dir, mut session) = open_temp();
    let a = session.create_group("same name").expect("create");
    let b = session.create_group("same name").expect("create");
    assert_ne!(a, b);
    assert_eq!(session.view().tree.rows.len(), 2);
}

// ---- refusals ------------------------------------------------------------

#[test]
fn a_signing_key_of_the_wrong_length_is_refused_with_its_length() {
    let dir = tempfile::tempdir().expect("tempdir");
    for len in [0usize, 31, 33] {
        let err = Session::open(&dir.path().join("s.redb"), &vec![0u8; len])
            .expect_err("a wrong-length key must be refused");
        match err {
            SessionError::BadKeyLength { got } => assert_eq!(got, len),
            other => panic!("expected BadKeyLength, got {other:?}"),
        }
    }
}

#[test]
fn sending_with_no_group_selected_is_refused_rather_than_dropped() {
    // `chat_core::update` drops this intent — correct for a total reducer, and
    // indistinguishable from success once it has crossed a boundary.
    let (_dir, mut session) = open_temp();
    type_into(&mut session, "orphan");
    assert!(matches!(
        session.dispatch(Intent::SendMessage),
        Err(SessionError::NoGroupSelected)
    ));
}

#[test]
fn sending_an_empty_draft_is_refused() {
    let (_dir, mut session) = open_temp();
    let group = session.create_group("g").expect("create");
    session
        .dispatch(Intent::SelectGroup(group))
        .expect("select");
    type_into(&mut session, "   ");
    assert!(matches!(
        session.dispatch(Intent::SendMessage),
        Err(SessionError::EmptyDraft)
    ));
}

#[test]
fn selecting_a_group_this_session_is_not_in_is_refused_by_name() {
    let (_dir, mut session) = open_temp();
    let ghost = social_tree_core::model::GroupId::new([0x99; 32]);
    match session.dispatch(Intent::SelectGroup(ghost)) {
        Err(SessionError::NoSuchGroup { group }) => assert_eq!(group, ghost),
        other => panic!("expected NoSuchGroup, got {other:?}"),
    }
}

#[test]
fn a_store_path_that_cannot_be_opened_is_refused_with_the_stores_own_words() {
    let dir = tempfile::tempdir().expect("tempdir");
    let err = Session::open(&dir.path().join("no/such/dir/store.redb"), &KEY)
        .expect_err("an unopenable path must be refused");
    match err {
        SessionError::Storage { reason } => assert!(
            !reason.trim().is_empty(),
            "the refusal must carry a reason, not just a category"
        ),
        other => panic!("expected Storage, got {other:?}"),
    }
}

#[test]
fn refreshing_re_reads_the_store_rather_than_clearing_the_model() {
    // `chat_core::Intent::Refresh` carries the snapshot, so a shell that asks
    // for a refresh is really asking the SESSION to go and read. Handing the
    // pond a default snapshot instead would "succeed" and empty the screen —
    // a wipe that looks exactly like having no groups.
    let (_dir, mut session) = open_temp();
    let group = session.create_group("still there").expect("create");
    session
        .dispatch(Intent::SelectGroup(group))
        .expect("select");
    type_into(&mut session, "and so is this");
    session.dispatch(Intent::SendMessage).expect("send");

    let view = session
        .dispatch(Intent::Refresh(Default::default()))
        .expect("refresh");
    assert_eq!(view.tree.rows.len(), 1, "the group must survive a refresh");
    assert_eq!(
        view.timeline.lines.len(),
        1,
        "and so must the selected group's timeline"
    );
}

#[test]
fn every_refusal_carries_its_sentence_across_the_boundary() {
    // Found by S1: uniffi builds a generated exception's `message` from the
    // variant's FIELDS, not from the Rust `Display` impl. So a fieldless
    // variant crosses with `message == ""` — and `NoGroupSelected` and
    // `EmptyDraft`, the two refusals a person is most likely to hit, were
    // exactly the fieldless ones. The typed exception arrived; the sentence
    // did not, and a shell showing `e.message` showed nothing.
    //
    // The fix keeps ONE source of truth: the words stay in the Rust `#[error]`
    // attributes and ride across in a `reason` field. Translating them
    // Kotlin-side would put a product commitment in two places, which is how
    // the two drift.
    use croft_ffi::FfiError;

    let cases: Vec<FfiError> = vec![
        SessionError::BadKeyLength { got: 31 }.into(),
        SessionError::NoGroupSelected.into(),
        SessionError::EmptyDraft.into(),
        SessionError::NoSuchGroup {
            group: social_tree_core::model::GroupId::new([0x11; 32]),
        }
        .into(),
        SessionError::Storage {
            reason: "the parent directory does not exist".to_string(),
        }
        .into(),
        SessionError::Refused {
            reason: "the fold said no".to_string(),
        }
        .into(),
    ];

    for case in cases {
        let reason = case.reason();
        assert!(
            !reason.trim().is_empty(),
            "{case:?} crossed the boundary with nothing a person could read"
        );
    }
}

// ---------------------------------------------------------------------------
// S2: sealed chat. The core crosses the FFI already; now the KEY LAYER does.
// ---------------------------------------------------------------------------
//
// The honest claim S2 reaches is "sealed at AEAD grade on this device, and it
// survives the app closing". Transport between two devices is the tier above,
// and these tests deliberately do not pretend to reach it: they seal on one
// substrate and open on another, in-process, which is what a JVM-grade test
// can actually prove.

#[test]
fn a_session_seats_a_real_mls_group_when_it_founds_one() {
    // Before S2 a group was a governance fact with no crypto behind it. The
    // difference is not visible in the timeline, which is exactly why it needs
    // asserting somewhere: an unsealed group and a sealed one look identical
    // to a reader of the projection.
    let (_dir, mut session) = open_temp();
    session.create_group("sealed").expect("create");
    assert!(
        session.has_mls_group(),
        "founding a group must seat real MLS, not just fold a genesis assertion",
    );
}

#[test]
fn a_sealed_message_is_not_the_plaintext() {
    let (_dir, mut session) = open_temp();
    session.create_group("g").expect("create");
    let sealed = session.seal(b"the quick brown fox").expect("seal");
    assert!(
        !sealed.windows(b"quick".len()).any(|w| w == b"quick"),
        "the plaintext must not survive in the ciphertext — the one assertion \
         that would catch a seal that quietly did nothing",
    );
    assert!(
        sealed.len() > b"the quick brown fox".len(),
        "AEAD adds overhead"
    );
}

#[test]
fn the_mls_group_survives_the_app_closing() {
    // The S2 property at FFI grade. Two stores are involved — the governance
    // store and the MLS store — and both have to come back, in step. A session
    // that reloaded the fold and lost the epoch would show the whole
    // conversation and be unable to add a word to it.
    let dir = tempfile::tempdir().expect("tempdir");
    let store = dir.path().join("store.redb");

    let epoch = {
        let mut session = Session::open(&store, &KEY).expect("open");
        session.create_group("durable").expect("create");
        session.mls_epoch().expect("a seated group has an epoch")
    };

    let reopened = Session::open(&store, &KEY).expect("reopen");
    assert!(reopened.has_mls_group(), "the MLS group must come back");
    assert_eq!(
        reopened.mls_epoch().expect("epoch"),
        epoch,
        "and at the epoch it left off at",
    );
}

#[test]
fn a_second_session_on_the_same_store_is_refused_rather_than_racing() {
    // redb holds an exclusive lock, and the MLS store inherits that. Two
    // sessions writing one identity's MLS state would be corruption; a refusal
    // is the good outcome and the shell needs to see it as one.
    let dir = tempfile::tempdir().expect("tempdir");
    let store = dir.path().join("store.redb");
    let _first = Session::open(&store, &KEY).expect("first");
    assert!(
        Session::open(&store, &KEY).is_err(),
        "a second session on one store must refuse",
    );
}

#[test]
fn what_one_member_seals_another_can_open() {
    // Seal on one substrate, open on another — the plan's wiring-test shape,
    // at the tier a JVM test can honestly reach. The transport between two
    // real devices is the tier above this and is not simulated here.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut alice = Session::open(&dir.path().join("alice.redb"), &[0x20; 32]).expect("alice");
    let mut bob = Session::open(&dir.path().join("bob.redb"), &[0x24; 32]).expect("bob");

    alice.create_group("shared").expect("create");
    let welcome = alice
        .invite(&bob.mls_key_package().expect("bob's key package"))
        .expect("alice invites bob");
    bob.accept_invite(&welcome).expect("bob seats");

    let sealed = alice.seal(b"for bob only").expect("alice seals");
    assert_eq!(
        bob.open_sealed(&sealed).expect("bob opens"),
        b"for bob only",
        "sealed on one substrate, opened on another",
    );
}

#[test]
fn a_stranger_cannot_open_what_was_sealed_for_the_group() {
    // The half that makes the previous test mean something. A "seal" that
    // everyone can open would pass every happy-path assertion above.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut alice = Session::open(&dir.path().join("alice.redb"), &[0x20; 32]).expect("alice");
    let mut stranger =
        Session::open(&dir.path().join("stranger.redb"), &[0x99; 32]).expect("stranger");
    stranger.create_group("their own").expect("their own group");

    alice.create_group("shared").expect("create");
    let sealed = alice.seal(b"not for you").expect("seal");

    assert!(
        stranger.open_sealed(&sealed).is_err(),
        "someone outside the group must not be able to open its messages",
    );
}
