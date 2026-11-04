//! The tally: where each riff process but the daemon adds its calls of
//! `gh` (#1582).
//!
//! # Design
//!
//! The daemon counts its own calls in memory ([`Calls`](super::Calls)).
//! The other callers are processes of their own: `riff top`, `riff pr
//! wait`, `riff verify`, the release commands, the look of a lead, and
//! each `gh` in a container session. They add their calls to one folder
//! of the daemon, `gh-calls`. The daemon reads it when it builds the
//! status.
//!
//! | Where the process runs | [`Tally`] | How a call gets to the folder |
//! |---|---|---|
//! | on the host | [`Tally::Folder`] | The process appends one line to the file of the hour. |
//! | in a container | [`Tally::Socket`] | The process sends [`Record`]s on the socket of its session. The daemon appends them. |
//!
//! A container has no access to the folder of the daemon, and a session
//! must not write the data of the host. So the daemon takes the records
//! of a session, and cleans them: it keeps the caller only when riff
//! knows its name, cuts the kind, and limits the points and the number of
//! records ([`clean`]).
//!
//! The folder holds one file for each hour of the clock,
//! `HOUR.jsonl`, with one [`Call`] on each line. A reader needs the last
//! hour, so it reads this hour and the one before. It removes the older
//! files, and the writer does too. No process rewrites a file, so a
//! reader never meets a half-written file. A line is one write in append
//! mode.
//!
//! A count never fails the call: when riff cannot write the record, it
//! drops it.
//!
//! ```mermaid
//! flowchart LR
//!     T["riff top, riff pr wait, ..."] -- "Tally::Folder" --> F["gh-calls/HOUR.jsonl"]
//!     G["gh in a container"] -- "Tally::Socket" --> D["the daemon:<br/>SessionAsk::GhCalls"]
//!     D -- "clean, then append" --> F
//!     F -- "Tally::read" --> S["Calls::shown_with"]
//! ```

use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{Call, OTHER, WINDOW_MS};
use crate::daemon::{PROTOCOL, SOCKET_VAR, SessionAsk};

/// The folder of the tally, in the folder of the daemon.
pub const FOLDER: &str = "gh-calls";

/// The caller of a `gh` in a container session: the `gh` that the agent
/// runs, and not a command of `riff`.
pub const SESSION: &str = "session";

/// The names of the callers that a session can give. They are the
/// commands of `riff` that call `gh`, and [`SESSION`]. A session cannot
/// add a new row to the status with a name of its own.
pub const CALLERS: &[&str] = &[
    SESSION, "top", "pr open", "pr wait", "verify", "release", "waves", "audit", "usage", "look",
];

/// The most records in one send of a session.
pub const MAX_RECORDS: usize = 64;

/// The most characters of the kind of a call.
const MAX_KIND: usize = 80;

/// The most points of one record. The most that a query of `gh` costs is
/// far less.
const MAX_POINTS: u32 = 100;

/// The size of a file of the folder at which riff adds no more to it.
const MAX_FILE: u64 = 4 << 20;

/// How long a process waits to send its records to the daemon: a `gh`
/// does not wait for the count.
const SEND_WAIT: Duration = Duration::from_millis(300);

/// One call, as a process sends it to the daemon: the daemon adds the
/// time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// Who called ([`super::Row::caller`]).
    pub caller: String,
    /// The kind of call ([`super::kind_of`]).
    pub kind: String,
    /// The GraphQL points that it cost ([`super::points_of`]).
    pub points: u32,
}

/// The record `record` of a session, as the daemon keeps it: the caller
/// is one of the [`CALLERS`] (else [`SESSION`]), the kind has at most 80
/// printable characters, and the points are at most 100.
///
/// ```
/// use riff::ghcalls::{Record, clean};
///
/// let record = |caller: &str, kind: &str, points| Record {
///     caller: caller.into(),
///     kind: kind.into(),
///     points,
/// };
/// assert_eq!(clean(record("pr wait", "pr view", 1)), record("pr wait", "pr view", 1));
/// // A name that riff does not know is a session.
/// assert_eq!(clean(record("not a name", "pr view", 1)).caller, "session");
/// // A kind has no control character and is short.
/// assert_eq!(clean(record("top", "pr\nview\u{1b}[31m", 1)).kind, "pr view [31m");
/// assert_eq!(clean(record("top", &"x".repeat(500), 1)).kind.chars().count(), 80);
/// assert_eq!(clean(record("top", "pr view", 4_000_000)).points, 100);
/// ```
pub fn clean(record: Record) -> Record {
    let caller = CALLERS
        .iter()
        .find(|name| **name == record.caller)
        .copied()
        .unwrap_or(SESSION);
    let kind: String = record
        .kind
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_KIND)
        .collect();
    Record {
        caller: caller.to_owned(),
        kind: if kind.is_empty() {
            OTHER.to_owned()
        } else {
            kind
        },
        points: record.points.min(MAX_POINTS),
    }
}

/// Where a riff process adds its calls of `gh`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tally {
    /// The folder of the daemon: a process on the host appends there.
    Folder(PathBuf),
    /// The socket of a session: a process in a container sends the
    /// records there, and the daemon appends them.
    Socket(PathBuf),
}

impl Tally {
    /// The tally in the folder `dir` of the daemon ([`crate::daemon::dir`]).
    pub fn in_folder(dir: &Path) -> Self {
        Self::Folder(dir.to_owned())
    }

    /// The tally on the socket `socket` of a session.
    pub fn on_socket(socket: &Path) -> Self {
        Self::Socket(socket.to_owned())
    }

    /// The tally of this process: the socket of the session when
    /// `RIFF_SOCKET` names one, else the folder of the daemon of this
    /// machine. `None` when the machine has no data folder.
    pub fn here() -> Option<Self> {
        match std::env::var_os(SOCKET_VAR).filter(|socket| !socket.is_empty()) {
            Some(socket) => Some(Self::Socket(socket.into())),
            None => crate::daemon::dir().ok().map(Self::Folder),
        }
    }

    /// Adds a call that ended now. It never fails: a count that cannot be
    /// written is dropped.
    pub fn record(&self, caller: &str, kind: &str, points: u32) {
        self.record_at(crate::daemon::now_ms(), caller, kind, points);
    }

    /// Adds a call that ended at `at_ms`. A [`Tally::Socket`] sends it, and
    /// the daemon gives it the time of its arrival.
    ///
    /// ```
    /// use riff::ghcalls::Tally;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let tally = Tally::in_folder(dir.path());
    /// tally.record_at(5_000, "top", "pr list", 2);
    /// tally.record_at(6_000, "pr wait", "pr view", 1);
    /// let calls = tally.read(10_000);
    /// assert_eq!(calls.len(), 2);
    /// assert_eq!((calls[0].caller.as_str(), calls[0].points), ("top", 2));
    /// // An hour later the calls are gone.
    /// assert!(tally.read(3_700_000).is_empty());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn record_at(&self, at_ms: i64, caller: &str, kind: &str, points: u32) {
        let record = Record {
            caller: caller.to_owned(),
            kind: kind.to_owned(),
            points,
        };
        self.record_all(at_ms, vec![record]);
    }

    /// Adds `records`, all at `at_ms`. A [`Tally::Folder`] keeps them as
    /// they are. The daemon cleans the records of a session first
    /// ([`clean`]).
    pub fn record_all(&self, at_ms: i64, records: Vec<Record>) {
        let _ = match self {
            Self::Folder(dir) => append(dir, at_ms, &records),
            Self::Socket(socket) => send(socket, records),
        };
    }

    /// The calls of the last hour at `now_ms` in the folder, and nothing
    /// for a [`Tally::Socket`]. It removes the files older than the last
    /// hour. A line that is no call is dropped.
    pub fn read(&self, now_ms: i64) -> Vec<Call> {
        let Self::Folder(dir) = self else {
            return Vec::new();
        };
        let folder = dir.join(FOLDER);
        let hour = now_ms.div_euclid(WINDOW_MS);
        let mut calls = Vec::new();
        for hour in [hour - 1, hour] {
            let Ok(file) = File::open(file_of(&folder, hour)) else {
                continue;
            };
            let lines = BufReader::new(file.take(2 * MAX_FILE)).lines();
            for line in lines.map_while(Result::ok) {
                if let Ok(call) = serde_json::from_str::<Call>(&line)
                    && now_ms - call.at_ms < WINDOW_MS
                {
                    calls.push(call);
                }
            }
        }
        forget(&folder, hour);
        calls
    }
}

/// The file of the hour `hour` of the clock in the folder `folder`.
fn file_of(folder: &Path, hour: i64) -> PathBuf {
    folder.join(format!("{hour}.jsonl"))
}

/// Adds `records` at `at_ms` to the file of its hour in the tally of the
/// daemon folder `dir`.
fn append(dir: &Path, at_ms: i64, records: &[Record]) -> std::io::Result<()> {
    if records.is_empty() {
        return Ok(());
    }
    let folder = dir.join(FOLDER);
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&folder)?;
    let hour = at_ms.div_euclid(WINDOW_MS);
    let path = file_of(&folder, hour);
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_FILE) {
        return Ok(());
    }
    let mut text = String::new();
    for record in records {
        let call = Call {
            at_ms,
            caller: record.caller.clone(),
            kind: record.kind.clone(),
            points: record.points,
        };
        text.push_str(&serde_json::to_string(&call)?);
        text.push('\n');
    }
    // One write in append mode: the lines of two processes do not mix.
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)?
        .write_all(text.as_bytes())?;
    forget(&folder, hour);
    Ok(())
}

/// Removes the files of `folder` that are older than the hour before
/// `hour`.
fn forget(folder: &Path, hour: i64) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .file_name()
            .to_str()
            .and_then(|name| name.strip_suffix(".jsonl"))
            .and_then(|name| name.parse::<i64>().ok())
            .is_some_and(|file| file < hour - 1);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Sends `records` to the daemon on the socket `socket` of a session
/// ([`SessionAsk::GhCalls`]), and does not wait for its answer.
fn send(socket: &Path, records: Vec<Record>) -> std::io::Result<()> {
    use std::os::unix::net::UnixStream;
    #[cfg(target_os = "linux")]
    let mut conn = {
        use std::os::fd::AsRawFd;
        let (Some(dir), Some(name)) = (socket.parent(), socket.file_name()) else {
            return Err(std::io::ErrorKind::InvalidInput.into());
        };
        // Through a descriptor of the folder, so that a long path works.
        let folder = File::open(dir)?;
        let path = format!(
            "/proc/self/fd/{}/{}",
            folder.as_raw_fd(),
            name.to_string_lossy()
        );
        UnixStream::connect(path)?
    };
    #[cfg(not(target_os = "linux"))]
    let mut conn = UnixStream::connect(socket)?;
    conn.set_write_timeout(Some(SEND_WAIT))?;
    let ask = SessionAsk::GhCalls {
        protocol: PROTOCOL,
        calls: records,
    };
    let mut line = serde_json::to_string(&ask)?;
    line.push('\n');
    conn.write_all(line.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 01M6JGF6Z6Y8FP86Q9VAS3NPZ7 (#1582): the calls of the other riff
    /// processes add up in one folder, and a reader sees the last hour.
    #[test]
    fn processes_add_their_calls_to_one_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (top, wait) = (Tally::in_folder(dir.path()), Tally::in_folder(dir.path()));
        top.record_at(1_000, "top", "pr list", 2);
        wait.record_at(2_000, "pr wait", "pr view", 1);
        top.record_at(3_000, "top", "pr list", 2);
        let calls = wait.read(4_000);
        let callers: Vec<&str> = calls.iter().map(|c| c.caller.as_str()).collect();
        assert_eq!(callers, ["top", "pr wait", "top"]);
        // The folder is private.
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join(FOLDER))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
    }

    /// A reader reads this hour and the one before, and the files that are
    /// older go.
    #[test]
    fn a_reader_reads_two_hours_and_removes_the_older_files() {
        let dir = tempfile::tempdir().unwrap();
        let tally = Tally::in_folder(dir.path());
        let hour = WINDOW_MS;
        tally.record_at(10, "top", "pr list", 2);
        tally.record_at(hour - 10, "top", "pr list", 2);
        tally.record_at(2 * hour + 10, "top", "pr list", 2);
        // The writer removed the file of hour 0 when it wrote hour 2.
        let files = |dir: &Path| -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(dir.join(FOLDER))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };
        assert_eq!(files(dir.path()), ["2.jsonl"]);
        // Only the call of hour 2 is less than an hour old at 2h + 20 s.
        assert_eq!(tally.read(2 * hour + 20_000).len(), 1);
        // A call at the end of one hour is still in the next hour.
        let next = tempfile::tempdir().unwrap();
        let tally = Tally::in_folder(next.path());
        tally.record_at(hour - 10, "top", "pr list", 2);
        assert_eq!(tally.read(hour + 100_000).len(), 1);
        assert_eq!(tally.read(hour + 100).len(), 1);
        assert_eq!(tally.read(2 * hour + 100).len(), 0);
    }

    /// A line that is no call does not stop the reader.
    #[test]
    fn a_line_that_is_no_call_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let tally = Tally::in_folder(dir.path());
        tally.record_at(1_000, "top", "pr list", 2);
        let file = dir.path().join(FOLDER).join("0.jsonl");
        let mut text = std::fs::read_to_string(&file).unwrap();
        text.push_str("not json\n{\"at_ms\":1}\n");
        std::fs::write(&file, text).unwrap();
        tally.record_at(2_000, "top", "pr list", 2);
        assert_eq!(tally.read(3_000).len(), 2);
    }

    /// A tally in a folder that does not exist makes the folder; a tally
    /// that cannot write drops the call and does not fail.
    #[test]
    fn a_count_never_fails_the_call() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not-a-folder");
        std::fs::write(&file, "x").unwrap();
        Tally::in_folder(&file).record_at(1, "top", "pr list", 2);
        assert!(Tally::in_folder(&file).read(2).is_empty());
        Tally::on_socket(&dir.path().join("no.sock")).record_at(1, "top", "pr list", 2);
        let deep = dir.path().join("a/b");
        Tally::in_folder(&deep).record_at(1, "top", "pr list", 2);
        assert_eq!(Tally::in_folder(&deep).read(2).len(), 1);
    }

    /// A file at its size limit takes no more.
    #[test]
    fn a_big_file_takes_no_more() {
        let dir = tempfile::tempdir().unwrap();
        let tally = Tally::in_folder(dir.path());
        tally.record_at(1_000, "top", "pr list", 2);
        let file = dir.path().join(FOLDER).join("0.jsonl");
        let big = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
        big.set_len(MAX_FILE + 1).unwrap();
        tally.record_at(2_000, "top", "pr list", 2);
        assert_eq!(std::fs::metadata(&file).unwrap().len(), MAX_FILE + 1);
    }

    /// 01M6JGF6Z6Y8FP86Q9VAS3NPZ7: a send to a socket reaches a listener as
    /// one line, the ask [`SessionAsk::GhCalls`].
    #[test]
    fn a_session_sends_its_records_on_its_socket() {
        use std::io::{BufRead, BufReader};
        use std::os::unix::net::UnixListener;
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("riff.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let reader = std::thread::spawn(move || {
            let (conn, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(conn).read_line(&mut line).unwrap();
            line
        });
        Tally::on_socket(&socket).record_at(1, "pr wait", "pr view", 1);
        let line = reader.join().unwrap();
        let ask: SessionAsk = serde_json::from_str(&line).unwrap();
        let SessionAsk::GhCalls { protocol, calls } = ask else {
            panic!("{line}");
        };
        assert_eq!(protocol, PROTOCOL);
        assert_eq!(
            calls,
            [Record {
                caller: "pr wait".into(),
                kind: "pr view".into(),
                points: 1
            }]
        );
    }
}
