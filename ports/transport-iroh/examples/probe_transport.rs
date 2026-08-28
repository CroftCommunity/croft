//! The same loopback exchange, but driven through `GossipTransport` instead of
//! raw iroh — to find where the wrapper differs from the working probe.
//!
//! Run: `cargo run -p transport-iroh --example probe_transport`

use std::time::Duration;

use transport_iroh::frame::ArtifactKind;
use transport_iroh::{GossipTransport, TopicKey};

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let topic = TopicKey::from_group_id(&[0xAA; 32]);

    let a = GossipTransport::start(&[1u8; 32], topic, &[]).expect("A starts");
    let card = a.dial_card();
    println!("A card = {card:?}");
    println!("A endpoint = {}", a.endpoint_id_short());

    let b = GossipTransport::start(&[2u8; 32], topic, &[card]).expect("B starts");
    println!("B endpoint = {}", b.endpoint_id_short());
    println!("B card = {:?}", b.dial_card());

    for i in 0..20 {
        println!(
            "t={:>4}ms  a.neighbours={}  b.neighbours={}",
            i * 500,
            a.neighbour_count(),
            b.neighbour_count()
        );
        if a.neighbour_count() > 0 && b.neighbour_count() > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    println!("broadcasting from A");
    match a.broadcast(ArtifactKind::Welcome, b"hello from A") {
        Ok(()) => println!("broadcast returned Ok"),
        Err(e) => println!("broadcast refused: {e}"),
    }

    match b.next_artifact(Duration::from_secs(10)) {
        Some(f) => println!("B RECEIVED: kind={:?} payload={:?}", f.kind, f.payload),
        None => println!("B RECEIVED NOTHING"),
    }
}
