//! The arc's own endpoint record: what it publishes, when, and how it reads
//! one back.
//!
//! Contract v2 §1: one `ing.croft.iroh.endpoint` record per device, any
//! stable rkey. The arc publishes under ITS OWN rkey (its label) and never
//! touches `self` — the phones' records are the phones'. The camp mint only
//! needs *a* record of the DID to name this endpoint id; which one is
//! immaterial to it and material to everyone else.

use croft_arc::records::{
    iso8601_utc, parse_record, reconcile, record_json, EndpointRecord, Reconcile,
};

fn wanted() -> EndpointRecord {
    EndpointRecord {
        endpoint_id: "ab".repeat(32),
        home_relay: "https://relay.croft.ing:8443".to_string(),
        label: "croft-arc".to_string(),
    }
}

#[test]
fn no_record_means_publish() {
    assert_eq!(reconcile(None, &wanted()), Reconcile::Publish);
}

#[test]
fn the_same_record_is_left_alone() {
    // Every run would otherwise rewrite the record; a PDS write per run is
    // noise in the repo's history and a place for a race with a real device.
    assert_eq!(reconcile(Some(&wanted()), &wanted()), Reconcile::Unchanged);
}

#[test]
fn a_record_naming_another_endpoint_or_relay_is_republished() {
    let mut other_id = wanted();
    other_id.endpoint_id = "cd".repeat(32);
    assert_eq!(reconcile(Some(&other_id), &wanted()), Reconcile::Publish);
    let mut other_relay = wanted();
    other_relay.home_relay = "https://relay.croft.ing:8444".to_string();
    assert_eq!(reconcile(Some(&other_relay), &wanted()), Reconcile::Publish);
}

#[test]
fn the_wire_form_is_the_contracts_and_carries_its_type() {
    // Field names from connect docs/contract.md §1 and a live record read
    // 2026-09-14: $type, createdAt, endpointId, homeRelay, label.
    let v = record_json(&wanted(), "2026-09-14T00:00:00Z");
    assert_eq!(v["$type"], "ing.croft.iroh.endpoint");
    assert_eq!(v["endpointId"], "ab".repeat(32));
    assert_eq!(v["homeRelay"], "https://relay.croft.ing:8443");
    assert_eq!(v["label"], "croft-arc");
    assert_eq!(v["createdAt"], "2026-09-14T00:00:00Z");
    assert_eq!(
        v.as_object().map(serde_json::Map::len),
        Some(5),
        "exactly the contract's fields"
    );
}

#[test]
fn reading_back_tolerates_missing_optionals_and_refuses_a_missing_id() {
    let full = serde_json::json!({
        "$type": "ing.croft.iroh.endpoint", "endpointId": "ab", "homeRelay": "https://r", "label": "x", "createdAt": "t"
    });
    assert_eq!(
        parse_record(&full),
        Some(EndpointRecord {
            endpoint_id: "ab".to_string(),
            home_relay: "https://r".to_string(),
            label: "x".to_string()
        })
    );
    let bare = serde_json::json!({ "endpointId": "ab" });
    assert_eq!(
        parse_record(&bare).map(|r| r.home_relay),
        Some(String::new()),
        "homeRelay is optional in the contract"
    );
    // A record with no endpointId is malformed, not "not listed" (contract §1).
    assert_eq!(parse_record(&serde_json::json!({ "label": "x" })), None);
    assert_eq!(parse_record(&serde_json::json!({ "endpointId": "" })), None);
}

#[test]
fn created_at_is_iso8601_utc_to_the_second() {
    assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00Z");
    assert_eq!(iso8601_utc(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(iso8601_utc(1_757_808_000), "2025-09-14T00:00:00Z");
    assert_eq!(iso8601_utc(1_757_808_000 + 3_661), "2025-09-14T01:01:01Z");
}
