//! `real-session --riff PATH [--skip DIR]... [--gate-wait SECONDS]`: the
//! real-session test of a release. `just real-session` runs it. See the
//! library docs.

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use real_session::{Options, USAGE};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match Options::parse(&args) {
        Ok(options) => options,
        Err(e) => {
            eprintln!("error: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let stop = Arc::new(AtomicBool::new(false));
    if let Err(e) = real_session::watch_signals(stop.clone()) {
        eprintln!("real-session: warning: Ctrl-C ends the test with no clean-up: {e}");
    }
    ExitCode::from(real_session::run(&options, &stop))
}
