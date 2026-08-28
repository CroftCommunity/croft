//! A probe, not a test: two gossip endpoints on loopback with the relay
//! disabled and no discovery service, printing what actually happens.
//!
//! The house rule is to confirm an external API's real shapes before building
//! logic on them. This exists to answer four questions that source-reading
//! could not settle:
//!
//! 1. Does `presets::Minimal` + `RelayMode::Disabled` bind at all?
//! 2. Does `endpoint.addr()` carry usable direct IP addrs with no relay and
//!    no discovery, and how long until it does?
//! 3. Does `MemoryLookup` alone let one endpoint dial another?
//! 4. Does a gossip broadcast actually arrive?
//!
//! Run with: `cargo run -p transport-iroh --example probe_loopback`

use std::collections::BTreeSet;
use std::time::Duration;

use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::presets;
use iroh::protocol::Router;
use iroh::{Endpoint, RelayMode};
use iroh_gossip::api::Event;
use iroh_gossip::net::{Gossip, GOSSIP_ALPN};
use iroh_gossip::proto::TopicId;
use n0_future::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .init();

    let lookup_a = MemoryLookup::new();
    let lookup_b = MemoryLookup::new();

    let ep_a = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .address_lookup(lookup_a.clone())
        .bind()
        .await?;
    let ep_b = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .address_lookup(lookup_b.clone())
        .bind()
        .await?;

    println!("Q1 bind: OK");
    println!("   a id = {}", ep_a.id().fmt_short());
    println!("   b id = {}", ep_b.id().fmt_short());

    // Q2: how long until direct addrs appear, with no relay and no discovery?
    for i in 0..40 {
        let a = ep_a.addr();
        let b = ep_b.addr();
        if !a.addrs.is_empty() && !b.addrs.is_empty() {
            println!("Q2 addrs after ~{}ms:", i * 50);
            println!("   a addrs = {:?}", a.addrs);
            println!("   b addrs = {:?}", b.addrs);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let addr_a = ep_a.addr();
    let addr_b = ep_b.addr();
    if addr_a.addrs.is_empty() || addr_b.addrs.is_empty() {
        println!("Q2 FAILED: no direct addrs appeared");
        return Ok(());
    }
    // Anything relay-shaped here would mean the severance is not structural.
    let relayish: BTreeSet<_> = addr_a.addrs.iter().filter(|a| a.is_relay()).collect();
    println!("Q2 relay-shaped addrs on A (must be empty): {relayish:?}");

    // Q3: teach each side about the other out-of-band, the way a pairing blob
    // would, and nothing else.
    // ONE-WAY, to isolate the variable: B learns A (a scanned QR), A learns
    // nothing. This is exactly what GossipTransport does.
    let _ = &addr_b;
    lookup_b.add_endpoint_info(addr_a.clone());
    println!("Q3 ONE-WAY registration: B knows A, A knows nothing about B");

    let gossip_a = Gossip::builder().spawn(ep_a.clone());
    let gossip_b = Gossip::builder().spawn(ep_b.clone());

    let _router_a = Router::builder(ep_a.clone())
        .accept(GOSSIP_ALPN, gossip_a.clone())
        .spawn();
    let _router_b = Router::builder(ep_b.clone())
        .accept(GOSSIP_ALPN, gossip_b.clone())
        .spawn();

    let topic = TopicId::from_bytes([0x42; 32]);

    // B subscribes with A as bootstrap; A subscribes with no bootstrap.
    let mut topic_a = gossip_a.subscribe(topic, vec![]).await?;
    let topic_b = gossip_b.subscribe(topic, vec![ep_a.id()]).await?;

    let (_tx_b, mut rx_b) = topic_b.split();

    // Q4: does a broadcast arrive?
    let recv = tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        while let Ok(Some(event)) =
            tokio::time::timeout_at(deadline, rx_b.next()).await
        {
            match event {
                Ok(Event::Received(msg)) => {
                    return Some(String::from_utf8_lossy(&msg.content).to_string());
                }
                Ok(other) => println!("   b event: {other:?}"),
                Err(e) => {
                    println!("   b error: {e:?}");
                    return None;
                }
            }
        }
        None
    });

    // Wait for the swarm to actually form before broadcasting.
    topic_a.joined().await?;
    println!("Q4 topic A joined; neighbors = {}", topic_a.neighbors().count());

    topic_a.broadcast("hello from A".as_bytes().to_vec().into()).await?;
    println!("Q4 broadcast sent");

    match recv.await? {
        Some(got) => println!("Q4 RECEIVED ON B: {got:?}"),
        None => println!("Q4 FAILED: nothing received on B"),
    }

    Ok(())
}
