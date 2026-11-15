//! The log of the daemon in a file of its own, with a size cap (#1858,
//! 01M749XM23FTKZ95QEZCMVQ2K5).
//!
//! # Design
//!
//! The daemon writes each line of its log to its stdout and its stderr.
//! systemd puts them in the journal of the user. On a busy host, that
//! journal can rotate in a few hours, and the log of a past fault is gone.
//! So on a host where the service manager keeps the log in a journal
//! ([`crate::service::ServiceManager::keeps_log`]), [`tee`] puts a pipe in
//! place of the stdout and of the stderr of the daemon at its start. A
//! thread of each pipe writes each byte to where it went before, and each
//! line, after its time in UTC, to [`FILE`] in the folder of the daemon
//! ([`Capped`]). When the file has [`CAP`] bytes or more, the next line
//! moves it to [`OLD`] and starts a new file. So the log of the daemon
//! holds the last 10 to 20 MB of lines. The processes that the daemon
//! starts write into the same pipes. launchd writes the log of the daemon
//! on a Mac to a file already: there, the daemon puts no pipe in place.
//!
//! Only the daemon that holds the socket puts the pipes in place
//! (01M74C23RM3VA63DX65BY64ET6): a second `riff daemon` gives its
//! refusal on its own stderr, and writes nothing to the log of the first.
//! At its stop, the daemon puts its old stdout and stderr back
//! ([`Tee::end`]), and waits up to [`DRAIN`] for the threads to copy the
//! last lines in the pipes. So its last line is not lost when the process
//! ends, and an error after it goes in full to the old stderr.
//!
//! ```mermaid
//! flowchart LR
//!     D["riff daemon: its lines, the lines of its children"] --> P["the pipes of stdout and stderr"]
//!     P --> J["the journal of the user, as before"]
//!     P --> F["daemon.log in the folder of the daemon, each line with its time"]
//!     F -->|"at 10 MB, at the next line"| O["daemon.log.1"]
//! ```
//!
//! # riff daemon log
//!
//! `riff daemon log` prints the lines of [`OLD`] and [`FILE`] that a
//! [`Query`] picks ([`read`], #2156, 01M7EX1PVSG64G8EE5SNDRHM7G): the
//! lines at or after a time (`--since`), with one of some texts
//! (`--grep`), and of them the newest (`--lines`, at most
//! [`MOST_LINES`]). Each line goes through [`clean`] first
//! (01M7EX1Q1F468NWGPJJS0W0ZG9): the filter of the events of a session
//! ([`Scrub`]), each key of Anthropic, and the filter of the log of the
//! real-session test ([`redact`](super::release_test::redact)). So no
//! line has a secret, and `--grep` finds no part of a secret.
//!
//! Only the riff of the host reads the files, in the folder of the
//! daemon: no session can write there (01M7EX1PYJ99BDSVJK7MAPDFD0). On
//! the host, riff reads them itself, so it reads them also while the
//! daemon is down. The lead in its container asks the daemon on the
//! socket of its session (`ServerCall::DaemonLog`). With `--host HOST`,
//! riff asks the daemon of HOST through riff-server, as `riff daemon
//! status --host` does ([`super::remote::Request::Log`]). Only the lead
//! and the person may.
//!
//! ```mermaid
//! flowchart LR
//!     P["riff daemon log on the host"] --> F["daemon.log.1 and daemon.log"]
//!     L["riff daemon log in the lead"] -- "ServerCall::DaemonLog" --> D["the riff daemon of the host"]
//!     D --> F
//!     H["riff daemon log --host thelio"] -- "daemon log, through riff-server" --> T["the riff daemon of thelio"]
//!     T --> G["its daemon.log.1 and daemon.log"]
//! ```

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDateTime, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};

use crate::run::{HIDDEN, Scrub};
use crate::text;

/// The log file of the daemon, in its folder.
pub const FILE: &str = "daemon.log";

/// The older log file of the daemon, in its folder.
pub const OLD: &str = "daemon.log.1";

/// The size of the log file at which the next line starts a new file.
pub const CAP: u64 = 10 * 1024 * 1024;

/// The longest time that [`Tee::end`] waits for the threads to copy the
/// last lines in the pipes. A child of the daemon that still runs keeps
/// a pipe open: its thread does not end.
pub const DRAIN: Duration = Duration::from_secs(2);

/// A log file with a size cap. Each line starts with its time. When the
/// file has its cap or more, the next line moves it to the older file
/// ([`OLD`]), and starts a new file: a line is never cut in two files. A
/// fault of the disk loses the line, and stops no writer.
///
/// ```
/// use riff::daemon::log::{Capped, FILE, OLD};
///
/// let dir = tempfile::tempdir()?;
/// let mut log = Capped::new(dir.path().join(FILE), 60);
/// log.write(b"riff daemon: a first line\nand a part", "2026-11-10T15:36:12Z");
/// log.write(b" of a line\n", "2026-11-10T15:36:13Z");
/// let text = std::fs::read_to_string(dir.path().join(FILE))?;
/// assert_eq!(
///     text,
///     "2026-11-10T15:36:12Z riff daemon: a first line\n\
///      2026-11-10T15:36:12Z and a part of a line\n"
/// );
/// // The file has its cap: the next line starts a new file.
/// log.write(b"a new line\n", "2026-11-10T15:36:14Z");
/// assert_eq!(std::fs::read_to_string(dir.path().join(OLD))?, text);
/// assert_eq!(
///     std::fs::read_to_string(dir.path().join(FILE))?,
///     "2026-11-10T15:36:14Z a new line\n"
/// );
/// # Ok::<(), std::io::Error>(())
/// ```
pub struct Capped {
    path: PathBuf,
    cap: u64,
    file: Option<File>,
    size: u64,
    /// True when the next byte starts a line.
    fresh: bool,
}

impl Capped {
    /// The log in the file `path` with the cap `cap`. A file that is
    /// there already gets the new lines at its end.
    pub fn new(path: PathBuf, cap: u64) -> Capped {
        Capped {
            path,
            cap,
            file: None,
            size: 0,
            fresh: true,
        }
    }

    /// Writes `bytes`. Each line that starts in them starts with `time`.
    pub fn write(&mut self, bytes: &[u8], time: &str) {
        for piece in bytes.split_inclusive(|b| *b == b'\n') {
            if self.fresh {
                self.open();
                if self.size >= self.cap {
                    self.start_again();
                }
                self.put(time.as_bytes());
                self.put(b" ");
            }
            self.put(piece);
            self.fresh = piece.ends_with(b"\n");
        }
    }

    /// Opens the file when it is not open, and reads its size.
    fn open(&mut self) {
        if self.file.is_some() {
            return;
        }
        let opened = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path);
        if let Ok(file) = opened {
            self.size = file.metadata().map_or(0, |m| m.len());
            self.file = Some(file);
        }
    }

    /// Writes `bytes` at the end of the file.
    fn put(&mut self, bytes: &[u8]) {
        self.open();
        if let Some(file) = &mut self.file
            && file.write_all(bytes).is_ok()
        {
            self.size += bytes.len() as u64;
        }
    }

    /// Moves the file to the older file, and starts a new one.
    fn start_again(&mut self) {
        self.file = None;
        let _ = std::fs::rename(&self.path, self.path.with_file_name(OLD));
        self.size = 0;
    }
}

/// Writes `line` and its end to `out` in one write. `eprintln!` writes
/// each part of its format in a write of its own, and the stderr of the
/// daemon has no buffer. Its stdout and its stderr go to the same journal
/// or file: a line on stdout from another task can come between two
/// parts, and cut the line in two. A fault of `out` loses the line, and
/// stops no writer.
///
/// ```
/// use std::io::Write;
///
/// /// Keeps each write.
/// #[derive(Default)]
/// struct Writes(Vec<String>);
///
/// impl Write for Writes {
///     fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
///         self.0.push(String::from_utf8_lossy(buf).into_owned());
///         Ok(buf.len())
///     }
///     fn flush(&mut self) -> std::io::Result<()> {
///         Ok(())
///     }
/// }
///
/// let (repo, what) = ("o/r", "cannot follow the changes of the plan");
/// let mut one = Writes::default();
/// riff::daemon::log::line(&mut one, &format!("riff daemon: {repo}: {what}"));
/// assert_eq!(one.0, ["riff daemon: o/r: cannot follow the changes of the plan\n"]);
/// // `writeln!` writes each part on its own.
/// let mut parts = Writes::default();
/// writeln!(parts, "riff daemon: {repo}: {what}")?;
/// assert_eq!(parts.0.concat(), one.0[0]);
/// assert!(parts.0.len() > 1);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn line(out: &mut impl Write, line: &str) {
    let _ = out.write_all(format!("{line}\n").as_bytes());
}

/// The time of a line of the log: now, in UTC, to the second.
fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Puts a file in place of the stdout or of the stderr of this process.
type Dup2 = fn(BorrowedFd<'_>) -> nix::Result<()>;

/// The pipes that [`tee`] put in place of the stdout and of the stderr
/// of this process. [`Tee::end`] puts the old ones back.
#[must_use = "Tee::end puts the old stdout and stderr back"]
pub struct Tee {
    pipes: Vec<Pipe>,
}

/// One pipe of a [`Tee`].
struct Pipe {
    /// Puts a file in place of the stdout or of the stderr.
    dup2: Dup2,
    /// Where the stdout or the stderr went before the pipe.
    before: OwnedFd,
    /// Gets no value: it ends when the thread of the pipe ends.
    done: mpsc::Receiver<()>,
}

impl Tee {
    /// Puts the old stdout and stderr back: a line after this goes only
    /// where it went before the pipes. Waits up to [`DRAIN`] for the
    /// threads to copy the lines that are still in the pipes.
    pub fn end(self) {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        for pipe in &self.pipes {
            let _ = (pipe.dup2)(pipe.before.as_fd());
        }
        let until = Instant::now() + DRAIN;
        for pipe in &self.pipes {
            let left = until.saturating_duration_since(Instant::now());
            let _ = pipe.done.recv_timeout(left);
        }
    }
}

/// Puts a pipe in place of the stdout and of the stderr of this process.
/// A thread of each pipe writes each byte to where it went before, and
/// each line to the log [`FILE`] in the folder `dir`, with its time.
pub fn tee(dir: &Path) -> Result<Tee> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let log = Arc::new(Mutex::new(Capped::new(dir.join(FILE), CAP)));
    // The lines that wait in the buffers go out before the pipes.
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    let stdout: Dup2 = |fd| nix::unistd::dup2_stdout(fd);
    let stderr: Dup2 = |fd| nix::unistd::dup2_stderr(fd);
    let out = pipe_in_place(std::io::stdout().as_fd(), stdout, log.clone())?;
    let mut tee = Tee { pipes: vec![out] };
    match pipe_in_place(std::io::stderr().as_fd(), stderr, log) {
        Ok(err) => tee.pipes.push(err),
        Err(e) => {
            tee.end();
            return Err(e);
        }
    }
    Ok(tee)
}

/// Puts a new pipe in place of `fd` with `dup2`, and starts the thread
/// that copies it to where `fd` went before, and to `log`.
fn pipe_in_place(fd: BorrowedFd<'_>, dup2: Dup2, log: Arc<Mutex<Capped>>) -> Result<Pipe> {
    let copy = || {
        fd.try_clone_to_owned()
            .context("cannot copy the stdout or the stderr of the daemon")
    };
    let (before, to) = (copy()?, copy()?);
    let (mut reader, writer) = std::io::pipe().context("cannot make a pipe for the log")?;
    dup2(writer.as_fd()).context("cannot put the pipe of the log in place")?;
    drop(writer);
    let (ended, done) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        // The end of the thread drops it: `done` ends.
        let _ended = ended;
        let mut to = File::from(to);
        let mut buf = [0u8; 8192];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) => return,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return,
            };
            let _ = to.write_all(&buf[..n]);
            log.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .write(&buf[..n], &now());
        }
    });
    Ok(Pipe { dup2, before, done })
}

/// The lines of `riff daemon log` with no `--lines`.
pub const LINES: usize = 200;

/// The most lines of one `riff daemon log`.
pub const MOST_LINES: usize = 2000;

/// The most characters of one line of `riff daemon log`: the rest of a
/// longer line goes.
pub const LINE_CHARS: usize = 1000;

/// The most bytes of the lines of one `riff daemon log`: the reply of
/// the daemon of another host is one message on riff-server. The oldest
/// lines go first.
pub const MOST_BYTES: usize = 1024 * 1024;

/// What `riff daemon log` asks for (#2156). [`Query::new`] checks it.
///
/// ```
/// use riff::daemon::log::{LINES, Query};
///
/// let query = Query::new(Some(1_794_640_320), None, vec!["issue-12".into()])?;
/// assert_eq!(query.lines, LINES);
/// assert!(Query::new(None, Some(0), vec![]).is_err());
/// assert!(Query::new(None, Some(2001), vec![]).is_err());
/// assert!(Query::new(None, None, vec!["".into()]).is_err());
/// assert!(Query::new(None, None, vec!["a\nb".into()]).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    /// The first time of a line, in seconds since the epoch: an older
    /// line does not show. `None`: from the first line.
    pub since: Option<i64>,
    /// The most lines: the newest ones show.
    pub lines: usize,
    /// A line shows only when it has one of these texts. Empty: each
    /// line can show.
    pub grep: Vec<String>,
}

impl Query {
    /// The query of `--since`, `--lines` and each `--grep`. `lines` is 1
    /// to [`MOST_LINES`], with [`LINES`] for `None`. Each text has a
    /// character, and no line break.
    pub fn new(since: Option<i64>, lines: Option<usize>, grep: Vec<String>) -> Result<Query> {
        let lines = lines.unwrap_or(LINES);
        if !(1..=MOST_LINES).contains(&lines) {
            bail!("{}", text::daemon_log_bad_lines(lines));
        }
        if let Some(bad) = grep
            .iter()
            .find(|t| t.is_empty() || t.contains(['\n', '\r']))
        {
            bail!("{}", text::daemon_log_bad_grep(bad));
        }
        Ok(Query { since, lines, grep })
    }
}

/// The lines that `riff daemon log` shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    /// The newest lines that the query picks, the oldest first, with no
    /// secret.
    pub lines: Vec<String>,
    /// The older lines that the query picks too, which do not show.
    pub more: usize,
}

/// The time of `--since` in `text`, in seconds since the epoch, at the
/// time `now`. A time of the day, `HH:MM` or `HH:MM:SS`, is in the time
/// zone of `now`: the last one at or before `now`. A date and a time
/// has its zone: `Z` or an offset, with or without the seconds.
///
/// ```
/// use chrono::{FixedOffset, TimeZone, Utc};
/// use riff::daemon::log::since_at;
///
/// let utc = |d, h, m| Utc.with_ymd_and_hms(2026, 11, d, h, m, 0).unwrap().timestamp();
/// // 03:00 at UTC-5 is 08:00 in UTC.
/// let zone = FixedOffset::west_opt(5 * 3600).unwrap();
/// let now = zone.with_ymd_and_hms(2026, 11, 14, 3, 0, 0).unwrap();
/// assert_eq!(since_at("02:12", &now)?, utc(14, 7, 12));
/// assert_eq!(since_at("02:12:30", &now)?, utc(14, 7, 12) + 30);
/// assert_eq!(since_at("2026-11-14T07:12Z", &now)?, utc(14, 7, 12));
/// assert_eq!(since_at("2026-11-14T07:12:00Z", &now)?, utc(14, 7, 12));
/// assert_eq!(since_at("2026-11-14T02:12-05:00", &now)?, utc(14, 7, 12));
/// // The last 04:00 before 03:00 is on the day before.
/// assert_eq!(since_at("04:00", &now)?, utc(13, 9, 0));
/// for bad in ["", "25:00", "2026-11-14T07:12", "yesterday"] {
///     assert!(since_at(bad, &now).is_err(), "{bad}");
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn since_at<Tz: TimeZone>(text: &str, now: &DateTime<Tz>) -> Result<i64> {
    let text = text.trim();
    let of_day = ["%H:%M", "%H:%M:%S"]
        .iter()
        .find_map(|form| NaiveTime::parse_from_str(text, form).ok());
    if let Some(time) = of_day {
        let (zone, today) = (now.timezone(), now.date_naive());
        let last = (0..3)
            .filter_map(|back| today.checked_sub_days(chrono::Days::new(back)))
            .filter_map(|day| zone.from_local_datetime(&day.and_time(time)).earliest())
            .find(|at| at <= now);
        if let Some(at) = last {
            return Ok(at.timestamp());
        }
    }
    if let Ok(at) = DateTime::parse_from_rfc3339(text) {
        return Ok(at.timestamp());
    }
    if let Ok(at) = DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M%:z") {
        return Ok(at.timestamp());
    }
    if let Some(utc) = text.strip_suffix(['Z', 'z'])
        && let Ok(at) = NaiveDateTime::parse_from_str(utc, "%Y-%m-%dT%H:%M")
    {
        return Ok(at.and_utc().timestamp());
    }
    bail!("{}", text::daemon_log_bad_since(text))
}

/// The time of `--since` in `text`, now, in the local time of this
/// machine ([`since_at`]).
pub fn since(text: &str) -> Result<i64> {
    since_at(text, &chrono::Local::now())
}

/// The secrets that [`clean`] takes out on this host: the secrets of
/// this process and the sign-in file of `claude`
/// ([`crate::session_relay::secrets_here`]).
pub fn scrub_here() -> Scrub {
    Scrub::new(crate::session_relay::secrets_here())
}

/// `line` with no secret: [`Scrub::clean`], then each key of Anthropic
/// (`sk-ant-` and the letters, digits, `-` and `_` after it), then
/// [`redact`](super::release_test::redact).
///
/// ```
/// use riff::daemon::log::clean;
/// use riff::run::Scrub;
///
/// let scrub = Scrub::new(["a-secret-of-the-host".to_owned()]);
/// assert_eq!(clean("riff daemon: a-secret-of-the-host", &scrub), "riff daemon: [secret]");
/// assert_eq!(clean("git push with ghs_16C7e42F292c6912E7710c838347Ae178B4a", &scrub), "git push with [secret]");
/// assert_eq!(clean(r#"{"token":"sk-ant-oat01-Ab_c-9"}"#, &scrub), r#"{"token":"[secret]"}"#);
/// assert_eq!(clean("podman run -e GH_TOKEN=abc", &scrub), "podman run -e GH_TOKEN=[secret]");
/// assert_eq!(clean("Authorization: Bearer xyz", &scrub), "Authorization: Bearer [secret]");
/// assert_eq!(clean("riff daemon: o/r: 2 sessions", &scrub), "riff daemon: o/r: 2 sessions");
/// ```
pub fn clean(line: &str, scrub: &Scrub) -> String {
    let line = scrub.clean(line);
    let line = keys_out(&line);
    super::release_test::redact(&line)
}

/// `text` with each key of Anthropic replaced by [`HIDDEN`].
fn keys_out(text: &str) -> String {
    const START: &str = "sk-ant-";
    let part = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(START) {
        let after = &rest[at + START.len()..];
        let len = after.find(|c: char| !part(c)).unwrap_or(after.len());
        out.push_str(&rest[..at]);
        out.push_str(HIDDEN);
        rest = &after[len..];
    }
    out.push_str(rest);
    out
}

/// The time at the start of a line of the log, as [`Capped`] writes it.
fn time_of(line: &str) -> Option<i64> {
    let word = line.split(' ').next()?;
    DateTime::parse_from_rfc3339(word)
        .ok()
        .map(|t| t.timestamp())
}

/// `line` with at most [`LINE_CHARS`] characters.
fn cut(line: String) -> String {
    match line.char_indices().nth(LINE_CHARS) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line,
    }
}

/// Picks the lines of a [`Query`], one line after the other.
struct Picker<'a> {
    query: &'a Query,
    scrub: &'a Scrub,
    /// The time of the last line with a time: a line with no time, for
    /// example a part of a long line, has the time of the line before.
    time: Option<i64>,
    kept: VecDeque<String>,
    picked: usize,
}

impl Picker<'_> {
    fn line(&mut self, raw: &str) {
        if let Some(time) = time_of(raw) {
            self.time = Some(time);
        }
        if let Some(since) = self.query.since
            && self.time.is_none_or(|time| time < since)
        {
            return;
        }
        // The text is found after the clean: no part of a secret.
        let line = clean(raw, self.scrub);
        let grep = &self.query.grep;
        if !grep.is_empty() && !grep.iter().any(|g| line.contains(g.as_str())) {
            return;
        }
        self.picked += 1;
        self.kept.push_back(cut(line));
        if self.kept.len() > self.query.lines {
            self.kept.pop_front();
        }
    }

    fn found(mut self) -> Found {
        let mut bytes: usize = self.kept.iter().map(|l| l.len() + 1).sum();
        while bytes > MOST_BYTES
            && let Some(old) = self.kept.pop_front()
        {
            bytes -= old.len() + 1;
        }
        Found {
            more: self.picked - self.kept.len(),
            lines: self.kept.into(),
        }
    }
}

/// The lines of `logs`, each one the text of a log, the oldest first,
/// that `query` picks, with no secret of `scrub` ([`clean`]).
///
/// ```
/// use riff::daemon::log::{Query, pick};
/// use riff::run::Scrub;
///
/// let log = "2026-11-14T07:00:00Z riff daemon: o/r: claims issue-11\n\
///            2026-11-14T07:12:00Z riff daemon: o/r: claims issue-12\n\
///            2026-11-14T07:13:00Z riff daemon: issue-12 pushes with ghs_abc1\n\
///            2026-11-14T07:14:00Z riff daemon: o/r: claims issue-13\n";
/// let since = Some(1_794_640_320); // 2026-11-14T07:12:00Z
/// let query = Query::new(since, None, vec!["issue-12".into()])?;
/// let found = pick([log], &query, &Scrub::default());
/// assert_eq!(
///     found.lines,
///     [
///         "2026-11-14T07:12:00Z riff daemon: o/r: claims issue-12",
///         "2026-11-14T07:13:00Z riff daemon: issue-12 pushes with [secret]",
///     ]
/// );
/// let newest = Query::new(None, Some(1), vec!["claims".into()])?;
/// let found = pick([log], &newest, &Scrub::default());
/// assert_eq!(found.lines, ["2026-11-14T07:14:00Z riff daemon: o/r: claims issue-13"]);
/// assert_eq!(found.more, 2);
/// // No part of a secret is found.
/// let probe = Query::new(None, None, vec!["ghs_abc".into()])?;
/// assert!(pick([log], &probe, &Scrub::default()).lines.is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn pick<'a>(logs: impl IntoIterator<Item = &'a str>, query: &Query, scrub: &Scrub) -> Found {
    let mut picker = Picker {
        query,
        scrub,
        time: None,
        kept: VecDeque::new(),
        picked: 0,
    };
    for log in logs {
        for line in log.lines() {
            picker.line(line);
        }
    }
    picker.found()
}

/// The lines of the log of the daemon in its folder `dir` that `query`
/// picks ([`pick`]): [`OLD`], then [`FILE`]. With neither file, the
/// error names `log_command`, the command that shows the log of the
/// daemon on this host, for example on a Mac.
pub fn read(dir: &Path, query: &Query, log_command: &str) -> Result<Found> {
    let mut picker = Picker {
        query,
        scrub: &scrub_here(),
        time: None,
        kept: VecDeque::new(),
        picked: 0,
    };
    let mut any = false;
    for path in [dir.join(OLD), dir.join(FILE)] {
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        any = true;
        let mut reader = BufReader::new(file);
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            let n = reader
                .read_until(b'\n', &mut bytes)
                .with_context(|| format!("cannot read {}", path.display()))?;
            if n == 0 {
                break;
            }
            let line = String::from_utf8_lossy(&bytes);
            picker.line(line.trim_end_matches(['\n', '\r']));
        }
    }
    if !any {
        bail!("{}", text::daemon_no_log(&dir.join(FILE), log_command));
    }
    Ok(picker.found())
}

/// [`read`] in the folder `dir` of the daemon of this host, with the
/// log command of its service manager, as one text
/// ([`text::daemon_log`]).
pub fn read_here(dir: &Path, query: &Query) -> Result<String> {
    let command = crate::platform::service_manager()
        .map(|m| crate::service::ServiceManager::log_command(&m))
        .unwrap_or_default();
    read(dir, query, &command).map(|found| text::daemon_log(&found))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1858, 01M749XM23FTKZ95QEZCMVQ2K5: a log file that is there gets
    /// the new lines at its end, and counts its size from its bytes: the
    /// cap holds over a new start of the daemon.
    #[test]
    fn a_new_start_writes_at_the_end_of_the_log_and_keeps_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        std::fs::write(&path, "x".repeat(30) + "\n").unwrap();
        let mut log = Capped::new(path.clone(), 40);
        log.write(b"one\n", "T1");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "x".repeat(30) + "\nT1 one\n"
        );
        log.write(b"two\nthree\n", "T2");
        assert_eq!(
            std::fs::read_to_string(dir.path().join(OLD)).unwrap(),
            "x".repeat(30) + "\nT1 one\nT2 two\n"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "T2 three\n");
        // A new start with a file over the cap starts a new file at its
        // first line. Only one older file stays: the next one replaces it.
        let mut log = Capped::new(path.clone(), 4);
        log.write(b"four\nfive\n", "T3");
        assert_eq!(
            std::fs::read_to_string(dir.path().join(OLD)).unwrap(),
            "T3 four\n"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "T3 five\n");
    }

    /// A line to an output that fails is lost, and does not stop the
    /// writer.
    #[test]
    fn a_line_to_a_failed_output_is_lost() {
        struct Fails;
        impl Write for Fails {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        line(&mut Fails, "riff daemon: o/r: a line");
    }

    /// A log in a folder that is gone loses its lines, and does not fail.
    #[test]
    fn a_log_that_cannot_write_loses_the_line() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = Capped::new(dir.path().join("gone").join(FILE), 40);
        log.write(b"a line\n", "T1");
        assert!(!dir.path().join("gone").exists());
    }

    /// 2026-11-14T07:12:00Z, in seconds since the epoch.
    const AT_0712: i64 = 1_794_640_320;

    /// #2156: `riff daemon log` reads the older file first, then the log
    /// file, and gives the newest lines that the query picks, with no
    /// secret. A line with no time has the time of the line before.
    #[test]
    fn read_picks_the_lines_of_both_files_since_a_time_with_a_text() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(OLD),
            "2026-11-14T07:00:00Z riff daemon: o/r: claims issue-12\n\
             2026-11-14T07:12:00Z riff daemon: o/r: claims issue-12 again\n",
        )
        .unwrap();
        // A line that is no UTF-8 is read with a replacement character.
        let mut log = b"2026-11-14T07:13:00Z riff daemon: issue-12 \xff ghs_abc1\n".to_vec();
        log.extend(b"the rest of the line of issue-12 before\n");
        log.extend(b"2026-11-14T07:14:00Z riff daemon: o/r: claims issue-13\n");
        std::fs::write(dir.path().join(FILE), log).unwrap();
        let query = Query::new(Some(AT_0712), None, vec!["issue-12".into()]).unwrap();
        let found = read(dir.path(), &query, "journalctl --user -u riff").unwrap();
        assert_eq!(
            found.lines,
            [
                "2026-11-14T07:12:00Z riff daemon: o/r: claims issue-12 again",
                "2026-11-14T07:13:00Z riff daemon: issue-12 \u{fffd} [secret]",
                "the rest of the line of issue-12 before",
            ]
        );
        assert_eq!(found.more, 0);
        // With no --since, the line of 07:00 matches too: the newest two
        // of the five lines show, and three more match.
        let newest = Query::new(None, Some(2), vec!["issue-1".into()]).unwrap();
        let found = read(dir.path(), &newest, "").unwrap();
        assert_eq!(found.more, 3);
        assert_eq!(
            found.lines[1],
            "2026-11-14T07:14:00Z riff daemon: o/r: claims issue-13"
        );
        // With one file only, riff reads that one.
        std::fs::remove_file(dir.path().join(OLD)).unwrap();
        let all = Query::new(None, None, vec![]).unwrap();
        assert_eq!(read(dir.path(), &all, "").unwrap().lines.len(), 3);
    }

    /// #2156: with no log file, for example on a Mac, the error names the
    /// file and the command that shows the log.
    #[test]
    fn read_with_no_log_file_names_the_log_command() {
        let dir = tempfile::tempdir().unwrap();
        let query = Query::new(None, None, vec![]).unwrap();
        let error = read(dir.path(), &query, "tail -f /l/daemon.log").unwrap_err();
        assert_eq!(
            error.to_string(),
            text::daemon_no_log(&dir.path().join(FILE), "tail -f /l/daemon.log")
        );
    }

    /// #2156: a long line is cut, and the oldest lines go when the lines
    /// have more than [`MOST_BYTES`]: they count as more lines.
    #[test]
    fn a_long_line_is_cut_and_the_answer_keeps_its_byte_cap() {
        let line = format!("2026-11-14T07:12:00Z {}", "x".repeat(3 * LINE_CHARS));
        let log = std::iter::repeat_n(line.as_str(), MOST_LINES)
            .collect::<Vec<_>>()
            .join("\n");
        let query = Query::new(None, Some(MOST_LINES), vec![]).unwrap();
        let found = pick([log.as_str()], &query, &Scrub::default());
        assert!(
            found
                .lines
                .iter()
                .all(|l| l.chars().count() == LINE_CHARS + 1)
        );
        let bytes: usize = found.lines.iter().map(|l| l.len() + 1).sum();
        assert!(bytes <= MOST_BYTES, "{bytes}");
        assert!(found.more > 0);
        assert_eq!(found.more + found.lines.len(), MOST_LINES);
    }

    /// #2156: with --since, a line with no time before the first line
    /// with a time does not show: riff does not know its time.
    #[test]
    fn a_first_line_with_no_time_shows_only_with_no_since() {
        let log = "a line with no time\n2026-11-14T07:12:00Z a line\n";
        let since = Query::new(Some(AT_0712), None, vec![]).unwrap();
        let found = pick([log], &since, &Scrub::default());
        assert_eq!(found.lines, ["2026-11-14T07:12:00Z a line"]);
        let all = Query::new(None, None, vec![]).unwrap();
        assert_eq!(pick([log], &all, &Scrub::default()).lines.len(), 2);
    }

    /// #2156: each key of Anthropic goes, also inside a word, with each
    /// `-` and `_` of it.
    #[test]
    fn each_key_of_anthropic_goes() {
        assert_eq!(
            keys_out("a=sk-ant-oat01-A_b-9,sk-ant-api03-x end sk-ant-"),
            "a=[secret],[secret] end [secret]"
        );
        assert_eq!(keys_out("sk-other and ask-ant"), "sk-other and ask-ant");
    }
}
