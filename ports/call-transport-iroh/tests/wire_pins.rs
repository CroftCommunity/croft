//! The v0 wire format, pinned byte-for-byte against the Kotlin
//! (`android/app/.../net/WireFormat.kt`): this port will dial a phone running
//! the released app, so the ALPN and the hello frame must match exactly, not
//! approximately.
//!
//! Frame: u16 length (big-endian) + UTF-8 JSON `{"hello":"<from>"}`.

use call_transport_iroh::wire::{decode_hello, encode_hello, frame_length, ALPN};

#[test]
fn the_alpn_is_the_v0_string_the_app_binds_with() {
    assert_eq!(ALPN, b"croft-call/0");
}

#[test]
fn a_hello_is_a_big_endian_length_then_json() {
    let frame = encode_hello("alice").expect("small hello encodes");
    let body = br#"{"hello":"alice"}"#;
    assert_eq!(&frame[..2], &[0x00, body.len() as u8]);
    assert_eq!(&frame[2..], body);
}

#[test]
fn a_hello_escapes_quotes_and_backslashes_the_way_the_kotlin_does() {
    // WireFormat.jsonString escapes exactly `\` and `"`. serde_json escapes
    // the same two (plus control characters, which a handle cannot carry).
    let frame = encode_hello(r#"a"b\c"#).expect("encodes");
    assert_eq!(&frame[2..], br#"{"hello":"a\"b\\c"}"#);
}

#[test]
fn frame_length_reads_the_two_header_bytes_big_endian() {
    assert_eq!(frame_length([0x01, 0x02]), 258);
    assert_eq!(frame_length([0x00, 0x00]), 0);
    assert_eq!(frame_length([0xFF, 0xFF]), 0xFFFF);
}

#[test]
fn a_hello_too_large_for_the_u16_header_is_refused() {
    let big = "x".repeat(0x1_0000);
    assert!(
        encode_hello(&big).is_err(),
        "a body over 0xFFFF bytes cannot be framed"
    );
}

#[test]
fn a_hello_body_decodes_to_the_name_and_garbage_to_none() {
    assert_eq!(decode_hello(br#"{"hello":"bob"}"#), Some("bob".to_string()));
    assert_eq!(decode_hello(b"not json"), None);
    assert_eq!(decode_hello(br#"{"other":"bob"}"#), None);
}

#[test]
fn a_hello_round_trips() {
    let frame = encode_hello("croftcall-android").expect("encodes");
    let len = frame_length([frame[0], frame[1]]);
    assert_eq!(len, frame.len() - 2);
    assert_eq!(
        decode_hello(&frame[2..]),
        Some("croftcall-android".to_string())
    );
}
