//! The command line: two roles and a handful of flags, refused with words.
//!
//! Hand-rolled rather than a parser crate: six flags, and a refusal here is
//! read by a person at a terminal, so it names the flag that was wrong.

use std::path::PathBuf;
use std::time::Duration;

use call_transport_iroh::relay::CROFT_RELAY_URL;

/// Which side of the arc this process is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// Camp and wait to be dialled.
    Callee,
    /// Camp and dial `callee`'s `device`.
    Caller {
        /// A handle or DID.
        callee: String,
        /// The rkey of the callee's endpoint record to dial.
        device: String,
    },
}

/// Everything a run needs from the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// Callee or caller.
    pub role: Role,
    /// Where the key, session and pass live; `None` is the default under
    /// the user's state directory.
    pub state_dir: Option<PathBuf>,
    /// The relay to camp on.
    pub relay: String,
    /// This device's rkey and label.
    pub label: String,
    /// Caller: how long to stay connected before hanging up.
    pub hold: Duration,
    /// Callee: how long to wait for a call.
    pub wait: Duration,
}

/// The default rkey/label for an arc's device record.
pub const DEFAULT_LABEL: &str = "croft-arc";

/// Parse the arguments after the program name.
pub fn parse<I: IntoIterator<Item = String>>(argv: I) -> Result<Args, String> {
    let mut it = argv.into_iter();
    let role_word = it.next().ok_or_else(usage)?;
    let mut role = match role_word.as_str() {
        "callee" => Role::Callee,
        "call" => Role::Caller {
            callee: it
                .next()
                .filter(|w| !w.starts_with("--"))
                .ok_or_else(|| "`call` needs who to call: a handle or DID".to_string())?,
            device: DEFAULT_LABEL.to_string(),
        },
        _ => return Err(usage()),
    };
    let mut args = Args {
        role: Role::Callee,
        state_dir: None,
        relay: CROFT_RELAY_URL.to_string(),
        label: DEFAULT_LABEL.to_string(),
        hold: Duration::from_secs(5),
        wait: Duration::from_secs(60),
    };
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .filter(|v| !v.starts_with("--"))
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--state-dir" => args.state_dir = Some(PathBuf::from(value()?)),
            "--relay" => args.relay = value()?,
            "--label" => args.label = value()?,
            "--hold" => args.hold = seconds(&flag, &value()?)?,
            "--wait" => args.wait = seconds(&flag, &value()?)?,
            "--device" => match &mut role {
                Role::Caller { device, .. } => *device = value()?,
                Role::Callee => return Err("--device only makes sense with `call`".to_string()),
            },
            other => return Err(format!("unknown flag {other}\n{}", usage())),
        }
    }
    args.role = role;
    Ok(args)
}

fn seconds(flag: &str, v: &str) -> Result<Duration, String> {
    v.parse::<u64>()
        .map(Duration::from_secs)
        .map_err(|_| format!("{flag} wants a whole number of seconds, got {v:?}"))
}

fn usage() -> String {
    "usage: croft-arc callee [--wait <secs>] | croft-arc call <handle-or-did> [--device <rkey>] [--hold <secs>]\n       common: --state-dir <dir> --relay <url> --label <rkey>\n       first run: CROFT_ARC_HANDLE and CROFT_ARC_APP_PASSWORD in the environment".to_string()
}
