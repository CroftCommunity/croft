//! The gossip link across the boundary.
//!
//! The transport itself is pinned in `transport-iroh`; these are the boundary's
//! own obligations — that the types cross, that the refusals arrive with their
//! words, and that the pairing round trip works through the shapes uniffi can
//! actually carry. Anything provable one layer down is proved one layer down.

use croft_ffi::link::{GossipLink, PairingCode};
use croft_ffi::FfiError;
use transport_iroh::testing::SwarmLock;

fn topic() -> Vec<u8> {
    vec![0x5A; 32]
}

#[test]
fn a_link_starts_and_offers_a_dial_card_with_somewhere_to_dial() {
    let _swarm = SwarmLock::acquire();
    let link = GossipLink::start(vec![1u8; 32], topic(), vec![]).expect("a link starts");

    let card = link.dial_card();

    assert_eq!(card.endpoint_id.len(), 64, "a 32-byte id is 64 hex chars");
    assert!(
        !card.addrs.is_empty(),
        "a dial card with no addresses cannot be paired against"
    );
    link.shutdown();
}

#[test]
fn a_signing_key_of_the_wrong_length_is_refused_with_its_length() {
    let _swarm = SwarmLock::acquire();
    let err =
        GossipLink::start(vec![1u8; 31], topic(), vec![]).expect_err("a 31-byte key is refused");

    match &err {
        FfiError::BadKeyLength { got, reason } => {
            assert_eq!(*got, 31);
            assert!(
                reason.contains("31"),
                "the reason names the length: {reason}"
            );
        }
        other => panic!("expected a key-length refusal, got {other:?}"),
    }
}

#[test]
fn a_topic_of_the_wrong_length_is_refused_with_its_length() {
    let _swarm = SwarmLock::acquire();
    let err = GossipLink::start(vec![1u8; 32], vec![0x5A; 16], vec![])
        .expect_err("a 16-byte topic is refused");

    assert!(
        matches!(err, FfiError::BadGroupIdLength { got: 16, .. }),
        "expected a group-id-length refusal, got {err:?}"
    );
}

/// Every refusal that crosses uniffi must carry its sentence in a FIELD. This
/// is the S1 finding and it is pinned here rather than assumed, because the
/// failure mode is invisible: the typed exception arrives and `message` is
/// empty, so a shell rendering `e.message` renders nothing at all.
#[test]
fn every_link_refusal_crosses_with_its_words() {
    let _swarm = SwarmLock::acquire();
    let refusals = vec![
        GossipLink::start(vec![1u8; 3], topic(), vec![]).unwrap_err(),
        GossipLink::start(vec![1u8; 32], vec![0u8; 2], vec![]).unwrap_err(),
        PairingCode::read("not a pairing code at all").unwrap_err(),
    ];

    for err in refusals {
        assert!(
            !err.reason().is_empty(),
            "a refusal crossed the boundary with no words: {err:?}"
        );
    }
}

#[test]
fn a_pairing_code_round_trips_through_the_boundary_types() {
    let _swarm = SwarmLock::acquire();
    let link = GossipLink::start(vec![2u8; 32], topic(), vec![]).expect("a link starts");
    let key_package = vec![0xAB; 200];

    let code = link
        .pairing_code(key_package.clone())
        .expect("a code is made");
    let read = PairingCode::read(&code).expect("the code reads back");

    assert_eq!(read.key_package, key_package);
    assert_eq!(read.card, link.dial_card());
    link.shutdown();
}

/// The refusal a person mid-pairing actually sees. It must name the problem in
/// words, not in a type name.
#[test]
fn a_mistyped_pairing_code_is_refused_in_words_a_person_can_act_on() {
    let _swarm = SwarmLock::acquire();
    let link = GossipLink::start(vec![3u8; 32], topic(), vec![]).expect("a link starts");
    let code = link.pairing_code(vec![0xCD; 100]).unwrap();

    // Flip one character, the way a person reading it aloud would.
    let mut chars: Vec<char> = code.chars().collect();
    chars[4] = if chars[4] == 'A' { 'B' } else { 'A' };
    let mistyped: String = chars.into_iter().collect();

    let err = PairingCode::read(&mistyped).expect_err("a mistyped code is refused");

    let said = err.reason().to_lowercase();
    assert!(
        said.contains("typo") || said.contains("check it"),
        "the refusal should tell the person to check the code, got {said:?}"
    );
    link.shutdown();
}

#[test]
fn a_link_with_no_peer_reports_no_neighbours_and_hears_nothing() {
    let _swarm = SwarmLock::acquire();
    let link = GossipLink::start(vec![4u8; 32], topic(), vec![]).expect("a link starts");

    assert_eq!(link.neighbour_count(), 0);
    assert!(
        link.next_artifact(200).is_none(),
        "a link with nobody on the topic must hear nothing, not block forever"
    );
    link.shutdown();
}

/// Quiet is not failure. A shell that treated a timeout as an error would show
/// a refusal on a phone that is simply waiting, which is the most common state
/// during a device run.
#[test]
fn a_timeout_is_none_rather_than_a_refusal() {
    let _swarm = SwarmLock::acquire();
    let link = GossipLink::start(vec![5u8; 32], topic(), vec![]).expect("a link starts");

    let got = link.next_artifact(100);

    assert!(got.is_none());
    link.shutdown();
}
