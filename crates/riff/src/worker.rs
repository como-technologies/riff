//! How a worker session ends, and what its lead sees.
//!
//! # Design
//!
//! Each worker pane of `riff workers start` runs `claude` through
//! `riff workers run` (01M493YZVZGA7TSRJH6F67VN0H). The wrapper starts
//! `claude`, and waits. It gives `claude` its own process ID in
//! [`WRAPPER`]. A worker ends in one of three ways:
//!
//! | End | Who acts | The lead gets |
//! |---|---|---|
//! | `claude` exits on its own, for example after a crash | the wrapper | a note with the pane, the session ID and the exit code |
//! | `riff workers stop` | the command | nothing: the person or the lead asked for it |
//! | the server stops an idle worker | `riff mcp` of the worker sends SIGTERM to the wrapper | a note of the server |
//! | the pane dies with the wrapper, for example a memory kill | `riff workers host`, or `riff mcp` of the lead ([`crate::reap`]) | a note with the pane, the session, the item and the cause |
//!
//! The wrapper never starts `claude` again. The rollout starts a new
//! worker for the free work ([`crate::rollout`]). An exit with a fault
//! is a death of the worker: the wrapper records it ([`crate::deaths`]).
//! A loop of deaths stops the starts on the machine, and the lead gets
//! one message.
//!
//! A worker with no work does not end. It keeps its watch and ends its
//! turn (01M3K0AXMCVRST7HYH4DM8B3AN). riff shows it idle
//! (01M3Q555KC1RKNEC4ZA9HQYJG2). An idle session costs nothing. A
//! request of the lead wakes it with its next item
//! (01M3K0AXRNA0F2920E9QCSDFQZ). When more idle workers wait on its host
//! than the riff keeps, the server asks it to stop
//! (01M3Q5A0NKY1FCS0YH6N6YD3GN). The reply to the next keep-alive of
//! `riff mcp` carries the ask, and `riff mcp` stops the wrapper
//! (01M3Q5A0QZTSTXHHNYCE8HFJSB).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as tmux pane
//!     participant W as riff workers run
//!     participant C as claude
//!     participant M as riff mcp
//!     participant S as riff-server
//!     participant L as lead
//!     P->>W: start
//!     W->>C: start, RIFF_WORKER=1, RIFF_WORKER_WRAPPER=pid
//!     C->>M: start
//!     alt claude exits
//!         C-->>W: exit code
//!         W->>W: an exit with a fault: record the death
//!         W->>S: a note to the lead: pane, session, exit code
//!         S-->>L: at its next read
//!     else riff workers stop
//!         P->>W: SIGHUP, the pane closes
//!         W->>C: SIGTERM
//!     else the server stops an idle worker
//!         M->>S: keep-alive
//!         S-->>M: stop
//!         M->>W: SIGTERM
//!         W->>C: SIGTERM
//!     end
//! ```
//!
//! On SIGTERM or SIGHUP, the wrapper stops `claude` and sends no
//! message. A `claude` that a SIGHUP ended counts as stopped too.
//!
//! The wrapper tells the lead as the person, never as the session of
//! the worker. So a crashed worker does not come back in `riff who`.
//!
//! # Temp folder
//!
//! The wrapper gives `claude` a temp folder of its own on disk, and
//! deletes it when `claude` ended ([`crate::temp`]).
//!
//! # Permission rules
//!
//! The wrapper adds the permission rules of the profile of a worker to
//! the flag settings of `claude` ([`crate::role_rules`],
//! 01M4BT33R71HXAVQGHFD4ZFGR5). `riff workers rules` prints them.
//!
//! # Limits
//!
//! The wrapper gives `claude` the limits of a worker
//! ([`crate::limits`]): the number of its compile jobs and test threads,
//! a nice value, and a scope in the slice of the workers. The wrapper
//! itself stays outside the slice. So when the OS kills `claude` for the
//! memory of the workers, the wrapper lives and tells the lead: the
//! message names the signal, and says that the work that is not
//! committed is in the worktree of the worker
//! (01M3WFZ03Z9Y60HPHJJ9ZE6AQZ). The next worker of the item goes on
//! from that worktree.

use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::Duration;

use anyhow::{Context, Result};
use riff_core::name::Place;
use tokio::signal::unix::{SignalKind, signal};

use crate::api::Api;
use crate::limits::{self, Limits};
use crate::machine::Machine;
use crate::terminal::{self, Program, Terminal, WorkerPane};
use crate::{
    enable, forge, hygiene, identity, jobserver, local, settings, temp, text, worker_lsp,
    worker_mcp, workload,
};

/// The variable that marks a worker session.
pub const WORKER: &str = "RIFF_WORKER";

/// The variable with the process ID of the `riff workers run` wrapper
/// of a worker (01M3Q5A0QZTSTXHHNYCE8HFJSB).
pub const WRAPPER: &str = "RIFF_WORKER_WRAPPER";

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

/// The process ID of the wrapper of this worker, from [`WRAPPER`].
///
/// ```
/// assert_eq!(riff::worker::wrapper_value(Some("4242")), Some(4242));
/// assert_eq!(riff::worker::wrapper_value(Some("x")), None);
/// assert_eq!(riff::worker::wrapper_value(None), None);
/// ```
pub fn wrapper() -> Option<u32> {
    wrapper_value(std::env::var(WRAPPER).ok().as_deref())
}

/// Sends SIGTERM to the `riff workers run` wrapper `pid` of this worker.
/// The wrapper stops `claude` (01M3Q5A0QZTSTXHHNYCE8HFJSB).
pub fn stop_wrapper(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
}

/// The wrapper of this process when it is a worker: `RIFF_WORKER` is 1
/// and `RIFF_WORKER_WRAPPER` names the wrapper.
pub fn wrapper_of_worker() -> Option<u32> {
    is_worker().then(wrapper).flatten()
}

/// [`wrapper`] for the value of `RIFF_WORKER_WRAPPER`.
pub fn wrapper_value(value: Option<&str>) -> Option<u32> {
    value?.parse().ok()
}

/// Holds the pool of build jobs of this machine in `dir` with the
/// `tokens` for the `counted` workers of `limits`, or `None`: 0 tokens,
/// no local dir, or a pool that riff cannot make. Then the worker gets
/// the fixed share (01M3ZGZMRHXRBP762QPVCV0YX8).
fn hold_pool(dir: Option<&Path>, limits: &Limits) -> Option<jobserver::Pool> {
    if limits.tokens == 0 {
        return None;
    }
    jobserver::Pool::hold(dir?, limits.tokens, limits.counted)
        .inspect_err(|e| eprintln!("{}", text::no_jobserver(&e.to_string())))
        .ok()
}

/// The thread that checks the share of a worker in the pool each
/// [`jobserver::SHARE_EVERY`] ([`jobserver::share`]). In the first
/// worker, it also holds tokens back under memory pressure
/// ([`jobserver::hold_back`]). The drop stops it and gives its tokens
/// back.
struct Share {
    stop: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Share {
    fn start(pool: &jobserver::Pool, member: jobserver::Member) -> Share {
        let (stop, stopped) = std::sync::mpsc::channel::<()>();
        let (fifo, counted) = (pool.fifo(), pool.counted());
        let thread = std::thread::spawn(move || {
            let (mut kept, mut held) = (None, None);
            loop {
                jobserver::share(&fifo, counted, &member, &mut kept);
                let pressure = (member.rank() == 1)
                    .then(jobserver::pressure_here)
                    .flatten();
                jobserver::hold_back(&fifo, pressure, &mut held);
                match stopped.recv_timeout(jobserver::SHARE_EVERY) {
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    _ => break,
                }
            }
        });
        Share {
            stop: Some(stop),
            thread: Some(thread),
        }
    }
}

impl Drop for Share {
    fn drop(&mut self) {
        self.stop.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Runs `claude` with `args` as a worker, and waits. It gives `claude`
/// the limits of a worker of this machine (see [`crate::limits`]): the
/// jobs (01M3WFYZRK5CT22GJW6ZHYT9CC), the absolute nice value
/// (01M3WFYZTX05CGDP2NQF9B356K, 01M407J8R79WVYVABVCSHFAMJ9), and a scope
/// in the slice that [`limits::SLICE_VAR`] names, with the memory of
/// the settings (01M3WFYZX6GVFYW6NTTTKF144R), when the machine has
/// systemd and a scope works in the pane (01M3WFYZZENNHVH8Z2BAFSR6TS,
/// 01M407J8X25H9AT8M789EG5RQZ). It is the one process of a worker that
/// calls systemd: it runs outside each sandbox
/// (01M4C2PXZ5WNE4C2CJW2HABPY0). It gives no compile cache
/// (01M4BQA5K7DQHQ4DSJGQJH8ZQE). When `claude` exits on its own, it
/// tells the lead. Returns the exit code for the wrapper: the code of
/// `claude`, or 0 after a stop.
pub async fn run(claude: &Path, args: &[String], server: &str) -> Result<i32> {
    let mut term = signal(SignalKind::terminate())?;
    let mut hup = signal(SignalKind::hangup())?;
    let dir = local::dir().map(|local| jobserver::dir(&local));
    // This worker counts in the workers that run (01M3WFYZRK5CT22GJW6ZHYT9CC).
    let member = dir
        .as_deref()
        .and_then(|dir| jobserver::Member::join(dir).ok());
    let workers = dir.as_deref().map_or(0, jobserver::workers);
    let machine = Machine::here();
    let cores = limits::Cores::here(&machine);
    let settings = settings::path()?;
    let limit = Limits::of(&settings, &cores, workers)?;
    let slice = std::env::var(limits::SLICE_VAR)
        .ok()
        .filter(|s| !s.is_empty());
    let max_gb = limits::memory(machine.mem_gb, settings::workers_memory(&settings)?);
    let (slice, said) = limits::worker_slice(
        slice.as_deref(),
        local::dir().as_deref(),
        |slice| limits::set_slice(slice, max_gb),
        limits::try_scope,
    );
    if let Some(said) = said {
        eprintln!("{said}");
    }
    let here = limits::nice_here();
    if limit.nice > 0 && here > limit.nice {
        eprintln!("{}", crate::text::nice_above(here, limit.nice));
    }
    let nice = limits::nice_by(limit.nice, here);
    // The temp folder of this worker, on disk. Its drop deletes it after
    // `claude` ends (01M41VAGJC69S9R2TD1B1EQ4W4, 01M41VAGQ2VA2Q0VSFJNG4H08W).
    let folder = std::env::var(identity::SESSION_VARS[0])
        .ok()
        .and_then(|session| temp::Folder::make(&session));
    let args = match &folder {
        Some(folder) => temp::with_env(args, folder.path()),
        None => args.to_vec(),
    };
    // The permission rules of the profile of a worker
    // (01M4BT33R71HXAVQGHFD4ZFGR5).
    let args = match profile_rules(claude, folder.as_ref().map(temp::Folder::path), server) {
        Ok(rules) => crate::role_rules::flag(&args, &rules),
        Err(why) => {
            eprintln!("{why}");
            args
        }
    };
    // The scope has the name of the worker, so the clear, the reap and
    // the stop find each of its processes (01M49SV9W4S1HJ4BYANA388VD2).
    let session = std::env::var(identity::SESSION_VARS[0]).ok();
    let unit = session
        .as_deref()
        .map(|session| workload::scope_unit(session, std::process::id()));
    let command = limits::command(claude, &args, nice, slice.as_deref(), unit.as_deref());
    // The pool lives while this wrapper lives (01M3ZGZMJ9RF1C4AHG78GQ2NM4).
    let pool = hold_pool(dir.as_deref(), &limit);
    // A worker past the count of the pool keeps one token out of it.
    // With no pool, the worker still counts while it lives.
    let (_share, _member) = match (&pool, member) {
        (Some(pool), Some(member)) => (Some(Share::start(pool, member)), None),
        (_, member) => (None, member),
    };
    let riff = crate::binary::this_on_disk()?;
    let mut cmd = tokio::process::Command::new(&command[0]);
    cmd.args(&command[1..])
        .env(WORKER, "1")
        .env(WRAPPER, std::process::id().to_string())
        // A tmux server that a context started gives each pane the
        // variable of that context. `claude` is of no context
        // (01M3ZV0QSFVCHRSEKYK57B88VA).
        .env_remove(crate::next::Agent::context_var(&crate::next::ClaudeCode));
    if let Some(folder) = &folder {
        for var in temp::VARS {
            cmd.env(var, folder.path());
        }
    }
    // The forge token of the role of this session, in place of the token
    // of the person (01M4BV707FYHJDNC1499YAWR8D, 01M4BV709WGHZ57AM3STC15B69).
    let _keep = match forge_keeper(folder.as_ref(), session.as_deref(), server, &riff).await {
        Some((env, keep)) => {
            for (var, value) in env {
                match value {
                    Some(value) => cmd.env(var, value),
                    None => cmd.env_remove(var),
                };
            }
            keep.map(AbortOnDrop)
        }
        None => None,
    };
    let makeflags = pool.as_ref().map(jobserver::Pool::makeflags);
    for (var, value) in limits::jobs_env(&limit, makeflags.as_deref(), &riff) {
        match value {
            Some(value) => cmd.env(var, value),
            None => cmd.env_remove(var),
        };
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("cannot start {}", Path::new(&command[0]).display()))?;
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
    let body = crate::text::worker_stopped(pane.as_deref(), session.as_deref(), &status);
    eprintln!("{body}");
    if let Err(e) = note_lead(server, &body).await {
        eprintln!("riff: cannot post the note to the lead: {e:#}");
    }
    // An exit with a fault is a death (01M493YZZEW1FTDBNA090WT2AG).
    if !status.success()
        && let Some(session) = &session
        && crate::deaths::record_here(session).is_some_and(|r| r.starts_loop)
    {
        let host =
            identity::here(None).map_or_else(|_| identity::this_host(), |p| p.host().to_owned());
        let body = crate::text::death_loop(&host, crate::deaths::here());
        eprintln!("{body}");
        if let Err(e) = tell_lead(None, server, &body).await {
            eprintln!("riff: cannot tell the lead: {e:#}");
        }
    }
    Ok(status.code().unwrap_or(1))
}

/// The permission rules of a worker in this main clone, or the line
/// that says why it has none (01M4BT33Z914GBHCGCAXFVQ2X7).
pub fn profile_rules(
    claude: &Path,
    temp: Option<&Path>,
    server: &str,
) -> std::result::Result<crate::permissions::Rules, String> {
    let no = |why: &str| crate::text::no_role_rules(crate::profile::Role::Worker, why);
    let here = std::env::current_dir().map_err(|e| no(&e.to_string()))?;
    let clone = identity::main_worktree(&here).ok_or_else(|| no("it runs in no git repository"))?;
    let temp = temp.map_or_else(std::env::temp_dir, Path::to_path_buf);
    let session = crate::role_rules::clone_session(&clone, &temp, claude, server)
        .ok_or_else(|| no("it has no HOME, or the server URL has no host"))?;
    crate::role_rules::here(crate::profile::Role::Worker, &session)
}

/// Stops a task when it drops.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The forge token of a worker session, when the settings name a GitHub
/// App ([`forge`]): the environment of `claude` and the task that keeps
/// the token. The environment holds no token of the person also when
/// riff cannot make a token, so a session never falls back to the
/// rights of the person. `None` with no App.
async fn forge_keeper(
    folder: Option<&temp::Folder>,
    session: Option<&str>,
    server: &str,
    riff: &Path,
) -> Option<(
    Vec<(&'static str, Option<String>)>,
    Option<tokio::task::JoinHandle<()>>,
)> {
    let settings = settings::path().ok()?;
    if !matches!(settings::forge_app(&settings), Ok(Some(_))) {
        return None;
    }
    let no_token = |why: String| eprintln!("{}", crate::text::forge_no_token(&why));
    let Some(folder) = folder else {
        no_token("the worker has no temp folder".into());
        // Files in no folder: `gh` and git find no token.
        let none = forge::Files::in_temp(Path::new("/nonexistent"));
        return Some((none.env(riff), None));
    };
    let files = forge::Files::in_temp(folder.path());
    let env = files.env(riff);
    let made = (|| {
        let app = forge::App::here(&settings)?.context("no forge.app")?;
        let place = identity::here(None)?;
        let repo = place.repo_text();
        anyhow::ensure!(repo != "-", "this folder is in no repository");
        let session = session.context("the worker has no session ID")?.to_owned();
        anyhow::Ok((app, repo, place, session))
    })();
    let (app, repo, place, id) = match made {
        Ok(made) => made,
        Err(e) => {
            no_token(format!("{e:#}"));
            return Some((env, None));
        }
    };
    let server = server.to_owned();
    let claims = move || {
        let (place, id, server) = (place.clone(), id.clone(), server.clone());
        async move {
            let api = Api::new(&server).one_try(forge::LOOK_EVERY / 2);
            let me = identity::agent(&place, &id, api.base()).ok()?;
            let info = api.signed_in(me.who().session()).ok()?.me(&me).await.ok()?;
            Some(info.session?.uri.claims().to_vec())
        }
    };
    let mut keeper = forge::Keeper::new(app, forge::GitHub::here(), repo, files);
    // The first token comes before `claude` starts.
    if let Err(e) = keeper.step(claims().await.as_deref()).await {
        no_token(format!("{e:#}"));
    }
    Some((env, Some(tokio::spawn(forge::keep(keeper, claims)))))
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

/// Posts `body` as a note to the lead of the person in the repository
/// of this directory, as the person. A note wakes nobody.
async fn note_lead(server: &str, body: &str) -> Result<()> {
    let place = identity::here(None)?;
    let me = identity::person(&place, server)?;
    let api = Api::new(server).signed_in(None)?;
    let to = riff_core::selector::Selector::lead(me.who().user(), &place.repo_text());
    api.post(&me, None, &[to], body, riff_core::wire::Kind::Note)
        .await?;
    Ok(())
}

/// Sends `body` to the lead of the person in the repository of
/// `place`, else of this directory, as the person.
pub(crate) async fn tell_lead(place: Option<&Place>, server: &str, body: &str) -> Result<()> {
    let place = identity::here(place)?;
    let me = identity::person(&place, server)?;
    let api = Api::new(server).signed_in(None)?;
    api.tell(&me, crate::api::LEAD, body).await?;
    Ok(())
}

/// True when `one` names the worker `w`: its pane, or its session ID or
/// the start of it (01M3Q5A0Z5DK0YV1MWTM4AQD5Z).
///
/// ```
/// use riff::terminal::WorkerPane;
/// use riff::worker::is_one;
///
/// let w = WorkerPane { pane: "%3".into(), session: "1a2b3c4d-5e6f".into() };
/// assert!(is_one(&w, "%3"));
/// assert!(is_one(&w, "1a2b3c4d"));
/// assert!(is_one(&w, "1a2b3c4d-5e6f"));
/// assert!(!is_one(&w, "%31"));
/// assert!(!is_one(&w, "1a2"), "a start of fewer than 4 characters names no worker");
/// ```
pub fn is_one(w: &WorkerPane, one: &str) -> bool {
    w.pane == one || (one.len() >= 4 && w.session.starts_with(one))
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
    /// What riff says the first time that it cannot make the pool of
    /// build jobs of the machine (01M3ZGZMRHXRBP762QPVCV0YX8).
    pub no_pool: Option<String>,
    /// What riff says the first time that it cannot read the physical
    /// cores of the machine (01M3WFYZRK5CT22GJW6ZHYT9CC).
    pub no_cores: Option<String>,
}

/// The refusal of a start of workers for `dir`, when riff is off where
/// a worker starts: the main clone of the repository of `dir`
/// ([`enable::State::of_workers`], 01M3XY2T542DCHBN95H9PX4AGQ). A
/// worker there is a plain session: it never joins the riff, so the
/// rollout would start the next one.
pub fn off(dir: &Path) -> Option<String> {
    let user = crate::plugin::user_settings();
    let state = enable::State::of_workers(dir, user.as_deref(), enable::forced());
    (!state.on).then(|| text::workers_off(&state))
}

/// The variables that a new worker gets from a process with `forced`:
/// `RIFF_ON=1` turned riff on for the process, so it turns riff on for
/// its workers (01M3XY2SWEK0N8MC3MY4TMYTD3). A tmux pane does not get
/// the variables of the process that makes it.
///
/// ```
/// assert_eq!(riff::worker::on_env(true), Some(("RIFF_ON".into(), "1".into())));
/// assert_eq!(riff::worker::on_env(false), None);
/// ```
pub fn on_env(forced: bool) -> Option<(String, String)> {
    forced.then(|| (enable::VAR.into(), "1".into()))
}

/// Starts at most `count` workers in `tmux`, in the main worktree of
/// `dir` (01M3JD392Q5ANX0FPZ51W7B0E3): at most the limit of the machine
/// minus the workers that run (01M3JPQT57PJCRBQYJNDVESS04). Each loads
/// only the MCP servers of `workers.mcp` (01M3NB5R92ZC61VW6Y45SJEAY9), and
/// no plugin with a language server (01M3ZJ1FAF7EJXP9CSET8ZY1K3). It
/// starts none while the available memory is less than the floor
/// (01M3WFZ01PTAYYKG3T5CFA2W4D), and while the disk of the main clone is
/// low (01M41A11DX1QRP48YPTDNT67W4). It makes the slice of the workers ready
/// first (01M3WFYZX6GVFYW6NTTTKF144R). It starts none where riff is off
/// in the main clone ([`off`], 01M3XY2T542DCHBN95H9PX4AGQ): the command,
/// the rollout of the lead and a workers host all start workers here.
/// A worker of a process with `RIFF_ON=1` gets `RIFF_ON=1`. The inner
/// error is the refusal to show when it started nothing. The caller
/// checks who may start workers.
pub fn start(
    tmux: &dyn Terminal,
    count: u16,
    claude: &Path,
    server: &str,
    dir: &Path,
) -> Result<std::result::Result<Started, String>> {
    if let Some(why) = off(dir) {
        return Ok(Err(why));
    }
    let settings = settings::path()?;
    let limit = settings::workers_limit(&settings)?;
    if limit == 0 {
        return Ok(Err(text::NO_WORKER_LIMIT.into()));
    }
    let run = tmux.worker_panes()?.len();
    let start = terminal::room(count, limit, run);
    if start == 0 {
        return Ok(Err(text::workers_full(limit, run)));
    }
    let machine = Machine::here();
    let floor = settings::workers_floor(&settings)?;
    if machine.low(floor) {
        return Ok(Err(text::workers_low(machine.avail_gb, floor)));
    }
    let main = identity::main_worktree(dir)
        .ok_or_else(|| anyhow::anyhow!("run it in a git repository"))?;
    if let Some(disk) = crate::disk::Disk::here(&main).filter(|d| d.low()) {
        return Ok(Err(text::workers_disk_low(&disk)));
    }
    let fresh = hygiene::fast_forward(&main).line();
    let base = Api::new(server).base().to_owned();
    let riff = crate::binary::this_on_disk()?;
    let mcp = worker_mcp::prepare(&main, &riff)?;
    let flags = terminal::worker_settings(&worker_lsp::here());
    let claude = terminal::Claude {
        bin: claude,
        mcp: &mcp,
        settings: &flags,
    };
    let programs: Vec<Program> = (0..start)
        .map(|_| {
            let mut worker = Program::worker(
                &riff,
                &claude,
                &main,
                &base,
                &terminal::new_session_id(),
                // The wrapper sets the slice: it runs outside each
                // sandbox (01M4C2PXZ5WNE4C2CJW2HABPY0).
                Some(limits::SLICE),
            );
            worker.env.extend(on_env(enable::forced()));
            worker
        })
        .collect();
    let (window, panes) = tmux.workers(&programs)?;
    Ok(Ok(Started {
        panes,
        window,
        main,
        fresh,
        limited: (start < count).then(|| text::workers_limited(count - start, limit, run)),
        no_pool: jobserver::check(local::dir().as_deref()),
        no_cores: limits::say_cores(&limits::Cores::here(&machine), local::dir().as_deref()),
    }))
}

/// Ends each worker of `tmux`, or the one in `pane`: it kills the pane,
/// stops each process of the worker that lives after the pane
/// (01M3ZV0TMNQDK9WC3BR1NPGAC2), then sends the end call of the session
/// (01M3JPQTDFW3C7QBSZZ2M831MH).
/// `pane` is a pane, or the session ID of a worker or its start
/// (01M3Q5A0Z5DK0YV1MWTM4AQD5Z). Returns the number of stopped workers.
pub async fn stop(tmux: &dyn Terminal, pane: Option<&str>, server: &str) -> Result<usize> {
    let mut panes = tmux.worker_panes()?;
    if let Some(pane) = pane {
        panes.retain(|w| is_one(w, pane));
        if panes.is_empty() {
            anyhow::bail!("no worker runs in the pane {pane}. `riff workers` lists them");
        }
    }
    let here = identity::place(&identity::working_dir()?)?;
    let api = Api::new(server);
    let var = crate::next::Agent::context_var(&crate::next::ClaudeCode);
    for worker in &panes {
        tmux.kill(&worker.pane)?;
        // Each process of the worker, also one that left the pane
        // (01M3ZV0TMNQDK9WC3BR1NPGAC2).
        let all = workload::all(var);
        let found = workload::of_worker(&all, &worker.session, std::process::id());
        workload::say_here(found.by, &worker.session);
        let stopped = workload::stop(&found.procs, var);
        if !stopped.is_empty() {
            println!("{}", text::stopped_after_pane(&worker.pane, &stopped));
        }
        // The wrapper deletes the folder when `claude` ended. This is for
        // a wrapper that died with the pane (01M41VAGQ2VA2Q0VSFJNG4H08W).
        if let Some(dir) = temp::here(&worker.session) {
            temp::end_session(&dir, &temp::users());
        }
        let ended = async {
            let me = identity::agent(&here, &worker.session, api.base())?;
            // The end call has a budget, so a server that gives no reply
            // does not hold the next pane (01M3WN72M02P3J24ACCHTMNSFY).
            let api = api.with_budget(crate::mcp::END_WAIT);
            api.signed_in(Some(&worker.session))?.end(&me).await
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

/// Stops the orphan processes of each worker of `tmux`, or of the one
/// in `pane` (01M3ZV0TKBP201FKY32ZD81G4E): each process of an old
/// context of the worker ([`workload::old_context`]). The start of the
/// current context comes from the local dir `dir`. Returns one line for
/// each worker and each process that it stopped.
pub fn reap(tmux: &dyn Terminal, pane: Option<&str>, dir: &Path) -> Result<Vec<String>> {
    let mut panes = tmux.worker_panes()?;
    if let Some(pane) = pane {
        panes.retain(|w| is_one(w, pane));
        if panes.is_empty() {
            anyhow::bail!("no worker runs in the pane {pane}. `riff workers` lists them");
        }
    }
    let var = crate::next::Agent::context_var(&crate::next::ClaudeCode);
    let all = workload::all(var);
    let mut lines = Vec::new();
    for worker in &panes {
        let Some(since) = workload::context_start(dir, &worker.session) else {
            lines.push(text::reap_no_start(&worker.pane));
            continue;
        };
        let old = workload::old_context(&all, &worker.session, std::process::id(), Some(since));
        workload::say_here(old.by, &worker.session);
        let stopped = workload::stop(&old.procs, var);
        if stopped.is_empty() {
            lines.push(text::reaped_none(&worker.pane));
        }
        lines.extend(stopped.iter().map(|p| text::reaped(&worker.pane, p)));
    }
    Ok(lines)
}
