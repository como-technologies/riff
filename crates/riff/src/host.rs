//! Workers on another machine of the user.
//!
//! # Design
//!
//! `riff workers host` offers the workers of a machine to the lead of
//! its user (01M3N7AK8TVYV8S0WR3RP0TN8X). The person runs it in tmux, in
//! the main clone, and leaves it. It is a riff session of its own with a
//! watch. It never calls register, so it never becomes the lead. Its
//! status tells the lead its limit and its workers ([`HostStatus`]).
//!
//! The lead asks it with a signed direct message
//! (01M3N7AKB3KXS2XYK0309C4M18): `riff workers start N --host HOST`
//! sends [`Request::Start`], `riff workers stop --host HOST` sends
//! [`Request::Stop`], and `riff workers stop PANE --host HOST` sends
//! [`Request::StopOne`] (01M3Q5A0Z5DK0YV1MWTM4AQD5Z). The host starts and stops workers with the same
//! code as `riff workers start` and `riff workers stop` on its own
//! machine ([`crate::worker::start`], [`crate::worker::stop`]), so its
//! own limit counts. It acts only on a verified request from the lead of
//! its user in its repository ([`judge`], 01M3N7AKDE7DEA6NXS9ZMECRMH).
//! It replies to the sender with a note: the result, or the refusal
//! (01M3Q5QEE4MQNCRKVJK3D54G9Z). A note does not wake the lead.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant L as lead on thelio
//!     participant S as riff-server
//!     participant H as riff workers host on pangolin
//!     participant T as tmux on pangolin
//!     H->>S: watch, status "workers host: limit 2, no workers"
//!     L->>S: riff workers start 2 --host pangolin
//!     S->>H: wake: direct message "workers start 2"
//!     H->>H: judge: verified, lead of its user
//!     H->>T: 2 worker panes
//!     H->>S: a note to the lead: panes and sessions
//!     H->>S: status "workers host: limit 2, workers: %3 1a2b3c4d, %4 5e6f7a8b"
//!     L->>S: riff workers: who
//!     S-->>L: the host and its workers
//! ```
//!
//! `riff workers` of the lead lists each live host of its user with its
//! workers (01M3N7AKFPX3ZGQARSG2V64GBD).
//!
//! # Start and stop
//!
//! The host takes Ctrl-C, SIGTERM and SIGHUP first, before any other
//! step, in a task of its own (`stop_on_signal`). A step of the host
//! can block its own task: a call to the OS keyring, or `tmux`. The
//! signal task still runs, ends the session of the host, and ends the
//! process (01M3NBV405PVYHKTMQ5VN87FYN). A keyring call waits at most
//! [`crate::secrets::KEYRING_WAIT`].
//!
//! Then it prints one line with its host, its limit, the lead that it
//! serves and the repository (01M3NBV4294DS3WZFEKR7M3PNF). It holds the
//! lock `host-USER-REPO` of [`crate::local`] while it runs, so a second
//! host of the same user and repository on the machine refuses to start
//! (01M3NBV44GKAX6WS391PN6R72W). It reads no input, and its children get
//! no input from it (01M3NBV46R0VB0JQNQ1ERG16J6).
//!
//! # A new binary
//!
//! When a new `riff` is on disk, the host runs it in its place, as
//! `riff watch` does ([`crate::binary`], 01M3Q55KJ8BKMPE9RADB63X8SP). It
//! does so only between two requests, never while it starts or stops
//! workers. It gives the new process its session in the hidden option
//! `--session`, so the lead sees the same host. The workers go on: the
//! new host finds them by the marks of their panes. The lock of the host
//! is on a file with close-on-exec, so the new process takes it again.
//! Each worker pane runs the `riff` on disk, never the path of a binary
//! that a new one replaced (01M3Q55KMQSSJVQEN86XFB8PSG).

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{Result, bail};
use futures::StreamExt;
use riff_core::name::SessionUri;
use riff_core::selector::Selector;
use riff_core::wire::{Kind, SessionInfo, Status};

use crate::api::{Api, Checked, Reconnect, follow};
use crate::binary::{Follow, with_last};
use crate::machine::Machine;
use crate::terminal::{self, Terminal, Tmux, WorkerPane};
use crate::{identity, local, settings, text, worker};

/// The start of the status of a host.
pub const MARK: &str = "workers host";

/// How often a host sets its status again, so that it shows the workers
/// that ended.
pub const REFRESH: Duration = Duration::from_secs(30);

/// The time between two tries to connect the watch of a host.
const RETRY: Duration = Duration::from_secs(5);

/// The longest time that a host waits for the end of its session after
/// a signal.
pub const END_WAIT: Duration = Duration::from_secs(1);

/// What the status of a host tells: its limit, the numbers of its
/// machine (01M3Q5QE4SQ8VYN2PSF42KB3QJ), and the pane and short session
/// ID of each worker.
///
/// ```
/// use riff::host::HostStatus;
/// use riff::machine::Machine;
///
/// let none = HostStatus { limit: 2, machine: None, workers: vec![] };
/// assert_eq!(none.line(), "workers host: limit 2, no workers");
/// let two = HostStatus {
///     limit: 3,
///     machine: Some(Machine { cores: 16, mhz: 4500, mem_gb: 32, load: 1.5 }),
///     workers: vec![("%3".into(), "1a2b3c4d".into()), ("%4".into(), "5e6f7a8b".into())],
/// };
/// assert_eq!(
///     two.line(),
///     "workers host: limit 3, cpu 16x4500MHz, mem 32GB, load 1.50, workers: %3 1a2b3c4d, %4 5e6f7a8b",
/// );
/// assert_eq!(HostStatus::parse(&two.line()), Some(two));
/// assert_eq!(HostStatus::parse(&none.line()), Some(none));
/// assert_eq!(HostStatus::parse("idle: waits for work"), None);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct HostStatus {
    pub limit: u16,
    /// The numbers of the machine. `None` from a host that does not
    /// tell them.
    pub machine: Option<Machine>,
    /// The pane and the first 8 characters of the session ID of each
    /// worker.
    pub workers: Vec<(String, String)>,
}

impl HostStatus {
    /// The status of a host with `panes` on `machine`.
    pub fn of(limit: u16, machine: Machine, panes: &[WorkerPane]) -> Self {
        HostStatus {
            limit,
            machine: Some(machine),
            workers: panes
                .iter()
                .map(|w| (w.pane.clone(), w.session.chars().take(8).collect()))
                .collect(),
        }
    }

    /// The status line. It fits in a status for 10 workers.
    pub fn line(&self) -> String {
        let machine = self.machine.map(|m| format!("{m}, ")).unwrap_or_default();
        if self.workers.is_empty() {
            return format!("{MARK}: limit {}, {machine}no workers", self.limit);
        }
        let workers: Vec<String> = self
            .workers
            .iter()
            .map(|(pane, short)| format!("{pane} {short}"))
            .collect();
        format!(
            "{MARK}: limit {}, {machine}workers: {}",
            self.limit,
            workers.join(", ")
        )
    }

    /// The host status in a status step, or `None` for another status.
    pub fn parse(step: &str) -> Option<Self> {
        let rest = step.strip_prefix(MARK)?.strip_prefix(": limit ")?;
        let (limit, mut rest) = rest.split_once(", ")?;
        let limit = limit.parse().ok()?;
        let mut machine = None;
        if rest.starts_with("cpu ") {
            let end = rest
                .find(", no workers")
                .or_else(|| rest.find(", workers: "))?;
            machine = Some(Machine::parse(&rest[..end])?);
            rest = &rest[end + 2..];
        }
        if rest == "no workers" {
            return Some(HostStatus {
                limit,
                machine,
                workers: Vec::new(),
            });
        }
        let workers = rest
            .strip_prefix("workers: ")?
            .split(", ")
            .map(|w| {
                w.split_once(' ')
                    .map(|(pane, short)| (pane.to_owned(), short.to_owned()))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(HostStatus {
            limit,
            machine,
            workers,
        })
    }
}

/// A request of the lead to a host.
///
/// ```
/// use riff::host::Request;
///
/// assert_eq!("workers start 2".parse(), Ok(Request::Start(2)));
/// assert_eq!("workers stop".parse(), Ok(Request::Stop));
/// assert_eq!("workers stop %3".parse(), Ok(Request::StopOne("%3".into())));
/// assert_eq!(Request::Start(3).to_string(), "workers start 3");
/// assert_eq!(Request::Stop.to_string(), "workers stop");
/// assert_eq!(Request::StopOne("1a2b3c4d".into()).to_string(), "workers stop 1a2b3c4d");
/// assert!("workers start 0".parse::<Request>().is_err());
/// assert!("request: claim issue-12".parse::<Request>().is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Start this many workers.
    Start(u16),
    /// Stop each worker of the host.
    Stop,
    /// Stop the worker in this pane, or with this session ID or its
    /// start (01M3Q5A0Z5DK0YV1MWTM4AQD5Z).
    StopOne(String),
}

impl FromStr for Request {
    type Err = ();

    fn from_str(body: &str) -> Result<Self, ()> {
        match body.split_whitespace().collect::<Vec<_>>()[..] {
            ["workers", "stop"] => Ok(Request::Stop),
            ["workers", "stop", one] => Ok(Request::StopOne(one.to_owned())),
            ["workers", "start", n] => match n.parse() {
                Ok(n) if n > 0 => Ok(Request::Start(n)),
                _ => Err(()),
            },
            _ => Err(()),
        }
    }
}

impl fmt::Display for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Request::Start(n) => write!(f, "workers start {n}"),
            Request::Stop => write!(f, "workers stop"),
            Request::StopOne(one) => write!(f, "workers stop {one}"),
        }
    }
}

/// Decides on one message to the host `me`: `None` when it is no
/// request, the request when the host acts on it, or the refusal. The
/// host acts only on a verified message from the lead of its user in
/// its repository (01M3N7AKDE7DEA6NXS9ZMECRMH).
///
/// ```
/// use riff::api::Checked;
/// use riff::host::{Request, judge};
/// use riff_core::wire::Message;
///
/// let host = "riff://mike@pangolin/como-technologies/riff?session=h1".parse()?;
/// let from = |uri: &str, body: &str, verified: bool| Checked {
///     message: Message {
///         seq: 1,
///         from: uri.parse().unwrap(),
///         to: vec![],
///         body: body.into(),
///         at_ms: 0,
///         kind: Default::default(),
///         sig: None,
///         payload: None,
///     },
///     verified,
/// };
/// let lead = "riff://mike@thelio/como-technologies/riff?session=l1&lead=true";
/// assert_eq!(judge(&from(lead, "workers start 2", true), &host), Some(Ok(Request::Start(2))));
/// assert!(judge(&from(lead, "workers start 2", false), &host).unwrap().is_err());
/// let not_lead = "riff://mike@thelio/como-technologies/riff?session=w1";
/// assert!(judge(&from(not_lead, "workers stop", true), &host).unwrap().is_err());
/// let other_user = "riff://brett@kadomony/como-technologies/riff?session=b1&lead=true";
/// assert!(judge(&from(other_user, "workers stop", true), &host).unwrap().is_err());
/// assert_eq!(judge(&from(lead, "hello", true), &host), None);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn judge(checked: &Checked, me: &SessionUri) -> Option<std::result::Result<Request, String>> {
    let request: Request = checked.message.body.parse().ok()?;
    let from = &checked.message.from;
    if !checked.verified {
        return Some(Err(text::host_refused_not_verified(&request)));
    }
    let lead = from.lead()
        && from.who().user() == me.who().user()
        && from.place().repo() == me.place().repo();
    if !lead {
        return Some(Err(text::host_refused_not_the_lead(&request, me)));
    }
    Some(Ok(request))
}

/// Each live host of `user` in `sessions`, with its status.
pub fn hosts<'a>(sessions: &'a [SessionInfo], user: &str) -> Vec<(&'a SessionInfo, HostStatus)> {
    sessions
        .iter()
        .filter(|s| s.live && s.uri.who().user() == user)
        .filter_map(|s| {
            let status = HostStatus::parse(&s.status.as_ref()?.status.step)?;
            Some((s, status))
        })
        .collect()
}

/// The worker panes of a host, with the full session ID that `sessions`
/// has for each short ID.
pub fn panes(status: &HostStatus, sessions: &[SessionInfo]) -> Vec<WorkerPane> {
    status
        .workers
        .iter()
        .map(|(pane, short)| {
            let session = sessions
                .iter()
                .filter_map(|s| s.uri.who().session())
                .find(|id| id.starts_with(short.as_str()))
                .unwrap_or(short);
            WorkerPane {
                pane: pane.clone(),
                session: session.to_owned(),
            }
        })
        .collect()
}

/// A running host.
struct Host {
    api: Api,
    me: SessionUri,
    tmux: Tmux,
    claude: PathBuf,
    main: PathBuf,
    server: String,
}

/// The hidden option that gives a new binary the session of the host.
pub const SESSION_ARG: &str = "--session";

/// Runs `riff workers host` in `dir` until Ctrl-C, SIGTERM or SIGHUP
/// (01M3N7AK8TVYV8S0WR3RP0TN8X). It takes the session `resume` of the
/// host that ran it after an update, else a new one. On a new binary it
/// runs it between two requests (01M3Q55KJ8BKMPE9RADB63X8SP).
pub async fn serve(dir: &Path, claude: &Path, server: &str, resume: Option<&str>) -> Result<()> {
    let binary = Follow::this();
    let session = Arc::new(OnceLock::new());
    stop_on_signal(session.clone())?;
    let Some(tmux) = Tmux::from_env() else {
        bail!(text::HOST_NEEDS_TMUX);
    };
    let limit = settings::workers_limit(&settings::path()?)?;
    if limit == 0 {
        bail!(text::HOST_NEEDS_A_LIMIT);
    }
    let Some(main) = identity::main_worktree(dir) else {
        bail!("run riff workers host in the main clone of a repository");
    };
    let place = identity::place(&main)?;
    let api = Api::new(server);
    let id = resume.map_or_else(terminal::new_session_id, str::to_owned);
    let me = identity::agent(&place, &id, api.base())?;
    let _lock = match local::dir() {
        Some(dir) => {
            let user = me.who().user();
            match local::host(&dir, user, &place.repo_text(), std::process::id(), &id)? {
                Ok(held) => Some(held),
                Err(first) => bail!(text::host_runs(&me, &first)),
            }
        }
        None => None,
    };
    println!("{}", text::host_serves(&me, limit));
    let host = Host {
        api: api.signed_in(Some(&id))?,
        me,
        tmux,
        claude: claude.to_owned(),
        main,
        server: server.to_owned(),
    };
    let _ = session.set((host.api.clone(), host.me.clone()));
    let mut wakes = Box::pin(follow(|| host.api.watch(&host.me), RETRY));
    let mut refresh = tokio::time::interval(REFRESH);
    let mut link = Reconnect::default();
    let mut update = std::pin::pin!(binary.new_one());
    let mut following = true;
    loop {
        tokio::select! {
            () = &mut update, if following => {
                binary.run(with_last(std::env::args_os().skip(1), SESSION_ARG, &id));
                // Only an error comes back. The host goes on with this binary.
                following = false;
                continue;
            }
            wake = wakes.next() => {
                let Some(wake) = wake else { return Ok(()) };
                if let Some(line) = link.line(&wake) {
                    anstream::eprintln!("{line}");
                }
                if wake.is_ok() {
                    host.answer().await;
                }
            }
            _ = refresh.tick() => {}
        }
        if let Err(e) = host.set_status().await {
            eprintln!("riff: cannot set the status of the host: {e:#}");
        }
    }
}

/// On Ctrl-C, SIGTERM or SIGHUP: ends the session in `session`, when it
/// is set, and ends the process (01M3NBV405PVYHKTMQ5VN87FYN). It runs in
/// a task of its own, so a step that blocks the host does not hold it.
/// It waits at most [`END_WAIT`] for the end of the session.
fn stop_on_signal(session: Arc<OnceLock<(Api, SessionUri)>>) -> Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut int = signal(SignalKind::interrupt())?;
    let mut term = signal(SignalKind::terminate())?;
    let mut hup = signal(SignalKind::hangup())?;
    tokio::spawn(async move {
        tokio::select! {
            _ = int.recv() => {}
            _ = term.recv() => {}
            _ = hup.recv() => {}
        }
        if let Some((api, me)) = session.get().cloned() {
            let end = tokio::spawn(async move { api.end(&me).await });
            match tokio::time::timeout(END_WAIT, end).await {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(e))) => eprintln!("riff: cannot end the session of the host: {e:#}"),
                _ => eprintln!("riff: the session of the host did not end in time"),
            }
        }
        println!("{}", text::HOST_STOPPED);
        std::process::exit(0);
    });
    Ok(())
}

impl Host {
    /// Sets the status: the limit and the workers of the machine.
    async fn set_status(&self) -> Result<()> {
        let limit = settings::workers_limit(&settings::path()?)?;
        let status = HostStatus::of(limit, Machine::here(), &self.tmux.worker_panes()?);
        let status = Status {
            step: status.line(),
            blocked: None,
        };
        self.api.status(&self.me, &status).await
    }

    /// Reads the unread direct messages, and answers each request.
    async fn answer(&self) {
        let inbox = match self.api.inbox(&self.me, None, false).await {
            Ok(inbox) => inbox,
            Err(e) => {
                eprintln!("riff: cannot read the requests: {e:#}");
                return;
            }
        };
        let messages = inbox
            .iter()
            .filter(|i| i.thread.is_direct())
            .flat_map(|i| &i.messages);
        for checked in messages {
            let Some(judged) = judge(checked, &self.me) else {
                continue;
            };
            let reply = match judged {
                Ok(request) => self.run(request).await,
                Err(refusal) => refusal,
            };
            println!("{reply}");
            let Some(to) = checked.message.from.who().session() else {
                continue;
            };
            if let Err(e) = self.reply(to, &reply).await {
                eprintln!("riff: cannot reply: {e:#}");
            }
        }
    }

    /// Posts `reply` to the session `to` as a note in the repository
    /// thread. A note wakes nobody, so a start by the rollout does not
    /// wake the lead (01M3Q5QEE4MQNCRKVJK3D54G9Z).
    async fn reply(&self, to: &str, reply: &str) -> Result<()> {
        let to: Selector = format!("session={to}").parse()?;
        let thread = self.me.default_thread();
        self.api
            .post(&self.me, thread.as_ref(), &[to], reply, Kind::Note)
            .await?;
        Ok(())
    }

    /// Does one request. Returns the reply.
    async fn run(&self, request: Request) -> String {
        let host = self.me.place().host();
        match request {
            Request::Start(n) => {
                match worker::start(&self.tmux, n, &self.claude, &self.server, &self.main) {
                    Ok(Ok(started)) => text::host_started(host, &started),
                    Ok(Err(why)) => format!("{host}: {why}"),
                    Err(e) => format!("{host}: riff workers start failed: {e:#}"),
                }
            }
            Request::Stop => match worker::stop(&self.tmux, None, &self.server).await {
                Ok(n) => format!("{host}: {}", text::workers_stopped(n)),
                Err(e) => format!("{host}: riff workers stop failed: {e:#}"),
            },
            Request::StopOne(one) => {
                match worker::stop(&self.tmux, Some(&one), &self.server).await {
                    Ok(n) => format!("{host}: {}", text::workers_stopped(n)),
                    Err(e) => format!("{host}: riff workers stop failed: {e:#}"),
                }
            }
        }
    }
}
