//! The session decisions, pure: when to sign in, use, or refresh — and what
//! a dead session is allowed to say.
//!
//! The §15.2 case (runbook): both phones came up `Signed in` over `NOT camped`
//! with `invalid_grant` after six days idle. A stored session is not a
//! session. This binary must never print anything resembling "signed in"
//! from a stored token alone, and a refresh the PDS refuses is a REFUSAL with
//! words, not a retry.

use croft_arc::session::{
    access_expiry_secs, decide, refusal_words, RefreshFailure, Step, StoredSession,
    REFRESH_MARGIN_SECS,
};

fn stored(expires_at: u64) -> StoredSession {
    StoredSession {
        did: "did:plc:abc".to_string(),
        handle: "alice.example".to_string(),
        pds: "https://pds.example".to_string(),
        access_jwt: "access".to_string(),
        refresh_jwt: "refresh".to_string(),
        access_expires_at_secs: expires_at,
    }
}

#[test]
fn nothing_stored_means_sign_in() {
    assert_eq!(decide(None, 1_000), Step::SignIn);
}

#[test]
fn a_fresh_access_token_is_used_as_is() {
    let s = stored(1_000 + REFRESH_MARGIN_SECS + 1);
    assert_eq!(decide(Some(&s), 1_000), Step::Use);
}

#[test]
fn an_access_token_inside_the_margin_is_refreshed_first() {
    // Exactly at the margin still refreshes: a token that dies mid-mint helps
    // nobody (the Kotlin's EXPIRY_MARGIN_MS, same reasoning).
    assert_eq!(
        decide(Some(&stored(1_000 + REFRESH_MARGIN_SECS)), 1_000),
        Step::Refresh
    );
    assert_eq!(decide(Some(&stored(1_000)), 1_000), Step::Refresh);
    assert_eq!(decide(Some(&stored(10)), 1_000), Step::Refresh);
}

#[test]
fn the_access_expiry_is_read_from_the_jwt_without_trusting_it() {
    // Only `exp` is read, and only to schedule a refresh; nothing is verified
    // and nothing else is believed. Claims seen live 2026-09-14:
    // aud, exp, iat, scope, sub.
    let payload = r#"{"aud":"did:web:pds","exp":1789000000,"iat":1788990000,"scope":"com.atproto.appPass","sub":"did:plc:abc"}"#;
    let b64 = data_encoding::BASE64URL_NOPAD.encode(payload.as_bytes());
    let jwt = format!("eyJ0eXAiOiJhdCtqd3QiLCJhbGciOiJFUzI1NksifQ.{b64}.sig");
    assert_eq!(access_expiry_secs(&jwt), Some(1_789_000_000));
    assert_eq!(access_expiry_secs("not.a.jwt"), None);
    assert_eq!(access_expiry_secs("nodots"), None);
}

#[test]
fn a_dead_session_is_refused_with_words_that_say_dead_and_never_signed_in() {
    let words = refusal_words(&RefreshFailure::Dead {
        error: "invalid_grant".to_string(),
    });
    assert!(words.contains("invalid_grant"));
    assert!(words.to_lowercase().contains("sign in again"));
    assert!(
        !words.to_lowercase().contains("signed in"),
        "a dead session must not be described as signed in: {words}"
    );
}

#[test]
fn an_unreachable_pds_is_not_a_dead_session() {
    // Unavailable is not an authorization answer (the admit's own rule).
    let words = refusal_words(&RefreshFailure::Unavailable {
        reason: "connection refused".to_string(),
    });
    assert!(words.contains("connection refused"));
    assert!(
        !words.to_lowercase().contains("dead"),
        "an outage must not be reported as a dead session: {words}"
    );
}
