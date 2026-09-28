//! How a worker session ends, and what its lead sees.
//!
//! # Design
//!
//! Each worker pane of `riff workers start` runs `claude` through
//! `riff workers run` (01M3JQC8ANFYYEXSHBS2DCZYBX). The wrapper starts
//! `claude`, and waits. A worker ends in one of two ways:
//!
//! | End | Who acts | The lead gets |
//! |---|---|---|
//! | `claude` exits on its own, for example after a crash | the wrapper | a direct message with the pane, the session ID and the exit code |
//! | `riff workers stop` | the command | nothing: the person or the lead asked for it |
//!
//! The wrapper never starts `claude` again: a crash loop costs tokens.
//! The lead decides.
//!
//! A worker with no work does not end. It sets its status
//! [`IDLE`], keeps its watch and ends its turn
//! (01M3K0AXMCVRST7HYH4DM8B3AN). An idle session costs nothing. A
//! request of the lead wakes it with its next item
//! (01M3K0AXRNA0F2920E9QCSDFQZ). No command ends a worker from inside
//! (01M3K0AXPFSWNG7YPVXE65W464).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as tmux pane
//!     participant W as riff workers run
//!     participant C as claude
//!     participant S as riff-server
//!     participant L as lead
//!     P->>W: start
//!     W->>C: start, RIFF_WORKER=1
//!     alt claude exits
//!         C-->>W: exit code
//!         W->>S: tell lead: pane, session, exit code
//!         S->>L: wake
//!     else riff workers stop
//!         P->>W: SIGHUP, the pane closes
//!         W->>C: SIGTERM
//!     end
//! ```
//!
//! On SIGTERM or SIGHUP, the wrapper stops `claude` and sends no
//! message. A `claude` that a SIGHUP ended counts as stopped too.
//!
//! The wrapper tells the lead as the person, never as the session of
//! the worker. So a crashed worker does not come back in `riff who`.

use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::ExitStatus;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::signal::unix::{SignalKind, signal};

use crate::api::Api;
use crate::identity;

/// The variable that marks a worker session.
pub const WORKER: &str = "RIFF_WORKER";

/// The status of a worker with no work. It waits for a request of the
/// lead (01M3K0AXMCVRST7HYH4DM8B3AN).
///
/// ```
/// assert_eq!(riff::worker::IDLE, "idle: waits for work");
/// ```
pub const IDLE: &str = "idle: waits for work";

/// How long the wrapper waits for `claude` after it sends SIGTERM.
pub const STOP_WAIT: Duration = Duration::from_secs(5);

/// The number of SIGHUP.
const SIGHUP: i32 = 1;

/// True in a worker session: `RIFF_WORKER` is `1`.
///
/// ```
/// assert!(riff::worker::is_worker_value(Some("1")));
/// assert!(!riff::worker::is_worker_value(Some("0")));
/// assert!(!riff::worker::is_worker_value(None));
/// ```
pub fn is_worker() -> bool {
    is_worker_value(std::env::var(WORKER).ok().as_deref())
}

/// [`is_worker`] for the value of `RIFF_WORKER`.
pub fn is_worker_value(value: Option<&str>) -> bool {
    value == Some("1")
}

/// Runs `claude` with `args` as a worker, and waits. When `claude`
/// exits on its own, it tells the lead. Returns the exit code for the
/// wrapper: the code of `claude`, or 0 after a stop.
pub async fn run(claude: &Path, args: &[String], server: &str) -> Result<i32> {
    let mut term = signal(SignalKind::terminate())?;
    let mut hup = signal(SignalKind::hangup())?;
    let mut child = tokio::process::Command::new(claude)
        .args(args)
        .env(WORKER, "1")
        .spawn()
        .with_context(|| format!("cannot start {}", claude.display()))?;
    let status = tokio::select! {
        status = child.wait() => status?,
        _ = term.recv() => return stop(&mut child).await,
        _ = hup.recv() => return stop(&mut child).await,
    };
    // A signal to the wrapper can come just after claude ended from the
    // same stop.
    let stopped = status.signal() == Some(SIGHUP)
        || tokio::select! {
            _ = term.recv() => true,
            _ = hup.recv() => true,
            () = tokio::time::sleep(Duration::from_millis(200)) => false,
        };
    if stopped {
        return Ok(0);
    }
    let pane = std::env::var("TMUX_PANE").ok();
    let session = std::env::var(identity::SESSION_VARS[0]).ok();
    let body = crate::text::worker_stopped(pane.as_deref(), session.as_deref(), &status);
    eprintln!("{body}");
    if let Err(e) = tell_lead(server, &body).await {
        eprintln!("riff: cannot tell the lead: {e:#}");
    }
    Ok(status.code().unwrap_or(1))
}

/// Stops `claude` with SIGTERM, then kills it after [`STOP_WAIT`].
async fn stop(child: &mut tokio::process::Child) -> Result<i32> {
    if let Some(pid) = child.id() {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
    if tokio::time::timeout(STOP_WAIT, child.wait()).await.is_err() {
        child.kill().await?;
    }
    Ok(0)
}

/// Sends `body` to the lead of the person in the repository of this
/// directory, as the person.
async fn tell_lead(server: &str, body: &str) -> Result<()> {
    let place = identity::place(&std::env::current_dir()?)?;
    let me = identity::person(&place, server)?;
    let api = Api::new(server).signed_in(None)?;
    api.tell(&me, crate::api::LEAD, body).await?;
    Ok(())
}

/// The exit of `claude` in words.
///
/// ```
/// use std::os::unix::process::ExitStatusExt;
/// use std::process::ExitStatus;
///
/// assert_eq!(riff::worker::exit_words(&ExitStatus::from_raw(1 << 8)), "exit code 1");
/// assert_eq!(riff::worker::exit_words(&ExitStatus::from_raw(9)), "signal 9");
/// ```
pub fn exit_words(status: &ExitStatus) -> String {
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exit code {code}"),
        (None, Some(signal)) => format!("signal {signal}"),
        (None, None) => "an unknown exit".into(),
    }
}
