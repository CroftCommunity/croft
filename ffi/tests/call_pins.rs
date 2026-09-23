//! R4's calling object at the FFI boundary, pinned from Rust.
//!
//! These drive `croft_ffi::CallSession` — the uniffi OBJECT with its
//! boundary types, not `call_session::steps` underneath — so what they pin
//! is the translation: the options record becomes a session, the view
//! crosses with its words intact, a refusal crosses as a typed error that
//! still carries a sentence, and a call is a second object a shell can hang
//! up while another thread waits on its ending. The Swift wiring test
//! (`shell/apple/Tests`) drives the same surface through the generated
//! bindings; this file is the part of that a `cargo test` can reach.
//!
//! Hermetic, as `ports/call-session/tests/session_steps.rs` is: TEST-NET-1
//! relays and a loopback dial by direct address.

use std::sync::Arc;
use std::time::Duration;

use croft_ffi::{ActiveCall, CallError, CallOptions, CallSession, PeerAddress};
use transport_iroh::testing::SwarmLock;

fn hermetic(dir: &std::path::Path) -> CallOptions {
    CallOptions {
        state_dir: Some(dir.display().to_string()),
        relay: "https://192.0.2.1:443".to_string(),
        label: "ffi-test".to_string(),
        discovery_n0: false,
        attach_patience_secs: 1,
    }
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
fn the_production_relay_is_the_ports_constant_not_a_shells_string() {
    assert_eq!(
        croft_ffi::croft_relay_url(),
        call_transport_iroh::relay::CROFT_RELAY_URL
    );
}

#[test]
fn a_fresh_session_crosses_with_honest_words() {
    let dir = tempfile::tempdir().expect("tempdir");
    let s = CallSession::open(hermetic(dir.path())).expect("opens");
    let view = s.view();
    assert!(!view.signed_in);
    assert_eq!(view.session, "not signed in");
    assert!(!view.camped);
    assert!(view.presence.contains("NOT camped") && view.presence.contains("calls cannot reach"));
    assert_eq!(view.endpoint_id, None);
    assert!(
        view.log.is_empty(),
        "nothing has happened, so nothing is logged"
    );
}

#[test]
fn a_refusal_crosses_typed_and_with_a_sentence() {
    let dir = tempfile::tempdir().expect("tempdir");
    let s = CallSession::open(hermetic(dir.path())).expect("opens");
    let err = s
        .dial_endpoint(PeerAddress {
            endpoint_id: "00".repeat(32),
            relay_url: None,
            addrs: vec!["127.0.0.1:9".to_string()],
        })
        .expect_err("nothing is bound");
    assert!(matches!(err, CallError::NotBound { .. }), "{err:?}");
    // The S1 finding, again: a typed exception whose message is empty is a
    // calm blank screen where the truth was a refusal.
    assert!(!err.reason().trim().is_empty());
    assert_eq!(err.reason(), err.to_string());
}

#[test]
fn a_signed_out_camp_reads_not_camped_and_the_log_says_tokenless() {
    let _swarm = SwarmLock::acquire();
    let dir = tempfile::tempdir().expect("tempdir");
    let s = CallSession::open(hermetic(dir.path())).expect("opens");
    let camped = s.camp().expect("a refused attach is not an error");
    assert!(!camped);
    let view = s.view();
    assert!(view.endpoint_id.is_some());
    assert!(view.presence.contains("NOT camped"));
    assert!(
        view.log.iter().any(|l| l.contains("camping tokenless")),
        "{:?}",
        view.log
    );
    s.shut_down();
    let err = s.camp().expect_err("a closed session refuses");
    assert!(matches!(err, CallError::Closed { .. }), "{err:?}");
}

#[test]
fn two_sessions_call_over_loopback_and_the_endings_cross_as_words() {
    let _swarm = SwarmLock::acquire();
    let a_dir = tempfile::tempdir().expect("tempdir");
    let b_dir = tempfile::tempdir().expect("tempdir");
    let a = Arc::new(CallSession::open(hermetic(a_dir.path())).expect("opens"));
    let b = Arc::new(CallSession::open(hermetic(b_dir.path())).expect("opens"));
    assert!(!a.camp().expect("binds"));
    assert!(!b.camp().expect("binds"));

    let peer = PeerAddress {
        endpoint_id: b.view().endpoint_id.expect("bound"),
        relay_url: None,
        addrs: direct_addrs(&b),
    };
    assert!(!peer.addrs.is_empty());

    let waiting = {
        let b = Arc::clone(&b);
        std::thread::spawn(move || {
            b.wait_for_call(15)
                .expect("waiting does not fail")
                .expect("a call arrives")
        })
    };
    let outgoing: Arc<ActiveCall> = a.dial_endpoint(peer).expect("connects over loopback");
    assert!(outgoing.outgoing());
    let incoming = waiting.join().expect("callee thread");
    assert!(!incoming.outgoing());
    assert_eq!(
        incoming.peer_endpoint_id(),
        a.view().endpoint_id.expect("bound")
    );
    assert_eq!(incoming.peer_hello(), Some("ffi-test".to_string()));

    // A shell hangs up from one thread while another waits on the ending.
    let ending = {
        let outgoing = Arc::clone(&outgoing);
        std::thread::spawn(move || outgoing.ended(10))
    };
    outgoing.hang_up();
    assert_eq!(
        ending.join().expect("ending thread"),
        Some("call ended: you hung up".to_string())
    );
    assert_eq!(
        incoming.ended(10),
        Some("call ended: closed by peer: hangup (code 0)".to_string())
    );
    assert!(
        a.view()
            .log
            .iter()
            .any(|l| l.contains("kept the camping pass")),
        "R0 crosses the boundary too"
    );
    a.shut_down();
    b.shut_down();
}
