//! The relay's last disconnect reason, readable by the shell.
//!
//! The relay journal (2026-09-21/23) shows the Pixel re-making its relay
//! connection every few minutes at rest and every few seconds while driven,
//! on Wi-Fi and on LTE alike, the phone closing each time — while the Samsung
//! holds one connection per session. The client knows WHY it dropped: iroh's
//! `RelayStatus::last_error()` carries the reason ("Ping timeout", a stream
//! error, a refused dial). The port surfaces it so the attach line can name
//! it on the next run — the instrument that E128 (no native logging on
//! Android) otherwise leaves missing.
//!
//! Hermetic. iroh only reports a status for a relay it has SELECTED as home,
//! and it selects by probing `GET /ping`; so the relay here is a loopback
//! listener that answers the probe and drops everything else. The relay
//! dial then fails, and the failure is the reason on record. Loopback
//! proper, never the LAN address (JOURNAL 2026-09-23).
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use call_transport_iroh::{BindOptions, CallEndpoint, Discovery, RelayTarget};

/// A relay that passes iroh's probe and refuses to be a relay: answers
/// `GET /ping` with 200 and closes every other connection.
fn probe_only_relay() -> RelayTarget {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("bound").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut head = [0u8; 512];
            let n = stream.read(&mut head).unwrap_or(0);
            if head[..n].starts_with(b"GET /ping") {
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
            }
            // Dropped here: a relay upgrade never completes.
        }
    });
    RelayTarget::new(format!("http://127.0.0.1:{port}"))
}

#[test]
fn a_failed_relay_dial_is_named_in_the_last_relay_error() {
    let ep = CallEndpoint::bind(BindOptions {
        secret_key: [0x71; 32],
        relay: probe_only_relay(),
        token: None,
        discovery: Discovery::None,
    })
    .expect("an endpoint binds regardless of whether its relay answers");

    let deadline = Instant::now() + Duration::from_secs(30);
    let reason = loop {
        if let Some(reason) = ep.last_relay_error() {
            break reason;
        }
        assert!(
            Instant::now() < deadline,
            "iroh reported no relay error within 30 s of a relay that drops the upgrade"
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    eprintln!("last relay error on record: {reason}");
    assert!(
        !reason.trim().is_empty(),
        "the reason is words, not an empty string"
    );
    assert_eq!(
        ep.attached_relay(Duration::from_millis(500)),
        None,
        "and the endpoint is honestly NOT attached"
    );
    ep.shutdown();
}

#[test]
fn every_relay_transition_is_recorded_with_its_reason_and_drained_once() {
    // RUN 2026-09-28 (runbook §17, "The flap, run"): the Pixel at rest on
    // Wi-Fi re-made its relay connection seven times in 31 min with 0.5 s
    // gaps, and the app's 5 s attach probe saw NONE of them — `last_error`
    // is gone once the connection is back. So the port watches the status
    // itself and keeps every transition with its reason until the shell
    // drains them.
    let ep = CallEndpoint::bind(BindOptions {
        secret_key: [0x72; 32],
        relay: probe_only_relay(),
        token: None,
        discovery: Discovery::None,
    })
    .expect("binds");
    let deadline = Instant::now() + Duration::from_secs(30);
    let transitions = loop {
        let t = ep.drain_relay_transitions();
        if !t.is_empty() {
            break t;
        }
        assert!(Instant::now() < deadline, "no relay transition within 30 s");
        std::thread::sleep(Duration::from_millis(200));
    };
    let failed = transitions
        .iter()
        .find(|t| !t.connected && t.error.as_deref().is_some_and(|e| !e.trim().is_empty()))
        .expect("a failed dial is a transition with words");
    assert!(failed.at_unix_ms > 0, "stamped with wall-clock time");
    assert_eq!(
        failed
            .relay_url
            .as_deref()
            .map(|u| u.starts_with("http://127.0.0.1:")),
        Some(true),
        "the transition names its relay"
    );
    assert!(
        ep.drain_relay_transitions().is_empty(),
        "a drain hands each transition over once"
    );
    ep.shutdown();
}
