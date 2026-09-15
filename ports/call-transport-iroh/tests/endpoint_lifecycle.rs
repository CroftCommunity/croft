//! The endpoint lifecycle the §15 defect lived in, hermetic.
//!
//! The relay here is UNROUTABLE by construction (TEST-NET-1, RFC 5737), so no
//! external host is contacted and no admission token is needed: these rows
//! are about what the port does to ITS OWN endpoint when asked to rebind, and
//! whether it tells the truth about not being camped. The attach itself is the
//! `:live` regression's job (`tests/live_s15_regression.rs`).
//!
//! The R0 rule under test — a dial never lowers admission — is
//! `call_core::dial::rebind`, and these rows prove the port OBEYS it rather
//! than merely that the rule exists: a port that closes and re-binds on every
//! call, as `CallPeer.rebindWithToken` did before §15.3, fails
//! `rebinding_to_tokenless_over_a_live_pass_keeps_the_endpoint` because the
//! endpoint it held is gone.

use std::time::Duration;

use call_transport_iroh::{BindOptions, CallEndpoint, Discovery, Rebound, RelayTarget};

/// RFC 5737 TEST-NET-1: reserved for documentation, guaranteed not a host.
fn unroutable_relay() -> RelayTarget {
    RelayTarget::new("https://192.0.2.1:443")
}

fn bind(seed: u8, token: Option<&str>) -> CallEndpoint {
    CallEndpoint::bind(BindOptions {
        secret_key: [seed; 32],
        relay: unroutable_relay(),
        token: token.map(str::to_string),
        discovery: Discovery::None,
    })
    .expect("an endpoint binds regardless of whether its relay answers")
}

#[test]
fn the_endpoint_id_is_the_persisted_keys_public_half() {
    // The mint binds a pass to an endpoint id; a persisted secret key is what
    // keeps that id stable across binds (M4c). Same key, same id, every time.
    let first = bind(0x31, None);
    let again = bind(0x31, None);
    assert_eq!(first.endpoint_id(), again.endpoint_id());
    assert_eq!(
        first.endpoint_id().len(),
        64,
        "an endpoint id prints as 32 bytes of lowercase hex"
    );
    assert_eq!(first.secret_key_bytes(), [0x31; 32]);
    let other = bind(0x32, None);
    assert_ne!(first.endpoint_id(), other.endpoint_id());
    first.shutdown();
    again.shutdown();
    other.shutdown();
}

#[test]
fn the_short_id_is_what_the_relay_journal_prints() {
    // `admitted endpoint_id=14af214d8c …` — ten hex characters, the prefix.
    let ep = bind(0x33, None);
    assert_eq!(ep.endpoint_id_short(), &ep.endpoint_id()[..10]);
    ep.shutdown();
}

#[test]
fn rebinding_to_tokenless_over_a_live_pass_keeps_the_endpoint() {
    // §15.3, the defect: the tokenless dial path asked for `null` over a live
    // camping pass, the endpoint was torn down and re-attached with nothing,
    // and enforce refused it ~20 times. The port must keep the endpoint AND
    // the pass.
    let mut ep = bind(0x34, Some("a-live-camping-pass"));
    let before = ep.endpoint_generation();
    let outcome = ep.rebind(None).expect("a keep cannot fail");
    assert_eq!(outcome, Rebound::Kept);
    assert_eq!(
        ep.bound_token(),
        Some("a-live-camping-pass"),
        "the pass is still bound"
    );
    assert_eq!(
        ep.endpoint_generation(),
        before,
        "the SAME endpoint is still bound — no stop/start"
    );
    ep.shutdown();
}

#[test]
fn rebinding_to_the_token_already_bound_keeps_the_endpoint() {
    let mut ep = bind(0x35, Some("same"));
    let before = ep.endpoint_generation();
    assert_eq!(ep.rebind(Some("same")).expect("keep"), Rebound::Kept);
    assert_eq!(ep.endpoint_generation(), before);
    ep.shutdown();
}

#[test]
fn rebinding_to_a_different_token_swaps_and_keeps_the_id() {
    // The designed M4c path: a dial minted its own admitting token. The swap
    // costs a stop/start (the token belongs to the endpoint), and the persisted
    // key must make the id survive it — the mint bound the token to that id.
    let mut ep = bind(0x36, Some("the-camping-pass"));
    let id_before = ep.endpoint_id();
    let gen_before = ep.endpoint_generation();
    assert_eq!(
        ep.rebind(Some("the-dial-token")).expect("swap"),
        Rebound::Swapped
    );
    assert_eq!(ep.bound_token(), Some("the-dial-token"));
    assert_eq!(
        ep.endpoint_id(),
        id_before,
        "the id must survive a token swap"
    );
    assert_ne!(
        ep.endpoint_generation(),
        gen_before,
        "a swap is a new endpoint"
    );
    ep.shutdown();
}

#[test]
fn rebinding_from_tokenless_to_a_token_swaps() {
    // Upgrading is never a downgrade: a tokenless endpoint given a pass binds it.
    let mut ep = bind(0x37, None);
    assert_eq!(
        ep.rebind(Some("first-pass")).expect("swap"),
        Rebound::Swapped
    );
    assert_eq!(ep.bound_token(), Some("first-pass"));
    ep.shutdown();
}

#[test]
fn an_endpoint_whose_relay_never_answers_is_honestly_not_camped() {
    // E135(a): the CONFIGURED relay is not the ATTACHED relay. Against a relay
    // that never answers, the port must say None, never the configured URL.
    let ep = bind(0x38, Some("irrelevant"));
    let attached = ep.attached_relay(Duration::from_millis(1500));
    assert_eq!(
        attached, None,
        "a relay that never answered must not read as camped"
    );
    ep.shutdown();
}
