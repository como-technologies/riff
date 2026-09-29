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
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::signal::unix::{SignalKind, signal};

use crate::api::Api;
use crate::terminal::{self, Program, Terminal, WorkerPane};
use crate::{hygiene, identity, settings, text, worker_mcp};

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
        _ = term.recv() => return stop_child(&mut child).await,
        _ = hup.recv() => return stop_child(&mut child).await,
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
async fn stop_child(child: &mut tokio::process::Child) -> Result<i32> {
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
pub(crate) async fn tell_lead(server: &str, body: &str) -> Result<()> {
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

/// The workers that one start opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    /// The pane and the session of each new worker.
    pub panes: Vec<WorkerPane>,
    /// The tmux window of the workers.
    pub window: String,
    /// The main worktree where each worker starts.
    pub main: PathBuf,
    /// The line of the fast-forward of the main clone, if any.
    pub fresh: Option<String>,
    /// The workers that the limit kept from a start, and why.
    pub limited: Option<String>,
}

/// Starts at most `count` workers in `tmux`, in the main worktree of
/// `dir` (01M3JD392Q5ANX0FPZ51W7B0E3): at most the limit of the machine
/// minus the workers that run (01M3JPQT57PJCRBQYJNDVESS04). Each loads
/// only the MCP servers of `workers.mcp` (01M3NB5R92ZC61VW6Y45SJEAY9). The inner
/// error is the refusal to show when it started nothing. The caller
/// checks who may start workers.
pub fn start(
    tmux: &dyn Terminal,
    count: u16,
    claude: &Path,
    server: &str,
    dir: &Path,
) -> Result<std::result::Result<Started, String>> {
    let limit = settings::workers_limit(&settings::path()?)?;
    if limit == 0 {
        return Ok(Err(text::NO_WORKER_LIMIT.into()));
    }
    let run = tmux.worker_panes()?.len();
    let start = terminal::room(count, limit, run);
    if start == 0 {
        return Ok(Err(text::workers_full(limit, run)));
    }
    let main = identity::main_worktree(dir)
        .ok_or_else(|| anyhow::anyhow!("run it in a git repository"))?;
    let fresh = hygiene::fast_forward(&main).line();
    let base = Api::new(server).base().to_owned();
    let riff = std::env::current_exe()?;
    let mcp = worker_mcp::prepare(&main, &riff)?;
    let programs: Vec<Program> = (0..start)
        .map(|_| {
            Program::worker(
                &riff,
                claude,
                &main,
                &base,
                &terminal::new_session_id(),
                &mcp,
            )
        })
        .collect();
    let (window, panes) = tmux.workers(&programs)?;
    Ok(Ok(Started {
        panes,
        window,
        main,
        fresh,
        limited: (start < count).then(|| text::workers_limited(count - start, limit, run)),
    }))
}

/// Ends each worker of `tmux`, or the one in `pane`: it kills the pane,
/// then sends the end call of the session (01M3JPQTDFW3C7QBSZZ2M831MH).
/// Returns the number of stopped workers.
pub async fn stop(tmux: &dyn Terminal, pane: Option<&str>, server: &str) -> Result<usize> {
    let mut panes = tmux.worker_panes()?;
    if let Some(pane) = pane {
        panes.retain(|w| w.pane == pane);
        if panes.is_empty() {
            anyhow::bail!("no worker runs in the pane {pane}. `riff workers` lists them");
        }
    }
    let here = identity::place(&std::env::current_dir()?)?;
    let api = Api::new(server);
    for worker in &panes {
        tmux.kill(&worker.pane)?;
        let ended = async {
            let me = identity::agent(&here, &worker.session, api.base())?;
            api.clone().signed_in(Some(&worker.session))?.end(&me).await
        };
        if let Err(e) = ended.await {
            eprintln!(
                "riff: stopped the pane {}, but the end call of its session failed: {e:#}",
                worker.pane
            );
        }
    }
    Ok(panes.len())
}
