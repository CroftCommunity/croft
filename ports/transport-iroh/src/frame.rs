//! The wire frame: which of the two artifact kinds this gossip payload is.
//!
//! S2 carries exactly two things between devices — the Welcome, once, at
//! invite, and sealed application messages, ongoing — and gossip hands the
//! receiver an undifferentiated `Vec<u8>`. Something has to say which kind
//! arrived, because the receiving side calls a *different* key-layer method
//! for each and guessing wrong is not a recoverable error: `accept_invite` on
//! a sealed message and `open_sealed` on a Welcome both fail deep inside
//! openmls with a parse error that names neither.
//!
//! This is **wire-visible** and therefore versioned. A frame from a version
//! this build does not know is refused by number rather than parsed
//! optimistically — the house's fail-loud rule, and the reason is specific
//! here: two phones running different builds is the normal case during a
//! device run, not an exotic one.
//!
//! Deliberately in its own module with no iroh in it. The framing is the part
//! worth testing exhaustively and the part that needs no network to test, so
//! it is kept reachable without starting an endpoint or a runtime.

use crate::TransportError;

/// The wire version of the frame header.
///
/// Bump policy: any change to the header's LAYOUT or the meaning of an
/// existing kind byte bumps this. Adding a NEW kind byte does not — an old
/// build refuses an unknown kind by number, which is the degrade path we want,
/// and bumping the version would instead make old builds refuse every frame
/// including the ones they still understand.
pub const WIRE_VERSION: u8 = 1;

/// How many bytes precede the payload: the version and the kind.
const HEADER_LEN: usize = 2;

/// Which of S2's two artifact kinds a frame carries.
///
/// Two variants, not a general envelope. A third kind is a design decision
/// (what else do two devices owe each other?) and should have to be written
/// here rather than fitting through a generic field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    /// An MLS Welcome, delivered once when a device is invited.
    Welcome,
    /// A sealed application message.
    Sealed,
}

impl ArtifactKind {
    /// This kind's wire byte.
    ///
    /// Starts at 1, not 0: a zero byte is what a truncated or zero-filled
    /// buffer looks like, and it should not name a valid kind.
    #[must_use]
    pub fn as_byte(self) -> u8 {
        match self {
            ArtifactKind::Welcome => 1,
            ArtifactKind::Sealed => 2,
        }
    }

    /// The kind a wire byte names, or `None` if this build does not know it.
    #[must_use]
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(ArtifactKind::Welcome),
            2 => Some(ArtifactKind::Sealed),
            _ => None,
        }
    }
}

/// One decoded frame: what kind arrived, and the bytes it carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Which artifact this is.
    pub kind: ArtifactKind,
    /// The artifact's own bytes, header stripped.
    pub payload: Vec<u8>,
}

/// Frame `payload` as `kind` for the wire.
///
/// Refuses an empty payload rather than emitting a header-only frame. Neither
/// kind has a meaningful empty form — an empty Welcome seats nobody and an
/// empty sealed message opens to nothing — so an empty payload here is a
/// caller bug, and the cheapest place to catch it is before it reaches the
/// network and becomes someone else's parse error.
pub fn encode(kind: ArtifactKind, payload: &[u8]) -> Result<Vec<u8>, TransportError> {
    if payload.is_empty() {
        return Err(TransportError::EmptyPayload { kind: kind.name() });
    }
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.push(WIRE_VERSION);
    out.push(kind.as_byte());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Decode a frame off the wire, or refuse in words that name what was wrong.
pub fn decode(wire: &[u8]) -> Result<Frame, TransportError> {
    if wire.len() < HEADER_LEN {
        return Err(TransportError::ShortFrame { got: wire.len() });
    }
    let version = wire[0];
    if version != WIRE_VERSION {
        return Err(TransportError::WireVersion {
            got: version,
            expected: WIRE_VERSION,
        });
    }
    let kind =
        ArtifactKind::from_byte(wire[1]).ok_or(TransportError::UnknownKind { got: wire[1] })?;
    let payload = &wire[HEADER_LEN..];
    if payload.is_empty() {
        return Err(TransportError::EmptyPayload { kind: kind.name() });
    }
    Ok(Frame {
        kind,
        payload: payload.to_vec(),
    })
}

impl ArtifactKind {
    /// The kind's name, for a refusal a person reads.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ArtifactKind::Welcome => "welcome",
            ArtifactKind::Sealed => "sealed message",
        }
    }
}
