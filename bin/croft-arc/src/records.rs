//! This device's `ing.croft.iroh.endpoint` record (contract v2 §1).
//!
//! The arc publishes under its OWN rkey — its label — and never `self`:
//! the phones' records are the phones'. The camp mint only needs *a* record
//! of the DID naming this endpoint id.

/// The collection, as the contract names it.
pub const ENDPOINT_COLLECTION: &str = "ing.croft.iroh.endpoint";

/// One device's record, the fields this arc reads and writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointRecord {
    /// The iroh endpoint id to dial, lowercase hex.
    pub endpoint_id: String,
    /// The relay hint; empty if absent (discovery-only).
    pub home_relay: String,
    /// The human name; empty if absent.
    pub label: String,
}

/// Whether the stored record needs writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconcile {
    /// Absent or different: write it.
    Publish,
    /// Already says exactly this: leave the repo alone.
    Unchanged,
}

/// Compare what is published with what this run would publish.
#[must_use]
pub fn reconcile(existing: Option<&EndpointRecord>, wanted: &EndpointRecord) -> Reconcile {
    match existing {
        Some(e) if e == wanted => Reconcile::Unchanged,
        _ => Reconcile::Publish,
    }
}

/// The wire form: exactly the contract's fields, with the collection as
/// `$type` (a live record read 2026-09-14 carries the same five).
#[must_use]
pub fn record_json(record: &EndpointRecord, created_at: &str) -> serde_json::Value {
    serde_json::json!({
        "$type": ENDPOINT_COLLECTION,
        "endpointId": record.endpoint_id,
        "homeRelay": record.home_relay,
        "label": record.label,
        "createdAt": created_at,
    })
}

/// Read a record value back. `None` for a record with no `endpointId`,
/// which the contract calls malformed rather than absent.
#[must_use]
pub fn parse_record(value: &serde_json::Value) -> Option<EndpointRecord> {
    let endpoint_id = value.get("endpointId")?.as_str()?;
    if endpoint_id.is_empty() {
        return None;
    }
    let text = |k: &str| {
        value
            .get(k)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Some(EndpointRecord {
        endpoint_id: endpoint_id.to_string(),
        home_relay: text("homeRelay"),
        label: text("label"),
    })
}

/// `createdAt` for a record written now: ISO-8601 UTC to the second, from
/// unix seconds, with no calendar crate (the civil-from-days algorithm).
#[must_use]
pub fn iso8601_utc(unix_secs: u64) -> String {
    let days = unix_secs / 86_400;
    let rem = unix_secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant's days-to-civil, for the proleptic Gregorian calendar.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}
