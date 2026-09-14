//! The rebind rows of `docs/ENFORCEMENT-SCENARIOS.md`: a dial never lowers the
//! endpoint's admission (§15.3, R0). Mirrors `RebindPolicyTest.kt`. The
//! interesting case is the one that regressed on a phone: a tokenless want over
//! a live camping pass KEEPS.

use call_core::dial::{self, Rebind};

#[test]
fn a_tokenless_dial_keeps_a_live_camping_pass() {
    // Reachability is never lowered.
    assert_eq!(dial::rebind(Some("camp-pass"), None), Rebind::Keep);
}

#[test]
fn the_same_token_is_not_a_rebind() {
    assert_eq!(
        dial::rebind(Some("camp-pass"), Some("camp-pass")),
        Rebind::Keep
    );
}

#[test]
fn tokenless_while_already_tokenless_is_not_a_rebind() {
    assert_eq!(dial::rebind(None, None), Rebind::Keep);
}

#[test]
fn a_minted_call_token_swaps() {
    // A different admitting token is the M4c path, accepting the camp gap.
    assert_eq!(
        dial::rebind(Some("camp-pass"), Some("call-token")),
        Rebind::Swap {
            token: "call-token".to_string()
        }
    );
}

#[test]
fn an_unbound_endpoint_binds_the_minted_token() {
    assert_eq!(
        dial::rebind(None, Some("call-token")),
        Rebind::Swap {
            token: "call-token".to_string()
        }
    );
}
