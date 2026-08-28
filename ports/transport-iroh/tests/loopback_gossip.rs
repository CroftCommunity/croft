//! Two transports, one topic, relay disabled — the loopback rung under the
//! device run.
//!
//! **Per-plane rung, stated honestly** (the house convention from
//! `keylayer-openmls/tests/loopback_e2e.rs`): transport here is *real* — real
//! iroh endpoints, real QUIC, real gossip swarm, real UDP over the host's LAN
//! interface. What is NOT real is the topology: two processes' worth of
//! endpoints inside ONE process on ONE host. That is why this file cannot
//! close S2 and the runbook's device rungs can. Two phones is the proof; this
//! is the thing that has to be green before touching one.
//!
//! Every test here asserts the relay severance as well as the delivery,
//! because "it worked" and "it worked without a relay" are different claims
//! and only the second is the one Q2 asked for.

use std::time::Duration;

use transport_iroh::frame::ArtifactKind;
use transport_iroh::{GossipTransport, TopicKey};

/// Long enough for a swarm to form on loopback under a loaded machine, short
/// enough that a genuine hang fails the suite rather than the CI job's wall
/// clock. A healthy run never spends it: the whole file finishes in ~2s.
///
/// **Sized from an episode worth recording.** At 20s this suite failed four of
/// six tests, three runs running, while every test passed alone and two
/// standalone probes (six transports sequential, six concurrent) formed swarms
/// in 500–2500ms. The failures coincided with a leaked probe process holding
/// two live iroh endpoints for ten minutes; once it was gone the suite went
/// green four runs running with **no code change** — the `worker_threads(2)`
/// pin now in `transport.rs` had already been applied before the last failure,
/// so it is not what fixed this and is not claimed to be.
///
/// The cause is therefore *unproven*, not diagnosed, and this constant is
/// headroom rather than a fix. If this suite ever fails again, the thing to
/// suspect first is another process on the host holding iroh endpoints — not
/// the transport, and not this timeout. A swarm that has not formed in a
/// minute is broken in a way more patience cannot mend.
const PATIENCE: Duration = Duration::from_secs(60);

fn topic(seed: u8) -> TopicKey {
    TopicKey::from_group_id(&[seed; 32])
}

/// A founds the swarm; B is handed A's dial card the way a scanned QR would
/// hand it over, and nothing else. Deliberately one-way: on a real device run
/// only one phone shows a code, so a test where both sides know each other up
/// front would be testing a rig that does not exist.
#[test]
fn a_welcome_crosses_from_the_device_that_minted_it() {
    let a = GossipTransport::start(&[1u8; 32], topic(0xAA), &[]).expect("A starts");
    let card = a.dial_card();

    let b = GossipTransport::start(&[2u8; 32], topic(0xAA), &[card]).expect("B starts");
    assert!(a.wait_for_peer(PATIENCE), "the swarm must form before A sends");

    let welcome = b"an openmls welcome, as bytes".to_vec();
    a.broadcast(ArtifactKind::Welcome, &welcome)
        .expect("A broadcasts the welcome");

    let got = b
        .next_artifact(PATIENCE)
        .expect("B receives the welcome within the patience window");

    assert_eq!(got.kind, ArtifactKind::Welcome);
    assert_eq!(got.payload, welcome);
}

/// One-way would pass with a broken receive path on the quiet side — the same
/// argument the runbook makes for rung 5.
#[test]
fn sealed_messages_cross_in_both_directions() {
    let a = GossipTransport::start(&[3u8; 32], topic(0xBB), &[]).expect("A starts");
    let b = GossipTransport::start(&[4u8; 32], topic(0xBB), &[a.dial_card()]).expect("B starts");
    assert!(a.wait_for_peer(PATIENCE), "the swarm must form before A sends");
    assert!(b.wait_for_peer(PATIENCE), "B must see A too");

    a.broadcast(ArtifactKind::Sealed, b"from A").unwrap();
    let on_b = b.next_artifact(PATIENCE).expect("B hears A");
    assert_eq!(on_b.payload, b"from A".to_vec());
    assert_eq!(on_b.kind, ArtifactKind::Sealed);

    b.broadcast(ArtifactKind::Sealed, b"from B").unwrap();
    let on_a = a.next_artifact(PATIENCE).expect("A hears B");
    assert_eq!(on_a.payload, b"from B".to_vec());
    assert_eq!(on_a.kind, ArtifactKind::Sealed);
}

/// The kind survives the wire, which is the whole reason the frame exists: the
/// receiver calls a different key-layer method for each.
#[test]
fn the_kind_survives_the_crossing() {
    let a = GossipTransport::start(&[5u8; 32], topic(0xCC), &[]).expect("A starts");
    let b = GossipTransport::start(&[6u8; 32], topic(0xCC), &[a.dial_card()]).expect("B starts");
    assert!(a.wait_for_peer(PATIENCE), "the swarm must form before A sends");

    a.broadcast(ArtifactKind::Welcome, b"w").unwrap();
    a.broadcast(ArtifactKind::Sealed, b"s").unwrap();

    let mut kinds = vec![
        b.next_artifact(PATIENCE).expect("first artifact").kind,
        b.next_artifact(PATIENCE).expect("second artifact").kind,
    ];
    kinds.sort_by_key(|k| k.as_byte());

    assert_eq!(kinds, vec![ArtifactKind::Welcome, ArtifactKind::Sealed]);
}

/// A group is a topic. Two groups must not leak into each other, and the
/// cheapest place to prove that is here rather than on two phones.
#[test]
fn a_transport_on_a_different_topic_hears_nothing() {
    let a = GossipTransport::start(&[7u8; 32], topic(0xD1), &[]).expect("A starts");
    let card = a.dial_card();

    let same = GossipTransport::start(&[8u8; 32], topic(0xD1), &[card.clone()]).expect("same topic");
    let other =
        GossipTransport::start(&[9u8; 32], topic(0xD2), &[card]).expect("different topic starts");
    assert!(a.wait_for_peer(PATIENCE), "the swarm must form before A sends");

    a.broadcast(ArtifactKind::Sealed, b"for the group").unwrap();

    assert!(
        same.next_artifact(PATIENCE).is_some(),
        "the same-topic transport must receive"
    );
    assert!(
        other.next_artifact(Duration::from_secs(2)).is_none(),
        "a transport on another topic must not receive the group's traffic"
    );
}

/// Q2's actual requirement, asserted rather than assumed. `RelayMode::Disabled`
/// means iroh will neither dial nor listen on a relay, so no address this
/// endpoint publishes can be relay-shaped. If this ever fails, the severance
/// between group admission and fabric admission has stopped being structural.
#[test]
fn no_address_this_transport_offers_is_relay_shaped() {
    let a = GossipTransport::start(&[10u8; 32], topic(0xEE), &[]).expect("A starts");

    let card = a.dial_card();

    assert!(
        !card.addrs.is_empty(),
        "a dial card with no addresses cannot be paired against"
    );
    for addr in &card.addrs {
        assert!(
            !addr.contains("http"),
            "a relay-shaped address escaped into a dial card: {addr}"
        );
    }
}

/// The probe found this the hard way: dropping an `Endpoint` without closing it
/// makes iroh log `Aborting ungracefully`. On a phone that is a socket left for
/// the OS to reap, and rung 6 force-stops the app deliberately — so the clean
/// path must at least be clean.
#[test]
fn a_transport_can_be_shut_down_and_another_started_after_it() {
    let a = GossipTransport::start(&[11u8; 32], topic(0xF1), &[]).expect("A starts");
    a.shutdown();

    let again = GossipTransport::start(&[11u8; 32], topic(0xF1), &[]);
    assert!(
        again.is_ok(),
        "the same identity must be able to start again after a clean shutdown"
    );
}
