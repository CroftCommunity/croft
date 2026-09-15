//! **The arc, run.** `:live`, against PRODUCTION, asserting on the relay's
//! own journal — the plan's R3 wiring test: "a test that runs the binary and
//! greps the journal for this endpoint's `admitted … sponsorship=`".
//!
//! Two processes of the same binary, two test accounts, no phone: a callee
//! camps and waits; a caller camps, dials the callee's `croft-arc` device,
//! holds, hangs up. Then the production journal must carry `admitted
//! endpoint_id=<short> sponsorship=` for both endpoints and no `no_token`
//! for either after they camped.
//!
//! # Why `:live` and `#[ignore]`
//!
//! It signs in to two real accounts, publishes each one's `croft-arc` device
//! record, holds real connections to `relay.croft.ing:8443`, and reads a
//! journal over ssh. Per `VERIFICATION.md` the suffix is the recorded reason
//! it is not on the gate:
//!
//! ```text
//! cargo test -p croft-arc --test live_arc -- --ignored --nocapture
//! ```
//!
//! # Inputs (environment)
//!
//! - `CROFT_ARC_CALLEE_HANDLE`, `CROFT_ARC_CALLEE_PASSWORD` and
//!   `CROFT_ARC_CALLER_HANDLE`, `CROFT_ARC_CALLER_PASSWORD` — the two test
//!   accounts (TESTBED § Accounts; values live in `CroftC/.env`, never here).
//! - `CROFT_LIVE_JOURNAL` — default
//!   `ssh croft-vps sudo -n journalctl -u iroh-relay -o cat --no-pager --since`.
//!
//! Shared-state contract (plan R3): read-only against the relay; each account
//! gains or keeps ONE record under rkey `croft-arc` and nothing else of its
//! repo is touched. Claim `testbed--relay-live` first.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const JOURNAL: &str = "ssh croft-vps sudo -n journalctl -u iroh-relay -o cat --no-pager --since";

fn env_required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set — see this file's header"))
}

fn journal(since_unix: u64) -> Vec<String> {
    let cmd = std::env::var("CROFT_LIVE_JOURNAL").unwrap_or_else(|_| JOURNAL.to_string());
    let mut parts = cmd.split_whitespace();
    let program = parts.next().expect("a journal command");
    let out = Command::new(program)
        .args(parts)
        .arg(format!("@{since_unix}"))
        .output()
        .unwrap_or_else(|e| panic!("could not run `{cmd}`: {e}"));
    assert!(
        out.status.success(),
        "journal: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// A running `croft-arc`, its stdout streamed line by line.
struct Arc {
    child: std::process::Child,
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
}

impl Arc {
    fn spawn(role_args: &[&str], handle: &str, password: &str, state_dir: &std::path::Path) -> Arc {
        let mut child = Command::new(env!("CARGO_BIN_EXE_croft-arc"))
            .args(role_args)
            .arg("--state-dir")
            .arg(state_dir)
            .env("CROFT_ARC_HANDLE", handle)
            .env("CROFT_ARC_APP_PASSWORD", password)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("croft-arc spawns");
        let stdout = child.stdout.take().expect("piped");
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                eprintln!("  > {line}");
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Arc {
            child,
            lines,
            seen: Vec::new(),
        }
    }

    /// Wait until a line containing `needle` appears, or fail with everything seen.
    fn wait_for(&mut self, needle: &str, patience: Duration) -> String {
        let deadline = Instant::now() + patience;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    self.seen.push(line.clone());
                    if line.contains(needle) {
                        return line;
                    }
                }
                Err(_) => panic!(
                    "no line containing {needle:?} within {patience:?}; seen:\n{}",
                    self.seen.join("\n")
                ),
            }
        }
    }

    fn finish(mut self, patience: Duration) -> (i32, Vec<String>) {
        let deadline = Instant::now() + patience;
        let status = loop {
            if let Some(s) = self.child.try_wait().expect("wait") {
                break s;
            }
            assert!(
                Instant::now() < deadline,
                "croft-arc did not exit within {patience:?}"
            );
            std::thread::sleep(Duration::from_millis(100));
        };
        while let Ok(line) = self.lines.try_recv() {
            self.seen.push(line);
        }
        (status.code().unwrap_or(-1), std::mem::take(&mut self.seen))
    }
}

impl Drop for Arc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The ten-hex short id from a `[<short>] …` line.
fn short_id(line: &str) -> String {
    line.trim_start_matches('[').chars().take(10).collect()
}

#[test]
#[ignore = "live: signs in to two test accounts, camps on relay.croft.ing:8443 and reads its journal over ssh — run deliberately with --ignored"]
fn one_command_per_side_walks_the_arc_and_the_relay_journal_says_so() {
    let callee_handle = env_required("CROFT_ARC_CALLEE_HANDLE");
    let callee_password = env_required("CROFT_ARC_CALLEE_PASSWORD");
    let caller_handle = env_required("CROFT_ARC_CALLER_HANDLE");
    let caller_password = env_required("CROFT_ARC_CALLER_PASSWORD");
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("after 1970")
        .as_secs()
        - 30;

    let callee_state = tempfile::tempdir().expect("tmp");
    let caller_state = tempfile::tempdir().expect("tmp");

    // ---- the callee: sign in, camp, wait ---------------------------------
    let mut callee = Arc::spawn(
        &["callee", "--wait", "120"],
        &callee_handle,
        &callee_password,
        callee_state.path(),
    );
    let camped = callee.wait_for("camped on", Duration::from_secs(90));
    let callee_id = short_id(&camped);
    callee.wait_for("waiting for a call", Duration::from_secs(10));

    // ---- the caller: sign in, camp, dial the callee's croft-arc device ----
    let mut caller = Arc::spawn(
        &[
            "call",
            &callee_handle,
            "--device",
            "croft-arc",
            "--hold",
            "3",
        ],
        &caller_handle,
        &caller_password,
        caller_state.path(),
    );
    let camped = caller.wait_for("camped on", Duration::from_secs(90));
    let caller_id = short_id(&camped);
    caller.wait_for("connected", Duration::from_secs(60));
    let (caller_exit, caller_lines) = caller.finish(Duration::from_secs(60));
    assert_eq!(
        caller_exit,
        0,
        "the caller's arc must complete:\n{}",
        caller_lines.join("\n")
    );
    assert!(
        caller_lines.iter().any(|l| l.contains("you hung up")),
        "the caller must report its own hang-up:\n{}",
        caller_lines.join("\n")
    );

    callee.wait_for("call ended", Duration::from_secs(30));
    let (callee_exit, callee_lines) = callee.finish(Duration::from_secs(30));
    assert_eq!(
        callee_exit,
        0,
        "the callee's arc must complete:\n{}",
        callee_lines.join("\n")
    );

    // ---- leave both repos as found ----------------------------------------
    // The rig's state dirs are temporary, so the record each run published
    // names a key that will never bind again; a real croft-arc keeps its key
    // and its record. The test binary is the arc's own library, so the
    // sessions it just made are the ones that delete.
    for dir in [callee_state.path(), caller_state.path()] {
        let state = call_session::state::StateDir::open(Some(dir.to_path_buf())).expect("state");
        let session = state
            .session()
            .expect("readable")
            .expect("the arc stored a session");
        let pds = call_session::atproto::Client::new().expect("client");
        pds.delete_record(
            &session,
            call_session::records::ENDPOINT_COLLECTION,
            "croft-arc",
        )
        .expect("the rig's record deletes");
        eprintln!("  cleaned: {}'s croft-arc record deleted", session.handle);
    }

    // ---- the relay's own word ----------------------------------------------
    std::thread::sleep(Duration::from_secs(3));
    let lines = journal(since);
    for id in [&callee_id, &caller_id] {
        let mine: Vec<&String> = lines.iter().filter(|l| l.contains(id)).collect();
        assert!(
            mine.iter()
                .any(|l| l.contains("admitted") && l.contains("sponsorship=")),
            "the production journal must carry `admitted … sponsorship=` for {id}; it carries:\n{}",
            mine.iter()
                .map(|l| l.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
        let admitted_at = mine
            .iter()
            .position(|l| l.contains("admitted"))
            .expect("just asserted");
        let refused_after: Vec<&&String> = mine[admitted_at..]
            .iter()
            .filter(|l| l.contains("no_token") || l.contains("denied"))
            .collect();
        assert!(
            refused_after.is_empty(),
            "{id} was refused after it camped — §15's shape:\n{}",
            refused_after
                .iter()
                .map(|l| l.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
