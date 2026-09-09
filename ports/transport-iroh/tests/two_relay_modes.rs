//! **D1 probe** — can a relay-disabled endpoint and a relay-attaching one share
//! a process without the first acquiring the second's behaviour?
//!
//! The decision this answers is in
//! `plans/2026-09-08-plan-call-core-and-apple-shell.md` D1: whether the calling
//! endpoint belongs in THIS crate as a second mode, or in a sibling port. The
//! plan recommends a sibling, on the grounds that `RelayMode::Disabled` here is
//! a guarantee held **by construction** — CLAUDE.md's words — and a crate
//! containing both endpoint kinds downgrades that to a convention.
//!
//! That recommendation is about where code lives. This file asks the narrower
//! empirical question underneath it: *if* both existed in one process, would
//! the disabled one stay severed? If the answer is no, the sibling port is not
//! merely preferable but forced — and P7 S2's separation claim, which rests on
//! the same severance, would need re-examining on a device where the calling
//! app and the social module already share a phone.
//!
//! **Hermetic on purpose.** The relay-attaching endpoint points at an
//! unroutable address, so no external host is contacted and the test needs no
//! admission token. Attaching is not the contamination vector; *attempting* is
//! — a shared crypto provider, a process-global relay map, or a shared runtime
//! would leak whether or not the far end answers.

use std::time::Duration;

use transport_iroh::testing::SwarmLock;
use transport_iroh::{GossipTransport, TopicKey};

fn topic(seed: u8) -> TopicKey {
    TopicKey::from_group_id(&[seed; 32])
}

/// The probe. A relay-disabled transport keeps its severance while a
/// relay-attaching endpoint is live and retrying in the same process.
#[test]
fn a_relay_attaching_endpoint_next_door_does_not_contaminate_the_severed_one() {
    let _swarm = SwarmLock::acquire();

    // The severed one: this crate's transport, exactly as the app builds it.
    let severed = GossipTransport::start(&[21u8; 32], topic(0xD1), &[]).expect("severed starts");

    // The neighbour: a raw iroh endpoint with a CUSTOM relay, in this same
    // process. Its own runtime, mirroring how a calling shell would own one.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("probe runtime");

    let neighbour = rt.block_on(async {
        iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
            .secret_key(iroh::SecretKey::from_bytes(&[22u8; 32]))
            // Unroutable by construction: TEST-NET-1 (RFC 5737), reserved for
            // documentation and guaranteed not to be a real host.
            .relay_mode(iroh::RelayMode::Custom(
                iroh::RelayMap::try_from_iter(["https://192.0.2.1:443"]).expect("relay map"),
            ))
            .bind()
            .await
    });

    let neighbour = neighbour.expect(
        "a relay-attaching endpoint must at least BIND beside a severed one; \
         if this fails, the two modes cannot share a process at all",
    );

    // Give the neighbour time to try, fail, and retry. The contamination
    // vector is the attempt, so the window has to contain some.
    std::thread::sleep(Duration::from_millis(1500));

    // The claim under test: the severed transport is still severed.
    let card = severed.dial_card();
    assert!(
        !card.addrs.is_empty(),
        "a dial card with no addresses cannot be paired against"
    );
    for addr in &card.addrs {
        assert!(
            !addr.contains("http"),
            "a relay-shaped address appeared in the SEVERED transport's dial card \
             while a relay-attaching endpoint lived in the same process: {addr}. \
             The severance is not process-local, and D1 is forced to a sibling port."
        );
    }

    rt.block_on(async { neighbour.close().await });
}
