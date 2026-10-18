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
//! a message to the session of the daemon of HOST in the thread of the
//! repository ([`ask`]). The daemon judges it ([`judge`]), does it
//! through its own control socket, as the command does on that host
//! ([`answer`]), and posts the reply as a note to the sender in the same
//! thread. The note starts with the sequence number of the request
//! ([`reply_body`]). riff reads the thread after its request until the
//! reply comes, prints it, and exits with 1 when the daemon refused or
//! failed. A note wakes nobody.
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
//!     D->>S: a note to the lead: "re 41, the daemon of thelio: done"
//!     L->>S: read the thread after 41
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
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use futures::StreamExt;
use riff_client::api::{Api, Checked, follow};
use riff_client::link::STREAM_RETRY;
use riff_core::name::SessionUri;
use riff_core::selector::Selector;
use riff_core::wire::{Kind, SessionInfo, Status};
use serde::{Deserialize, Serialize};

use super::{Ask, Say};
use crate::room::Change;
use crate::text;
use crate::wave::Wave;

/// The file of the repositories that the daemon serves, in its folder.
pub const SERVED_FILE: &str = "served.json";

/// The start of the status of the session of a daemon.
pub const PREFIX: &str = "daemon host:";

/// How often the daemon sets its status again, and reads the requests
/// that it did not read.
pub const REFRESH: Duration = Duration::from_secs(30);

/// The longest wait of riff for the reply of the daemon of another
/// host. A `wave run` can pull the session image first.
pub const REPLY_WAIT: Duration = Duration::from_secs(600);

/// How often riff reads the thread for the reply.
const READ_EVERY: Duration = Duration::from_millis(300);

/// The most lines of the events of a session in one reply.
pub const EVENT_LINES: usize = 60;

/// How long the daemon waits for one more event of a session for a
/// reply of `daemon events`.
const EVENT_GAP: Duration = Duration::from_millis(500);

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
///     "daemon events riff-issue-12-1a2b3c4d",
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
    /// `riff daemon events NAME`: the events so far.
    Events(String),
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
            ["daemon", "events", name] => Ok(Request::Events(name.to_owned())),
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
            Request::Events(name) => write!(f, "daemon events {name}"),
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
    let thread = me
        .default_thread()
        .ok_or_else(|| anyhow!("{}", text::RUN_NO_REPO))?;
    let sessions = api.who(me, false).await?;
    let repo = me.place().repo_text();
    let Some(to) = find(&sessions, me.who().user(), &repo, host) else {
        bail!("{}", text::no_daemon_host(host, &repo));
    };
    let to = to.to_owned();
    let posted = api
        .post(
            me,
            Some(&thread),
            &[Selector::session(&to)],
            &request.to_string(),
            Kind::Message,
        )
        .await?;
    let start = tokio::time::Instant::now();
    loop {
        let messages = api.read_after(me, &thread, posted.seq).await?;
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
        Request::Events(name) => events(dir, name).await,
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

/// The events of the live session `name` so far, as `riff daemon events
/// NAME` shows them: at most the last [`EVENT_LINES`] lines.
async fn events(dir: &Path, name: &str) -> Result<String> {
    let mut client = super::Client::connect(dir).await?;
    client
        .ask(Ask::Follow {
            name: name.into(),
            after: 0,
        })
        .await?;
    let mut view = crate::run::View::default();
    let mut lines = Vec::new();
    loop {
        let Ok(next) = tokio::time::timeout(EVENT_GAP, client.next()).await else {
            break;
        };
        match next? {
            Some(Say::Event { line, .. }) => {
                if let Ok(event) = serde_json::from_str(&line) {
                    lines.extend(view.lines(&event));
                }
            }
            Some(Say::Line { text }) => lines.push(text),
            Some(Say::Ended(ended)) => {
                lines.push(text::daemon_session_ends(name, ended.code));
                break;
            }
            Some(Say::Failed { error }) => bail!("{error}"),
            Some(_) => {}
            None => break,
        }
    }
    let skip = lines.len().saturating_sub(EVENT_LINES);
    let mut shown = lines.split_off(skip);
    if skip > 0 {
        shown.insert(0, text::daemon_events_cut(skip));
    }
    Ok(shown.join("\n"))
}

/// The session of the daemon in the folder `dir` on riff-server for
/// the repository of `served`, until the task ends. It sets its status,
/// keeps the session alive, and answers each request.
pub async fn serve(dir: PathBuf, served: Served) {
    let repo = served.wave.repo.clone();
    let session = match Session::new(&dir, &served) {
        Ok(session) => session,
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
    let mut refresh = tokio::time::interval(REFRESH);
    loop {
        tokio::select! {
            wake = wakes.next() => {
                if wake.is_none() {
                    return;
                }
            }
            _ = refresh.tick() => {}
        }
        if let Err(e) = session.set_status().await {
            eprintln!("riff daemon: {repo}: cannot set the status: {e:#}");
        }
        session.answer().await;
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

    async fn set_status(&self) -> Result<()> {
        let status = super::status(&self.dir).await?;
        let step = status_line(status.pid, &status.build, status.sessions.len());
        self.api.status(&self.me, &Status { step }).await
    }

    /// Reads the unread messages of the thread of the repository, and
    /// answers each request to this session.
    async fn answer(&self) {
        let Some(thread) = self.me.default_thread() else {
            return;
        };
        let messages = match self.api.read(&self.me, &thread, false).await {
            Ok(messages) => messages,
            Err(e) => {
                eprintln!("riff daemon: cannot read the requests: {e:#}");
                return;
            }
        };
        for checked in &messages {
            let Some(judged) = judge(checked, &self.me) else {
                continue;
            };
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
            let post = self
                .api
                .post(&self.me, Some(&thread), &to, &body, Kind::Note);
            if let Err(e) = post.await {
                eprintln!("riff daemon: cannot reply: {e:#}");
            }
        }
    }
}
