//! Does the transport still work when several of them are alive in one
//! process? The loopback suite passes with `--test-threads=1` and fails with
//! 2, which points at the number of live transports rather than at load.
//!
//! Two arms, selected by argv so they can be compared in isolation:
//!   `sequential` — three pairs started one after another, all left alive
//!   `concurrent` — three pairs started from three threads at once
//!
//! Run: `cargo run -p transport-iroh --example probe_many -- sequential`

use std::time::Duration;

use transport_iroh::{DialCard, GossipTransport, TopicKey};

fn pair(seed: u8) -> (GossipTransport, GossipTransport, DialCard) {
    let topic = TopicKey::from_group_id(&[seed; 32]);
    let a = GossipTransport::start(&[seed; 32], topic, &[]).expect("A starts");
    let card = a.dial_card();
    let b = GossipTransport::start(
        &[seed.wrapping_add(100); 32],
        topic,
        std::slice::from_ref(&card),
    )
    .expect("B starts");
    (a, b, card)
}

fn main() {
    let arm = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sequential".into());
    println!("arm = {arm}");

    let pairs: Vec<(GossipTransport, GossipTransport, DialCard)> = if arm == "concurrent" {
        let handles: Vec<_> = [1u8, 2, 3]
            .into_iter()
            .map(|s| std::thread::spawn(move || pair(s)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    } else {
        [1u8, 2, 3].into_iter().map(pair).collect()
    };

    for (i, (a, b, card)) in pairs.iter().enumerate() {
        println!(
            "pair {i}: A={} B={} card_addrs={:?}",
            a.endpoint_id_short(),
            b.endpoint_id_short(),
            card.addrs
        );
    }

    for t in 0..24 {
        let counts: Vec<String> = pairs
            .iter()
            .map(|(a, b, _)| format!("{}/{}", a.neighbour_count(), b.neighbour_count()))
            .collect();
        println!(
            "t={:>5}ms  a/b neighbours per pair: {}",
            t * 500,
            counts.join("  ")
        );
        if pairs.iter().all(|(a, _, _)| a.neighbour_count() > 0) {
            println!("ALL PAIRS FORMED at ~{}ms", t * 500);
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
