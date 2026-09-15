//! The words the arc prints — the deliverable, not a side effect (plan R3,
//! Observability). Each step names its outcome; the silence cases are named
//! OUT LOUD: a successful mint is silent at every layer, and a binary that
//! prints nothing between "camping" and "admitted" rebuilds the ambiguity
//! that made §13 misread its own evidence.

use call_core::camp::Action as CampAction;
use call_core::model::{CampPass, CampRefusal};
use call_transport_iroh::Ending;
use croft_arc::report::{camp_line, camped_line, ending_line, line};

#[test]
fn every_line_leads_with_the_endpoints_short_id() {
    // So a run can be laid beside the relay journal, which prints
    // `endpoint_id=<ten hex>`.
    assert_eq!(
        line("d54207da19", "bind", "bound"),
        "[d54207da19] bind: bound"
    );
}

#[test]
fn a_mint_names_its_own_silence() {
    let action = CampAction::Camp {
        auth_token: "SECRET-PASS-BYTES".to_string(),
        pass: CampPass {
            token: "SECRET-PASS-BYTES".to_string(),
            expires_at_millis: 0,
        },
    };
    let words = camp_line(&action);
    assert!(words.contains("minted"), "{words}");
    assert!(
        words.contains("silent") && words.contains("admitted"),
        "the mint's silence and where the proof actually is must both be named: {words}"
    );
    assert!(
        !words.contains("SECRET-PASS-BYTES"),
        "never the token: {words}"
    );
}

#[test]
fn a_refused_mint_carries_call_cores_words() {
    let action = call_core::camp::action(
        call_core::model::CampOutcome::Refused(CampRefusal::EndpointUnbound),
        0,
    );
    let words = camp_line(&action);
    match action {
        CampAction::CampTokenless { note } => {
            let note = note.as_deref().expect("a refusal carries a note");
            assert!(words.contains(note), "{words} ⊉ {note}");
        }
        CampAction::Camp { .. } => panic!("a refusal cannot camp"),
    }
}

#[test]
fn camped_is_true_only_with_a_relay_and_says_unreachable_otherwise() {
    let yes = camped_line(Some("https://relay.croft.ing:8443/"));
    assert!(
        yes.contains("camped on https://relay.croft.ing:8443/"),
        "{yes}"
    );
    let no = camped_line(None);
    assert!(no.contains("NOT camped"), "{no}");
    assert!(
        no.contains("cannot reach"),
        "the consequence is named, as on the phone: {no}"
    );
}

#[test]
fn endings_are_reported_as_observed_never_guessed() {
    assert!(ending_line(&Ending::LocalHangUp).contains("you hung up"));
    let peer = ending_line(&Ending::ClosedByPeer {
        code: 0,
        reason: "hangup".to_string(),
    });
    assert!(
        peer.contains("closed by peer") && peer.contains("hangup") && peer.contains("code 0"),
        "{peer}"
    );
    let lost = ending_line(&Ending::Lost {
        reason: "timed out".to_string(),
    });
    assert!(
        lost.contains("connection lost") && lost.contains("timed out"),
        "{lost}"
    );
}
