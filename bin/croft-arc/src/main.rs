//! The `croft-arc` command. Everything it does lives in the library; this
//! file hands the arguments in and the exit code out.
//!
//! Exit codes: 0 the arc completed; 2 an honest refusal the output explains
//! (bad arguments, a dead session, not camped); 1 anything else.

use std::process::ExitCode;

fn main() -> ExitCode {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();
    let args = match croft_arc::args::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(words) => {
            eprintln!("{words}");
            return ExitCode::from(2);
        }
    };
    let mut out = |l: &str| println!("{l}");
    match croft_arc::arc::run(&args, &mut out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("croft-arc: {e}");
            ExitCode::from(u8::try_from(e.exit_code()).unwrap_or(1))
        }
    }
}
