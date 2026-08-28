//! The pairing blob: the one artifact in S2 that a person carries.
//!
//! One phone shows this, the other reads it — by camera if the QR works, by
//! typing if it does not. It carries exactly three things: who to dial, where
//! to dial them, and the MLS key package to invite them with.
//!
//! # Why not the calling app's exchange-invite machinery
//!
//! Because that speaks the connect contract, and the social app deliberately
//! does not. Reusing it would make a dev app a contract consumer and put a
//! P7-standing-constraint violation one refactor away. This artifact is small
//! enough to own outright.
//!
//! # Why the encoding is what it is
//!
//! Uppercase base32 (RFC 4648, no padding) so the whole blob sits inside QR
//! **alphanumeric mode** — roughly 40% denser than byte mode for the same
//! payload, which matters because a key package is ~300 bytes and a QR that
//! will not focus is a failed pairing.
//!
//! Every blob carries a 4-byte blake3 checksum over its body. That is not
//! belt-and-braces: base32 has no error detection of its own, and the failure
//! it prevents is specific and nasty — a single mistyped character decodes into
//! a *structurally valid* endpoint id that simply does not exist, and the
//! symptom is "waiting for the other device" forever, on the device that did
//! nothing wrong. A checksum turns that into a refusal at the moment of typing.
//!
//! The key package is carried as opaque bytes. This crate knows nothing about
//! MLS and should not: it is the transport, and the payload is the key layer's
//! business.

use crate::{DialCard, TransportError};

/// The wire version of the pairing blob.
///
/// Separate from the frame's version because the two travel differently: a
/// frame crosses between two running builds, a blob crosses between a screen
/// and a camera. They can and will need to change at different times.
pub const BLOB_VERSION: u8 = 1;

/// How many bytes of blake3 ride along as the checksum.
///
/// Four bytes is ~7 base32 characters and gives a 1-in-4-billion chance that a
/// corruption slips through. Eight would double the cost in QR density for a
/// margin nobody can perceive.
const CHECKSUM_LEN: usize = 4;

/// Everything one device needs to admit another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingBlob {
    /// Who to dial, and where.
    pub card: DialCard,
    /// The MLS key package, opaque to this crate.
    pub key_package: Vec<u8>,
}

/// Render a blob as the text a QR code or a paste buffer carries.
pub fn encode_blob(blob: &PairingBlob) -> Result<String, TransportError> {
    if blob.card.addrs.is_empty() {
        return Err(TransportError::BadDialCard {
            reason: "this device has no address to be dialled at, so pairing would strand the \
                     other phone"
                .to_string(),
        });
    }
    if blob.key_package.is_empty() {
        return Err(TransportError::BadDialCard {
            reason: "there is no key package in this code, so it can invite nobody".to_string(),
        });
    }

    let body = encode_body(blob)?;

    let mut out = Vec::with_capacity(1 + body.len() + CHECKSUM_LEN);
    out.push(BLOB_VERSION);
    out.extend_from_slice(&body);
    out.extend_from_slice(&checksum(&out));

    Ok(data_encoding::BASE32_NOPAD.encode(&out))
}

/// Read a blob back, or refuse in words a person mid-pairing can act on.
pub fn decode_blob(text: &str) -> Result<PairingBlob, TransportError> {
    // A paste buffer adds whitespace and a phone keyboard adds lowercase.
    // Both are the operator being normal, not the operator being wrong.
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect();

    if cleaned.is_empty() {
        return Err(TransportError::BadPairingCode {
            reason: "there is no pairing code here".to_string(),
        });
    }

    let raw = data_encoding::BASE32_NOPAD
        .decode(cleaned.as_bytes())
        .map_err(|_| TransportError::BadPairingCode {
            reason: "this does not look like a pairing code".to_string(),
        })?;

    if raw.len() < 1 + CHECKSUM_LEN {
        return Err(TransportError::BadPairingCode {
            reason: format!("a pairing code is longer than this ({} bytes)", raw.len()),
        });
    }

    let split = raw.len() - CHECKSUM_LEN;
    let (head, tail) = raw.split_at(split);
    if checksum(head) != tail {
        return Err(TransportError::BadPairingCode {
            reason: "this pairing code has a typo in it — check it and try again".to_string(),
        });
    }

    let version = head[0];
    if version != BLOB_VERSION {
        return Err(TransportError::BlobVersion {
            got: version,
            expected: BLOB_VERSION,
        });
    }

    decode_body(&head[1..])
}

/// The body layout, once, so encode and decode cannot drift:
///
/// ```text
/// endpoint_id : 32 bytes
/// n_addrs     : 1 byte
/// for each addr:
///   len       : 1 byte
///   utf8      : len bytes
/// key_package : the rest
/// ```
fn encode_body(blob: &PairingBlob) -> Result<Vec<u8>, TransportError> {
    let id = data_encoding::HEXLOWER
        .decode(blob.card.endpoint_id.as_bytes())
        .map_err(|_| TransportError::BadDialCard {
            reason: format!("endpoint id is not hex: {:?}", blob.card.endpoint_id),
        })?;
    if id.len() != 32 {
        return Err(TransportError::BadDialCard {
            reason: format!("an endpoint id is 32 bytes, got {}", id.len()),
        });
    }

    let n = u8::try_from(blob.card.addrs.len()).map_err(|_| TransportError::BadDialCard {
        reason: format!("too many addresses to pair with: {}", blob.card.addrs.len()),
    })?;

    let mut body = Vec::new();
    body.extend_from_slice(&id);
    body.push(n);
    for addr in &blob.card.addrs {
        let bytes = addr.as_bytes();
        let len = u8::try_from(bytes.len()).map_err(|_| TransportError::BadDialCard {
            reason: format!("an address too long to pair with: {addr:?}"),
        })?;
        body.push(len);
        body.extend_from_slice(bytes);
    }
    body.extend_from_slice(&blob.key_package);
    Ok(body)
}

fn decode_body(body: &[u8]) -> Result<PairingBlob, TransportError> {
    let short = || TransportError::BadPairingCode {
        reason: "this pairing code is cut short".to_string(),
    };

    if body.len() < 33 {
        return Err(short());
    }
    let endpoint_id = data_encoding::HEXLOWER.encode(&body[..32]);
    let n = body[32] as usize;

    let mut cursor = 33;
    let mut addrs = Vec::with_capacity(n);
    for _ in 0..n {
        if cursor >= body.len() {
            return Err(short());
        }
        let len = body[cursor] as usize;
        cursor += 1;
        if cursor + len > body.len() {
            return Err(short());
        }
        let addr = std::str::from_utf8(&body[cursor..cursor + len]).map_err(|_| {
            TransportError::BadPairingCode {
                reason: "an address in this pairing code is not text".to_string(),
            }
        })?;
        addrs.push(addr.to_string());
        cursor += len;
    }

    let key_package = body[cursor..].to_vec();
    if key_package.is_empty() {
        return Err(TransportError::BadPairingCode {
            reason: "this pairing code carries no key package".to_string(),
        });
    }

    Ok(PairingBlob {
        card: DialCard { endpoint_id, addrs },
        key_package,
    })
}

fn checksum(bytes: &[u8]) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"croft/s2/pairing-blob/v1");
    hasher.update(bytes);
    hasher.finalize().as_bytes()[..CHECKSUM_LEN].to_vec()
}

/// Helpers that exist only so the pins can corrupt a blob deliberately.
///
/// Public because integration tests are external crates and cannot reach
/// private items. Kept in one clearly-named module so that "test-only" is
/// visible at the call site rather than inferred.
pub mod testing {
    use super::{checksum, CHECKSUM_LEN};

    /// Re-encode an existing blob with a different version byte, fixing the
    /// checksum so the version is the ONLY thing wrong.
    ///
    /// Without this a version test would corrupt the checksum too, and would
    /// pass for the wrong reason — the refusal would be "typo", not "version".
    #[must_use]
    pub fn reencode_with_version(text: &str, version: u8) -> String {
        let mut raw = data_encoding::BASE32_NOPAD
            .decode(text.as_bytes())
            .expect("the pins only pass well-formed text here");
        let split = raw.len() - CHECKSUM_LEN;
        raw.truncate(split);
        raw[0] = version;
        let sum = checksum(&raw);
        raw.extend_from_slice(&sum);
        data_encoding::BASE32_NOPAD.encode(&raw)
    }
}
