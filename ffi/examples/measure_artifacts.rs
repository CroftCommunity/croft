//! How big are the things S2 puts on the wire, actually?
//!
//! gossip's `DEFAULT_MAX_MESSAGE_SIZE` is 4096 bytes, which is a hard ceiling
//! on a single broadcast. Everything S2 sends has to fit under it or the design
//! needs chunking. Measured rather than estimated.
//!
//! Run: `cargo run -p croft-ffi --example measure_artifacts`

use chat_core::model::Intent;
use croft_ffi::session::Session;
use std::path::PathBuf;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "croft-measure-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("store.redb")
}

fn verdict(label: &str, n: usize) {
    let cap = 4096usize;
    let mark = if n <= cap {
        "fits"
    } else {
        "OVER THE 4096 CAP"
    };
    println!("  {label:<38} {n:>6} bytes   {mark}");
}

fn main() {
    println!("gossip DEFAULT_MAX_MESSAGE_SIZE = 4096 bytes\n");

    let mut a = Session::open(&temp("a"), &[1u8; 32]).expect("A opens");
    let mut b = Session::open(&temp("b"), &[2u8; 32]).expect("B opens");

    a.create_group("supper club").expect("A founds a group");

    let kp = b.mls_key_package().expect("B's key package");
    verdict("MLS key package (in pairing code)", kp.len());

    let welcome = a.invite(&kp).expect("A invites B");
    verdict("MLS Welcome (artifact kind 1)", welcome.len());
    b.accept_invite(&welcome).expect("B seated");

    // Select and type a short message, the way the shell does.
    let group = match a.view().tree.rows.first() {
        Some(chat_core::view::TreeRow::Group(g)) => {
            let mut raw = [0u8; 32];
            raw.copy_from_slice(&g.id.as_bytes()[..32]);
            social_tree_core::model::GroupId::new(raw)
        }
        _ => panic!("no group"),
    };
    a.dispatch(Intent::SelectGroup(group)).unwrap();
    for c in "bring bread".chars() {
        a.dispatch(Intent::TypeChar(c)).unwrap();
    }

    let sealed = a.send_sealed().expect("A sends sealed");
    verdict("sealed assertion, 11-char message", sealed.len());

    // A long message, to see how the envelope scales.
    for c in "x".repeat(500).chars() {
        a.dispatch(Intent::TypeChar(c)).unwrap();
    }
    let big = a.send_sealed().expect("A sends a long one");
    verdict("sealed assertion, 500-char message", big.len());

    // The pairing code a person actually carries.
    let code_len = kp.len() * 8 / 5 + 80; // base32 expansion + card + checksum
    println!("\n  pairing code, approx characters       {code_len:>6}");
    println!("  (that is what has to fit in a QR and, at worst, be typed)");

    println!("\nWhat a history transfer would have to carry, per assertion:");
    println!("  one envelope is 115 bytes of header + payload + 64-byte signature");
    println!("  a 2-person group's record is genesis + one MembershipAdd");
}
