//! What the two artifact kinds look like on the wire, and every way a frame is
//! refused.
//!
//! These pins exist because the framing is the one part of the transport that
//! is **wire-visible between two builds**. During a device run the normal case
//! is two phones that were flashed at different moments; a frame the receiver
//! misreads is worse than one it rejects, because openmls's own parse error
//! names neither the kind nor the version and the failure surfaces as "the
//! message will not open" on the far device.
//!
//! No network, no runtime, no endpoint: the framing is deliberately reachable
//! without any of that, which is what lets it be tested exhaustively.

use transport_iroh::frame::{decode, encode, ArtifactKind, Frame, WIRE_VERSION};
use transport_iroh::TransportError;

#[test]
fn a_welcome_round_trips_its_kind_and_its_bytes() {
    let payload = b"a welcome, as openmls serialized it".to_vec();
    let wire = encode(ArtifactKind::Welcome, &payload).expect("a non-empty welcome encodes");

    let got = decode(&wire).expect("what we just encoded decodes");

    assert_eq!(
        got,
        Frame {
            kind: ArtifactKind::Welcome,
            payload,
        }
    );
}

#[test]
fn a_sealed_message_round_trips_its_kind_and_its_bytes() {
    let payload = vec![0xAB; 512];
    let wire = encode(ArtifactKind::Sealed, &payload).expect("a non-empty sealed message encodes");

    let got = decode(&wire).expect("what we just encoded decodes");

    assert_eq!(got.kind, ArtifactKind::Sealed);
    assert_eq!(got.payload, payload);
}

/// The whole reason the frame exists. If the two kinds could be confused, the
/// receiver would call the wrong key-layer method and fail inside openmls.
#[test]
fn the_two_kinds_do_not_decode_as_each_other() {
    let payload = b"same bytes, different kind".to_vec();

    let welcome = decode(&encode(ArtifactKind::Welcome, &payload).unwrap()).unwrap();
    let sealed = decode(&encode(ArtifactKind::Sealed, &payload).unwrap()).unwrap();

    assert_eq!(welcome.kind, ArtifactKind::Welcome);
    assert_eq!(sealed.kind, ArtifactKind::Sealed);
    assert_ne!(welcome.kind, sealed.kind);
    // Same payload, so any confusion would have to be in the kind alone.
    assert_eq!(welcome.payload, sealed.payload);
}

/// The two-phones-on-different-builds case, refused by number rather than
/// parsed optimistically.
#[test]
fn a_frame_from_a_future_wire_version_is_refused_naming_both_versions() {
    let mut wire = encode(ArtifactKind::Sealed, b"from a newer build").unwrap();
    wire[0] = WIRE_VERSION + 1;

    let err = decode(&wire).expect_err("a future version is refused, not guessed at");

    match err {
        TransportError::WireVersion { got, expected } => {
            assert_eq!(got, WIRE_VERSION + 1);
            assert_eq!(expected, WIRE_VERSION);
        }
        other => panic!("expected a wire-version refusal, got {other:?}"),
    }
}

/// A kind byte this build does not know is the degrade path a NEW kind takes
/// on an OLD build. It must name the byte, because that number is the only
/// clue to which build sent it.
#[test]
fn an_unknown_kind_byte_is_refused_naming_the_byte() {
    let mut wire = encode(ArtifactKind::Sealed, b"some third thing").unwrap();
    wire[1] = 0x7F;

    let err = decode(&wire).expect_err("an unknown kind is refused");

    match err {
        TransportError::UnknownKind { got } => assert_eq!(got, 0x7F),
        other => panic!("expected an unknown-kind refusal, got {other:?}"),
    }
}

/// Zero must not name a kind: a zero-filled or truncated buffer is exactly
/// what a bug produces, and it should not look like a valid Welcome.
#[test]
fn a_zero_kind_byte_names_nothing() {
    assert_eq!(ArtifactKind::from_byte(0), None);
}

#[test]
fn a_frame_too_short_to_hold_a_header_is_refused_naming_its_length() {
    for short in [vec![], vec![WIRE_VERSION]] {
        let err = decode(&short).expect_err("a frame shorter than the header is refused");
        match err {
            TransportError::ShortFrame { got } => assert_eq!(got, short.len()),
            other => panic!("expected a short-frame refusal, got {other:?}"),
        }
    }
}

/// A header with nothing after it. Neither kind has a meaningful empty form,
/// so this is refused on decode as well as on encode — the encode check
/// catches our own bugs, this one catches the wire's.
#[test]
fn a_header_with_no_payload_is_refused() {
    let wire = vec![WIRE_VERSION, ArtifactKind::Welcome.as_byte()];

    let err = decode(&wire).expect_err("a header-only frame is refused");

    assert!(
        matches!(err, TransportError::EmptyPayload { .. }),
        "expected an empty-payload refusal, got {err:?}"
    );
}

#[test]
fn encoding_an_empty_payload_is_refused_before_it_reaches_the_wire() {
    for kind in [ArtifactKind::Welcome, ArtifactKind::Sealed] {
        let err = encode(kind, b"").expect_err("an empty payload is refused at encode");
        assert!(
            matches!(err, TransportError::EmptyPayload { .. }),
            "expected an empty-payload refusal for {kind:?}, got {err:?}"
        );
    }
}

/// The refusal has to be readable by a person reading logcat on a phone, which
/// is the only instrument available during a device run.
///
/// Each case names the thing that was actually wrong, and that thing is not
/// always a number — which is what the first draft of this test got wrong. The
/// three malformed-wire refusals carry the offending byte or length, because
/// that number is the whole diagnostic. `EmptyPayload` carries the KIND
/// instead: nothing numeric was wrong, and asserting a digit there would have
/// forced a number into a sentence that has no use for one.
#[test]
fn every_framing_refusal_names_the_thing_that_was_wrong() {
    let numeric = vec![
        decode(&[]).unwrap_err(),
        decode(&[WIRE_VERSION + 9, 1, 0xFF]).unwrap_err(),
        decode(&[WIRE_VERSION, 0x7F, 0xFF]).unwrap_err(),
    ];
    for err in numeric {
        let said = err.to_string();
        assert!(!said.is_empty(), "a refusal with no words: {err:?}");
        assert!(
            said.chars().any(|c| c.is_ascii_digit()),
            "a malformed-wire refusal should name the number that was wrong, got {said:?}"
        );
    }

    for (kind, word) in [
        (ArtifactKind::Welcome, "welcome"),
        (ArtifactKind::Sealed, "sealed message"),
    ] {
        let said = encode(kind, b"").unwrap_err().to_string();
        assert!(
            said.contains(word),
            "an empty-payload refusal should name the kind {word:?}, got {said:?}"
        );
    }
}
