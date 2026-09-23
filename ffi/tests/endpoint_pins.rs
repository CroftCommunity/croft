//! D3.3: the calling endpoint at the boundary — the port's lifecycle as an
//! object the phone's `CallPeer.kt` holds instead of upstream iroh-ffi's
//! `Endpoint`. Hermetic, as the port's own loopback test is: TEST-NET-1
//! relays, a dial by direct address. What it pins is the translation and
//! two rules crossing intact — R0 at `rebind` (a dial never lowers
//! admission) and the endpoint id stable across a token swap — plus the
//! typed ending and the path words a screen renders.

use std::sync::Arc;
use std::time::Duration;

use croft_ffi::{CallEnding, CallEndpoint, EndpointOptions, PeerAddress};
use transport_iroh::testing::SwarmLock;

fn hermetic(secret: Option<Vec<u8>>) -> EndpointOptions {
    EndpointOptions {
        secret_key: secret,
        relay_url: "https://192.0.2.1:443".to_string(),
        quic_port: 7824,
        token: None,
        discovery_n0: false,
    }
}

/// The callee's addresses rewritten to loopback PROPER. The port reports the
/// LAN address; a dial to the host's own LAN address is a UDP hairpin that the
/// macOS application firewall in stealth mode silently drops — measured
/// 2026-09-23 with a raw socket (the LAN address: no packet; 127.0.0.1:
/// delivered), after two days of green. Loopback is what these tests claim.
fn direct_addrs(ep: &CallEndpoint) -> Vec<String> {
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
fn a_bind_without_a_key_generates_one_and_the_key_rebinds_to_the_same_id() {
    let _swarm = SwarmLock::acquire();
    let first = CallEndpoint::bind(hermetic(None)).expect("binds");
    let key = first.secret_key();
    assert_eq!(key.len(), 32);
    let id = first.endpoint_id();
    first.shutdown();
    let again = CallEndpoint::bind(hermetic(Some(key))).expect("binds with the key");
    assert_eq!(again.endpoint_id(), id, "the persisted key IS the identity");
    again.shutdown();
}

#[test]
fn a_dial_never_lowers_admission_and_a_swap_keeps_the_id() {
    let _swarm = SwarmLock::acquire();
    let ep = CallEndpoint::bind(hermetic(Some(vec![0x51; 32]))).expect("binds");
    let id = ep.endpoint_id();
    assert!(
        ep.rebind(Some("pass".to_string())).expect("swaps"),
        "a pass binds"
    );
    assert_eq!(ep.endpoint_id(), id);
    assert!(
        !ep.rebind(None).expect("kept"),
        "R0: dropping a pass is a Keep"
    );
    assert!(
        !ep.rebind(Some("pass".to_string())).expect("kept"),
        "the same token is not a rebind"
    );
    assert_eq!(
        ep.attached_relay(1),
        None,
        "an unroutable relay is NOT camped, whatever is bound"
    );
    ep.shutdown();
}

#[test]
fn two_endpoints_call_over_loopback_with_the_ending_typed_and_the_path_named() {
    let _swarm = SwarmLock::acquire();
    let callee = Arc::new(CallEndpoint::bind(hermetic(Some(vec![0x52; 32]))).expect("binds"));
    let caller = CallEndpoint::bind(hermetic(Some(vec![0x53; 32]))).expect("binds");
    let peer = PeerAddress {
        endpoint_id: callee.endpoint_id(),
        relay_url: None,
        addrs: direct_addrs(&callee),
    };
    assert!(!peer.addrs.is_empty());
    let accepting = {
        let callee = Arc::clone(&callee);
        std::thread::spawn(move || {
            callee
                .accept_next(15)
                .expect("accept does not fail")
                .expect("a call arrives")
        })
    };
    let outgoing = caller
        .dial(peer, "caller-hello".to_string(), 15)
        .expect("connects");
    let incoming = accepting.join().expect("accept thread");
    assert!(outgoing.outgoing());
    assert_eq!(incoming.peer_hello(), Some("caller-hello".to_string()));
    assert!(
        outgoing.path().starts_with("direct "),
        "{}",
        outgoing.path()
    );

    outgoing.hang_up();
    assert_eq!(outgoing.ending(10), Some(CallEnding::LocalHangUp));
    assert_eq!(
        incoming.ending(10),
        Some(CallEnding::ClosedByPeer {
            code: 0,
            reason: "hangup".to_string()
        })
    );
    caller.shutdown();
    callee.shutdown();
}

#[test]
fn a_closed_endpoint_refuses_with_words() {
    let _swarm = SwarmLock::acquire();
    let ep = CallEndpoint::bind(hermetic(Some(vec![0x54; 32]))).expect("binds");
    ep.shutdown();
    let err = ep.accept_next(1).expect_err("closed");
    assert!(!err.reason().trim().is_empty());
}
