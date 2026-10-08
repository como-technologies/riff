//! The door of a session in its sandbox to tmux and to the processes of
//! the workers.
//!
//! # Design
//!
//! Each AI role runs in the sandbox of its role
//! (01M4DDWP9XSA14E0YF211XZYKR, 01M4BTB72DY0C5Y0EJVR0ZH6FZ): `riff
//! workers lead` and `riff workers run` start `claude` through `riff
//! workers sandbox`. In the sandbox, a session reaches no tmux socket
//! (the seccomp filter of [`crate::confine`]) and sends no signal to a
//! process outside its domain (the Landlock scope of signals). The lead
//! still starts, lists, stops and reaps the workers of the machine, and
//! opens the `riff tail` pane. A worker still gets `/clear` in its own
//! pane after its last item ([`crate::next`]), and ends when more
//! workers run than the limit.
//!
//! Each of these steps is a named operation of the broker of the
//! session, never a free command. A [`Door`] is tmux in a session with
//! no broker, and the broker in a session with one, so each caller has
//! one code path. There are two sets of operations:
//!
//! - **The operations of the lead** ([`LEAD_OPS`],
//!   01M4DDWPC693RNWHY7P7XBZ9TB). Only the broker of a lead runs them.
//!   The broker of a worker or a verifier refuses each of them.
//! - **The operations of the own pane** ([`OWN_OPS`],
//!   01M4DVW24ESG6XCBNMFV7T9Z4E). The broker of each role runs them, on
//!   the pane of the session that asks: the `TMUX_PANE` of the broker.
//!   None of them takes a pane, so a session cannot name the pane of
//!   another session ([`OwnPane`]).
//!
//! | Operation | Set | Arguments | What the broker does | Reply |
//! |---|---|---|---|---|
//! | `worker-panes` | lead | none | lists the worker panes of the machine, with the systemd scope of each ([`crate::reap::scope_of`]) | [`Watched`] list |
//! | `tail-pane` | lead | none | adds the `riff tail` pane beside the pane of the lead | true when it added one |
//! | `workers-start` | lead | a count, 1 to [`MAX_START`] | [`crate::worker::start`] with the `claude` of `PATH`, the server and the clone of the broker | [`Started`], or why not |
//! | `workers-stop` | lead | none, or a pane or session | [`crate::worker::stop`] on the workers of its clone | [`Stopped`] |
//! | `workers-reap` | lead | none, or a pane or session | [`crate::worker::reap`] on the workers of its clone | the lines |
//! | `oom-journal` | lead | none | the lines of `systemd-oomd` ([`crate::reap::journal`]) | the text |
//! | `pane-id` | own | none | the name of the own pane, for example `%5` | the name |
//! | `pane-screen` | own | none | the text of the own pane | the text |
//! | `pane-type` | own | the text | types the text and Enter into the own pane | nothing |
//! | `end-over-limit` | own | none | [`crate::next::end_over_limit`] of the session of the broker in the own pane | true when it ended the worker |
//!
//! - **The own pane.** The compact check of the lead reads its pane and
//!   types `/compact` there ([`crate::compact`]). The check of the clear
//!   of a worker types `/clear` and the start prompt there
//!   (01M4DVW26Q5MX025XBW7JCK44S). Each uses [`OwnPane`].
//! - **The end over the limit** (01M4DVW290H2AQF6EQFHTY1EE1). The broker
//!   counts the workers of the machine, posts the note to the lead and
//!   ends its own worker under the lock of the limit, as the check does
//!   outside a sandbox. The session, the server and the pane come from
//!   the broker, not from the request.
//! - **The end of the broker** (01M4DVW2DGX8N9NJZHSAGK5EH4). An
//!   `end-over-limit` closes the pane of its own worker and stops each
//!   process of the worker. So the broker ignores a hangup, and it ends
//!   only after each operation of this module that runs: the end call
//!   of the session comes after the pane closed.
//!
//! - **The clone** (01M4DDWPEGBAXKTS0X3THFB8VZ). tmux marks each worker
//!   pane with its main clone ([`CLONE_MARK`], 01M4DDWPJZFHQ4N7CHPQ5VC1QF).
//!   The broker stops and reaps only the panes with the mark of its own
//!   clone ([`OfClone`]). So a lead never stops the worker of another
//!   repository, also when it names the pane. A start counts each worker
//!   of the machine against the limit, as before.
//! - **No value of the request.** The program, the server and the folder
//!   of a start come from the broker. A count or a pane is the only
//!   argument.
//! - **The pane of the session.** The broker runs outside the sandbox
//!   with the `TMUX` and the `TMUX_PANE` of its session: the shim
//!   removes them only for `claude`.
//! - **The reply.** The broker writes the JSON of the reply to the
//!   stdout of the request, and replies with the exit code 0. A
//!   refusal is the reply [`Reply::Refused`].
//!
//! ```mermaid
//! sequenceDiagram
//!     participant M as riff mcp, riff workers (lead, in the sandbox)
//!     participant B as riff workers broker --role lead (outside)
//!     participant T as tmux, /proc, the processes of a worker
//!     M->>B: workers-stop %7, a pipe as stdout
//!     B->>T: the panes with @riff-clone of the clone
//!     B->>T: kill-pane %7, stop the processes of its worker
//!     B-->>M: JSON of Stopped on the pipe, code 0
//! ```
//!
//! ```
//! use riff::door::{LEAD_OPS, OWN_OPS, is_op, refusal};
//!
//! assert!(LEAD_OPS.contains(&"workers-stop"));
//! assert!(OWN_OPS.contains(&"pane-type"));
//! assert!(is_op("end-over-limit") && !is_op("shell"));
//! assert_eq!(refusal("workers-start", &["2".into()]), None);
//! assert!(refusal("workers-start", &["0".into()]).is_some());
//! assert!(refusal("workers-start", &["x".into()]).is_some());
//! assert!(refusal("workers-stop", &["%7".into(), "%8".into()]).is_some());
//! assert!(refusal("tail-pane", &["sh".into()]).is_some());
//! assert!(refusal("pane-type", &[]).is_some());
//! assert_eq!(refusal("pane-type", &["/compact".into()]), None);
//! // An operation of the own pane names no pane.
//! assert!(refusal("pane-screen", &["%9".into()]).is_some());
//! assert!(refusal("end-over-limit", &["%9".into()]).is_some());
//! ```

use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::broker::{Reply, Request};
use crate::reap::Watched;
use crate::terminal::{Program, Terminal, Tmux, WorkerPane};
use crate::worker::{Started, Stopped};

pub use crate::terminal::CLONE_MARK;

/// The operations that only the broker of a lead runs
/// (01M4DDWPC693RNWHY7P7XBZ9TB).
pub const LEAD_OPS: [&str; 6] = [
    "worker-panes",
    "tail-pane",
    "workers-start",
    "workers-stop",
    "workers-reap",
    "oom-journal",
];

/// The operations of the own pane, which the broker of each role runs
/// (01M4DVW24ESG6XCBNMFV7T9Z4E).
pub const OWN_OPS: [&str; 4] = ["pane-id", "pane-screen", "pane-type", "end-over-limit"];

/// True when `op` is an operation of this module.
pub fn is_op(op: &str) -> bool {
    LEAD_OPS.contains(&op) || OWN_OPS.contains(&op)
}

/// The most workers that one `workers-start` asks for.
pub const MAX_START: u16 = 64;

/// Why the broker refuses the operation `op` with `args`, or `None`. It
/// checks only the form of the arguments; the broker checks a pane
/// against its clone when it runs the operation.
pub fn refusal(op: &str, args: &[OsString]) -> Option<String> {
    if !is_op(op) {
        return Some(crate::text::broker_no_op(op));
    }
    let max = match op {
        "workers-start" | "workers-stop" | "workers-reap" | "pane-type" => 1,
        _ => 0,
    };
    let needs = matches!(op, "workers-start" | "pane-type");
    if args.len() > max || (needs && args.is_empty()) {
        return Some(crate::text::door_args(op));
    }
    if op == "workers-start" && count(&args[0]).is_none() {
        return Some(crate::text::door_count(&args[0].to_string_lossy()));
    }
    None
}

/// The count of a `workers-start`: 1 to [`MAX_START`].
fn count(arg: &std::ffi::OsStr) -> Option<u16> {
    let n: u16 = arg.to_str()?.parse().ok()?;
    (1..=MAX_START).contains(&n).then_some(n)
}

/// The tmux of the workers of this machine, or the broker of the lead.
#[derive(Debug, Clone)]
pub enum Door {
    /// A session with no broker: tmux itself.
    Tmux(Tmux),
    /// A session in its sandbox: the broker, by its file descriptor.
    Broker(RawFd),
}

impl Door {
    /// The door of this session: its broker, else the tmux of its pane,
    /// else `None` (outside tmux).
    pub fn of_session() -> Option<Self> {
        match crate::broker::here() {
            Some(fd) => Some(Door::Broker(fd)),
            None => Tmux::from_env().map(Door::Tmux),
        }
    }

    /// The door of a command of the machine, for example `riff
    /// workers stop`: the broker of this session, else the tmux of the
    /// machine ([`Tmux::machine`]).
    pub fn of_machine() -> Self {
        crate::broker::here().map_or_else(|| Door::Tmux(Tmux::machine()), Door::Broker)
    }

    /// The worker panes of this machine, with the systemd scope of
    /// each.
    pub fn watched(&self) -> Result<Vec<Watched>> {
        match self {
            Door::Tmux(tmux) => Ok(watched(tmux)?),
            Door::Broker(fd) => call(*fd, "worker-panes", &[]),
        }
    }

    /// The worker panes of this machine.
    pub fn worker_panes(&self) -> Result<Vec<WorkerPane>> {
        match self {
            Door::Tmux(tmux) => tmux.worker_panes(),
            Door::Broker(_) => Ok(self.watched()?.into_iter().map(|w| w.pane).collect()),
        }
    }

    /// Adds `tail` beside the pane of the session. True when it added
    /// one. The broker makes its own `riff tail` program.
    pub fn tail(&self, tail: &Program) -> Result<bool> {
        match self {
            Door::Tmux(tmux) => tmux.beside(crate::terminal::TAIL, tail),
            Door::Broker(fd) => call(*fd, "tail-pane", &[]),
        }
    }

    /// [`crate::worker::start`] of `count` workers with `claude`, the
    /// server `server`, in the main worktree of `dir`. The broker uses
    /// its own program, server and clone.
    pub fn start(
        &self,
        count: u16,
        claude: &Path,
        server: &str,
        dir: &Path,
    ) -> Result<std::result::Result<Started, String>> {
        match self {
            Door::Tmux(tmux) => crate::worker::start(tmux, count, claude, server, dir),
            Door::Broker(fd) => call(*fd, "workers-start", &[count.to_string().into()]),
        }
    }

    /// [`crate::worker::stop`] of each worker, or of the one in `pane`.
    pub async fn stop(&self, pane: Option<&str>, server: &str) -> Result<Stopped> {
        match self {
            Door::Tmux(tmux) => crate::worker::stop(tmux, pane, server).await,
            Door::Broker(fd) => {
                let (fd, args) = (*fd, pane_arg(pane));
                tokio::task::spawn_blocking(move || call(fd, "workers-stop", &args)).await?
            }
        }
    }

    /// [`crate::worker::reap`] of each worker, or of the one in `pane`,
    /// with the local dir `dir`.
    pub fn reap(&self, pane: Option<&str>, dir: &Path) -> Result<Vec<String>> {
        match self {
            Door::Tmux(tmux) => crate::worker::reap(tmux, pane, dir),
            Door::Broker(fd) => call(*fd, "workers-reap", &pane_arg(pane)),
        }
    }

    /// The lines of `systemd-oomd` ([`crate::reap::journal`]).
    pub fn journal(&self) -> Option<String> {
        match self {
            Door::Tmux(_) => crate::reap::journal(),
            Door::Broker(fd) => call(*fd, "oom-journal", &[]).ok().flatten(),
        }
    }
}

/// A door is a terminal. Through the broker, it adds only the `riff
/// tail` pane and lists the worker panes: a start, a stop and a reap go
/// through [`Door::start`], [`Door::stop`] and [`Door::reap`].
impl Terminal for Door {
    fn beside(&self, mark: &str, program: &Program) -> Result<bool> {
        match self {
            Door::Tmux(tmux) => tmux.beside(mark, program),
            Door::Broker(_) if mark == crate::terminal::TAIL => self.tail(program),
            Door::Broker(_) => bail!("{}", crate::text::DOOR_NO_LAYOUT),
        }
    }

    fn workers(&self, programs: &[Program]) -> Result<(String, Vec<WorkerPane>)> {
        match self {
            Door::Tmux(tmux) => tmux.workers(programs),
            Door::Broker(_) => bail!("{}", crate::text::DOOR_NO_LAYOUT),
        }
    }

    fn worker_panes(&self) -> Result<Vec<WorkerPane>> {
        Door::worker_panes(self)
    }

    fn kill(&self, pane: &str) -> Result<()> {
        match self {
            Door::Tmux(tmux) => tmux.kill(pane),
            Door::Broker(_) => bail!("{}", crate::text::DOOR_NO_LAYOUT),
        }
    }

    fn screen(&self, pane: &str) -> Result<String> {
        match self {
            Door::Tmux(tmux) => tmux.screen(pane),
            Door::Broker(_) => bail!("{}", crate::text::DOOR_NO_LAYOUT),
        }
    }

    fn type_line(&self, pane: &str, text: &str) -> Result<()> {
        match self {
            Door::Tmux(tmux) => tmux.type_line(pane, text),
            Door::Broker(_) => bail!("{}", crate::text::DOOR_NO_LAYOUT),
        }
    }
}

/// The pane of this session: a tmux pane, or the pane of the broker of
/// the lead in its sandbox.
#[derive(Debug, Clone)]
pub enum OwnPane {
    /// The pane, in the tmux of the machine.
    Tmux(Tmux, String),
    /// The pane of the broker.
    Broker(RawFd),
}

impl OwnPane {
    /// The pane `pane` of this session, else the pane of its broker,
    /// else `None`.
    pub fn of(pane: Option<&str>) -> Option<Self> {
        match (pane, crate::broker::here()) {
            (Some(pane), _) => Some(OwnPane::Tmux(Tmux::machine(), pane.to_owned())),
            (None, Some(fd)) => Some(OwnPane::Broker(fd)),
            (None, None) => None,
        }
    }

    /// The text that the pane shows now.
    pub fn screen(&self) -> Result<String> {
        match self {
            OwnPane::Tmux(tmux, pane) => tmux.screen(pane),
            OwnPane::Broker(fd) => call(*fd, "pane-screen", &[]),
        }
    }

    /// Types `text` into the pane, then Enter.
    pub fn type_line(&self, text: &str) -> Result<()> {
        match self {
            OwnPane::Tmux(tmux, pane) => tmux.type_line(pane, text),
            OwnPane::Broker(fd) => call(*fd, "pane-type", &[text.into()]),
        }
    }

    /// The name of the pane, for example `%5`.
    pub fn id(&self) -> Result<String> {
        match self {
            OwnPane::Tmux(_, pane) => Ok(pane.clone()),
            OwnPane::Broker(fd) => call(*fd, "pane-id", &[]),
        }
    }
}

fn pane_arg(pane: Option<&str>) -> Vec<OsString> {
    pane.map(OsString::from).into_iter().collect()
}

/// The worker panes of `tmux`, with the systemd scope of each.
fn watched(tmux: &Tmux) -> Result<Vec<Watched>> {
    Ok(tmux
        .worker_panes()?
        .into_iter()
        .map(|pane| Watched {
            scope: tmux.pane_pid(&pane.pane).and_then(crate::reap::scope_of),
            pane,
        })
        .collect())
}

/// Asks the broker `broker` for the operation `op` with `args`, and
/// reads the JSON of the reply from a pipe.
pub fn call<T: DeserializeOwned>(broker: RawFd, op: &str, args: &[OsString]) -> Result<T> {
    let (read, write) = nix::unistd::pipe().context("cannot make a pipe for the broker")?;
    let reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        std::fs::File::from(read)
            .read_to_end(&mut text)
            .map(|_| text)
    });
    let request = Request {
        op: op.into(),
        args: args.to_vec(),
        cwd: std::env::current_dir()?,
        env: vec![],
    };
    let reply = crate::broker::ask_with(broker, &request, [0, write.as_raw_fd(), 2]);
    drop(write);
    let text = reader
        .join()
        .map_err(|_| anyhow::anyhow!("the read of the broker stopped"))??;
    match reply? {
        Reply::Code(0) => serde_json::from_slice(&text)
            .with_context(|| format!("the broker gave no reply that riff can read for {op}")),
        Reply::Code(code) => bail!("the broker ended {op} with the exit code {code}"),
        Reply::Refused(why) => bail!("{}", crate::text::broker_refused(&why)),
    }
}

/// The facts of a broker.
#[derive(Debug, Clone)]
pub struct Here {
    /// The riff server.
    pub server: String,
    /// The tmux of the session: `None` outside tmux.
    pub tmux: Option<Tmux>,
    /// The facts of the broker of a lead: `None` for each other role.
    pub lead: Option<Lead>,
}

/// The facts of the broker of a lead.
#[derive(Debug, Clone)]
pub struct Lead {
    /// The main clone of the lead.
    pub clone: PathBuf,
    /// The riff of the `riff tail` pane.
    pub riff: PathBuf,
}

impl Here {
    /// The broker of a session with the riff server `server`, and the
    /// tmux of its environment. With `lead`, it is the broker of the
    /// lead of `clone`.
    pub fn of(lead: bool, clone: &Path, server: &str) -> Result<Self> {
        let lead = if lead {
            Some(Lead {
                clone: crate::confine::resolve(clone),
                riff: crate::binary::this_on_disk()?,
            })
        } else {
            None
        };
        Ok(Here {
            server: server.to_owned(),
            tmux: Tmux::from_env(),
            lead,
        })
    }
}

/// Runs the operation of `request` for the broker `here`, and writes
/// the JSON of its result to `stdout`. The broker of a worker or a
/// verifier refuses each operation of the lead
/// (01M4DDWPC693RNWHY7P7XBZ9TB).
pub fn answer(here: &Here, request: &Request, stdout: Option<OwnedFd>) -> Reply {
    if LEAD_OPS.contains(&request.op.as_str()) && here.lead.is_none() {
        return Reply::Refused(crate::text::door_not_lead(&request.op));
    }
    if let Some(why) = refusal(&request.op, &request.args) {
        return Reply::Refused(why);
    }
    let Some(stdout) = stdout else {
        return Reply::Refused("a request needs stdin, stdout and stderr".into());
    };
    match run(here, &request.op, request.args.first()) {
        Ok(json) => {
            let mut out = std::fs::File::from(stdout);
            match out.write_all(json.as_bytes()) {
                Ok(()) => Reply::Code(0),
                Err(e) => Reply::Refused(format!("cannot write the reply: {e}")),
            }
        }
        Err(e) => Reply::Refused(format!("{e:#}")),
    }
}

/// Runs the operation `op` with the argument `arg` for the broker
/// `here`, and gives the JSON of its result.
fn run(here: &Here, op: &str, arg: Option<&OsString>) -> Result<String> {
    let tmux = || {
        here.tmux
            .as_ref()
            .context(crate::text::DOOR_NO_TMUX.to_owned())
    };
    match op {
        "pane-id" => return json(&tmux()?.pane()),
        "pane-screen" => {
            let tmux = tmux()?;
            return json(&tmux.screen(tmux.pane())?);
        }
        "pane-type" => {
            let tmux = tmux()?;
            let text = arg.context("no text")?.to_string_lossy();
            tmux.type_line(tmux.pane(), &text)?;
            return json(&());
        }
        "end-over-limit" => {
            let pane = tmux()?.pane().to_owned();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            return json(&runtime.block_on(end_own(&here.server, &pane))?);
        }
        _ => {}
    }
    let lead = here
        .lead
        .as_ref()
        .with_context(|| crate::text::door_not_lead(op))?;
    let pane = arg.map(|a| a.to_string_lossy().into_owned());
    let of_clone = || {
        tmux().map(|tmux| OfClone {
            tmux,
            clone: &lead.clone,
        })
    };
    match op {
        "worker-panes" => json(&watched(tmux()?)?),
        "tail-pane" => {
            let tail = Program::tail(&lead.riff, &lead.clone, &here.server);
            json(&tmux()?.beside(crate::terminal::TAIL, &tail)?)
        }
        "workers-start" => {
            let n = arg.and_then(|a| count(a)).context("no count")?;
            let claude = Path::new("claude");
            json(&crate::worker::start(
                tmux()?,
                n,
                claude,
                &here.server,
                &lead.clone,
            )?)
        }
        "workers-stop" => {
            let of_clone = of_clone()?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let stopped = runtime.block_on(crate::worker::stop(
                &of_clone,
                pane.as_deref(),
                &here.server,
            ))?;
            json(&stopped)
        }
        "workers-reap" => {
            let dir = crate::local::dir().context("no HOME: riff has no local dir")?;
            json(&crate::worker::reap(&of_clone()?, pane.as_deref(), &dir)?)
        }
        "oom-journal" => json(&crate::reap::journal()),
        _ => bail!("{}", crate::text::broker_no_op(op)),
    }
}

/// The end of the worker of this broker in `pane` when more workers run
/// than the limit ([`crate::next::end_over_limit`],
/// 01M4DVW290H2AQF6EQFHTY1EE1). The session is the `RIFF_SESSION` of
/// the broker, with the riff server `server`.
async fn end_own(server: &str, pane: &str) -> Result<bool> {
    let session = crate::identity::session_id().context(crate::text::DOOR_NO_SESSION)?;
    let here = crate::identity::place(&crate::identity::working_dir()?)?;
    let api = crate::api::Api::new(server);
    let me = crate::identity::agent(&here, &session, api.base())?;
    let api = api.signed_in(Some(&session))?;
    crate::next::end_over_limit_in(&api, &me, &Tmux::machine(), pane).await
}

fn json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

/// The workers of one clone in `tmux` (01M4DDWPEGBAXKTS0X3THFB8VZ): each
/// worker pane with the [`CLONE_MARK`] of `clone`. It kills only such a
/// pane. It lays out no pane: a start counts each worker of the
/// machine, so it uses the tmux of the machine.
pub struct OfClone<'a> {
    /// The tmux of the machine.
    pub tmux: &'a dyn Marked,
    /// The main clone, resolved.
    pub clone: &'a Path,
}

/// A terminal that knows the [`CLONE_MARK`] of a pane.
pub trait Marked: Terminal {
    /// The main clone of the worker in `pane`, or `None`.
    fn clone_of(&self, pane: &str) -> Option<PathBuf>;
}

impl Marked for Tmux {
    fn clone_of(&self, pane: &str) -> Option<PathBuf> {
        self.pane_clone(pane)
    }
}

impl Terminal for OfClone<'_> {
    fn beside(&self, _: &str, _: &Program) -> Result<bool> {
        bail!("{}", crate::text::DOOR_NO_LAYOUT)
    }

    fn workers(&self, _: &[Program]) -> Result<(String, Vec<WorkerPane>)> {
        bail!("{}", crate::text::DOOR_NO_LAYOUT)
    }

    fn worker_panes(&self) -> Result<Vec<WorkerPane>> {
        let mut panes = self.tmux.worker_panes()?;
        panes.retain(|w| {
            self.tmux
                .clone_of(&w.pane)
                .is_some_and(|c| crate::confine::resolve(&c) == self.clone)
        });
        Ok(panes)
    }

    fn kill(&self, pane: &str) -> Result<()> {
        if !self.worker_panes()?.iter().any(|w| w.pane == pane) {
            bail!("{}", crate::text::door_other_clone(pane, self.clone));
        }
        self.tmux.kill(pane)
    }

    fn screen(&self, _: &str) -> Result<String> {
        bail!("{}", crate::text::DOOR_NO_LAYOUT)
    }

    fn type_line(&self, _: &str, _: &str) -> Result<()> {
        bail!("{}", crate::text::DOOR_NO_LAYOUT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A terminal with worker panes, each with its clone mark.
    #[derive(Default)]
    struct Fake {
        panes: Vec<(WorkerPane, Option<PathBuf>)>,
        killed: RefCell<Vec<String>>,
    }

    impl Fake {
        fn with(panes: &[(&str, &str, Option<&str>)]) -> Self {
            Fake {
                panes: panes
                    .iter()
                    .map(|(p, s, c)| {
                        let pane = WorkerPane {
                            pane: (*p).into(),
                            session: (*s).into(),
                        };
                        (pane, c.map(PathBuf::from))
                    })
                    .collect(),
                killed: RefCell::default(),
            }
        }
    }

    impl Terminal for Fake {
        fn beside(&self, _: &str, _: &Program) -> Result<bool> {
            unreachable!()
        }
        fn workers(&self, _: &[Program]) -> Result<(String, Vec<WorkerPane>)> {
            unreachable!()
        }
        fn worker_panes(&self) -> Result<Vec<WorkerPane>> {
            Ok(self.panes.iter().map(|(p, _)| p.clone()).collect())
        }
        fn kill(&self, pane: &str) -> Result<()> {
            self.killed.borrow_mut().push(pane.into());
            Ok(())
        }
        fn screen(&self, _: &str) -> Result<String> {
            unreachable!()
        }
        fn type_line(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
    }

    impl Marked for Fake {
        fn clone_of(&self, pane: &str) -> Option<PathBuf> {
            self.panes
                .iter()
                .find(|(p, _)| p.pane == pane)
                .and_then(|(_, c)| c.clone())
        }
    }

    /// 01M4DDWPEGBAXKTS0X3THFB8VZ: the broker of a lead sees and kills
    /// only the workers of its own clone. A pane of another clone, and
    /// a pane with no mark, are refused, also by name.
    #[test]
    fn the_broker_of_a_lead_acts_only_on_the_workers_of_its_clone() {
        let fake = Fake::with(&[
            ("%1", "s-mine", Some("/nowhere/app")),
            ("%2", "s-other", Some("/nowhere/other")),
            ("%3", "s-old", None),
        ]);
        let mine = OfClone {
            tmux: &fake,
            clone: Path::new("/nowhere/app"),
        };
        let panes = mine.worker_panes().unwrap();
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].pane, "%1");
        for other in ["%2", "%3"] {
            let why = format!("{:#}", mine.kill(other).unwrap_err());
            assert!(why.contains(other), "{why}");
        }
        mine.kill("%1").unwrap();
        assert_eq!(*fake.killed.borrow(), ["%1"]);
        assert!(
            mine.beside("tail", &Program::tail("/r".as_ref(), "/".as_ref(), "s"))
                .is_err()
        );
    }

    /// 01M4DDWPEGBAXKTS0X3THFB8VZ: a stop of a worker of another clone
    /// stops nothing, also when the lead names its pane or its session.
    #[tokio::test]
    async fn a_stop_of_a_worker_of_another_clone_stops_nothing() {
        let fake = Fake::with(&[
            ("%1", "s-mine-1234", Some("/nowhere/app")),
            ("%2", "s-other-5678", Some("/nowhere/other")),
        ]);
        let mine = OfClone {
            tmux: &fake,
            clone: Path::new("/nowhere/app"),
        };
        for other in ["%2", "s-other"] {
            let e = crate::worker::stop(&mine, Some(other), "http://127.0.0.1:9")
                .await
                .unwrap_err();
            assert!(format!("{e:#}").contains("no worker runs"), "{e:#}");
        }
        assert!(fake.killed.borrow().is_empty());
    }

    fn request(op: &str, args: &[&str]) -> Request {
        Request {
            op: op.into(),
            args: args.iter().map(Into::into).collect(),
            cwd: "/".into(),
            env: vec![],
        }
    }

    /// The broker of a worker in the pane `pane` of the fake tmux `bin`.
    fn worker(bin: Option<&Path>, pane: &str) -> Here {
        Here {
            server: "http://127.0.0.1:9".into(),
            tmux: bin.map(|bin| Tmux::new(bin, pane)),
            lead: None,
        }
    }

    /// 01M4DDWPC693RNWHY7P7XBZ9TB: the broker of another role refuses
    /// each operation of the lead, and a lead broker refuses a bad
    /// argument before it runs anything.
    #[test]
    fn only_the_broker_of_a_lead_runs_the_operations_of_the_lead() {
        let worker = worker(None, "%5");
        for op in LEAD_OPS {
            let reply = answer(&worker, &request(op, &[]), None);
            assert!(
                matches!(&reply, Reply::Refused(why) if why.contains("only for the lead")),
                "{op}: {reply:?}"
            );
        }
        let lead = Here {
            lead: Some(Lead {
                clone: "/nowhere/app".into(),
                riff: "/nowhere/riff".into(),
            }),
            ..worker
        };
        for (op, args) in [
            ("workers-start", &["99999"][..]),
            ("workers-start", &[][..]),
            ("tail-pane", &["sh", "-c"][..]),
            ("shell", &[][..]),
        ] {
            let reply = answer(&lead, &request(op, args), None);
            assert!(matches!(reply, Reply::Refused(_)), "{op}: {reply:?}");
        }
        // With no tmux, the broker says so.
        let (_read, write) = nix::unistd::pipe().unwrap();
        let reply = answer(&lead, &request("worker-panes", &[]), Some(write));
        assert!(
            matches!(&reply, Reply::Refused(why) if why.contains("tmux")),
            "{reply:?}"
        );
    }

    /// Asks `here` for `op` with `args`, and gives the reply and the
    /// text on stdout.
    fn ask(here: &Here, op: &str, args: &[&str]) -> (Reply, String) {
        let (read, write) = nix::unistd::pipe().unwrap();
        let reply = answer(here, &request(op, args), Some(write));
        let mut text = String::new();
        std::fs::File::from(read).read_to_string(&mut text).unwrap();
        (reply, text)
    }

    /// 01M4DVW24ESG6XCBNMFV7T9Z4E: the broker of a worker runs the
    /// operations of the own pane, only on the pane of its session. A
    /// request that names a pane, also its own pane, is refused before
    /// tmux runs.
    #[test]
    fn the_operations_of_the_own_pane_act_only_on_the_pane_of_the_session() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("tmux");
        let log = dir.path().join("log");
        std::fs::write(
            &bin,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n\
                 [ \"$1\" = capture-pane ] && echo 'the screen'\nexit 0\n",
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let worker = worker(Some(&bin), "%5");
        for (op, args) in [
            ("pane-id", &["%9"][..]),
            ("pane-screen", &["%9"][..]),
            ("pane-screen", &["%5"][..]),
            ("pane-type", &["%9", "/clear"][..]),
            ("end-over-limit", &["%9"][..]),
        ] {
            let (reply, _) = ask(&worker, op, args);
            assert!(
                matches!(&reply, Reply::Refused(why) if why.contains("other arguments")),
                "{op} {args:?}: {reply:?}"
            );
        }
        assert!(!log.exists(), "tmux ran for a refused request");

        assert_eq!(
            ask(&worker, "pane-id", &[]),
            (Reply::Code(0), "\"%5\"".into())
        );
        let (reply, screen) = ask(&worker, "pane-screen", &[]);
        assert_eq!(reply, Reply::Code(0));
        assert!(screen.contains("the screen"), "{screen}");
        assert_eq!(
            ask(&worker, "pane-type", &["/clear"]),
            (Reply::Code(0), "null".into())
        );
        let log = std::fs::read_to_string(&log).unwrap();
        let targets: Vec<&str> = log
            .lines()
            .filter_map(|l| l.split(" -t ").nth(1))
            .map(|rest| rest.split(' ').next().unwrap())
            .collect();
        assert!(!targets.is_empty(), "{log}");
        assert!(targets.iter().all(|t| *t == "%5"), "{log}");
        assert!(log.contains("send-keys -t %5 -l /clear"), "{log}");

        // Outside tmux, the broker of a worker says so.
        let outside = Here {
            tmux: None,
            ..worker
        };
        let (reply, _) = ask(&outside, "pane-type", &["/clear"]);
        assert!(
            matches!(&reply, Reply::Refused(why) if why.contains("tmux")),
            "{reply:?}"
        );
    }
}
