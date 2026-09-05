//! A group's record, offered to a joining device — two ways.
//!
//! # Why this exists
//!
//! An MLS Welcome hands a device the key to the lockbox. It does not hand it
//! the record: the joiner does not know the group exists, who is seated, or
//! that the inviter is entitled to act for the principal their assertions name.
//! Without the record the joiner decrypts every message correctly and folds
//! none of them, and the screen stays empty with nothing logged as wrong.
//!
//! # Why there are two forms
//!
//! Measured: a two-person record is under a kilobyte, an MLS Welcome is 847
//! bytes, and gossip's ceiling on one broadcast is **4096 bytes**. So Inline
//! works now and provably stops working for a group of any size. If the wire
//! only ever admitted "here is the whole thing", the day a record outgrew a
//! broadcast would be a breaking change to every deployed build. Admitting both
//! forms from the start makes that day a new *form* of something already
//! understood.
//!
//! **Elsewhere is decoded and refused, not implemented.** There is no fetcher,
//! deliberately: S2 has nothing that could exercise one, and this plan has
//! already paid once for code that looked done and had never run (the three
//! export-tree methods, written and removed). What the form buys today is an
//! honest sentence — "this invite points somewhere I cannot fetch from" —
//! instead of a parse error that sends the reader hunting for a corrupted
//! frame.
//!
//! # What the joiner does with it
//!
//! Replays the envelopes into its own fold, verifying each. The sender is a
//! **courier, not an authority**: derived state would have to be taken on
//! trust, whole signed envelopes do not.
//!
//! Accepting the record at all is trust on first use, and the scan is not what
//! establishes it. The scan opens a **bounded exchange** — scan, see who this is
//! and what they claim, accept — and the trust judgment is that exchange's
//! output, in a relationship, the way accepting an SSH host key is. The system
//! records that judgment; it never computes it. That is why the session offers
//! `read_record` and `accept_record` as two calls with a person in between,
//! rather than one call that folds on arrival.

use crate::TransportError;

/// The form byte for an inline record.
const FORM_INLINE: u8 = 1;
/// The form byte for a locator.
const FORM_ELSEWHERE: u8 = 2;

/// How a record was offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordOffer {
    /// The envelopes themselves, in replay order.
    Inline(Vec<Vec<u8>>),
    /// Where and how to fetch them. Nothing fetches yet.
    Elsewhere(String),
}

/// Offer a record as the envelopes themselves.
///
/// Order is preserved and load-bearing: a `MembershipAdd` folded before its
/// genesis is refused, so a shuffled record strands the joiner.
pub fn encode_inline(envelopes: &[Vec<u8>]) -> Result<Vec<u8>, TransportError> {
    if envelopes.is_empty() {
        return Err(TransportError::EmptyRecord);
    }
    let mut out = vec![FORM_INLINE];
    let count = u32::try_from(envelopes.len()).map_err(|_| TransportError::EmptyRecord)?;
    out.extend_from_slice(&count.to_be_bytes());
    for env in envelopes {
        let len = u32::try_from(env.len()).map_err(|_| TransportError::EmptyRecord)?;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(env);
    }
    Ok(out)
}

/// Offer a record as a locator.
pub fn encode_elsewhere(locator: &str) -> Result<Vec<u8>, TransportError> {
    if locator.is_empty() {
        return Err(TransportError::EmptyRecord);
    }
    let bytes = locator.as_bytes();
    let len = u16::try_from(bytes.len()).map_err(|_| TransportError::EmptyRecord)?;
    let mut out = vec![FORM_ELSEWHERE];
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(out)
}

/// Read an offer, or refuse in words that name what was wrong.
pub fn decode_offer(body: &[u8]) -> Result<RecordOffer, TransportError> {
    let short = || TransportError::ShortRecord { got: body.len() };

    let form = *body.first().ok_or_else(short)?;
    match form {
        FORM_INLINE => {
            if body.len() < 5 {
                return Err(short());
            }
            let count = u32::from_be_bytes(body[1..5].try_into().map_err(|_| short())?) as usize;
            let mut cursor = 5;
            let mut out = Vec::with_capacity(count.min(64));
            for _ in 0..count {
                if cursor + 4 > body.len() {
                    return Err(short());
                }
                let len =
                    u32::from_be_bytes(body[cursor..cursor + 4].try_into().map_err(|_| short())?)
                        as usize;
                cursor += 4;
                if cursor + len > body.len() {
                    return Err(short());
                }
                out.push(body[cursor..cursor + len].to_vec());
                cursor += len;
            }
            if out.is_empty() {
                return Err(TransportError::EmptyRecord);
            }
            Ok(RecordOffer::Inline(out))
        }
        FORM_ELSEWHERE => {
            if body.len() < 3 {
                return Err(short());
            }
            let len = u16::from_be_bytes(body[1..3].try_into().map_err(|_| short())?) as usize;
            if 3 + len > body.len() {
                return Err(short());
            }
            let locator = std::str::from_utf8(&body[3..3 + len])
                .map_err(|_| TransportError::ShortRecord { got: body.len() })?;
            if locator.is_empty() {
                return Err(TransportError::EmptyRecord);
            }
            Ok(RecordOffer::Elsewhere(locator.to_string()))
        }
        got => Err(TransportError::UnknownRecordForm { got }),
    }
}
