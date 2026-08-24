//! O1 (E117 P2/P3): the §9 conformance vectors as CI fixtures — the portable
//! slice first. `conformance-vectors/signing.json` is EMITTED by the discovery
//! corpus's conformance crate against the real reference implementation (its
//! cardinal rule: no cryptographic constant typed by hand), copied here
//! verbatim. This harness verifies the signed-preimage triples through the
//! core's own ed25519 port — accept the good vector, reject the tampered one.
//!
//! The fold-vector categories (§7.3–§7.5) join when the `[gates-release]`
//! encodings pin; the win today is that the harness EXISTS on the day they
//! appear (the vet's O1). Refresh path: re-run `emit-vectors` in
//! `alpha/Proofs/lineage-groups` and re-copy.

use social_tree_core::ports::ed25519::Ed25519Verifier;
use social_tree_core::ports::{DeviceId, Verifier};

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn triple(v: &serde_json::Value) -> ([u8; 32], Vec<u8>, Vec<u8>, String) {
    let key: [u8; 32] = unhex(v["verifying_key_hex"].as_str().expect("key"))
        .try_into()
        .expect("32-byte key");
    let msg = unhex(v["signing_bytes_hex"].as_str().expect("preimage"));
    let sig = unhex(v["signature_hex"].as_str().expect("sig"));
    let expect = v["expect"].as_str().expect("expect").to_string();
    (key, msg, sig, expect)
}

#[test]
fn emitted_signing_vectors_verify_through_the_core_port() {
    let raw = include_str!("conformance-vectors/signing.json");
    let doc: serde_json::Value = serde_json::from_str(raw).expect("valid vector file");
    let mut checked = 0;
    for name in ["good", "tampered"] {
        let (key, msg, sig, expect) = triple(&doc[name]);
        let outcome = Ed25519Verifier.verify(&DeviceId(key), &msg, &sig);
        match expect.as_str() {
            "accept" => assert!(outcome.is_ok(), "{name}: emitted-good vector must verify"),
            _ => assert!(outcome.is_err(), "{name}: tampered vector must be rejected"),
        }
        checked += 1;
    }
    assert_eq!(checked, 2, "both emitted vectors exercised");
}
