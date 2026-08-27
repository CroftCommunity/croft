//! The boundary says what it did, and says louder when it refused.
//!
//! An FFI layer is the one place in the stack where a failure can become
//! invisible: the Rust side returns an error, the binding turns it into a
//! foreign exception, and whatever the shell does with that exception is out of
//! this crate's hands. A refusal that was never logged leaves nothing behind to
//! find afterwards. So the boundary emits — DEBUG for a crossing, WARN for a
//! refusal — and these pins prove it does, rather than trusting that the calls
//! are there because someone wrote them.

use std::sync::{Arc, Mutex};

use chat_core::model::Intent;
use croft_ffi::session::Session;
use tracing::Level;
use tracing_subscriber::layer::SubscriberExt;

/// Collects `(level, target)` for every event, so a test can ask what the
/// boundary said without matching on formatted text.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<(Level, String)>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Captured {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let meta = event.metadata();
        self.0
            .lock()
            .expect("capture lock")
            .push((*meta.level(), meta.target().to_string()));
    }
}

impl Captured {
    fn levels(&self) -> Vec<Level> {
        self.0
            .lock()
            .expect("capture lock")
            .iter()
            .map(|(l, _)| *l)
            .collect()
    }
}

fn with_capture<T>(f: impl FnOnce() -> T) -> (T, Captured) {
    let captured = Captured::default();
    let subscriber = tracing_subscriber::registry().with(captured.clone());
    let out = tracing::subscriber::with_default(subscriber, f);
    (out, captured)
}

#[test]
fn a_successful_crossing_is_debug_and_never_warn() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (_out, captured) = with_capture(|| {
        let mut session = Session::open(&dir.path().join("s.redb"), &[0x11; 32]).expect("open");
        let group = session.create_group("g").expect("create");
        session
            .dispatch(Intent::SelectGroup(group))
            .expect("select");
    });

    let levels = captured.levels();
    assert!(
        levels.contains(&Level::DEBUG),
        "the boundary must record that a crossing happened, got {levels:?}"
    );
    assert!(
        !levels.contains(&Level::WARN),
        "nothing was refused, so nothing should warn — a boundary that warns on \
         success trains people to ignore its warnings, got {levels:?}"
    );
}

#[test]
fn every_refusal_warns() {
    let dir = tempfile::tempdir().expect("tempdir");

    // Each of the boundary's refusal shapes, one at a time, each in its own
    // capture — the assertion is per-refusal, not "at least one warning
    // happened somewhere in a long run".
    /// One refusal case: what it is called, and how to provoke it.
    type Case = (&'static str, Box<dyn Fn(&std::path::Path)>);

    let cases: Vec<Case> = vec![
        (
            "a wrong-length signing key",
            Box::new(|dir: &std::path::Path| {
                let _ = Session::open(&dir.join("bad.redb"), &[0u8; 31]);
            }),
        ),
        (
            "a send with no group selected",
            Box::new(|dir: &std::path::Path| {
                let mut s = Session::open(&dir.join("a.redb"), &[0x22; 32]).expect("open");
                let _ = s.dispatch(Intent::TypeChar('x'));
                let _ = s.dispatch(Intent::SendMessage);
            }),
        ),
        (
            "a group this session is not in",
            Box::new(|dir: &std::path::Path| {
                let mut s = Session::open(&dir.join("b.redb"), &[0x33; 32]).expect("open");
                let ghost = social_tree_core::model::GroupId::new([0x99; 32]);
                let _ = s.dispatch(Intent::SelectGroup(ghost));
            }),
        ),
        (
            "an unopenable store path",
            Box::new(|dir: &std::path::Path| {
                let _ = Session::open(&dir.join("no/such/dir/s.redb"), &[0x44; 32]);
            }),
        ),
    ];

    for (name, case) in cases {
        let (_out, captured) = with_capture(|| case(dir.path()));
        assert!(
            captured.levels().contains(&Level::WARN),
            "{name} was refused and left nothing in the log to find it by"
        );
    }
}
