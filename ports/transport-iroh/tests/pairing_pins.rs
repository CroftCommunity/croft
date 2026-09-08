//! The pairing blob: what one phone shows and the other reads.
//!
//! This is the step that does not exist anywhere else in the estate. The
//! calling app has exchange-invite machinery, and reusing it here would drag
//! the connect contract into a dev app that has no business speaking it — so
//! S2 gets its own, deliberately small artifact: who to dial, where, and the
//! key package to invite.
//!
//! The tests are heavy on refusals because a pairing blob is the one thing in
//! this phase a HUMAN transcribes. A mistyped character must fail loudly at
//! the moment of pairing, not silently produce an endpoint id nobody can dial
//! and a "waiting for the other device" that never ends.

use transport_iroh::pairing::{decode_blob, encode_blob, PairingBlob, BLOB_VERSION};
use transport_iroh::{DialCard, TransportError};

fn a_blob() -> PairingBlob {
    PairingBlob {
        card: DialCard {
            endpoint_id: "8a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c"
                .to_string(),
            addrs: vec!["192.168.50.235:55348".to_string()],
        },
        key_package: vec![0xAB; 300],
        group_id: vec![0x7C; 32],
    }
}

#[test]
fn a_pairing_blob_round_trips_everything_the_other_device_needs() {
    let blob = a_blob();

    let text = encode_blob(&blob).expect("a well-formed blob encodes");
    let got = decode_blob(&text).expect("what we just encoded decodes");

    assert_eq!(got, blob);
}

#[test]
fn several_addresses_all_survive_the_round_trip() {
    let mut blob = a_blob();
    blob.card.addrs = vec![
        "192.168.50.235:55348".to_string(),
        "10.0.0.4:1234".to_string(),
        "[::1]:9999".to_string(),
    ];

    let got = decode_blob(&encode_blob(&blob).unwrap()).unwrap();

    assert_eq!(got.card.addrs, blob.card.addrs);
}

/// QR codes have an alphanumeric mode covering `0-9 A-Z` and a few symbols; a
/// blob that stays inside uppercase base32 encodes far more densely than one
/// that forces byte mode. It also survives being read aloud or typed, which is
/// the fallback when a camera will not focus.
#[test]
fn the_blob_is_safe_for_a_qr_code_and_for_a_human_to_type() {
    let text = encode_blob(&a_blob()).unwrap();

    assert!(!text.is_empty(), "an empty pairing code pairs nothing");
    for c in text.chars() {
        assert!(
            c.is_ascii_uppercase() || c.is_ascii_digit(),
            "a pairing code must stay in QR alphanumeric mode, found {c:?} in {text:?}"
        );
    }
}

#[test]
fn a_blob_from_a_future_version_is_refused_naming_the_version() {
    let blob = a_blob();
    let text = encode_blob(&blob).unwrap();

    // Re-encode with the version byte bumped, checksum recomputed, so that the
    // ONLY thing wrong is the version.
    let bumped = transport_iroh::pairing::testing::reencode_with_version(&text, BLOB_VERSION + 1);

    let err = decode_blob(&bumped).expect_err("a future blob version is refused");

    match err {
        TransportError::BlobVersion { got, expected } => {
            assert_eq!(got, BLOB_VERSION + 1);
            assert_eq!(expected, BLOB_VERSION);
        }
        other => panic!("expected a blob-version refusal, got {other:?}"),
    }
}

/// The case this whole encoding is defensive about: a person reads a character
/// wrong. Every single-character corruption must be caught, not decoded into
/// something plausible.
#[test]
fn any_single_mistyped_character_is_refused() {
    let text = encode_blob(&a_blob()).unwrap();
    let alphabet: Vec<char> = ('A'..='Z').chain('2'..='7').collect();

    let mut checked = 0;
    for i in 0..text.len() {
        let original = text.as_bytes()[i] as char;
        // One substitution per position is enough to prove the checksum covers
        // that position; trying all 32 would make this test minutes long.
        let replacement = *alphabet
            .iter()
            .find(|c| **c != original)
            .expect("some other character exists");

        let mut corrupted: Vec<char> = text.chars().collect();
        corrupted[i] = replacement;
        let corrupted: String = corrupted.into_iter().collect();

        assert!(
            decode_blob(&corrupted).is_err(),
            "a mistyped character at position {i} ({original:?} -> {replacement:?}) decoded anyway"
        );
        checked += 1;
    }

    assert!(checked > 0, "the corruption sweep checked nothing");
}

#[test]
fn a_truncated_blob_is_refused() {
    let text = encode_blob(&a_blob()).unwrap();

    for cut in [0, 1, 5, text.len() / 2, text.len() - 1] {
        assert!(
            decode_blob(&text[..cut]).is_err(),
            "a blob truncated to {cut} characters decoded anyway"
        );
    }
}

#[test]
fn text_that_is_not_a_pairing_code_at_all_is_refused() {
    for junk in ["", "hello", "not a code", "!!!!", "aaaa"] {
        assert!(
            decode_blob(junk).is_err(),
            "junk text {junk:?} decoded as a pairing code"
        );
    }
}

/// A blob with no addresses produces a card that cannot be dialled, and with
/// no relay and no discovery there is no second chance to find the peer. Refuse
/// at the moment of encoding, where the operator can still do something.
#[test]
fn a_card_with_no_addresses_is_refused_at_encode() {
    let mut blob = a_blob();
    blob.card.addrs.clear();

    let err = encode_blob(&blob).expect_err("a card with nowhere to dial is refused");

    assert!(
        matches!(err, TransportError::BadDialCard { .. }),
        "expected a dial-card refusal, got {err:?}"
    );
}

#[test]
fn a_blob_with_no_key_package_is_refused_at_encode() {
    let mut blob = a_blob();
    blob.key_package.clear();

    let err = encode_blob(&blob).expect_err("a blob with no key package invites nobody");

    assert!(
        matches!(err, TransportError::BadDialCard { .. }),
        "expected a refusal naming the missing key package, got {err:?}"
    );
}

/// Whitespace and case are what a paste buffer and a human add. Accepting them
/// costs one line and saves a failed pairing on a phone.
#[test]
fn surrounding_whitespace_and_lowercase_are_tolerated() {
    let text = encode_blob(&a_blob()).unwrap();

    let messy = format!("  {}\n", text.to_lowercase());

    let got = decode_blob(&messy).expect("a pasted, lowercased code still pairs");
    assert_eq!(got, a_blob());
}

/// The gap the device run found. A joining device cannot join a swarm until it
/// knows the group, because the topic IS the group id — and until blob v2 the
/// only place a group id appeared was inside the record, which arrives over
/// that very swarm. The JVM tier missed it by handing the id between surfaces
/// directly; two phones have no such channel.
#[test]
fn a_hosts_code_carries_the_group_so_a_joiner_can_reach_its_swarm() {
    let blob = a_blob();

    let got = decode_blob(&encode_blob(&blob).unwrap()).unwrap();

    assert_eq!(got.group_id, blob.group_id, "the group survives the code");
    assert_eq!(got.group_id.len(), 32);
}

/// A joiner's code names no group, because the joiner has not got one. That has
/// to round-trip as cleanly as the host's, or the reply half of the exchange
/// cannot be encoded at all.
#[test]
fn a_joiners_code_carries_no_group_and_still_round_trips() {
    let mut blob = a_blob();
    blob.group_id = Vec::new();

    let got = decode_blob(&encode_blob(&blob).unwrap()).unwrap();

    assert!(got.group_id.is_empty());
    assert_eq!(
        got.key_package, blob.key_package,
        "the key package survives"
    );
}

/// A group id is 32 bytes or absent. Anything else is a malformed code, and
/// accepting it would put a truncated id into a topic derivation, which fails
/// later and somewhere else.
#[test]
fn a_group_id_of_the_wrong_length_is_refused_at_encode() {
    let mut blob = a_blob();
    blob.group_id = vec![0x01; 7];

    assert!(encode_blob(&blob).is_err());
}
