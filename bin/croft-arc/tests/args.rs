//! The command line, parsed into a plan and refused with words.
//!
//! Two roles: `callee` waits to be dialled; `call <who>` dials. Everything
//! else is a flag with a default. Credentials never travel as arguments —
//! they come from the environment, and only when no session is stored.

use std::path::PathBuf;
use std::time::Duration;

use croft_arc::args::{parse, Role};

fn argv(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}

#[test]
fn callee_waits_with_the_defaults() {
    let a = parse(argv("callee")).expect("parses");
    assert_eq!(a.role, Role::Callee);
    assert_eq!(a.wait, Duration::from_secs(60));
    assert_eq!(a.label, "croft-arc");
    assert_eq!(a.relay, "https://relay.croft.ing:8443");
    assert_eq!(a.state_dir, None);
}

#[test]
fn call_names_the_peer_and_the_device_to_dial() {
    let a = parse(argv("call alice.example --device phone --hold 3")).expect("parses");
    assert_eq!(
        a.role,
        Role::Caller {
            callee: "alice.example".to_string(),
            device: "phone".to_string()
        }
    );
    assert_eq!(a.hold, Duration::from_secs(3));
}

#[test]
fn the_device_dialled_defaults_to_the_arcs_own_rkey() {
    // Two laptops running croft-arc find each other by the same slug.
    let a = parse(argv("call did:plc:abc")).expect("parses");
    assert_eq!(
        a.role,
        Role::Caller {
            callee: "did:plc:abc".to_string(),
            device: "croft-arc".to_string()
        }
    );
    assert_eq!(a.hold, Duration::from_secs(5));
}

#[test]
fn state_dir_relay_and_label_are_flags() {
    let a = parse(argv(
        "callee --state-dir /tmp/x --relay https://relay.croft.ing:8444 --label laptop --wait 9",
    ))
    .expect("parses");
    assert_eq!(a.state_dir, Some(PathBuf::from("/tmp/x")));
    assert_eq!(a.relay, "https://relay.croft.ing:8444");
    assert_eq!(a.label, "laptop");
    assert_eq!(a.wait, Duration::from_secs(9));
}

#[test]
fn a_missing_role_an_unknown_flag_and_a_bad_number_are_refused_with_words() {
    for (line, want) in [
        ("", "callee"),
        ("--hold 3", "callee"),
        ("call", "who to call"),
        ("callee --bogus", "--bogus"),
        ("callee --wait soon", "--wait"),
        ("call bob --hold", "--hold"),
    ] {
        let err = parse(argv(line)).expect_err(line);
        assert!(
            err.contains(want),
            "{line:?} → {err:?} should mention {want:?}"
        );
    }
}
