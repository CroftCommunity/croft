//! One call between two endpoints in one process, over direct addresses.
//!
//! Hermetic: both endpoints point at an unroutable relay (TEST-NET-1) and the
//! caller is handed the callee's direct addresses, so the dial goes over
//! loopback and nothing outside this host is contacted. What it proves is the
//! port's call surface — dial, accept, the hello exchange each way, hang-up,
//! and the E129 endings as typed data — not admission, which is the `:live`
//! regression's job.

use std::time::Duration;

use call_transport_iroh::{
    BindOptions, CallEndpoint, Direction, Discovery, Ending, PeerAddr, RelayTarget,
};
use transport_iroh::testing::SwarmLock;

fn unroutable_relay() -> RelayTarget {
    RelayTarget::new("https://192.0.2.1:443")
}

/// Tracing to stderr under `RUST_LOG`, so a failing dial can say where it
/// went — a loopback dial that "gets no answer" is otherwise a blank.
fn logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();
}

fn bind(seed: u8) -> CallEndpoint {
    logging();
    CallEndpoint::bind(BindOptions {
        secret_key: [seed; 32],
        relay: unroutable_relay(),
        token: None,
        discovery: Discovery::None,
    })
    .expect("binds")
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

const PATIENCE: Duration = Duration::from_secs(15);

#[test]
fn a_call_connects_exchanges_hellos_and_ends_with_the_hang_up_named() {
    let _swarm = SwarmLock::acquire();
    let callee = bind(0x41);
    let caller = bind(0x42);

    let peer = PeerAddr {
        endpoint_id: callee.endpoint_id(),
        relay_url: None,
        addrs: direct_addrs(&callee),
    };
    assert!(
        !peer.addrs.is_empty(),
        "the callee must have a direct address to be dialled on"
    );
    tracing::info!(addrs = ?peer.addrs, "the callee's direct addresses");

    // Accept on its own thread: the callee's accept and the caller's dial
    // both block, as they would in two shells.
    let accepting = std::thread::spawn(move || {
        let call = callee
            .accept_next(PATIENCE)
            .expect("accept does not fail")
            .expect("a call arrives within patience");
        (callee, call)
    });

    let outgoing = caller
        .dial(&peer, "caller-hello", PATIENCE)
        .expect("the dial connects over loopback");
    assert_eq!(outgoing.direction(), Direction::Outgoing);
    assert_eq!(outgoing.peer_endpoint_id(), peer.endpoint_id);
    assert_eq!(
        outgoing.peer_hello(),
        Some("callee"),
        "the callee answers with the v0 hello"
    );

    let (callee, incoming) = accepting.join().expect("accept thread");
    assert_eq!(incoming.direction(), Direction::Incoming);
    assert_eq!(incoming.peer_endpoint_id(), caller.endpoint_id());
    assert_eq!(incoming.peer_hello(), Some("caller-hello"));

    // E129: the caller hangs up. Its own ending says so; the callee's ending
    // carries the peer's code and reason verbatim, never a guess.
    outgoing.hang_up();
    assert_eq!(outgoing.ended(PATIENCE), Some(Ending::LocalHangUp));
    assert_eq!(
        incoming.ended(PATIENCE),
        Some(Ending::ClosedByPeer {
            code: 0,
            reason: "hangup".to_string()
        })
    );

    caller.shutdown();
    callee.shutdown();
}

#[test]
fn a_dial_to_an_endpoint_nobody_is_running_is_refused_with_words() {
    let _swarm = SwarmLock::acquire();
    let caller = bind(0x43);
    let nobody = PeerAddr {
        endpoint_id: CallEndpoint::endpoint_id_for(&[0x44; 32]),
        relay_url: None,
        addrs: vec!["127.0.0.1:9".to_string()],
    };
    let err = caller
        .dial(&nobody, "caller-hello", Duration::from_secs(3))
        .expect_err("nobody answers on the discard port");
    // "dial failed: null" reached a real screen on 2026-09-08 (§15.3); the
    // matrix requires words, and an empty message is not words.
    assert!(
        !err.to_string().trim().is_empty(),
        "a refusal carries words"
    );
    caller.shutdown();
}

#[test]
fn a_loopback_call_reports_its_path_as_direct_in_the_screens_words() {
    let _swarm = SwarmLock::acquire();
    let callee = bind(0x45);
    let caller = bind(0x46);
    let peer = PeerAddr {
        endpoint_id: callee.endpoint_id(),
        relay_url: None,
        addrs: direct_addrs(&callee),
    };
    let accepting = std::thread::spawn(move || {
        let call = callee
            .accept_next(PATIENCE)
            .expect("accept does not fail")
            .expect("a call arrives");
        (callee, call)
    });
    let outgoing = caller
        .dial(&peer, "caller-hello", PATIENCE)
        .expect("connects");
    let (callee, incoming) = accepting.join().expect("accept thread");

    // The words `PathSummary.kt` committed to, now the port's: report what
    // the snapshot says, never infer. Over loopback the selected path is IP.
    assert!(
        outgoing.path_summary().starts_with("direct "),
        "{}",
        outgoing.path_summary()
    );
    assert!(
        incoming.path_summary().starts_with("direct "),
        "{}",
        incoming.path_summary()
    );
    outgoing.hang_up();
    let _ = outgoing.ended(PATIENCE);
    let _ = incoming.ended(PATIENCE);
    caller.shutdown();
    callee.shutdown();
}
