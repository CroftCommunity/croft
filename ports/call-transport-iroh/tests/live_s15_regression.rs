//! **The §15 regression, as a test** — `:live`, against the staging ENFORCE
//! listener, asserting on the relay's own journal.
//!
//! What §15 measured on a phone (croft `ops/RUNBOOK-two-device-call-test.md`
//! §15.3): the tokenless dial path rebound the endpoint to no token over a
//! live camping pass; the re-attach was refused ~20 times with `no_token`; the
//! phone stayed unreachable for four minutes. §16 verified the fix on a phone
//! by the same instrument — the relay journal carried ONE line from the tap
//! through the call and the hang-up. This file is that condition, runnable
//! from a laptop with no phone: *camp, dial tokenless, and the camp must hold,
//! with no `no_token` for the caller in the journal after the dial.*
//!
//! RED against a port that rebinds unconditionally (the pre-§15.3 Kotlin
//! shape); GREEN against one that applies `call_core::dial::rebind` at the
//! endpoint. Both runs are recorded in the child plan's Review Log.
//!
//! # Why `:live` and `#[ignore]`
//!
//! It holds a real connection to `relay.croft.ing:8444` and reads a journal
//! over ssh. Per `VERIFICATION.md`, a `:live` suffix is the recorded reason a
//! check is not on the gate. Run it deliberately, never in CI:
//!
//! ```text
//! cargo test -p call-transport-iroh --test live_s15_regression -- --ignored --nocapture
//! ```
//!
//! # Inputs (environment; see `live/README.md` for the mint recipe)
//!
//! - `CROFT_LIVE_CALLEE_SECRET_HEX`, `CROFT_LIVE_CALLER_SECRET_HEX` — the two
//!   rig endpoints' iroh secret keys (32 bytes hex). Rig material, never an
//!   identity.
//! - `CROFT_LIVE_CALLEE_PASS`, `CROFT_LIVE_CALLER_PASS` — camping passes minted
//!   for exactly those endpoint ids by a local `croft-relay-admit` signing
//!   with the STAGING mint key (`CROFT_STAGING_MINT_KEY`, `CroftC/.env`).
//! - `CROFT_LIVE_RELAY_URL` — default `https://relay.croft.ing:8444`.
//! - `CROFT_LIVE_JOURNAL` — a command that prints the relay's journal lines
//!   since a unix timestamp appended as its last argument; default
//!   `ssh croft-vps sudo -n journalctl -u croft-relay-staging -o cat --no-pager --since`.
//!
//! Shared-state contract (plan R2): read-only against the relay — attach,
//! camp, dial, hang up. Claim `testbed--relay-live` first.

use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use call_transport_iroh::{BindOptions, CallEndpoint, Discovery, PeerAddr, Rebound, RelayTarget};

const STAGING_RELAY: &str = "https://relay.croft.ing:8444";
// `sudo -n`: the box's login user is not in systemd-journal, and journalctl
// then prints "No entries" over a hint instead of failing — an empty set that
// grades green is exactly the shape VERIFICATION.md names.
const JOURNAL: &str =
    "ssh croft-vps sudo -n journalctl -u croft-relay-staging -o cat --no-pager --since";

/// Long enough that a slow-but-succeeding attach is not called a refusal
/// (the Kotlin's ONLINE_PROBE_TIMEOUT_MS is 6 s; TLS to a VPS is slower from
/// a laptop than from a phone on the same continent).
const ATTACH_PATIENCE: Duration = Duration::from_secs(20);
const CALL_PATIENCE: Duration = Duration::from_secs(20);

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn env_required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!("{name} is not set — the live rig needs it; the recipe is in live/README.md")
    })
}

fn secret(name: &str) -> [u8; 32] {
    let hex = env_required(name);
    let bytes = data_encoding::HEXLOWER
        .decode(hex.trim().as_bytes())
        .unwrap_or_else(|e| panic!("{name} is not lowercase hex: {e}"));
    bytes
        .as_slice()
        .try_into()
        .unwrap_or_else(|_| panic!("{name} must be 32 bytes, got {}", bytes.len()))
}

/// The relay's journal since `since_unix`, one line per entry.
fn journal(since_unix: u64) -> Vec<String> {
    let cmd = env_or("CROFT_LIVE_JOURNAL", JOURNAL);
    let mut parts = cmd.split_whitespace();
    let program = parts.next().expect("a journal command");
    let out = Command::new(program)
        .args(parts)
        .arg(format!("@{since_unix}"))
        .output()
        .unwrap_or_else(|e| panic!("could not run the journal command `{cmd}`: {e}"));
    assert!(
        out.status.success(),
        "the journal command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs()
}

fn lines_naming<'a>(lines: &'a [String], short_id: &str) -> Vec<&'a String> {
    lines.iter().filter(|l| l.contains(short_id)).collect()
}

fn bind(secret_key: [u8; 32], pass: &str, relay: &str) -> CallEndpoint {
    CallEndpoint::bind(BindOptions {
        secret_key,
        relay: RelayTarget::new(relay)
            .with_quic_port(call_transport_iroh::relay::CROFT_RELAY_QUIC_PORT),
        token: Some(pass.to_string()),
        discovery: Discovery::None,
    })
    .expect("binds")
}

#[test]
#[ignore = "live: holds a connection to relay.croft.ing:8444 and reads its journal over ssh — run deliberately with --ignored"]
fn a_tokenless_dial_over_a_live_camping_pass_keeps_the_camp_and_the_journal_stays_quiet() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();

    let relay = env_or("CROFT_LIVE_RELAY_URL", STAGING_RELAY);
    let callee_key = secret("CROFT_LIVE_CALLEE_SECRET_HEX");
    let caller_key = secret("CROFT_LIVE_CALLER_SECRET_HEX");
    let callee_pass = env_required("CROFT_LIVE_CALLEE_PASS");
    let caller_pass = env_required("CROFT_LIVE_CALLER_PASS");

    // Thirty seconds of slack for clock skew between this laptop and the box.
    let since = now_unix() - 30;

    // ---- camp: both endpoints attach with their passes ------------------
    let callee = bind(callee_key, &callee_pass, &relay);
    let mut caller = bind(caller_key, &caller_pass, &relay);
    let callee_id = callee.endpoint_id_short();
    let caller_id = caller.endpoint_id_short();
    eprintln!("callee {callee_id} caller {caller_id} relay {relay}");

    let callee_home = callee.attached_relay(ATTACH_PATIENCE);
    let caller_home = caller.attached_relay(ATTACH_PATIENCE);
    eprintln!("camped: callee={callee_home:?} caller={caller_home:?}");
    assert!(callee_home.is_some(), "the callee's pass must camp it (is the pass minted for {callee_id}, still in TTL, signed by the staging key?)");
    assert!(caller_home.is_some(), "the caller's pass must camp it (is the pass minted for {caller_id}, still in TTL, signed by the staging key?)");

    // The relay's own word for it, before anything else happens.
    let camped = journal(since);
    for id in [&callee_id, &caller_id] {
        let mine = lines_naming(&camped, id);
        assert!(
            mine.iter()
                .any(|l| l.contains("admitted") && l.contains("sponsorship=")),
            "the journal must carry `admitted … sponsorship=` for {id}; it carries:\n{}",
            mine.iter()
                .map(|l| l.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    let quiet_from = camped.len();

    // ---- the §15 action: a tokenless dial over the live pass -------------
    // `DialAdmission.plan` says DialTokenless for a callee with no grant, and
    // the shell asks the endpoint to bind `null`. The port must KEEP.
    let rebound = caller.rebind(None).expect("a keep cannot fail");
    eprintln!("rebind(None) over the live pass: {rebound:?}");

    // §15's signature, read from the relay before the dial is even placed: a
    // port that tore the endpoint down here re-attaches tokenless and enforce
    // refuses it. Asserted together so a RED run prints the journal that
    // convicts it, not just a Swapped where a Kept was wanted.
    let still_camped = caller.attached_relay(Duration::from_secs(10));
    std::thread::sleep(Duration::from_secs(2));
    let after_rebind = journal(since);
    let refused: Vec<&String> = after_rebind
        .iter()
        .skip(quiet_from)
        .filter(|l| l.contains(&caller_id) && (l.contains("denied") || l.contains("no_token")))
        .collect();
    assert!(
        rebound == Rebound::Kept && still_camped == caller_home && refused.is_empty(),
        "a dial never lowers admission (R0): rebind(None) over a live pass must KEEP the \
         endpoint and its camp. rebind={rebound:?} camped-before={caller_home:?} \
         camped-after={still_camped:?}; the relay journal for {caller_id} since the rebind:\n{}",
        refused
            .iter()
            .map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    let quiet_from = after_rebind.len();

    let accepting = std::thread::spawn(move || {
        let call = callee
            .accept_next(CALL_PATIENCE)
            .expect("accept does not fail")
            .expect("the call arrives");
        (callee, call)
    });
    // Dialled by id + relay, no discovery: the id its key implies, the relay
    // it is camped on. What a phone learns from the endpoint record instead.
    let peer = PeerAddr {
        endpoint_id: CallEndpoint::endpoint_id_for(&callee_key),
        relay_url: Some(relay.clone()),
        addrs: vec![],
    };
    let outgoing = caller
        .dial(&peer, "croftcall-r2-live", CALL_PATIENCE)
        .expect("the dial connects through the relay");
    let (callee, incoming) = accepting.join().expect("accept thread");
    eprintln!(
        "connected: caller sees hello={:?}, callee sees hello={:?}",
        outgoing.peer_hello(),
        incoming.peer_hello()
    );

    // The camp must have HELD through the dial — the same endpoint, still
    // attached — which is what a phone could not do on 2026-09-08.
    assert_eq!(
        caller.attached_relay(Duration::from_secs(5)),
        caller_home,
        "the caller must still be camped on the same relay after dialling"
    );

    // ---- hang up, as §16 did, and read the journal ----------------------
    outgoing.hang_up();
    let ending = incoming.ended(CALL_PATIENCE);
    eprintln!("callee's ending: {ending:?}");
    assert!(ending.is_some(), "the callee must observe the hang-up");

    // Let the relay write whatever it is going to write about the dial.
    std::thread::sleep(Duration::from_secs(3));
    let after = journal(since);
    let during_the_call: Vec<&String> = after.iter().skip(quiet_from).collect();
    eprintln!("journal during the call ({} lines):", during_the_call.len());
    for l in &during_the_call {
        eprintln!("  {l}");
    }
    let offending: Vec<&&String> = during_the_call
        .iter()
        .filter(|l| l.contains(&caller_id) || l.contains(&callee_id))
        .filter(|l| {
            l.contains("denied") || l.contains("no_token") || l.contains("Stream terminated")
        })
        .collect();
    assert!(
        offending.is_empty(),
        "§15's signature appeared in the journal during the dial:\n{}",
        offending
            .iter()
            .map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );

    caller.shutdown();
    callee.shutdown();
}
