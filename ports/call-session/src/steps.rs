//! The session a shell holds, a step at a time: sign in, camp, wait or dial,
//! hang up — and a view of what is known, in the product's words.
//!
//! The headless arc and the macOS shell both drive this. Every line a step
//! reports goes through the caller's `out`, so the binary prints them and the
//! shell keeps them; the [`View`] is what a screen renders, verbatim. The
//! screen-honesty rule that cost two device runs lives here as three facts:
//! a stored token is NOT a signed-in session until the PDS accepts something
//! (E135(b)), a dead session is refused with words and never reads "signed
//! in", and the presence line is the endpoint's own reachability answer,
//! re-asked every time the view is taken (possession is not reachability).

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use call_core::camp::{self, Action as CampAction, Plan as CampPlan};
use call_core::dial::{self, Plan as DialPlan};
use call_core::model::Callee;
use call_transport_iroh::relay::{CROFT_RELAY_QUIC_PORT, CROFT_RELAY_URL};
use call_transport_iroh::{
    BindOptions, Call, CallEndpoint, Discovery, PeerAddr, Rebound, RelayTarget,
};

use crate::admit::{camp_outcome, camp_request, ADMIT_AUD, ADMIT_BASE, ADMIT_CAMP_LXM};
use crate::atproto::{Client, ServiceAuthError};
use crate::records::{
    iso8601_utc, parse_record, reconcile, record_json, EndpointRecord, Reconcile,
    ENDPOINT_COLLECTION,
};
use crate::report::{camp_line, camped_line, line};
use crate::session::{decide, refusal_words, Step, StoredSession};
use crate::state::StateDir;
use crate::Error;

/// How long a dial may take.
const DIAL_PATIENCE: Duration = Duration::from_secs(20);
/// How long the VIEW waits for the endpoint's reachability answer. Prompt
/// when attached; a refused attach is reported as NOT camped after this.
const VIEW_PATIENCE: Duration = Duration::from_millis(300);
/// The short-id column before anything is bound.
const NO_ENDPOINT: &str = "----------";

/// What a session is opened with.
#[derive(Debug, Clone)]
pub struct Options {
    /// Where the key, session and pass live; `None` is the default under
    /// the user's state directory.
    pub state_dir: Option<PathBuf>,
    /// The relay to camp on.
    pub relay: String,
    /// This device's rkey and label.
    pub label: String,
    /// How the endpoint finds peers.
    pub discovery: Discovery,
    /// How long an attach may take before it is called a refusal.
    pub attach_patience: Duration,
}

impl Options {
    /// Production: the croft relay, n0 discovery, twenty seconds of patience.
    #[must_use]
    pub fn production(label: &str) -> Self {
        Options {
            state_dir: None,
            relay: CROFT_RELAY_URL.to_string(),
            label: label.to_string(),
            discovery: Discovery::N0,
            attach_patience: Duration::from_secs(20),
        }
    }
}

/// Whether the relay admitted this endpoint, as the endpoint itself reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presence {
    /// Attached: the relay named.
    Camped {
        /// The relay the endpoint is attached to.
        relay: String,
    },
    /// Not attached within patience: calls cannot reach this endpoint.
    NotCamped,
}

/// What a screen renders. Words only; a shell that composes its own here
/// is a shell that can flatter the record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Whether there is a session at all (stored or proven).
    pub signed_in: bool,
    /// The session line: "not signed in", the stored-but-unproven form, the
    /// proven form, or the refusal.
    pub session: String,
    /// Whether the endpoint reported itself attached to a relay just now.
    pub camped: bool,
    /// The presence line, from the endpoint's own answer.
    pub presence: String,
    /// The bound endpoint id, 64 hex, once camp has bound one.
    pub endpoint_id: Option<String>,
    /// The short form the relay journal prints.
    pub endpoint_id_short: Option<String>,
}

#[derive(Debug)]
enum SessionState {
    None,
    /// On disk; the PDS has not accepted anything this run.
    Stored(StoredSession),
    /// The PDS accepted something this run; `how` says what.
    Proven {
        session: StoredSession,
        how: String,
    },
    /// The PDS refused the refresh; the words say so.
    Dead {
        words: String,
    },
}

impl SessionState {
    fn words(&self) -> String {
        match self {
            SessionState::None => "not signed in".to_string(),
            SessionState::Stored(s) => format!(
                "stored session for {} ({}) — unproven until the PDS accepts it",
                s.handle, s.did
            ),
            SessionState::Proven { session, how } => {
                format!("signed in as {} ({}) — {how}", session.handle, session.did)
            }
            SessionState::Dead { words } => words.clone(),
        }
    }

    fn stored(&self) -> Option<&StoredSession> {
        match self {
            SessionState::Stored(s) | SessionState::Proven { session: s, .. } => Some(s),
            SessionState::None | SessionState::Dead { .. } => None,
        }
    }
}

/// The session: the state directory, the PDS client, what is known about the
/// account, and the endpoint once camp has bound one.
#[derive(Debug)]
pub struct CallSession {
    opts: Options,
    state: StateDir,
    pds: Client,
    session: SessionState,
    ep: Option<CallEndpoint>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_ms() -> i64 {
    i64::try_from(now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(1000)
}

impl CallSession {
    /// Open the state directory and read what it holds. Offline: nothing is
    /// asked of the PDS until a step needs it.
    pub fn open(opts: Options) -> Result<Self, Error> {
        let state = StateDir::open(opts.state_dir.clone())?;
        let pds = Client::new()?;
        let session = match state.session()? {
            Some(s) => SessionState::Stored(s),
            None => SessionState::None,
        };
        Ok(CallSession {
            opts,
            state,
            pds,
            session,
            ep: None,
        })
    }

    /// The screen, now. Re-asks the endpoint whether it is attached.
    #[must_use]
    pub fn view(&self) -> View {
        let attached = self
            .ep
            .as_ref()
            .and_then(|ep| ep.attached_relay(VIEW_PATIENCE));
        View {
            signed_in: self.session.stored().is_some(),
            session: self.session.words(),
            camped: attached.is_some(),
            presence: camped_line(attached.as_deref()),
            endpoint_id: self.ep.as_ref().map(CallEndpoint::endpoint_id),
            endpoint_id_short: self.ep.as_ref().map(CallEndpoint::endpoint_id_short),
        }
    }

    /// The bound endpoint's direct `ip:port` addresses, empty until bound.
    #[must_use]
    pub fn local_addrs(&self) -> Vec<String> {
        self.ep
            .as_ref()
            .map(CallEndpoint::local_addrs)
            .unwrap_or_default()
    }

    /// Sign in with an app password: resolve the handle, make a session on
    /// the account's PDS, store it. The PDS accepting the credentials is
    /// what earns the words "signed in".
    pub fn sign_in(
        &mut self,
        handle: &str,
        app_password: &str,
        out: &mut dyn FnMut(&str),
    ) -> Result<(), Error> {
        let did = self.pds.resolve_handle(handle)?;
        let their_pds = self.pds.resolve_pds(&did)?;
        let session = self.pds.create_session(&their_pds, handle, app_password)?;
        self.state.save_session(&session)?;
        out(&line(
            NO_ENDPOINT,
            "session",
            &format!(
                "signed in as {} ({}) at {} just now",
                session.handle, session.did, session.pds
            ),
        ));
        self.session = SessionState::Proven {
            session,
            how: "the PDS accepted the credentials just now".to_string(),
        };
        Ok(())
    }

    /// Bind (once), publish this device's record when signed in, camp as
    /// `call-core` decides, and report the endpoint's own attach answer.
    /// A refused attach is a [`Presence::NotCamped`], not an error: the
    /// endpoint stays bound so the screen can say so and the person can retry.
    pub fn camp(&mut self, out: &mut dyn FnMut(&str)) -> Result<Presence, Error> {
        let session = self.ensure_session(out)?;
        self.bind(out)?;
        let (short, endpoint_id) = {
            let ep = self.ep.as_ref().ok_or(Error::NotBound)?;
            (ep.endpoint_id_short(), ep.endpoint_id())
        };

        if let Some(session) = &session {
            let wanted = EndpointRecord {
                endpoint_id: endpoint_id.clone(),
                home_relay: self.opts.relay.clone(),
                label: self.opts.label.clone(),
            };
            let existing = self
                .pds
                .get_record(
                    &session.pds,
                    &session.did,
                    ENDPOINT_COLLECTION,
                    &self.opts.label,
                )?
                .as_ref()
                .and_then(parse_record);
            match reconcile(existing.as_ref(), &wanted) {
                Reconcile::Publish => {
                    self.pds.put_record(
                        session,
                        ENDPOINT_COLLECTION,
                        &self.opts.label,
                        record_json(&wanted, &iso8601_utc(now_secs())),
                    )?;
                    out(&line(
                        &short,
                        "record",
                        &format!(
                            "published {ENDPOINT_COLLECTION}/{} naming this endpoint for {}",
                            self.opts.label, session.did
                        ),
                    ));
                }
                Reconcile::Unchanged => out(&line(
                    &short,
                    "record",
                    &format!(
                        "{ENDPOINT_COLLECTION}/{} already names this endpoint; left alone",
                        self.opts.label
                    ),
                )),
            }
        }

        match camp::plan(session.is_some(), self.state.pass()?.as_ref(), now_ms()) {
            CampPlan::UseCached { token } => {
                out(&line(
                    &short,
                    "camp",
                    "reusing the stored pass (inside its margin; the token is the cache)",
                ));
                let ep = self.ep.as_mut().ok_or(Error::NotBound)?;
                bind_token(ep, &token, &short, out)?;
            }
            CampPlan::Mint => {
                let jwt = self.service_auth_with_refresh(out)?;
                let (status, body) = self
                    .pds
                    .camp_token(ADMIT_BASE, &camp_request(&endpoint_id, &jwt))?;
                let action = camp::action(camp_outcome(status, &body), now_ms());
                out(&line(&short, "camp", &camp_line(&action)));
                if let CampAction::Camp { auth_token, pass } = action {
                    self.state.save_pass(&pass)?;
                    let ep = self.ep.as_mut().ok_or(Error::NotBound)?;
                    bind_token(ep, &auth_token, &short, out)?;
                }
            }
            CampPlan::CampTokenless { note } => out(&line(
                &short,
                "camp",
                &format!(
                    "camping tokenless — {}",
                    note.as_deref().unwrap_or("not signed in")
                ),
            )),
        }

        let ep = self.ep.as_ref().ok_or(Error::NotBound)?;
        let attached = ep.attached_relay(self.opts.attach_patience);
        out(&line(&short, "attach", &camped_line(attached.as_deref())));
        Ok(attached.map_or(Presence::NotCamped, |relay| Presence::Camped { relay }))
    }

    /// Wait up to `patience` to be dialled. `None` means nobody did.
    pub fn wait_for_call(
        &mut self,
        patience: Duration,
        out: &mut dyn FnMut(&str),
    ) -> Result<Option<Call>, Error> {
        let ep = self.ep.as_ref().ok_or(Error::NotBound)?;
        let short = ep.endpoint_id_short();
        match ep.accept_next(patience)? {
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
                Ok(Some(call))
            }
            None => {
                out(&line(
                    &short,
                    "wait",
                    &format!("no call within {}s", patience.as_secs()),
                ));
                Ok(None)
            }
        }
    }

    /// Dial `who`'s device `device`: resolve the handle or DID, read their
    /// endpoint records, and dial the one named.
    pub fn dial(
        &mut self,
        who: &str,
        device: &str,
        out: &mut dyn FnMut(&str),
    ) -> Result<Call, Error> {
        let ep = self.ep.as_ref().ok_or(Error::NotBound)?;
        let short = ep.endpoint_id_short();
        let did = if who.starts_with("did:") {
            who.to_string()
        } else {
            self.pds.resolve_handle(who)?
        };
        let their_pds = self.pds.resolve_pds(&did)?;
        let devices = self.pds.list_endpoints(&their_pds, &did)?;
        let (_, record) = devices
            .iter()
            .find(|(rkey, r)| rkey == device || r.label == device)
            .ok_or_else(|| Error::NoSuchDevice {
                who: who.to_string(),
                device: device.to_string(),
            })?;
        out(&line(
            &short,
            "resolve",
            &format!(
                "{who} is {did}; device {device} is endpoint {} on {}",
                &record.endpoint_id[..10.min(record.endpoint_id.len())],
                if record.home_relay.is_empty() {
                    "discovery only"
                } else {
                    &record.home_relay
                }
            ),
        ));
        let peer = PeerAddr {
            endpoint_id: record.endpoint_id.clone(),
            relay_url: (!record.home_relay.is_empty()).then(|| record.home_relay.clone()),
            addrs: vec![],
        };
        self.dial_peer(&peer, Some(did), out)
    }

    /// Dial a peer whose address is already known — the resolved-record
    /// form, or a card carried by hand.
    pub fn dial_endpoint(
        &mut self,
        peer: &PeerAddr,
        out: &mut dyn FnMut(&str),
    ) -> Result<Call, Error> {
        self.dial_peer(peer, None, out)
    }

    fn dial_peer(
        &mut self,
        peer: &PeerAddr,
        did: Option<String>,
        out: &mut dyn FnMut(&str),
    ) -> Result<Call, Error> {
        let ep = self.ep.as_mut().ok_or(Error::NotBound)?;
        let short = ep.endpoint_id_short();
        // The decision is call-core's: no grant, so a tokenless dial.
        let plan = dial::plan(
            &Callee {
                did,
                grant: None,
                ticket_secret: None,
            },
            self.session.stored().is_some(),
        );
        let (want, note) = match plan {
            DialPlan::DialTokenless { note } => (None, note),
            DialPlan::Mint { .. } => (
                None,
                Some(
                    "a grant was offered but this session mints no dial token yet; dialling tokenless"
                        .to_string(),
                ),
            ),
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
        let call = ep.dial(peer, &self.opts.label, DIAL_PATIENCE)?;
        out(&line(
            &short,
            "call",
            &format!(
                "connected to {} (hello {:?})",
                &peer.endpoint_id[..10.min(peer.endpoint_id.len())],
                call.peer_hello().unwrap_or("none")
            ),
        ));
        Ok(call)
    }

    /// Close the endpoint, if one is bound.
    pub fn close(mut self) {
        if let Some(ep) = self.ep.take() {
            ep.shutdown();
        }
    }

    fn bind(&mut self, out: &mut dyn FnMut(&str)) -> Result<(), Error> {
        if self.ep.is_none() {
            let secret = self.state.secret_key()?;
            let relay = RelayTarget::new(&self.opts.relay).with_quic_port(CROFT_RELAY_QUIC_PORT);
            let ep = CallEndpoint::bind(BindOptions {
                secret_key: secret,
                relay,
                token: None,
                discovery: self.opts.discovery,
            })?;
            out(&line(
                &ep.endpoint_id_short(),
                "bind",
                &format!(
                    "endpoint {} on {} (tokenless until the pass is minted)",
                    ep.endpoint_id(),
                    self.opts.relay
                ),
            ));
            self.ep = Some(ep);
        }
        Ok(())
    }

    /// What is stored, made current: nothing, a session fresh enough to
    /// present, or one refreshed just now. A refused refresh is the
    /// [`Error::SessionDead`] refusal and the state remembers it.
    fn ensure_session(
        &mut self,
        out: &mut dyn FnMut(&str),
    ) -> Result<Option<StoredSession>, Error> {
        let stored = match &self.session {
            SessionState::None => return Ok(None),
            SessionState::Dead { words } => return Err(Error::SessionDead(words.clone())),
            SessionState::Stored(s) | SessionState::Proven { session: s, .. } => s.clone(),
        };
        match decide(Some(&stored), now_secs()) {
            Step::SignIn => Ok(None),
            Step::Use => {
                if matches!(self.session, SessionState::Stored(_)) {
                    out(&line(
                        NO_ENDPOINT,
                        "session",
                        &format!(
                            "stored access token for {} ({}) — unproven until the mint presents it",
                            stored.handle, stored.did
                        ),
                    ));
                }
                Ok(Some(stored))
            }
            Step::Refresh => self.refresh(&stored, out).map(Some),
        }
    }

    fn refresh(
        &mut self,
        s: &StoredSession,
        out: &mut dyn FnMut(&str),
    ) -> Result<StoredSession, Error> {
        match self.pds.refresh_session(s) {
            Ok(fresh) => {
                self.state.save_session(&fresh)?;
                out(&line(
                    NO_ENDPOINT,
                    "session",
                    &format!(
                        "refreshed for {} ({}) — the PDS accepted the refresh token just now",
                        fresh.handle, fresh.did
                    ),
                ));
                self.session = SessionState::Proven {
                    session: fresh.clone(),
                    how: "the PDS accepted the refresh token just now".to_string(),
                };
                Ok(fresh)
            }
            Err(failure) => {
                let words = refusal_words(&failure);
                out(&line(NO_ENDPOINT, "session", &words));
                self.session = SessionState::Dead {
                    words: words.clone(),
                };
                Err(Error::SessionDead(words))
            }
        }
    }

    /// `getServiceAuth`, refreshing once if the PDS says the access token has
    /// expired — the stored expiry is a schedule, the PDS's word is the fact.
    fn service_auth_with_refresh(&mut self, out: &mut dyn FnMut(&str)) -> Result<String, Error> {
        let session = self.session.stored().cloned().ok_or(Error::NotSignedIn)?;
        match self.pds.service_auth(&session, ADMIT_AUD, ADMIT_CAMP_LXM) {
            Ok(jwt) => {
                out(&line(
                    NO_ENDPOINT,
                    "session",
                    &format!(
                        "proven: the PDS minted a service-auth proof for {}",
                        session.did
                    ),
                ));
                self.session = SessionState::Proven {
                    session,
                    how: "the PDS minted a service-auth proof just now".to_string(),
                };
                Ok(jwt)
            }
            Err(ServiceAuthError::Expired) => {
                out(&line(
                    NO_ENDPOINT,
                    "session",
                    "the PDS says the access token is expired; refreshing",
                ));
                let fresh = self.refresh(&session, out)?;
                match self.pds.service_auth(&fresh, ADMIT_AUD, ADMIT_CAMP_LXM) {
                    Ok(jwt) => Ok(jwt),
                    Err(ServiceAuthError::Expired) => {
                        let words = "the PDS refreshed the session and still calls the access token expired".to_string();
                        self.session = SessionState::Dead {
                            words: words.clone(),
                        };
                        Err(Error::SessionDead(words))
                    }
                    Err(ServiceAuthError::Other(e)) => Err(e),
                }
            }
            Err(ServiceAuthError::Other(e)) => Err(e),
        }
    }
}

fn bind_token(
    ep: &mut CallEndpoint,
    token: &str,
    short: &str,
    out: &mut dyn FnMut(&str),
) -> Result<(), Error> {
    match ep.rebind(Some(token))? {
        Rebound::Swapped => out(&line(short, "rebind", "re-attaching with the pass")),
        Rebound::Kept => out(&line(short, "rebind", "the pass is already bound")),
    }
    Ok(())
}
