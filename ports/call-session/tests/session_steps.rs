//! The stepwise session: what a shell holds, driven a step at a time.
//!
//! Hermetic. No PDS, no admit, no routable relay: every endpoint here points
//! at TEST-NET-1 and the one call goes over loopback by direct address, as
//! the port's own loopback test does. What this proves is the STEPS and the
//! WORDS — a fresh session says so, a signed-out camp binds tokenless and
//! reads NOT camped, a dial before a bind is refused with words, two sessions
//! call each other and the E129 endings come back verbatim, and a dead
//! stored session never renders as signed in. Admission is the `:live` arc's
//! job, and the macOS shell's live run.

use std::path::PathBuf;
use std::time::Duration;

use call_session::session::StoredSession;
use call_session::steps::{CallSession, Options, Presence};
use call_session::Error;
use call_transport_iroh::{Direction, Discovery, Ending, PeerAddr};
use transport_iroh::testing::SwarmLock;

fn hermetic(dir: &std::path::Path) -> Options {
    Options {
        state_dir: Some(PathBuf::from(dir)),
        relay: "https://192.0.2.1:443".to_string(),
        label: "test-shell".to_string(),
        discovery: Discovery::None,
        attach_patience: Duration::from_secs(1),
    }
}

fn quiet() -> impl FnMut(&str) {
    |_| {}
}

fn collecting(lines: &mut Vec<String>) -> impl FnMut(&str) + '_ {
    move |l| lines.push(l.to_string())
}

/// The callee's addresses rewritten to loopback PROPER. The port reports the
/// LAN address; a dial to the host's own LAN address is a UDP hairpin that the
/// macOS application firewall in stealth mode silently drops — measured
/// 2026-09-23 with a raw socket (the LAN address: no packet; 127.0.0.1:
/// delivered), after two days of green. Loopback is what these tests claim.
fn direct_addrs(ep: &CallSession) -> Vec<String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let addrs = ep.local_addrs();
        if !addrs.is_empty() || std::time::Instant::now() > deadline {
            let mut out: Vec<String> = addrs
                .iter()
                .filter_map(|a| a.parse::<std::net::SocketAddr>().ok())
                .map(|a| format!("127.0.0.1:{}", a.port()))
                .collect();
            out.dedup();
            return out;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_fresh_session_says_not_signed_in_and_not_camped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let s = CallSession::open(hermetic(dir.path())).expect("opens offline");
    let view = s.view();
    assert!(!view.signed_in);
    assert_eq!(view.session, "not signed in");
    assert!(!view.camped);
    assert!(
        view.presence.contains("NOT camped") && view.presence.contains("calls cannot reach"),
        "the screen names the consequence before anything is bound: {}",
        view.presence
    );
    assert_eq!(view.endpoint_id, None, "nothing is bound until camp");
}

#[test]
fn a_stored_session_is_unproven_until_the_pds_says_so() {
    let dir = tempfile::tempdir().expect("tempdir");
    let stored = StoredSession {
        did: "did:plc:test".to_string(),
        handle: "someone.test".to_string(),
        pds: "http://127.0.0.1:9".to_string(),
        access_jwt: "x.y.z".to_string(),
        refresh_jwt: "r.s.t".to_string(),
        access_expires_at_secs: u64::MAX,
    };
    call_session::state::StateDir::open(Some(PathBuf::from(dir.path())))
        .expect("state")
        .save_session(&stored)
        .expect("saved");
    let s = CallSession::open(hermetic(dir.path())).expect("opens offline");
    let view = s.view();
    assert!(view.signed_in, "a stored session is a stored session");
    assert!(
        view.session.contains("someone.test") && view.session.contains("unproven"),
        "E135(b): a stored token is not a proven session: {}",
        view.session
    );
    assert!(
        !view.session.to_lowercase().starts_with("signed in"),
        "the words 'signed in' are earned from the PDS, not the disk: {}",
        view.session
    );
}

#[test]
fn a_dead_stored_session_is_refused_with_words_and_never_reads_signed_in() {
    let _swarm = SwarmLock::acquire();
    let dir = tempfile::tempdir().expect("tempdir");
    // Inside the refresh margin, so camp must refresh — against a port
    // nothing listens on, which the client reports at once.
    let stored = StoredSession {
        did: "did:plc:test".to_string(),
        handle: "someone.test".to_string(),
        pds: "http://127.0.0.1:9".to_string(),
        access_jwt: "x.y.z".to_string(),
        refresh_jwt: "r.s.t".to_string(),
        access_expires_at_secs: 0,
    };
    call_session::state::StateDir::open(Some(PathBuf::from(dir.path())))
        .expect("state")
        .save_session(&stored)
        .expect("saved");
    let mut s = CallSession::open(hermetic(dir.path())).expect("opens offline");
    let err = s
        .camp(&mut quiet())
        .expect_err("the refresh cannot succeed");
    assert!(matches!(err, Error::SessionDead(_)), "{err:?}");
    let words = err.to_string();
    assert!(!words.trim().is_empty(), "a refusal carries words");
    let view = s.view();
    assert!(
        !view.session.to_lowercase().contains("signed in"),
        "the E135(b) defect: a dead session rendering as signed in: {}",
        view.session
    );
    assert!(
        view.session.contains("session") && view.presence.contains("NOT camped"),
        "the screen says what is known and what follows: {} / {}",
        view.session,
        view.presence
    );
}

#[test]
fn dialling_before_camping_is_refused_with_words() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut s = CallSession::open(hermetic(dir.path())).expect("opens offline");
    let nobody = PeerAddr {
        endpoint_id: "00".repeat(32),
        relay_url: None,
        addrs: vec!["127.0.0.1:9".to_string()],
    };
    let err = s
        .dial_endpoint(&nobody, &mut quiet())
        .expect_err("no endpoint is bound");
    assert!(matches!(err, Error::NotBound), "{err:?}");
    assert!(
        !err.to_string().trim().is_empty(),
        "a refusal carries words"
    );
}

#[test]
fn camping_signed_out_binds_tokenless_and_reads_not_camped() {
    let _swarm = SwarmLock::acquire();
    let dir = tempfile::tempdir().expect("tempdir");
    let mut s = CallSession::open(hermetic(dir.path())).expect("opens offline");
    let mut lines = Vec::new();
    let presence = s
        .camp(&mut collecting(&mut lines))
        .expect("a refused attach is not an error");
    assert_eq!(presence, Presence::NotCamped);
    assert!(
        lines.iter().any(|l| l.contains("camping tokenless")),
        "the signed-out camp says so: {lines:?}"
    );
    let view = s.view();
    assert!(
        view.endpoint_id.is_some(),
        "the endpoint stays bound so the screen can be retried"
    );
    assert!(!view.camped);
    assert!(
        view.presence.contains("NOT camped") && view.presence.contains("calls cannot reach"),
        "possession is not reachability: {}",
        view.presence
    );
    s.close();
}

#[test]
fn two_sessions_call_each_other_over_loopback_and_the_endings_are_verbatim() {
    let _swarm = SwarmLock::acquire();
    let a_dir = tempfile::tempdir().expect("tempdir");
    let b_dir = tempfile::tempdir().expect("tempdir");
    let mut a = CallSession::open(hermetic(a_dir.path())).expect("opens");
    let mut b = CallSession::open(hermetic(b_dir.path())).expect("opens");
    assert_eq!(a.camp(&mut quiet()).expect("binds"), Presence::NotCamped);
    assert_eq!(b.camp(&mut quiet()).expect("binds"), Presence::NotCamped);

    let peer = PeerAddr {
        endpoint_id: b.view().endpoint_id.expect("bound"),
        relay_url: None,
        addrs: direct_addrs(&b),
    };
    assert!(!peer.addrs.is_empty(), "the callee has a direct address");

    let waiting = std::thread::spawn(move || {
        let call = b
            .wait_for_call(Duration::from_secs(15), &mut quiet())
            .expect("waiting does not fail")
            .expect("a call arrives");
        (b, call)
    });

    let mut lines = Vec::new();
    let outgoing = a
        .dial_endpoint(&peer, &mut collecting(&mut lines))
        .expect("the dial connects over loopback");
    assert_eq!(outgoing.direction(), Direction::Outgoing);
    assert!(
        lines.iter().any(|l| l.contains("kept the camping pass")),
        "R0 at the second surface too — a dial never lowers admission: {lines:?}"
    );
    let (b, incoming) = waiting.join().expect("callee thread");
    assert_eq!(incoming.direction(), Direction::Incoming);
    assert_eq!(
        incoming.peer_endpoint_id(),
        a.view().endpoint_id.expect("bound")
    );

    outgoing.hang_up();
    assert_eq!(
        outgoing.ended(Duration::from_secs(10)),
        Some(Ending::LocalHangUp)
    );
    let theirs = incoming
        .ended(Duration::from_secs(10))
        .expect("the close arrives");
    assert_eq!(
        call_session::report::ending_line(&theirs),
        "call ended: closed by peer: hangup (code 0)"
    );
    a.close();
    b.close();
}
