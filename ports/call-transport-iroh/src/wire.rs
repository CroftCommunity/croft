//! Protocol identity for Croft Call, v0 — the Rust of `WireFormat.kt`.
//!
//! This port dials phones running the released app, so these bytes are a
//! contract with a shipped artifact, not a design choice: the ALPN routes the
//! connection to the right handler on both sides, and the hello frame is the
//! one exchange v0 makes. Frame: u16 length (big-endian) + UTF-8 JSON
//! `{"hello":"<from>"}`.
//!
//! No iroh in this module, so the byte-level pins run without a network
//! (`tests/wire_pins.rs`).

use crate::CallTransportError;

/// The ALPN both sides bind and dial with.
pub const ALPN: &[u8] = b"croft-call/0";

/// The largest body a u16 header can announce.
const MAX_BODY: usize = 0xFFFF;

/// Encode a hello frame: two length bytes, then `{"hello":"<from>"}`.
///
/// Escaping matches the Kotlin's `jsonString` for everything a handle can
/// contain (`\` and `"`); serde_json additionally escapes control characters,
/// which the Kotlin would have emitted raw and no handle carries.
pub fn encode_hello(from: &str) -> Result<Vec<u8>, CallTransportError> {
    let body = serde_json::json!({ "hello": from }).to_string();
    let body = body.as_bytes();
    if body.len() > MAX_BODY {
        return Err(CallTransportError::HelloTooLarge {
            got: body.len(),
            max: MAX_BODY,
        });
    }
    // Checked above: the length fits in a u16.
    #[allow(clippy::cast_possible_truncation)]
    let len = body.len() as u16;
    let mut frame = Vec::with_capacity(2 + body.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(body);
    Ok(frame)
}

/// The body length a two-byte header announces, big-endian.
#[must_use]
pub fn frame_length(header: [u8; 2]) -> usize {
    usize::from(u16::from_be_bytes(header))
}

/// The name a hello body carries, or `None` if it is not a v0 hello.
///
/// A peer that sent something else is not an error the call needs to
/// surface — the Kotlin reads `null` and carries on — so this is an `Option`.
#[must_use]
pub fn decode_hello(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    value.get("hello")?.as_str().map(str::to_string)
}
