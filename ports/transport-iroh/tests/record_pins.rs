//! The third artifact kind: a group's record, offered two ways.
//!
//! An invite hands the joining device the key to the lockbox (the Welcome) and
//! the group's record. The record can travel two ways and the wire admits both
//! from the start:
//!
//! - **Inline** — the envelopes themselves, when they fit.
//! - **Elsewhere** — a locator saying where and how to fetch them.
//!
//! Only Inline is implemented. Elsewhere is decoded and refused in words,
//! deliberately: gossip's ceiling is 4096 bytes and a real group's record will
//! pass it, so the day a fetch is needed must not be a breaking wire change.
//! Writing a fetcher S2 cannot exercise would leave code that looks done and has
//! never run, which this plan has already paid for once.
//!
//! Note what does NOT change here: [`WIRE_VERSION`]. The frame's own bump policy
//! says adding a kind byte does not bump it, because an old build refuses an
//! unknown kind by number and that is the degrade path we want. Bumping would
//! instead make old builds refuse frames they still understand.

use transport_iroh::frame::{decode, encode, ArtifactKind, WIRE_VERSION};
use transport_iroh::record::{decode_offer, encode_elsewhere, encode_inline, RecordOffer};
use transport_iroh::TransportError;

fn envelopes() -> Vec<Vec<u8>> {
    vec![
        b"pretend this is a genesis envelope".to_vec(),
        b"and this a membership add".to_vec(),
        vec![0xEE; 700],
    ]
}

#[test]
fn an_inline_record_round_trips_its_envelopes_in_order() {
    let wire = encode_inline(&envelopes()).expect("a non-empty record encodes");

    match decode_offer(&wire).expect("it decodes") {
        RecordOffer::Inline(got) => assert_eq!(got, envelopes(), "order and bytes both survive"),
        other => panic!("expected an inline record, got {other:?}"),
    }
}

/// Replay order is not cosmetic: a MembershipAdd folded before its genesis is
/// refused, so a record that arrives shuffled strands the joining device.
#[test]
fn inline_order_is_preserved_exactly() {
    let ordered: Vec<Vec<u8>> = (0u8..12).map(|i| vec![i; (i as usize) + 1]).collect();

    let wire = encode_inline(&ordered).unwrap();
    let RecordOffer::Inline(got) = decode_offer(&wire).unwrap() else {
        panic!("expected inline")
    };

    assert_eq!(got, ordered);
}

#[test]
fn an_elsewhere_record_round_trips_its_locator() {
    let wire = encode_elsewhere("croft://record/abc123").expect("a locator encodes");

    match decode_offer(&wire).expect("it decodes") {
        RecordOffer::Elsewhere(loc) => assert_eq!(loc, "croft://record/abc123"),
        other => panic!("expected an elsewhere record, got {other:?}"),
    }
}

/// The point of admitting the form now: a build that cannot fetch must still be
/// able to SAY that is what happened. A generic parse error would send whoever
/// reads that logcat line looking for a corrupted frame.
#[test]
fn an_elsewhere_record_decodes_even_though_nothing_can_fetch_it_yet() {
    let wire = encode_elsewhere("https://example.invalid/record").unwrap();

    let offer = decode_offer(&wire).expect("the form is understood, not mistaken for damage");

    assert!(matches!(offer, RecordOffer::Elsewhere(_)));
}

/// An invite with no record leaves the joiner seated in the lockbox and absent
/// from the record — exactly the split this artifact exists to close.
#[test]
fn an_empty_inline_record_is_refused_at_encode() {
    let err = encode_inline(&[]).expect_err("an empty record is refused");

    assert!(
        matches!(err, TransportError::EmptyRecord),
        "expected an empty-record refusal, got {err:?}"
    );
}

#[test]
fn an_empty_locator_is_refused_at_encode() {
    let err = encode_elsewhere("").expect_err("a locator that names nowhere is refused");

    assert!(matches!(err, TransportError::EmptyRecord), "got {err:?}");
}

#[test]
fn an_unknown_record_form_is_refused_naming_the_byte() {
    let mut wire = encode_inline(&envelopes()).unwrap();
    wire[0] = 0x5B;

    let err = decode_offer(&wire).expect_err("an unknown form is refused");

    match err {
        TransportError::UnknownRecordForm { got } => assert_eq!(got, 0x5B),
        other => panic!("expected an unknown-form refusal, got {other:?}"),
    }
}

#[test]
fn a_truncated_record_is_refused_rather_than_half_folded() {
    let wire = encode_inline(&envelopes()).unwrap();

    for cut in [0, 1, 3, 9, wire.len() / 2, wire.len() - 1] {
        assert!(
            decode_offer(&wire[..cut]).is_err(),
            "a record truncated to {cut} bytes decoded anyway"
        );
    }
}

/// The kind byte is what tells the receiver to fold rather than to seat or to
/// open. It must be its own value and must not collide with the two that were
/// already on the wire.
#[test]
fn the_record_kind_is_distinct_from_the_other_two() {
    let kinds = [
        ArtifactKind::Welcome,
        ArtifactKind::Sealed,
        ArtifactKind::Record,
    ];

    let mut bytes: Vec<u8> = kinds.iter().map(|k| k.as_byte()).collect();
    bytes.sort_unstable();
    bytes.dedup();
    assert_eq!(bytes.len(), 3, "two kinds share a wire byte");

    for k in kinds {
        assert_eq!(ArtifactKind::from_byte(k.as_byte()), Some(k));
    }
}

/// A record crosses inside an ordinary frame, so the framing pins keep applying
/// to it — including the version refusal.
#[test]
fn a_record_travels_in_a_frame_like_anything_else() {
    let body = encode_inline(&envelopes()).unwrap();

    let framed = encode(ArtifactKind::Record, &body).unwrap();
    let got = decode(&framed).unwrap();

    assert_eq!(got.kind, ArtifactKind::Record);
    assert_eq!(got.payload, body);
    assert_eq!(framed[0], WIRE_VERSION, "the frame version is unchanged");
}

/// Adding a kind is not a wire-version change, and this pins the policy the
/// frame's own docs state — because the tempting thing on the next kind will be
/// to bump, and bumping makes every older build refuse frames it still reads.
#[test]
fn adding_a_third_kind_did_not_bump_the_frame_version() {
    assert_eq!(WIRE_VERSION, 1);
}
