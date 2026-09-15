//! The arc: session → bind → record → camp → (wait | dial) → hang up.
//!
//! Every step is `call_session::steps::CallSession`'s — the same code the
//! macOS shell drives from buttons — so this file is the command line's
//! composition of them: credentials from the environment on the first run,
//! the role, the hold, the exit code. Each step prints one line; the silence
//! cases are named; nothing is reported that was not observed.

use std::time::Duration;

use call_session::report::{ending_line, line};
use call_session::steps::{CallSession, Options, Presence};
use call_session::Error;
use call_transport_iroh::Discovery;

use crate::args::{Args, Role};
use crate::ArcError;

/// How long an attach may take before it is called a refusal.
const ATTACH_PATIENCE: Duration = Duration::from_secs(20);

/// Run the whole arc; every line goes through `out`. `Ok(())` means the arc
/// completed; an `Err` carries the words and the exit code.
pub fn run(args: &Args, out: &mut dyn FnMut(&str)) -> Result<(), ArcError> {
    let mut session = CallSession::open(Options {
        state_dir: args.state_dir.clone(),
        relay: args.relay.clone(),
        label: args.label.clone(),
        discovery: Discovery::N0,
        attach_patience: ATTACH_PATIENCE,
    })?;

    // ---- session: the first run signs in from the environment ----------
    if !session.view().signed_in {
        let handle = std::env::var("CROFT_ARC_HANDLE")
            .map_err(|_| ArcError::NoCredentials("CROFT_ARC_HANDLE"))?;
        let password = std::env::var("CROFT_ARC_APP_PASSWORD")
            .map_err(|_| ArcError::NoCredentials("CROFT_ARC_APP_PASSWORD"))?;
        session.sign_in(&handle, &password, out)?;
    }

    // ---- camp: NOT camped is an honest refusal, exit 2 -------------------
    if session.camp(out)? == Presence::NotCamped {
        session.close();
        return Err(Error::NotCamped {
            relay: args.relay.clone(),
        }
        .into());
    }
    let short = session
        .view()
        .endpoint_id_short
        .unwrap_or_else(|| "----------".to_string());

    // ---- the role -------------------------------------------------------
    match &args.role {
        Role::Callee => {
            out(&line(
                &short,
                "wait",
                &format!("waiting for a call for {}s", args.wait.as_secs()),
            ));
            if let Some(call) = session.wait_for_call(args.wait, out)? {
                let ending = call.ended(args.wait);
                out(&line(
                    &short,
                    "call",
                    &ending.as_ref().map_or_else(
                        || format!("still connected after {}s; leaving it", args.wait.as_secs()),
                        ending_line,
                    ),
                ));
            }
        }
        Role::Caller { callee, device } => {
            let call = session.dial(callee, device, out)?;
            out(&line(
                &short,
                "attach",
                &format!("after the dial: {}", session.view().presence),
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
    session.close();
    out(&line(&short, "done", "endpoint closed"));
    Ok(())
}
