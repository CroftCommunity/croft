//! The words the arc prints. This binary IS the diagnostic surface, so its
//! output is a deliverable (plan R3, Observability): every step with its
//! outcome and the endpoint's short id, and the silence cases named out loud.

use call_core::camp::Action as CampAction;
use call_transport_iroh::Ending;

/// One observation line: `[<short id>] <step>: <what>`.
#[must_use]
pub fn line(endpoint_short: &str, step: &str, what: &str) -> String {
    format!("[{endpoint_short}] {step}: {what}")
}

/// The camp step's outcome. A successful mint is SILENT at every layer —
/// no client log, no admit line at the default filter — so the line says so
/// and says where the proof actually is.
#[must_use]
pub fn camp_line(action: &CampAction) -> String {
    match action {
        CampAction::Camp { pass, .. } => format!(
            "pass minted (expires at unix ms {}) — the mint is silent on both sides; \
             the proof is the relay's `admitted … sponsorship=` line, not this one",
            pass.expires_at_millis
        ),
        CampAction::CampTokenless { note } => format!(
            "camping tokenless — {}",
            note.as_deref()
                .unwrap_or("silently, as a signed-out client would")
        ),
    }
}

/// The attach's outcome, as the phone words it: camped on a relay, or NOT
/// camped with the consequence named.
#[must_use]
pub fn camped_line(attached: Option<&str>) -> String {
    match attached {
        Some(relay) => format!("camped on {relay}"),
        None => "NOT camped on any relay; calls cannot reach this endpoint".to_string(),
    }
}

/// How the call ended, as observed on this side — never a guess.
#[must_use]
pub fn ending_line(ending: &Ending) -> String {
    match ending {
        Ending::LocalHangUp => "call ended: you hung up".to_string(),
        Ending::ClosedByPeer { code, reason } => {
            format!("call ended: closed by peer: {reason} (code {code})")
        }
        Ending::Lost { reason } => format!("call ended: connection lost: {reason}"),
    }
}
