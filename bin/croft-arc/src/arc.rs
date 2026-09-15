//! The arc: session → bind → record → camp → (wait | dial) → hang up.
//!
//! The call chain the plan names: `main` → session load →
//! `call_core::camp::plan`/`action` → port `rebind` → `call_core::dial::plan`
//! → port `dial` → relay. Each step prints one line; the silence cases are
//! named; nothing is reported that was not observed.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use call_core::camp::{self, Action as CampAction, Plan as CampPlan};
use call_core::dial::{self, Plan as DialPlan};
use call_core::model::Callee;
use call_transport_iroh::relay::CROFT_RELAY_QUIC_PORT;
use call_transport_iroh::{BindOptions, CallEndpoint, Discovery, PeerAddr, Rebound, RelayTarget};

use crate::admit::{camp_outcome, camp_request, ADMIT_AUD, ADMIT_BASE, ADMIT_CAMP_LXM};
use crate::args::{Args, Role};
use crate::atproto::{Client, ServiceAuthError};
use crate::records::{
    iso8601_utc, reconcile, record_json, EndpointRecord, Reconcile, ENDPOINT_COLLECTION,
};
use crate::report::{camp_line, camped_line, ending_line, line};
use crate::session::{decide, refusal_words, Step, StoredSession};
use crate::state::StateDir;
use crate::ArcError;

/// How long an attach may take before it is called a refusal.
const ATTACH_PATIENCE: Duration = Duration::from_secs(20);
/// How long a dial may take.
const DIAL_PATIENCE: Duration = Duration::from_secs(20);

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Run the whole arc; every line goes through `out`. `Ok(())` means the arc
/// completed; an `Err` carries the words and the exit code.
pub fn run(args: &Args, out: &mut dyn FnMut(&str)) -> Result<(), ArcError> {
    let state = StateDir::open(args.state_dir.clone())?;
    let pds = Client::new()?;

    // ---- session: stored is not proven ---------------------------------
    let mut session = establish_session(&state, &pds, out)?;

    // ---- bind: the persisted key, tokenless until the pass exists -------
    let secret = state.secret_key()?;
    let relay = RelayTarget::new(&args.relay).with_quic_port(CROFT_RELAY_QUIC_PORT);
    let mut ep = CallEndpoint::bind(BindOptions {
        secret_key: secret,
        relay: relay.clone(),
        token: None,
        discovery: Discovery::N0,
    })?;
    let short = ep.endpoint_id_short();
    out(&line(
        &short,
        "bind",
        &format!(
            "endpoint {} on {} (tokenless until the pass is minted)",
            ep.endpoint_id(),
            args.relay
        ),
    ));

    // ---- record: this device, under its own rkey ------------------------
    let wanted = EndpointRecord {
        endpoint_id: ep.endpoint_id(),
        home_relay: args.relay.clone(),
        label: args.label.clone(),
    };
    let existing = pds
        .get_record(&session.pds, &session.did, ENDPOINT_COLLECTION, &args.label)?
        .as_ref()
        .and_then(crate::records::parse_record);
    match reconcile(existing.as_ref(), &wanted) {
        Reconcile::Publish => {
            pds.put_record(
                &session,
                ENDPOINT_COLLECTION,
                &args.label,
                record_json(&wanted, &iso8601_utc(now_secs())),
            )?;
            out(&line(
                &short,
                "record",
                &format!(
                    "published {ENDPOINT_COLLECTION}/{} naming this endpoint for {}",
                    args.label, session.did
                ),
            ));
        }
        Reconcile::Unchanged => out(&line(
            &short,
            "record",
            &format!(
                "{ENDPOINT_COLLECTION}/{} already names this endpoint; left alone",
                args.label
            ),
        )),
    }

    // ---- camp: call-core decides, the arc performs ----------------------
    let now_ms = i64::try_from(now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1000);
    match camp::plan(true, state.pass()?.as_ref(), now_ms) {
        CampPlan::UseCached { token } => {
            out(&line(
                &short,
                "camp",
                "reusing the stored pass (inside its margin; the token is the cache)",
            ));
            bind_token(&mut ep, &token, &short, out)?;
        }
        CampPlan::Mint => {
            let jwt = service_auth_with_refresh(&state, &pds, &mut session, out)?;
            let (status, body) =
                pds.camp_token(ADMIT_BASE, &camp_request(&ep.endpoint_id(), &jwt))?;
            let action = camp::action(camp_outcome(status, &body), now_ms);
            out(&line(&short, "camp", &camp_line(&action)));
            if let CampAction::Camp { auth_token, pass } = action {
                state.save_pass(&pass)?;
                bind_token(&mut ep, &auth_token, &short, out)?;
            }
        }
        CampPlan::CampTokenless { note } => out(&line(
            &short,
            "camp",
            note.as_deref()
                .unwrap_or("camping tokenless: not signed in"),
        )),
    }
    let attached = ep.attached_relay(ATTACH_PATIENCE);
    out(&line(&short, "attach", &camped_line(attached.as_deref())));
    if attached.is_none() {
        ep.shutdown();
        return Err(ArcError::NotCamped {
            relay: args.relay.clone(),
        });
    }

    // ---- the role -------------------------------------------------------
    match &args.role {
        Role::Callee => {
            out(&line(
                &short,
                "wait",
                &format!("waiting for a call for {}s", args.wait.as_secs()),
            ));
            match ep.accept_next(args.wait)? {
                Some(call) => {
                    out(&line(
                        &short,
                        "call",
                        &format!(
                            "incoming from {} (hello {:?}) — connected",
                            &call.peer_endpoint_id()[..10],
                            call.peer_hello().unwrap_or("none")
                        ),
                    ));
                    let ending = call.ended(args.wait);
                    out(&line(
                        &short,
                        "call",
                        &ending.as_ref().map_or_else(
                            || {
                                format!(
                                    "still connected after {}s; leaving it",
                                    args.wait.as_secs()
                                )
                            },
                            ending_line,
                        ),
                    ));
                }
                None => out(&line(
                    &short,
                    "wait",
                    &format!("no call within {}s", args.wait.as_secs()),
                )),
            }
        }
        Role::Caller { callee, device } => {
            let did = if callee.starts_with("did:") {
                callee.clone()
            } else {
                pds.resolve_handle(callee)?
            };
            let their_pds = pds.resolve_pds(&did)?;
            let devices = pds.list_endpoints(&their_pds, &did)?;
            let (_, record) = devices
                .iter()
                .find(|(rkey, r)| rkey == device || r.label == *device)
                .ok_or_else(|| ArcError::NoSuchDevice {
                    who: callee.clone(),
                    device: device.clone(),
                })?;
            out(&line(
                &short,
                "resolve",
                &format!(
                    "{callee} is {did}; device {device} is endpoint {} on {}",
                    &record.endpoint_id[..10.min(record.endpoint_id.len())],
                    if record.home_relay.is_empty() {
                        "discovery only"
                    } else {
                        &record.home_relay
                    }
                ),
            ));
            // The decision is call-core's: no grant, so a tokenless dial.
            let plan = dial::plan(
                &Callee {
                    did: Some(did),
                    grant: None,
                    ticket_secret: None,
                },
                true,
            );
            let (want, note) = match plan {
                DialPlan::DialTokenless { note } => (None, note),
                DialPlan::Mint { .. } => {
                    // Unreachable with `grant: None`; stated rather than hidden.
                    (None, Some("a grant was offered but this instrument mints no dial token yet; dialling tokenless".to_string()))
                }
            };
            if let Some(note) = note {
                out(&line(&short, "dial", &note));
            }
            let rebound = ep.rebind(want)?;
            out(&line(
                &short,
                "rebind",
                match rebound {
                    Rebound::Kept => "kept the camping pass (a dial never lowers admission)",
                    Rebound::Swapped => "swapped the token",
                },
            ));
            let peer = PeerAddr {
                endpoint_id: record.endpoint_id.clone(),
                relay_url: (!record.home_relay.is_empty()).then(|| record.home_relay.clone()),
                addrs: vec![],
            };
            let call = ep.dial(&peer, "croft-arc", DIAL_PATIENCE)?;
            out(&line(
                &short,
                "call",
                &format!(
                    "connected to {} (hello {:?})",
                    &peer.endpoint_id[..10],
                    call.peer_hello().unwrap_or("none")
                ),
            ));
            let still = ep.attached_relay(Duration::from_secs(5));
            out(&line(
                &short,
                "attach",
                &format!("after the dial: {}", camped_line(still.as_deref())),
            ));
            std::thread::sleep(args.hold);
            call.hang_up();
            let ending = call.ended(Duration::from_secs(10));
            out(&line(
                &short,
                "call",
                &ending.as_ref().map_or_else(
                    || "hang-up sent; the close was not observed within 10s".to_string(),
                    ending_line,
                ),
            ));
        }
    }
    ep.shutdown();
    out(&line(&short, "done", "endpoint closed"));
    Ok(())
}

fn establish_session(
    state: &StateDir,
    pds: &Client,
    out: &mut dyn FnMut(&str),
) -> Result<StoredSession, ArcError> {
    let stored = state.session()?;
    match decide(stored.as_ref(), now_secs()) {
        Step::SignIn => {
            let handle = std::env::var("CROFT_ARC_HANDLE")
                .map_err(|_| ArcError::NoCredentials("CROFT_ARC_HANDLE"))?;
            let password = std::env::var("CROFT_ARC_APP_PASSWORD")
                .map_err(|_| ArcError::NoCredentials("CROFT_ARC_APP_PASSWORD"))?;
            let did = pds.resolve_handle(&handle)?;
            let their_pds = pds.resolve_pds(&did)?;
            let session = pds.create_session(&their_pds, &handle, &password)?;
            state.save_session(&session)?;
            out(&line(
                "----------",
                "session",
                &format!(
                    "signed in as {} ({}) at {} just now",
                    session.handle, session.did, session.pds
                ),
            ));
            Ok(session)
        }
        Step::Use => {
            let s = stored.expect("Use implies stored");
            out(&line(
                "----------",
                "session",
                &format!(
                    "stored access token for {} ({}) — unproven until the mint presents it",
                    s.handle, s.did
                ),
            ));
            Ok(s)
        }
        Step::Refresh => {
            let s = stored.expect("Refresh implies stored");
            refresh(state, pds, &s, out)
        }
    }
}

fn refresh(
    state: &StateDir,
    pds: &Client,
    s: &StoredSession,
    out: &mut dyn FnMut(&str),
) -> Result<StoredSession, ArcError> {
    match pds.refresh_session(s) {
        Ok(fresh) => {
            state.save_session(&fresh)?;
            out(&line(
                "----------",
                "session",
                &format!(
                    "refreshed for {} ({}) — the PDS accepted the refresh token just now",
                    fresh.handle, fresh.did
                ),
            ));
            Ok(fresh)
        }
        Err(failure) => {
            let words = refusal_words(&failure);
            out(&line("----------", "session", &words));
            Err(ArcError::SessionDead(words))
        }
    }
}

/// `getServiceAuth`, refreshing once if the PDS says the access token has
/// expired — the stored expiry is a schedule, the PDS's word is the fact.
fn service_auth_with_refresh(
    state: &StateDir,
    pds: &Client,
    session: &mut StoredSession,
    out: &mut dyn FnMut(&str),
) -> Result<String, ArcError> {
    match pds.service_auth(session, ADMIT_AUD, ADMIT_CAMP_LXM) {
        Ok(jwt) => {
            out(&line(
                "----------",
                "session",
                &format!(
                    "proven: the PDS minted a service-auth proof for {}",
                    session.did
                ),
            ));
            Ok(jwt)
        }
        Err(ServiceAuthError::Expired) => {
            out(&line(
                "----------",
                "session",
                "the PDS says the access token is expired; refreshing",
            ));
            *session = refresh(state, pds, session, out)?;
            match pds.service_auth(session, ADMIT_AUD, ADMIT_CAMP_LXM) {
                Ok(jwt) => Ok(jwt),
                Err(ServiceAuthError::Expired) => Err(ArcError::SessionDead(
                    "the PDS refreshed the session and still calls the access token expired"
                        .to_string(),
                )),
                Err(ServiceAuthError::Other(e)) => Err(e),
            }
        }
        Err(ServiceAuthError::Other(e)) => Err(e),
    }
}

fn bind_token(
    ep: &mut CallEndpoint,
    token: &str,
    short: &str,
    out: &mut dyn FnMut(&str),
) -> Result<(), ArcError> {
    match ep.rebind(Some(token))? {
        Rebound::Swapped => out(&line(short, "rebind", "re-attaching with the pass")),
        Rebound::Kept => out(&line(short, "rebind", "the pass is already bound")),
    }
    Ok(())
}
