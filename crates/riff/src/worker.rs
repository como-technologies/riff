//! How a worker session ends, and what its lead sees.
//!
//! # Design
//!
//! Each worker pane of `riff workers start` runs `claude` through
//! `riff workers run` (01M3JQC8ANFYYEXSHBS2DCZYBX). The wrapper starts
//! `claude`, and waits. A worker ends in one of three ways:
//!
//! | End | Who acts | The lead gets |
//! |---|---|---|
//! | `claude` exits on its own, for example after a crash | the wrapper | a direct message with the pane, the session ID and the exit code |
//! | the worker has no work | `riff workers done`, in the worker | a direct message from the worker: it has no work |
//! | `riff workers stop` | the command | nothing: the person or the lead asked for it |
//!
//! The wrapper never starts `claude` again: a crash loop costs tokens.
//! The lead decides.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as tmux pane
//!     participant W as riff workers run
//!     participant C as claude
//!     participant S as riff-server
//!     participant L as lead
//!     P->>W: start
//!     W->>C: start, RIFF_WORKER_PID=W
//!     alt claude exits
//!         C-->>W: exit code
//!         W->>S: tell lead: pane, session, exit code
//!         S->>L: wake
//!     else no work
//!         C->>S: riff workers done: tell lead, end
//!         S->>L: wake
//!         C->>W: SIGTERM
//!         W->>C: SIGTERM
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

use anyhow::{Context, Result, bail};
use tokio::signal::unix::{SignalKind, signal};

use crate::api::Api;
use crate::identity;

/// The variable that marks a worker session.
pub const WORKER: &str = "RIFF_WORKER";

/// The variable that holds the process ID of the wrapper.
pub const WRAPPER_PID: &str = "RIFF_WORKER_PID";

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
        .env(WRAPPER_PID, std::process::id().to_string())
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

/// `riff workers done`: a worker with no work tells the lead, sends the
/// end call, and stops its wrapper (01M3JQC8CN72WAVPE3189216C8). `api`
/// acts as the session `me`.
pub async fn done(api: &Api, me: &riff_core::name::SessionUri) -> Result<()> {
    let Some(pid) = std::env::var(WRAPPER_PID)
        .ok()
        .and_then(|p| p.parse::<u32>().ok())
    else {
        bail!("this session is not a worker: {WRAPPER_PID} is not set");
    };
    // A worker that holds a claim, for example while it waits for a
    // verify, does not end (01M3JQC8GVFWC47NTN4NKE730P).
    let sessions = api.who(me, false).await?;
    if let Some(info) = sessions.iter().find(|s| s.uri.who() == me.who())
        && !info.uri.claims().is_empty()
    {
        bail!(crate::text::done_holds_claims(info.uri.claims()));
    }
    let pane = std::env::var("TMUX_PANE").ok();
    api.tell(
        me,
        crate::api::LEAD,
        &crate::text::worker_done(pane.as_deref()),
    )
    .await?;
    api.end(me).await?;
    let killed = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()?;
    if !killed.success() {
        bail!("cannot stop the worker wrapper {pid}");
    }
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
