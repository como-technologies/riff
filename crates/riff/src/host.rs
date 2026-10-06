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
//! # A worker that dies
//!
//! A worker can die at each moment: a memory kill, a crash, a closed
//! pane. Each [`reap::EVERY`] the host looks at its worker panes. When
//! a pane is gone and its session is still live, the host ends the
//! session, so its claims are free at once, and posts one note to the
//! lead: the pane, the session, the item and the cause
//! (01M3WG2460P4GF7GEVBY92Q33W). See [`crate::reap`].
//!
//! # Calls to the server
//!
//! Each call of the host to the server has a time limit, [`CALL_WAIT`]
//! (01M3WN72M02P3J24ACCHTMNSFY): the status, the read of the requests,
//! the reply, the connect of the watch, the end of a session, and the
//! calls for a worker that died. When
//! no reply comes in time, the host says so on its output and goes on.
//! It sets its status again at the next [`REFRESH`]. When the read of
//! the requests failed, it reads them again at the next [`REFRESH`], so
//! no request of the lead is lost.
//!
//! ```mermaid
//! flowchart TD
//!     W[a wake, or the refresh] --> C[a call to the server]
//!     C -- a reply in time --> G[the host goes on]
//!     C -- no reply in CALL_WAIT --> S[the host says so on its output]
//!     S --> G
//!     G --> R[at the next refresh: the status again, and the requests that it did not read]
//! ```
//!
//! The watch of the host has a connection of its own, so no call waits
//! behind it (see "Streams" in [`crate::api`]).
//!
//! # A change of the settings
//!
//! Each [`reap::EVERY`] the host reads the worker settings of its
//! machine (`Mine`). When the limit or the floor changes, it sets its
//! status at once, so the lead sees the new value at its next look and
//! gets the message of the change from its own rollout
//! ([`crate::rollout`]). When the MCP servers of the workers change, the
//! host posts the note of the change to the lead: its status does not
//! hold them (01M3X30RJS8YE5TXJBQDC2FT0C).
//!
//! # A locked keyring
//!
//! Each [`crate::secrets::KEYRING_RETRY`] the host reads the sign-in of
//! its server from the OS keyring. When the keyring locks, or does not
//! answer, the host says one line and posts one note to the lead. It
//! says no new line while the keyring stays locked. When the keyring
//! answers again, it says one line, posts one note, and goes on with no
//! new start (01M4385CEWGCP31DP5PAMPXZ97). See
//! [`crate::secrets::Gate`].
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
use std::future::Future;
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
use crate::disk::Disk;
use crate::machine::Machine;
use crate::monitor::Numbers;
use crate::reap::{self, Reaper, Watched};
use crate::rollout::{Change, Effect};
use crate::terminal::{self, Terminal, Tmux, WorkerPane};
use crate::{identity, local, secrets, settings, text, worker};

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

/// The longest time that a host waits for the reply to one call to the
/// server (01M3WN72M02P3J24ACCHTMNSFY).
pub const CALL_WAIT: Duration = Duration::from_secs(20);

/// Runs `call`, a call to the server at `base`. It fails when no reply
/// comes in `wait`.
///
/// ```
/// use std::time::Duration;
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// let base = "http://127.0.0.1:7878";
/// let wait = Duration::from_millis(10);
/// let held = std::future::pending::<anyhow::Result<()>>();
/// let error = riff::host::in_time(base, wait, held).await.unwrap_err();
/// assert_eq!(error.to_string(), riff::text::no_reply(base, wait));
/// assert_eq!(riff::host::in_time(base, wait, async { anyhow::Ok(7) }).await.unwrap(), 7);
/// # }
/// ```
pub async fn in_time<T>(
    base: &str,
    wait: Duration,
    call: impl Future<Output = Result<T>>,
) -> Result<T> {
    match tokio::time::timeout(wait, call).await {
        Ok(result) => result,
        Err(_) => Err(crate::api::NoReply {
            base: base.to_owned(),
            wait,
        }
        .into()),
    }
}

/// What the status of a host tells: its limit, its floor of available
/// memory (01M3WFZ01PTAYYKG3T5CFA2W4D), the numbers of its machine
/// (01M3Q5QE4SQ8VYN2PSF42KB3QJ), and the pane and short session ID of
/// each worker.
///
/// ```
/// use riff::disk::Disk;
/// use riff::host::HostStatus;
/// use riff::machine::Machine;
/// use riff::monitor::Numbers;
///
/// let none = HostStatus { limit: 2, floor: 4, deaths: 0, machine: None, disk: None, monitor: None, workers: vec![] };
/// assert_eq!(none.line(), "workers host: limit 2, floor 4GB, no workers");
/// let two = HostStatus {
///     limit: 3,
///     floor: 4,
///     deaths: 0,
///     machine: Some(Machine { cores: 16, mhz: 4500, now_mhz: 4400, mem_gb: 32, avail_gb: 24, load: 1.5 }),
///     disk: None,
///     monitor: None,
///     workers: vec![("%3".into(), "1a2b3c4d".into()), ("%4".into(), "5e6f7a8b".into())],
/// };
/// assert_eq!(
///     two.line(),
///     "workers host: limit 3, floor 4GB, cpu 16x4500MHz (now 4400MHz), mem 32GB, 24GB available, \
///      load 1.50, workers: %3 1a2b3c4d, %4 5e6f7a8b",
/// );
/// assert_eq!(HostStatus::parse(&two.line()), Some(two.clone()));
/// assert_eq!(HostStatus::parse(&none.line()), Some(none.clone()));
/// assert_eq!(HostStatus::parse("idle: waits for work"), None);
///
/// // The disk of the host (01M41A11GHP78E2VYN14JSE27P).
/// let disk = Some(Disk { free_gb: 16, total_gb: 455 });
/// let with_disk = HostStatus { disk, ..two.clone() };
/// assert!(with_disk.line().contains(", load 1.50, disk 16GB free of 455GB (3%), workers: %3"));
/// assert_eq!(HostStatus::parse(&with_disk.line()), Some(with_disk.clone()));
/// let only_disk = HostStatus { disk, ..none.clone() };
/// assert_eq!(only_disk.line(), "workers host: limit 2, floor 4GB, disk 16GB free of 455GB (3%), no workers");
/// assert_eq!(HostStatus::parse(&only_disk.line()), Some(only_disk.clone()));
///
/// // The numbers of the monitor come after the disk
/// // (01M421QPX01BB15GJXHFYRETTX).
/// let monitor = Some(Numbers { on: true, load5: 9.8, limit: 12.0, physical: 8, jobs: 2, kill: None });
/// let with_monitor = HostStatus { monitor: monitor.clone(), ..with_disk.clone() };
/// assert!(with_monitor.line().contains(
///     "(3%), monitor on, load5 9.80 of 12.00, 8 cores, jobs 2, workers: %3"
/// ));
/// assert_eq!(HostStatus::parse(&with_monitor.line()), Some(with_monitor));
/// let only_monitor = HostStatus { monitor, ..none.clone() };
/// assert_eq!(
///     only_monitor.line(),
///     "workers host: limit 2, floor 4GB, monitor on, load5 9.80 of 12.00, 8 cores, jobs 2, no workers"
/// );
/// assert_eq!(HostStatus::parse(&only_monitor.line()), Some(only_monitor));
///
/// // A host of the release before: no floor, no available memory
/// // (01M407J917F9AH072C8DE80CRJ).
/// let old = HostStatus::parse(
///     "workers host: limit 3, cpu 16x4500MHz, mem 32GB, load 1.50, workers: %3 1a2b3c4d",
/// )
/// .unwrap();
/// assert_eq!((old.limit, old.floor), (3, riff::settings::WORKERS_FLOOR));
/// assert_eq!(old.machine.map(|m| m.avail_gb), Some(32));
/// assert_eq!(old.workers, [("%3".to_owned(), "1a2b3c4d".to_owned())]);
/// assert!(HostStatus::parse("workers host: limit 2, no workers").is_some());
///
/// // The deaths of the last hour come after the floor, when there are
/// // any (01M493YZZEW1FTDBNA090WT2AG).
/// let dying = HostStatus { deaths: 4, ..with_disk.clone() };
/// assert!(dying.line().starts_with("workers host: limit 3, floor 4GB, deaths 4, cpu 16x4500MHz"));
/// assert_eq!(HostStatus::parse(&dying.line()), Some(dying));
/// let only_deaths = HostStatus { deaths: 1, ..none.clone() };
/// assert_eq!(only_deaths.line(), "workers host: limit 2, floor 4GB, deaths 1, no workers");
/// assert_eq!(HostStatus::parse(&only_deaths.line()), Some(only_deaths));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct HostStatus {
    pub limit: u16,
    /// The available memory in GB under which the host starts no
    /// worker.
    pub floor: u32,
    /// The deaths of its workers in the last hour ([`crate::deaths`]).
    pub deaths: usize,
    /// The numbers of the machine. `None` from a host that does not
    /// tell them.
    pub machine: Option<Machine>,
    /// The disk of the main clone (01M41A11GHP78E2VYN14JSE27P). `None`
    /// from a host that does not tell it.
    pub disk: Option<Disk>,
    /// The numbers of the monitor (01M421QPX01BB15GJXHFYRETTX). `None`
    /// from a host that does not tell them.
    pub monitor: Option<Numbers>,
    /// The pane and the first 8 characters of the session ID of each
    /// worker.
    pub workers: Vec<(String, String)>,
}

impl HostStatus {
    /// The status of a host with `panes` on `machine`.
    pub fn of(limit: u16, floor: u32, machine: Machine, panes: &[WorkerPane]) -> Self {
        HostStatus {
            limit,
            floor,
            deaths: 0,
            machine: Some(machine),
            disk: None,
            monitor: None,
            workers: panes
                .iter()
                .map(|w| (w.pane.clone(), w.session.chars().take(8).collect()))
                .collect(),
        }
    }

    /// The status of this machine with `panes`: the settings, the
    /// numbers of the machine and of its monitor, and the disk of the
    /// clone `main`.
    pub fn here(main: Option<&Path>, panes: &[WorkerPane]) -> Result<Self> {
        let settings = settings::path()?;
        let machine = Machine::here();
        let mut status = HostStatus::of(
            settings::workers_limit(&settings)?,
            settings::workers_floor(&settings)?,
            machine,
            panes,
        );
        status.disk = main.and_then(Disk::here);
        status.deaths = crate::deaths::here();
        let workers = u16::try_from(panes.len()).unwrap_or(u16::MAX);
        status.monitor = Some(Numbers::here(&settings, &machine, workers)?);
        Ok(status)
    }

    /// The status line. It fits in a status for 10 workers.
    pub fn line(&self) -> String {
        let machine = self.machine.map(|m| format!("{m}, ")).unwrap_or_default();
        let disk = self.disk.map(|d| format!("{d}, ")).unwrap_or_default();
        let monitor = self
            .monitor
            .as_ref()
            .map(|n| format!("{n}, "))
            .unwrap_or_default();
        let deaths = match self.deaths {
            0 => String::new(),
            n => format!("deaths {n}, "),
        };
        let machine = format!("floor {}GB, {deaths}{machine}{disk}{monitor}", self.floor);
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
        let (limit, rest) = rest.split_once(", ")?;
        let limit = limit.parse().ok()?;
        // A host of the release before tells no floor
        // (01M407J917F9AH072C8DE80CRJ).
        let (floor, mut rest) = match rest.strip_prefix("floor ") {
            Some(rest) => {
                let (floor, rest) = rest.split_once("GB, ")?;
                (floor.parse().ok()?, rest)
            }
            None => (crate::settings::WORKERS_FLOOR, rest),
        };
        let mut deaths = 0;
        if let Some(after) = rest.strip_prefix("deaths ") {
            let (count, after) = after.split_once(", ")?;
            deaths = count.parse().ok()?;
            rest = after;
        }
        // The disk comes after the numbers of the machine
        // (01M41A11GHP78E2VYN14JSE27P). A host of the release before
        // tells none.
        let mut disk = None;
        let without;
        if let Some(at) = rest.find("disk ") {
            let end = at + rest[at..].find(", ")?;
            disk = Some(Disk::parse(&rest[at..end])?);
            without = format!("{}{}", &rest[..at], &rest[end + 2..]);
            rest = &without;
        }
        // The numbers of the monitor come after the disk
        // (01M421QPX01BB15GJXHFYRETTX). A host of the release before
        // tells none.
        let mut monitor = None;
        let without_monitor;
        if let Some(at) = rest.find("monitor ") {
            let end = at
                + rest[at..]
                    .find(", no workers")
                    .or_else(|| rest[at..].find(", workers: "))?;
            monitor = Some(Numbers::parse(&rest[at..end])?);
            without_monitor = format!("{}{}", &rest[..at], &rest[end + 2..]);
            rest = &without_monitor;
        }
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
                floor,
                deaths,
                machine,
                disk,
                monitor,
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
            floor,
            deaths,
            machine,
            disk,
            monitor,
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
/// assert_eq!("workers monitor on".parse(), Ok(Request::Monitor(true)));
/// assert_eq!("workers monitor off".parse(), Ok(Request::Monitor(false)));
/// assert_eq!(Request::Monitor(true).to_string(), "workers monitor on");
/// assert!("workers monitor maybe".parse::<Request>().is_err());
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
    /// Turn the monitor of the machine on or off
    /// (01M421QPTQ8BQ0KMG8F7CRHNMX).
    Monitor(bool),
}

impl FromStr for Request {
    type Err = ();

    fn from_str(body: &str) -> Result<Self, ()> {
        match body.split_whitespace().collect::<Vec<_>>()[..] {
            ["workers", "stop"] => Ok(Request::Stop),
            ["workers", "stop", one] => Ok(Request::StopOne(one.to_owned())),
            ["workers", "monitor", "on"] => Ok(Request::Monitor(true)),
            ["workers", "monitor", "off"] => Ok(Request::Monitor(false)),
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
            Request::Monitor(on) => write!(f, "workers monitor {}", if *on { "on" } else { "off" }),
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

/// The worker settings of this machine that the lead must know
/// (01M3X30RJS8YE5TXJBQDC2FT0C).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Mine {
    limit: u16,
    floor: u32,
    mcp: Vec<String>,
}

impl Mine {
    /// The settings in the settings file now.
    fn read() -> Result<Self> {
        let path = settings::path()?;
        Ok(Mine {
            limit: settings::workers_limit(&path)?,
            floor: settings::workers_floor(&path)?,
            mcp: settings::workers_mcp(&path)?,
        })
    }
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
    // The worktrees of the sessions that ended (01M3ZV0TM7ANJ1QQ7XTBDJQE1V).
    match crate::worktrees::clean_as_person(&main, server).await {
        Ok(lines) => lines.iter().for_each(|line| println!("{line}")),
        Err(e) => eprintln!("riff: cannot clean the worktrees: {e:#}"),
    }
    let host = Host {
        api: api.signed_in(Some(&id))?,
        me,
        tmux,
        claude: claude.to_owned(),
        main,
        server: server.to_owned(),
    };
    let _ = session.set((host.api.clone(), host.me.clone()));
    let connect = || in_time(host.api.base(), CALL_WAIT, host.api.watch(&host.me));
    let mut wakes = Box::pin(follow(connect, RETRY));
    let mut refresh = tokio::time::interval(REFRESH);
    let mut link = Reconnect::default();
    let mut update = std::pin::pin!(binary.new_one());
    let mut following = true;
    let mut reaper = Reaper::default();
    let mut look = tokio::time::interval(reap::EVERY);
    look.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // True while the host did not read the requests of a wake.
    let mut unread = false;
    let mut mine = Mine::read()?;
    let mut tidy = crate::tidy::timer();
    let mut guard = crate::tidy::Guard::default();
    // The monitor of the machine (01M421QPKWPX00X24F8V6DT8Z3).
    let monitor = tokio::spawn(crate::monitor::run(host.api.clone(), host.me.clone()));
    let _monitor = AbortOnDrop(monitor);
    // The OS keyring (01M4385CEWGCP31DP5PAMPXZ97).
    let mut keyring = secrets::Gate::default();
    loop {
        let mut changed = true;
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
                    unread = !host.answer().await;
                }
            }
            _ = refresh.tick() => {
                if unread {
                    unread = !host.answer().await;
                }
            }
            _ = look.tick() => changed = false,
            _ = tidy.tick() => {
                // Each 10 minutes: the worktrees and the disk
                // (01M41A118QPQKFAAHGQFFX4F3B).
                match crate::tidy::tidy_as_person(&host.main, &host.server, &mut guard).await {
                    Ok(lines) => lines.iter().for_each(|line| println!("{line}")),
                    Err(e) => eprintln!("riff: cannot tidy the worktrees: {e:#}"),
                }
            }
        }
        let lost = reap::lost(&mut reaper, &host.tmux);
        if !lost.is_empty() {
            host.reap(&lost).await;
            changed = true;
        }
        match Mine::read() {
            Ok(now) if now != mine => {
                if now.mcp != mine.mcp {
                    host.note_mcp(&mine.mcp, &now.mcp).await;
                }
                mine = now;
                changed = true;
            }
            Ok(_) => {}
            Err(e) => eprintln!("riff: cannot read the settings: {e:#}"),
        }
        host.look_at_keyring(&mut keyring).await;
        // A locked keyring says its line one time, not at each status.
        if changed
            && let Err(e) = host.set_status().await
            && !(keyring.locked() && secrets::is_locked(&e))
        {
            eprintln!("riff: cannot set the status of the host: {e:#}");
        }
    }
}

/// Ends a task when it drops.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
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
        let status = HostStatus::here(Some(&self.main), &self.tmux.worker_panes()?)?;
        let status = Status {
            step: status.line(),
        };
        self.call(self.api.status(&self.me, &status)).await
    }

    /// Runs `call`, a call to the server, with [`CALL_WAIT`] as its
    /// time limit (01M3WN72M02P3J24ACCHTMNSFY).
    async fn call<T>(&self, call: impl Future<Output = Result<T>>) -> Result<T> {
        in_time(self.api.base(), CALL_WAIT, call).await
    }

    /// Reads the unread direct messages, and answers each request.
    /// Returns false when it did not read them.
    async fn answer(&self) -> bool {
        let inbox = match self.call(self.api.inbox(&self.me, None, false)).await {
            Ok(inbox) => inbox,
            Err(e) => {
                eprintln!("riff: cannot read the requests: {e:#}");
                return false;
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
        true
    }

    /// Posts `reply` to the session `to` as a note in the repository
    /// thread. A note wakes nobody, so a start by the rollout does not
    /// wake the lead (01M3Q5QEE4MQNCRKVJK3D54G9Z).
    async fn reply(&self, to: &str, reply: &str) -> Result<()> {
        let to: Selector = format!("session={to}").parse()?;
        let thread = self.me.default_thread();
        let to = [to];
        let post = self
            .api
            .post(&self.me, thread.as_ref(), &to, reply, Kind::Note);
        self.call(post).await?;
        Ok(())
    }

    /// Ends the session of each worker in `lost` that is still live,
    /// and posts one note to the lead for each
    /// (01M3WG2460P4GF7GEVBY92Q33W). The pane of the worker ended with
    /// no end call. A pane that the host stopped itself has its end.
    async fn reap(&self, lost: &[Watched]) {
        let sessions = match self.call(self.api.who(&self.me, false)).await {
            Ok(sessions) => sessions,
            Err(e) => {
                eprintln!("riff: cannot end the session of a lost worker: {e:#}");
                return;
            }
        };
        let reaped = reap::reap(
            &self.api,
            &self.me,
            &sessions,
            lost,
            reap::journal,
            crate::deaths::record_here,
        )
        .await;
        for note in &reaped.notes {
            println!("{note}");
            if let Err(e) = self.note_lead(note).await {
                eprintln!("riff: cannot tell the lead: {e:#}");
            }
        }
        if let Some(alarm) = &reaped.alarm {
            println!("{alarm}");
            if let Err(e) = self.tell_lead(alarm).await {
                eprintln!("riff: cannot tell the lead: {e:#}");
            }
        }
    }

    /// Posts the note of a change of the MCP servers of the workers of
    /// this machine to the lead (01M3X30RJS8YE5TXJBQDC2FT0C).
    async fn note_mcp(&self, old: &[String], new: &[String]) {
        let change = Change::Mcp {
            host: self.me.place().host().to_owned(),
            old: old.to_vec(),
            new: new.to_vec(),
        };
        let note = text::setting_changed(&change, &Effect::Nothing);
        println!("{note}");
        if let Err(e) = self.note_lead(&note).await {
            eprintln!("riff: cannot tell the lead: {e:#}");
        }
    }

    /// Looks at the OS keyring when the last look is [`secrets::retry`]
    /// old. At a lock, and when it answers again, it says one line and
    /// posts one note to the lead (01M4385CEWGCP31DP5PAMPXZ97). The host
    /// goes on in both cases.
    async fn look_at_keyring(&self, gate: &mut secrets::Gate) {
        let now = std::time::Instant::now();
        if !gate.due(now, secrets::retry()) {
            return;
        }
        // The read of the sign-in needs an unlocked keyring. A read
        // blocks for at most `KEYRING_WAIT`: keep it off the runtime.
        let name = crate::login::secret_name(self.api.base());
        let refused = tokio::task::spawn_blocking(move || secrets::refuses(&name))
            .await
            .unwrap_or(false);
        let host = self.me.place().host();
        let note = match gate.look(now, refused) {
            None => return,
            Some(secrets::Said::Locked) => {
                eprintln!("riff: {}", text::keyring_locked(host));
                text::host_keyring_locked(host)
            }
            Some(secrets::Said::Back) => {
                let back = text::host_keyring_back(host);
                println!("{back}");
                back
            }
        };
        if let Err(e) = self.note_lead(&note).await {
            eprintln!("riff: cannot tell the lead: {e:#}");
        }
    }

    /// Posts `note` to the lead of the user in the repository, as a
    /// note in the repository thread. It wakes nobody.
    async fn note_lead(&self, note: &str) -> Result<()> {
        self.post_lead(note, Kind::Note).await
    }

    /// Posts `body` as a message to the lead: it wakes the lead.
    async fn tell_lead(&self, body: &str) -> Result<()> {
        self.post_lead(body, Kind::Message).await
    }

    async fn post_lead(&self, body: &str, kind: Kind) -> Result<()> {
        let Some(thread) = self.me.default_thread() else {
            bail!("the host is not in a repository");
        };
        let to = [Selector::lead(self.me.who().user(), &thread.to_string())];
        let post = self.api.post(&self.me, Some(&thread), &to, body, kind);
        self.call(post).await?;
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
            Request::Monitor(on) => {
                let set = settings::path()
                    .and_then(|path| settings::set_monitor_on(&path, on).map(|()| path))
                    .and_then(|path| settings::monitor(&path));
                match set {
                    Ok(monitor) => format!("{host}: {}", text::monitor_set(&monitor)),
                    Err(e) => format!("{host}: riff workers monitor failed: {e:#}"),
                }
            }
        }
    }
}
