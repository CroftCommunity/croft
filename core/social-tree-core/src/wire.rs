//! Wire codecs and the version register. One decoder for the canonical envelope
//! encoding — the experiment corpus carried three copies and one of them
//! silently mis-parsed v2; this is the only one now. Every serialized artifact
//! opens with a version byte and every decoder refuses unknown versions loudly
//! (O2; the register is ../WIRE-REGISTER.md).

use crate::model::*;

fn decode_envelope_from_canonical(raw: &[u8]) -> Result<AssertionEnvelope, String> {
    // Layout (from canonical_bytes_with_sig), envelope wire v2 — no wall-clock field:
    // version(1) + assertion_type(2) + author_device(32) + author_principal(32)
    // + group(32) + antecedents_count(4) + antecedents*(32) + lamport(8)
    // + payload_len(4) + payload + sig_len(4) + sig
    if raw.len() < 1 + 2 + 32 + 32 + 32 + 4 + 8 + 4 {
        return Err(format!("envelope too short: {} bytes", raw.len()));
    }
    let mut off = 0;
    let version = raw[off];
    if version != crate::model::ENVELOPE_WIRE_VERSION {
        return Err(format!(
            "unknown envelope wire version 0x{version:02x} (this build reads 0x{:02x}); \
             stale stores must be rebuilt, never reinterpreted",
            crate::model::ENVELOPE_WIRE_VERSION
        ));
    }
    off += 1;
    let at_u16 = u16::from_be_bytes(raw[off..off + 2].try_into().unwrap());
    off += 2;
    let assertion_type = crate::model::AssertionType::from_u16(at_u16)
        .ok_or_else(|| format!("unknown assertion type 0x{:04x}", at_u16))?;
    let mut dev = [0u8; 32];
    dev.copy_from_slice(&raw[off..off + 32]);
    off += 32;
    let mut prin = [0u8; 32];
    prin.copy_from_slice(&raw[off..off + 32]);
    off += 32;
    let mut grp = [0u8; 32];
    grp.copy_from_slice(&raw[off..off + 32]);
    off += 32;
    let ant_count = u32::from_be_bytes(raw[off..off + 4].try_into().unwrap()) as usize;
    off += 4;
    let mut antecedents = Vec::with_capacity(ant_count);
    for _ in 0..ant_count {
        if raw.len() < off + 32 {
            return Err("antecedents truncated".to_string());
        }
        let mut h = [0u8; 32];
        h.copy_from_slice(&raw[off..off + 32]);
        off += 32;
        antecedents.push(Hash::new(h));
    }
    if raw.len() < off + 8 + 4 {
        return Err("envelope truncated before lamport".to_string());
    }
    let lamport = u64::from_be_bytes(raw[off..off + 8].try_into().unwrap());
    off += 8;
    let payload_len = u32::from_be_bytes(raw[off..off + 4].try_into().unwrap()) as usize;
    off += 4;
    if raw.len() < off + payload_len + 4 {
        return Err("payload/sig truncated".to_string());
    }
    let payload = raw[off..off + payload_len].to_vec();
    off += payload_len;
    let sig_len = u32::from_be_bytes(raw[off..off + 4].try_into().unwrap()) as usize;
    off += 4;
    if raw.len() < off + sig_len {
        return Err("signature truncated".to_string());
    }
    let signature = raw[off..off + sig_len].to_vec();

    Ok(AssertionEnvelope {
        version,
        assertion_type,
        author_device: DeviceId::new(dev),
        author_principal: PrincipalId::new(prin),
        group: GroupId::new(grp),
        antecedents,
        lamport,
        payload,
        signature,
    })
}
