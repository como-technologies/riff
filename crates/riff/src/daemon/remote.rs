//! The daemon of another host (#1063): the lead asks it through
//! riff-server.
//!
//! # Design
//!
//! The daemon of a host has a session of its own on riff-server for each
//! repository that it serves, as `riff workers host` has
//! (01M579Z9JE6HY9HG3PWR1RR0RQ). It serves a repository after `riff wave
//! run` or `riff daemon host` in its main clone on that host. It keeps
//! each one in [`SERVED_FILE`] in its folder, with its session ID, so a
//! new start of the daemon serves with the same session. Its status is
//! [`PREFIX`] with its pid and its build: the lead finds it in `who`.
//!
//! `riff daemon status`, `riff daemon limit`, `riff daemon cache`, `riff
//! daemon events`, `riff daemon sweep`, `riff daemon end`, `riff daemon
//! restart`, `riff wave run` and `riff wave stop` take `--host HOST`
//! (01M579Z9MPS86MX7890T6HV18S). With it, riff posts the [`Request`] as
//! a direct message to the session of the daemon of HOST ([`ask`]).
//! The daemon judges it ([`judge`]), does it through its own control
//! socket, as the command does on that host ([`answer`]), and posts the
//! reply as a direct note to the sender. The note starts with the
//! sequence number of the request ([`reply_body`]). riff reads the
//! direct thread after its request until the reply comes, prints it,
//! and exits with 1 when the daemon refused or failed. A note wakes
//! nobody. Only the sender and the daemon read their direct thread
//! (01M57D4FTP2NXAE18XFRXCJ79H): a person in a terminal gets the reply
//! in the direct thread of the person (01M57D4FR8TPY47WPT9G66H4PE).
//!
//! `riff daemon events NAME --host HOST` follows the session as on HOST
//! (01M57D4FX5VHQR150QTDS6BA1C). It asks for one [`Page`] after the
//! other: `daemon events NAME --after N --notes K` gives the lines of
//! each event after the event N, and each line of the daemon after its
//! line K, for example why it stops the container
//! (01M57GCZXZZRPGGWC5HZS49DGM), at most [`PAGE_LINES`]. The daemon
//! draws them as `riff daemon events` draws them on HOST, from the same
//! events: the daemon took each secret out of each event before it kept
//! it (01M57D4G1TS8BHJF5A8PW2D79C). When no event comes, the daemon
//! holds the request for at most [`EVENT_HOLD`]. The last page has the
//! end of the session. No follower is on the session between two pages,
//! so the session can end there. The daemon keeps the end of a session
//! for [`KEEP_END`](super::KEEP_END) (01M57GCZVJAHAMPEYY1T6GA48P): the
//! next page gets the events and the end.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant L as riff daemon events --host thelio
//!     participant D as riff daemon on thelio
//!     L->>D: daemon events NAME --after 0
//!     D-->>L: events to 12 (no event for 500 ms)
//!     Note over D: event 13, then the end: no follower
//!     D->>D: keep the end: events 1 to 13, the lines, ended: done
//!     L->>D: daemon events NAME --after 12
//!     D-->>L: events to 13, ended: done
//! ```
//!
//! `riff view --host HOST` draws the view of `riff view` from the
//! status of the daemon of HOST ([`Request::State`],
//! 01M57D4FZGCVE62AP4MVG59HM4). It asks for it each [`VIEW_EVERY`].
//!
//! ```mermaid
//! sequenceDiagram
//!     participant L as lead on pangolin
//!     participant S as riff-server
//!     participant D as riff daemon on thelio
//!     D->>S: watch, status "daemon host: pid 812, build 3.0.1"
//!     L->>S: riff daemon limit --sessions 4 --host thelio
//!     S->>D: wake: the message "daemon limit --sessions 4"
//!     D->>D: judge: verified, the person or the lead of the person
//!     D->>D: the same step as riff daemon limit on thelio
//!     D->>S: a direct note to the lead: "re 41, the daemon of thelio: done"
//!     L->>S: read the direct thread after 41
//!     S-->>L: the note: riff prints it
//! ```
//!
//! Only the person and the lead of the person in the repository act on
//! the daemon (01M579Z9PZZCVWQKG7V8WZ5KRB). The daemon refuses each
//! other request with a note that says why, and changes nothing: a
//! message that is not verified, a message of another user, and a
//! message of a session that is not the lead, for example a worker.
//!
//! `riff daemon restart` asks the daemon to start again on the new riff
//! on disk ([`Again`], 01M579Z9SAN24QGN1VV5C7EHM9). It does so only when
//! it runs no session: else it says that it starts again after its last
//! session. Its session on riff-server stays the same, so the lead sees
//! the new pid and build in `riff daemon status --host HOST`.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use futures::StreamExt;
use riff_client::api::{Api, Checked, follow};
use riff_client::link::STREAM_RETRY;
use riff_core::name::SessionUri;
use riff_core::selector::Selector;
use riff_core::wire::{Kind, SessionInfo, Status};
use serde::{Deserialize, Serialize};

use super::facts::{News, Said, Short};
use super::{Ask, Say};
use crate::room::Change;
use crate::text;
use crate::wave::Wave;

/// The file of the repositories that the daemon serves, in its folder.
pub const SERVED_FILE: &str = "served.json";

/// The start of the status of the session of a daemon.
pub const PREFIX: &str = "daemon host:";

/// How often the daemon sets its status again, sends its facts
/// ([`super::facts`]), and reads the requests that it did not read. A
/// test sets [`super::facts::EVERY_VAR`].
pub const REFRESH: Duration = riff_client::daemons::EVERY;

/// The longest wait of riff for the reply of the daemon of another
/// host. A `wave run` can pull the session image first.
pub const REPLY_WAIT: Duration = Duration::from_secs(600);

/// How often riff reads the thread for the reply.
const READ_EVERY: Duration = Duration::from_millis(300);

/// The most lines of the events of a session in one [`Page`]. The page
/// ends after the event that reaches it.
pub const PAGE_LINES: usize = 200;

/// How long the daemon waits for one more event of a session for a
/// [`Page`], after a new event.
pub const EVENT_GAP: Duration = Duration::from_millis(500);

/// The longest hold of a request for a [`Page`] when no new event
/// comes.
pub const EVENT_HOLD: Duration = Duration::from_secs(20);

/// How often `riff view --host` asks the daemon of the host for its
/// status.
pub const VIEW_EVERY: Duration = Duration::from_secs(3);

/// A repository that the daemon serves, and its session on riff-server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Served {
    /// The repository, its main clone, its riff-server and this host.
    pub wave: Wave,
    /// The session ID of the daemon for the repository.
    pub session: String,
}

/// The repositories in [`SERVED_FILE`] of the folder `dir` of the
/// daemon, by repository. With no file, none.
pub fn read_served(dir: &Path) -> Result<BTreeMap<String, Served>> {
    let path = dir.join(SERVED_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .with_context(|| format!("cannot read the served repositories in {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// Adds `wave` to [`SERVED_FILE`] in the folder `dir`, and gives its
/// entry. A repository that the daemon serves keeps its session, with
/// the new facts of `wave`.
///
/// ```
/// use riff::daemon::remote::{read_served, serve_in_file};
/// use riff::wave::Wave;
///
/// let dir = tempfile::tempdir()?;
/// let wave = Wave {
///     repo: "o/r".into(),
///     main: "/src/r".into(),
///     server: "http://127.0.0.1:1".into(),
///     host: "thelio".into(),
/// };
/// let first = serve_in_file(dir.path(), &wave)?;
/// let moved = Wave { main: "/src/r2".into(), ..wave };
/// let second = serve_in_file(dir.path(), &moved)?;
/// assert_eq!(first.session, second.session);
/// assert_eq!(read_served(dir.path())?["o/r"].wave.main, std::path::Path::new("/src/r2"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn serve_in_file(dir: &Path, wave: &Wave) -> Result<Served> {
    let mut served = read_served(dir)?;
    let session = served
        .get(&wave.repo)
        .map_or_else(crate::terminal::new_session_id, |s| s.session.clone());
    let entry = Served {
        wave: wave.clone(),
        session,
    };
    served.insert(wave.repo.clone(), entry.clone());
    let path = dir.join(SERVED_FILE);
    let tmp = path.with_extension("json.new");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&served)?)
        .with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(entry)
}

/// A request of the lead to the daemon of a host: the body of its
/// message.
///
/// ```
/// use riff::daemon::remote::Request;
/// use riff::room::Change;
///
/// for body in [
///     "daemon status",
///     "daemon limit",
///     "daemon limit --sessions 4 --floor 8 --cpus 6 --memory 12",
///     "daemon cache",
///     "daemon cache 20G",
///     "daemon cache --clear",
///     "daemon state",
///     "daemon events riff-issue-12-1a2b3c4d",
///     "daemon events riff-issue-12-1a2b3c4d --after 7",
///     "daemon events riff-issue-12-1a2b3c4d --after 7 --notes 1",
///     "daemon sweep",
///     "daemon end riff-issue-12-1a2b3c4d",
///     "daemon restart",
///     "wave run",
///     "wave stop",
/// ] {
///     assert_eq!(body.parse::<Request>().unwrap().to_string(), body);
/// }
/// let limit = Change { sessions: Some(4), ..Change::default() };
/// assert_eq!("daemon limit --sessions 4".parse(), Ok(Request::Limit(limit)));
/// assert!("daemon limit --sessions".parse::<Request>().is_err());
/// assert!("daemon limit --sessions x".parse::<Request>().is_err());
/// assert!("daemon end".parse::<Request>().is_err());
/// assert!("daemon events n --after x".parse::<Request>().is_err());
/// assert!("daemon events n --after 7 --notes x".parse::<Request>().is_err());
/// assert!("daemon cache 20G --clear".parse::<Request>().is_err());
/// assert!("workers start 2".parse::<Request>().is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// `riff daemon status`.
    Status,
    /// `riff daemon limit`, with its flags.
    Limit(Change),
    /// `riff daemon cache`: the new most size, or the clear.
    Cache {
        /// The new most size, for example 20G.
        size: Option<String>,
        /// True empties the volume.
        clear: bool,
    },
    /// `riff view`: the status of the daemon, as JSON.
    State,
    /// `riff daemon events NAME`: the [`Page`] of the events of the
    /// session NAME after the event `after`, and of the lines of the
    /// daemon after the line `notes`.
    Events {
        /// The name of the container.
        name: String,
        /// The last event that the caller has: 0 for none.
        after: u64,
        /// The number of lines of the daemon that the caller has
        /// ([`Page::notes`]).
        notes: u64,
    },
    /// `riff daemon sweep`.
    Sweep,
    /// `riff daemon end NAME`.
    End(String),
    /// `riff daemon restart`.
    Restart,
    /// `riff wave run` in the repository.
    WaveRun,
    /// `riff wave stop` in the repository.
    WaveStop,
}

impl FromStr for Request {
    type Err = ();

    fn from_str(body: &str) -> Result<Self, ()> {
        match body.split_whitespace().collect::<Vec<_>>()[..] {
            ["daemon", "status"] => Ok(Request::Status),
            ["daemon", "cache"] => Ok(Request::Cache {
                size: None,
                clear: false,
            }),
            ["daemon", "cache", "--clear"] => Ok(Request::Cache {
                size: None,
                clear: true,
            }),
            ["daemon", "cache", size] => Ok(Request::Cache {
                size: Some(size.to_owned()),
                clear: false,
            }),
            ["daemon", "state"] => Ok(Request::State),
            ["daemon", "events", name] => Ok(Request::Events {
                name: name.to_owned(),
                after: 0,
                notes: 0,
            }),
            ["daemon", "events", name, "--after", after] => Ok(Request::Events {
                name: name.to_owned(),
                after: after.parse().map_err(|_| ())?,
                notes: 0,
            }),
            ["daemon", "events", name, "--after", after, "--notes", notes] => {
                Ok(Request::Events {
                    name: name.to_owned(),
                    after: after.parse().map_err(|_| ())?,
                    notes: notes.parse().map_err(|_| ())?,
                })
            }
            ["daemon", "sweep"] => Ok(Request::Sweep),
            ["daemon", "end", name] => Ok(Request::End(name.to_owned())),
            ["daemon", "restart"] => Ok(Request::Restart),
            ["wave", "run"] => Ok(Request::WaveRun),
            ["wave", "stop"] => Ok(Request::WaveStop),
            ["daemon", "limit", ref flags @ ..] => limit_of(flags).map(Request::Limit),
            _ => Err(()),
        }
    }
}

/// The [`Change`] of the flags of `riff daemon limit`.
fn limit_of(flags: &[&str]) -> Result<Change, ()> {
    let mut change = Change::default();
    for pair in flags.chunks(2) {
        let [flag, value] = pair else { return Err(()) };
        match *flag {
            "--sessions" => change.sessions = Some(value.parse().map_err(|_| ())?),
            "--floor" => change.floor_gb = Some(value.parse().map_err(|_| ())?),
            "--cpus" => change.cpus = Some(value.parse().map_err(|_| ())?),
            "--memory" => change.memory_gb = Some(value.parse().map_err(|_| ())?),
            _ => return Err(()),
        }
    }
    Ok(change)
}

impl fmt::Display for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Request::Status => write!(f, "daemon status"),
            Request::Limit(change) => {
                write!(f, "daemon limit")?;
                let flags = [
                    ("--sessions", change.sessions.map(u64::from)),
                    ("--floor", change.floor_gb),
                    ("--cpus", change.cpus.map(u64::from)),
                    ("--memory", change.memory_gb),
                ];
                for (flag, value) in flags {
                    if let Some(value) = value {
                        write!(f, " {flag} {value}")?;
                    }
                }
                Ok(())
            }
            Request::Cache { clear: true, .. } => write!(f, "daemon cache --clear"),
            Request::Cache {
                size: Some(size), ..
            } => write!(f, "daemon cache {size}"),
            Request::Cache { .. } => write!(f, "daemon cache"),
            Request::State => write!(f, "daemon state"),
            Request::Events {
                name,
                after: 0,
                notes: 0,
            } => write!(f, "daemon events {name}"),
            Request::Events {
                name,
                after,
                notes: 0,
            } => write!(f, "daemon events {name} --after {after}"),
            Request::Events { name, after, notes } => {
                write!(f, "daemon events {name} --after {after} --notes {notes}")
            }
            Request::Sweep => write!(f, "daemon sweep"),
            Request::End(name) => write!(f, "daemon end {name}"),
            Request::Restart => write!(f, "daemon restart"),
            Request::WaveRun => write!(f, "wave run"),
            Request::WaveStop => write!(f, "wave stop"),
        }
    }
}

/// Decides on one message in the thread of the daemon session `me`:
/// `None` when it is no request to `me`, the request when the daemon
/// acts on it, or the refusal. The daemon acts only on a verified
/// message of the person of its user, or of the lead of its user in its
/// repository (01M579Z9PZZCVWQKG7V8WZ5KRB).
///
/// ```
/// use riff_client::api::Checked;
/// use riff::daemon::remote::{Request, judge};
/// use riff_core::selector::Selector;
/// use riff_core::wire::Message;
///
/// let daemon = "riff://mike@thelio/como-technologies/riff?session=d1".parse()?;
/// let from = |uri: &str, to: &str, body: &str, verified: bool| Checked {
///     message: Message {
///         seq: 1,
///         from: uri.parse().unwrap(),
///         to: vec![Selector::session(to)],
///         body: body.into(),
///         at_ms: 0,
///         kind: Default::default(),
///         sig: None,
///         payload: None,
///     },
///     verified,
/// };
/// let lead = "riff://mike@pangolin/como-technologies/riff?session=l1&lead=true";
/// let person = "riff://mike@pangolin/como-technologies/riff";
/// assert_eq!(judge(&from(lead, "d1", "daemon status", true), &daemon), Some(Ok(Request::Status)));
/// assert_eq!(judge(&from(person, "d1", "wave stop", true), &daemon), Some(Ok(Request::WaveStop)));
/// assert!(judge(&from(lead, "d1", "daemon status", false), &daemon).unwrap().is_err());
/// let worker = "riff://mike@pangolin/como-technologies/riff?session=w1";
/// assert!(judge(&from(worker, "d1", "daemon sweep", true), &daemon).unwrap().is_err());
/// let other_user = "riff://brett@kadomony/como-technologies/riff?session=b1&lead=true";
/// assert!(judge(&from(other_user, "d1", "daemon status", true), &daemon).unwrap().is_err());
/// let other_repo = "riff://mike@pangolin/como-technologies/other?session=l2&lead=true";
/// assert!(judge(&from(other_repo, "d1", "daemon status", true), &daemon).unwrap().is_err());
/// // A request to another daemon, and a message that is no request.
/// assert_eq!(judge(&from(lead, "d2", "daemon status", true), &daemon), None);
/// assert_eq!(judge(&from(lead, "d1", "hello", true), &daemon), None);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn judge(checked: &Checked, me: &SessionUri) -> Option<std::result::Result<Request, String>> {
    let message = &checked.message;
    let mine = me.who().session()?;
    if !message
        .to
        .iter()
        .any(|s| s.session.as_deref() == Some(mine))
    {
        return None;
    }
    let request: Request = message.body.parse().ok()?;
    let from = &message.from;
    if !checked.verified {
        return Some(Err(text::daemon_refused_not_verified(&request)));
    }
    let user = from.who().user() == me.who().user();
    let person = from.who().session().is_none();
    let lead = from.lead() && from.place().repo() == me.place().repo();
    if !user || !(person || lead) {
        return Some(Err(text::daemon_refused(&request, me)));
    }
    Some(Ok(request))
}

/// The body of the reply to the request `seq`: the first line names it,
/// the host and the outcome, the next lines are the text.
///
/// ```
/// use riff::daemon::remote::{read_reply, reply_body};
///
/// let body = reply_body(41, "thelio", &Ok("Sessions: none.".into()));
/// assert_eq!(body, "re 41, the daemon of thelio: done\nSessions: none.");
/// assert_eq!(read_reply(&body, 41), Some(Ok("Sessions: none.".into())));
/// let refused = reply_body(42, "thelio", &Err("refused: no.".into()));
/// assert_eq!(read_reply(&refused, 42), Some(Err("refused: no.".into())));
/// assert_eq!(read_reply(&refused, 41), None);
/// ```
pub fn reply_body(seq: u64, host: &str, reply: &std::result::Result<String, String>) -> String {
    let (word, text) = match reply {
        Ok(text) => ("done", text),
        Err(text) => ("failed", text),
    };
    format!("re {seq}, the daemon of {host}: {word}\n{text}")
}

/// The outcome and the text of `body` when it is the reply to the
/// request `seq` ([`reply_body`]).
pub fn read_reply(body: &str, seq: u64) -> Option<std::result::Result<String, String>> {
    let rest = body.strip_prefix(&format!("re {seq}, the daemon of "))?;
    let (head, text) = rest.split_once('\n').unwrap_or((rest, ""));
    let text = text.to_owned();
    if head.ends_with(": done") {
        Some(Ok(text))
    } else if head.ends_with(": failed") {
        Some(Err(text))
    } else {
        None
    }
}

/// The status line of the session of a daemon.
///
/// ```
/// use riff::daemon::remote::status_line;
///
/// assert_eq!(status_line(812, "3.0.1", 2), "daemon host: pid 812, build 3.0.1, 2 sessions");
/// ```
pub fn status_line(pid: u32, build: &str, sessions: usize) -> String {
    format!("{PREFIX} pid {pid}, build {build}, {sessions} sessions")
}

/// The session ID of the live daemon of `user` on `host` in the
/// repository `repo`, from `sessions`.
pub fn find<'a>(
    sessions: &'a [SessionInfo],
    user: &str,
    repo: &str,
    host: &str,
) -> Option<&'a str> {
    sessions
        .iter()
        .filter(|s| s.live && s.uri.who().user() == user)
        .filter(|s| s.uri.place().repo_text() == repo && s.uri.place().host() == host)
        .filter(|s| {
            s.status
                .as_ref()
                .is_some_and(|st| st.status.step.starts_with(PREFIX))
        })
        .find_map(|s| s.uri.who().session())
}

/// Asks the daemon of `host` for `request` as `me`, in the repository
/// of `me`, and waits at most `wait` for its reply. It gives the text
/// of the reply: an error when the daemon refused or failed.
pub async fn ask(
    api: &Api,
    me: &SessionUri,
    host: &str,
    request: &Request,
    wait: Duration,
) -> Result<String> {
    if me.default_thread().is_none() {
        bail!("{}", text::RUN_NO_REPO);
    }
    let sessions = api.who(me, false).await?;
    let repo = me.place().repo_text();
    let Some(to) = find(&sessions, me.who().user(), &repo, host) else {
        bail!("{}", text::no_daemon_host(host, &repo));
    };
    let to = to.to_owned();
    // A direct message: only the two sessions read the thread
    // (01M57D4FTP2NXAE18XFRXCJ79H).
    let posted = api
        .post(
            me,
            None,
            &[Selector::session(&to)],
            &request.to_string(),
            Kind::Message,
        )
        .await?;
    let start = tokio::time::Instant::now();
    loop {
        let messages = api.read_after(me, &posted.thread, posted.seq).await?;
        let reply = messages
            .iter()
            .filter(|m| m.message.from.who().session() == Some(to.as_str()))
            .find_map(|m| read_reply(&m.message.body, posted.seq));
        match reply {
            Some(Ok(text)) => return Ok(text),
            Some(Err(text)) => bail!("{text}"),
            None if start.elapsed() >= wait => bail!("{}", text::daemon_no_reply(host, wait)),
            None => tokio::time::sleep(READ_EVERY).await,
        }
    }
}

/// The new start of a daemon on a new riff: is it old, and the ask to
/// systemd.
pub struct Again {
    /// True when the daemon runs an old riff, and is the process of its
    /// unit.
    pub old: Box<dyn Fn() -> bool + Send + Sync>,
    /// Asks systemd to start the unit again.
    pub restart: Box<dyn Fn() -> Result<()> + Send + Sync>,
}

/// What the daemon does for `riff daemon restart`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restart {
    /// It runs the riff on disk: nothing.
    Newest,
    /// It runs this many sessions, starts that wait and jobs: it starts
    /// again after the last one.
    Later(usize),
    /// It starts again now.
    Now,
}

impl Restart {
    /// The decision for a daemon that runs an old riff or not, and is
    /// busy with `busy` sessions, starts and jobs.
    ///
    /// ```
    /// use riff::daemon::remote::Restart;
    ///
    /// assert_eq!(Restart::of(false, 0), Restart::Newest);
    /// assert_eq!(Restart::of(true, 2), Restart::Later(2));
    /// assert_eq!(Restart::of(true, 0), Restart::Now);
    /// ```
    pub fn of(old: bool, busy: usize) -> Restart {
        match (old, busy) {
            (false, _) => Restart::Newest,
            (true, 0) => Restart::Now,
            (true, busy) => Restart::Later(busy),
        }
    }
}

/// Does `request` with the daemon in the folder `dir`, through its
/// control socket, as the command does on this host. It gives the reply
/// text, or the error.
pub async fn answer(
    dir: &Path,
    served: &Served,
    request: &Request,
) -> std::result::Result<String, String> {
    let done = match request {
        Request::Status => super::status(dir).await.map(|status| {
            let cache = crate::compile_cache::shown(dir);
            format!("{}{cache}", text::daemon_status(&status, super::now_ms()))
        }),
        Request::Limit(change) => {
            crate::room::set(dir, *change).map(|limit| text::daemon_limit(&limit))
        }
        Request::Cache { size, clear } => {
            crate::compile_cache::change(dir, size.as_deref(), *clear)
        }
        Request::State => super::status(dir)
            .await
            .and_then(|status| Ok(serde_json::to_string(&status)?)),
        Request::Events { name, after, notes } => page(dir, name, (*after, *notes), EVENT_HOLD)
            .await
            .map(|page| page.to_string()),
        Request::Sweep => lines_of(dir, Ask::Sweep).await,
        Request::End(name) => lines_of(dir, Ask::End { name: name.clone() }).await,
        Request::Restart => lines_of(dir, Ask::Restart).await,
        Request::WaveRun => wave_run(dir, &served.wave).await,
        Request::WaveStop => {
            let repo = served.wave.repo.clone();
            lines_of(dir, Ask::WaveStop { repo: repo.clone() })
                .await
                .map(|lines| {
                    let stops = text::wave_stops(&repo);
                    if lines.is_empty() {
                        stops
                    } else {
                        format!("{lines}\n{stops}")
                    }
                })
        }
    };
    done.map_err(|e| format!("{e:#}"))
}

/// `riff wave run` of the repository of `wave`, in its main clone.
async fn wave_run(dir: &Path, wave: &Wave) -> Result<String> {
    let here = wave.place(None)?;
    let mut lines = Vec::new();
    let repo = crate::wave::ask_with(dir, wave.main.clone(), &here, &wave.server, |line| {
        lines.push(line.to_owned());
    })
    .await?;
    lines.push(text::wave_asked(&repo));
    Ok(lines.join("\n"))
}

/// Each line of the daemon in the folder `dir` for `ask`, up to its
/// end, as one text.
async fn lines_of(dir: &Path, ask: Ask) -> Result<String> {
    let mut lines = Vec::new();
    super::ask_done(dir, ask, |line| lines.push(line.to_owned())).await?;
    Ok(lines.join("\n"))
}

/// One page of the events of a session for `riff daemon events --host`
/// (01M57D4FX5VHQR150QTDS6BA1C).
///
/// ```
/// use riff::daemon::remote::Page;
///
/// let page = Page { lines: vec!["> Bash: just check".into()], last: 7, notes: 0, end: None };
/// let text = page.to_string();
/// assert_eq!(text, "events to 7\n> Bash: just check");
/// assert_eq!(text.parse::<Page>(), Ok(page));
/// let ended = Page { lines: vec![], last: 9, notes: 1, end: Some(true) };
/// assert_eq!(ended.to_string(), "events to 9, notes 1, ended: failed");
/// assert_eq!(ended.to_string().parse::<Page>(), Ok(ended));
/// assert!("Sessions: none.".parse::<Page>().is_err());
/// assert!("events to 9, notes x".parse::<Page>().is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// The lines of the events and of the daemon, as `riff daemon
    /// events` shows them.
    pub lines: Vec<String>,
    /// The last event of the page: the next page starts after it.
    pub last: u64,
    /// The number of lines of the daemon about the session
    /// ([`Say::Line`]) up to the end of the page: the next page starts
    /// after them. So each line shows one time
    /// (01M57GCZXZZRPGGWC5HZS49DGM).
    pub notes: u64,
    /// The end of the session: true when it failed. `None` while it
    /// runs.
    pub end: Option<bool>,
}

impl fmt::Display for Page {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "events to {}", self.last)?;
        if self.notes > 0 {
            write!(f, ", notes {}", self.notes)?;
        }
        match self.end {
            Some(true) => write!(f, ", ended: failed")?,
            Some(false) => write!(f, ", ended: done")?,
            None => {}
        }
        for line in &self.lines {
            write!(f, "\n{line}")?;
        }
        Ok(())
    }
}

impl FromStr for Page {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, ()> {
        let mut lines = text.split('\n');
        let head = lines
            .next()
            .ok_or(())?
            .strip_prefix("events to ")
            .ok_or(())?;
        let (head, end) = match head.split_once(", ended: ") {
            Some((head, "failed")) => (head, Some(true)),
            Some((head, "done")) => (head, Some(false)),
            Some(_) => return Err(()),
            None => (head, None),
        };
        let (last, notes) = match head.split_once(", notes ") {
            Some((last, notes)) => (last, notes.parse().map_err(|_| ())?),
            None => (head, 0),
        };
        Ok(Page {
            lines: lines.map(str::to_owned).collect(),
            last: last.parse().map_err(|_| ())?,
            notes,
            end,
        })
    }
}

/// The [`Page`] of the events of the session `name` of the daemon in
/// the folder `dir`, after the event and the line of the daemon of
/// `after`. The daemon follows the session from its start, and draws
/// each event with one [`View`](crate::run::View), as `riff daemon
/// events` does: so each line is the same as on this host. The page ends
/// at the end of the session, after [`PAGE_LINES`] lines, after
/// [`EVENT_GAP`] with no new event, or after `hold` with no new event.
/// A session that ended after the page before gives its end: the daemon
/// keeps it ([`super::KEEP_END`]).
pub async fn page(
    dir: &Path,
    name: &str,
    (after, notes): (u64, u64),
    hold: Duration,
) -> Result<Page> {
    let mut client = super::Client::connect(dir).await?;
    client
        .ask(Ask::Follow {
            name: name.into(),
            after: 0,
        })
        .await?;
    let mut view = crate::run::View::default();
    let mut page = Page {
        lines: Vec::new(),
        last: after,
        notes,
        end: None,
    };
    let (mut said, mut new) = (0, false);
    let until = tokio::time::Instant::now() + hold;
    loop {
        let wait = if new {
            EVENT_GAP
        } else {
            until.saturating_duration_since(tokio::time::Instant::now())
        };
        let Ok(next) = tokio::time::timeout(wait, client.next()).await else {
            break;
        };
        match next? {
            Some(Say::Event { seq, line }) => {
                let shown = view.shown(&line);
                if seq > after {
                    page.lines.extend(shown);
                    page.last = seq;
                    new = true;
                    if page.lines.len() >= PAGE_LINES {
                        break;
                    }
                }
            }
            // The daemon gives each line of its own again, in its place.
            Some(Say::Line { text }) => {
                said += 1;
                if said > notes {
                    page.lines.push(text);
                    page.notes = said;
                    new = true;
                }
            }
            Some(Say::Ended(ended)) => {
                page.lines.push(text::daemon_session_ends(name, ended.code));
                page.end = Some(ended.failed);
                break;
            }
            Some(Say::Failed { error }) => bail!("{error}"),
            Some(_) => {}
            None => bail!("{}", text::DAEMON_GONE),
        }
    }
    Ok(page)
}

/// Follows the session `name` of the daemon of `host` as `me`, and
/// gives each line of each [`Page`] to `show`, up to the end of the
/// session (01M57D4FX5VHQR150QTDS6BA1C). It gives true when the session
/// failed.
pub async fn follow_events(
    api: &Api,
    me: &SessionUri,
    host: &str,
    name: &str,
    mut show: impl FnMut(&str),
) -> Result<bool> {
    let (mut after, mut notes) = (0, 0);
    loop {
        let request = Request::Events {
            name: name.to_owned(),
            after,
            notes,
        };
        let text = ask(api, me, host, &request, REPLY_WAIT).await?;
        let page: Page = text
            .parse()
            .map_err(|()| anyhow!("{}", text::daemon_no_page(host)))?;
        for line in &page.lines {
            show(line);
        }
        if let Some(failed) = page.end {
            return Ok(failed);
        }
        (after, notes) = (page.last, page.notes);
    }
}

/// The status of the daemon of `host`, for `riff view --host`
/// (01M57D4FZGCVE62AP4MVG59HM4).
pub async fn state(api: &Api, me: &SessionUri, host: &str) -> Result<super::Status> {
    let text = ask(api, me, host, &Request::State, REPLY_WAIT).await?;
    serde_json::from_str(&text).with_context(|| text::daemon_no_page(host))
}

/// The session of the daemon in the folder `dir` on riff-server for
/// the repository of `served`, until the task ends. It sets its status,
/// keeps the session alive, and answers each request. Each
/// [`super::facts::every`] it sends the facts of the host, and the notes
/// for the lead in `news` ([`super::facts`]).
pub async fn serve(dir: PathBuf, served: Served, news: News) {
    let repo = served.wave.repo.clone();
    let session = match Session::new(&dir, &served) {
        Ok(session) => Arc::new(session),
        Err(e) => {
            eprintln!(
                "riff daemon: {repo}: {}",
                text::daemon_host_failed(&format!("{e:#}"))
            );
            return;
        }
    };
    println!("{}", text::daemon_serves(&repo, &served.wave.host));
    let (api, me) = (session.api.clone(), session.me.clone());
    let alive = tokio::spawn(async move {
        riff_client::api::keep_alive_of_host(&api, &me, riff_core::wire::ALIVE_EVERY).await;
    });
    let _alive = Abort(alive);
    let mut wakes = Box::pin(follow(|| session.api.watch(&session.me), STREAM_RETRY));
    let mut refresh = tokio::time::interval(super::facts::every());
    let mut short = Short::default();
    loop {
        let look = tokio::select! {
            wake = wakes.next() => {
                if wake.is_none() {
                    return;
                }
                false
            }
            _ = refresh.tick() => true,
        };
        if let Err(e) = session.set_status().await {
            eprintln!("riff daemon: {repo}: cannot set the status: {e:#}");
        }
        session.answer().await;
        if look {
            session.send_facts(&mut short).await;
            for note in news.take(&repo) {
                session.note_lead(&note).await;
            }
        }
    }
}

/// Ends a task when it drops.
struct Abort(tokio::task::JoinHandle<()>);

impl Drop for Abort {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The session of a daemon for one repository.
struct Session {
    dir: PathBuf,
    served: Served,
    api: Api,
    me: SessionUri,
}

impl Session {
    fn new(dir: &Path, served: &Served) -> Result<Session> {
        let wave = &served.wave;
        let me = crate::identity::agent(&wave.place(None)?, &served.session, &wave.server)?;
        let api = Api::new(&wave.server).signed_in(Some(&served.session))?;
        Ok(Session {
            dir: dir.to_owned(),
            served: served.clone(),
            api,
            me,
        })
    }

    /// Sends the facts of the host as the numbers of the machine of the
    /// session, and the note of a change of the sign-in of `claude` to
    /// the lead (01M57D3F50347RD8ED79KEA9V7, 01M57D3FC5PB5EQ6Y38Y5R2DBB).
    async fn send_facts(&self, short: &mut Short) {
        let repo = &self.served.wave.repo;
        let facts = match super::facts::look(&self.dir, &self.served.wave, &self.api).await {
            Ok(facts) => facts,
            Err(e) => {
                eprintln!("riff daemon: {repo}: cannot read the facts: {e:#}");
                return;
            }
        };
        if let Err(e) = self.api.machine(&self.me, &facts.line()).await {
            eprintln!("riff daemon: {repo}: cannot send the facts: {e:#}");
        }
        let host = &self.served.wave.host;
        let note = match short.look(&facts.claude) {
            None => return,
            Some(Said::Short(minutes)) => text::daemon_claude_short(host, minutes),
            Some(Said::Back) => text::daemon_claude_back(host),
        };
        println!("riff daemon: {note}");
        self.note_lead(&note).await;
    }

    /// Posts `note` to the lead of the user in the repository, as a note
    /// in the repository thread. It wakes nobody.
    async fn note_lead(&self, note: &str) {
        let Some(thread) = self.me.default_thread() else {
            return;
        };
        let to = [Selector::lead(self.me.who().user(), &thread.to_string())];
        let post = self
            .api
            .post(&self.me, Some(&thread), &to, note, Kind::Note);
        if let Err(e) = post.await {
            eprintln!("riff daemon: cannot tell the lead: {e:#}");
        }
    }

    async fn set_status(&self) -> Result<()> {
        let status = super::status(&self.dir).await?;
        let step = status_line(status.pid, &status.build, status.sessions.len());
        self.api.status(&self.me, &Status { step }).await
    }

    /// Reads the unread messages of each thread of this session: the
    /// thread of the repository and each direct thread. It answers each
    /// request to this session in a task of its own, so a request that
    /// waits for events holds up no other.
    async fn answer(self: &Arc<Self>) {
        let inbox = match self.api.inbox(&self.me, None, false).await {
            Ok(inbox) => inbox,
            Err(e) => {
                eprintln!("riff daemon: cannot read the requests: {e:#}");
                return;
            }
        };
        for checked in inbox.into_iter().flat_map(|i| i.messages) {
            let Some(judged) = judge(&checked, &self.me) else {
                continue;
            };
            let session = self.clone();
            tokio::spawn(async move { session.reply(&checked, judged).await });
        }
    }

    /// Does the request of `checked`, or refuses it, and posts the reply
    /// as a direct note to its sender (01M57D4FTP2NXAE18XFRXCJ79H).
    async fn reply(&self, checked: &Checked, judged: std::result::Result<Request, String>) {
        let reply = match judged {
            Ok(request) => answer(&self.dir, &self.served, &request).await,
            Err(refusal) => Err(refusal),
        };
        let body = reply_body(checked.message.seq, &self.served.wave.host, &reply);
        println!("riff daemon: {}", body.lines().next().unwrap_or_default());
        let from = checked.message.from.who();
        let to = [match from.session() {
            Some(id) => Selector::session(id),
            None => Selector {
                user: Some(from.user().to_owned()),
                ..Selector::default()
            },
        }];
        let post = self.api.post(&self.me, None, &to, &body, Kind::Note);
        if let Err(e) = post.await {
            eprintln!("riff daemon: cannot reply: {e:#}");
        }
    }
}
