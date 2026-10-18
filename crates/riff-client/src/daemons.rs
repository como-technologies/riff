//! The daemons model: the facts of the riff daemon of each host in
//! `riff who` and `riff top` (01M57D3F50347RD8ED79KEA9V7,
//! 01M57D3F7FXTPZB3M72Z1DQ1T7, 01M57D3F9SWMKP22WNRPRY3X5T).
//!
//! # Design
//!
//! The daemon of a host has a session of its own on riff-server for
//! each repository that it serves (01M579Z9JE6HY9HG3PWR1RR0RQ). Each
//! [`EVERY`], each of these sessions sends the [`Facts`] of the host as
//! the numbers of its machine: one line, [`MARK`] and the JSON of the
//! facts ([`Facts::line`]). riff-server keeps the line and does not
//! read it. [`hosts`] reads the facts of each host back from the
//! sessions of `who`. Facts older than [`NUMBERS_TTL`] count as none:
//! the daemon stopped.
//!
//! ```mermaid
//! flowchart LR
//!     D["riff daemon on thelio"] -->|"each 30 s: machine, Facts::line"| S[riff-server]
//!     S -->|who| H["hosts: Facts::parse"]
//!     H --> W["riff who: the part Hosts"]
//!     H --> T["riff top: lines under the host"]
//! ```
//!
//! A line has at most [`MACHINE_CHARS`] characters. The last step of
//! each session has at most [`LAST_CHARS`]. When the line is still too
//! long, it lists fewer sessions, and counts the others in
//! [`Facts::more`].
//!
//! The sign-in of `claude` has an end: the daemon tells its minutes
//! left. The sign-ins of riff and of `gh` refresh, or have no end: the
//! daemon tells whether each one works ([`Sign`],
//! 01M57D3FGV57W96A7SWTEFRCGW).
//!
//! ```
//! use riff_client::daemons::{Facts, Sign};
//!
//! let facts = Facts {
//!     build: "3.1.0".into(),
//!     limit: 4,
//!     floor_gb: 8,
//!     claude: Sign::Left { minutes: 300 },
//!     ..Facts::default()
//! };
//! let line = facts.line();
//! assert!(line.starts_with("daemon facts: {"));
//! assert_eq!(Facts::parse(&line), Some(facts));
//! assert_eq!(Facts::parse("workers host: limit 2, floor 4GB, no workers"), None);
//! ```

use riff_core::wire::{MACHINE_CHARS, SessionInfo};
use serde::{Deserialize, Serialize};

use crate::machines::{Disk, Machine, NUMBERS_TTL};
use crate::sessions::Phase;

/// The start of the facts of a daemon in the numbers of its session.
pub const MARK: &str = "daemon facts: ";

/// How often the daemon sends its facts.
pub const EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// The most characters of the last step of a session in the facts.
pub const LAST_CHARS: usize = 80;

/// Under this many minutes left, the sign-in of `claude` is short: the
/// lead gets one note (01M57D3FC5PB5EQ6Y38Y5R2DBB).
pub const SHORT_MINUTES: i64 = 120;

/// The state of one sign-in on the host (01M57D3FGV57W96A7SWTEFRCGW).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Sign {
    /// The daemon did not look, or cannot say.
    #[default]
    Unknown,
    /// It works, and has no end that the host knows.
    On,
    /// It ends in this many minutes. Less than 0: it ended.
    Left {
        /// The minutes left.
        minutes: i64,
    },
    /// There is none, or it ended.
    Off,
}

impl Sign {
    /// True when the person must act soon: no sign-in, or less than
    /// [`SHORT_MINUTES`] left.
    ///
    /// ```
    /// use riff_client::daemons::Sign;
    ///
    /// assert!(Sign::Left { minutes: 119 }.short());
    /// assert!(!Sign::Left { minutes: 120 }.short());
    /// assert!(Sign::Off.short());
    /// assert!(!Sign::On.short() && !Sign::Unknown.short());
    /// ```
    pub fn short(&self) -> bool {
        match self {
            Sign::Left { minutes } => *minutes < SHORT_MINUTES,
            Sign::Off => true,
            Sign::On | Sign::Unknown => false,
        }
    }
}

/// A container session of the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// The name of its container.
    pub name: String,
    /// The work item.
    pub item: String,
    /// What it does now.
    pub phase: Phase,
    /// Its last step, at most [`LAST_CHARS`] characters.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last: String,
}

/// A wave that the daemon runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wave {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// What the wave did last.
    pub word: String,
}

/// The session image of a repository that the daemon serves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    /// The name of the image, with no digest.
    pub name: String,
    /// True when podman has the pinned image on the host.
    pub here: bool,
}

/// The facts of the daemon of a host (01M57D3F50347RD8ED79KEA9V7).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Facts {
    /// The riff build of the daemon.
    pub build: String,
    /// The most live sessions.
    pub limit: u32,
    /// The floor of free memory, in GB.
    pub floor_gb: u64,
    /// The live sessions that the line lists.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<Session>,
    /// The live sessions that the line does not list: it had no room.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub more: usize,
    /// The starts that wait for room, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waiting: Vec<String>,
    /// The waves that the daemon runs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waves: Vec<Wave>,
    /// The hardware threads, the memory and the load of the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<Machine>,
    /// The disk of the session folders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk: Option<Disk>,
    /// The version of podman, or `None` with no podman.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub podman: Option<String>,
    /// The session image of the repository, or `None` when it pins none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<Image>,
    /// The sign-in of riff to its riff-server.
    #[serde(default)]
    pub riff: Sign,
    /// The sign-in of `gh`.
    #[serde(default)]
    pub gh: Sign,
    /// The sign-in of `claude`.
    #[serde(default)]
    pub claude: Sign,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

impl Facts {
    /// The live sessions: the listed ones and the [`more`](Self::more).
    pub fn runs(&self) -> usize {
        self.sessions.len() + self.more
    }

    /// The facts as the numbers of a session: [`MARK`] and the JSON, at
    /// most [`MACHINE_CHARS`] characters. Each last step gets at most
    /// [`LAST_CHARS`]. When the line is still too long, it lists fewer
    /// sessions and counts the others, then fewer starts that wait.
    ///
    /// ```
    /// use riff_client::daemons::{Facts, LAST_CHARS, Session};
    /// use riff_client::sessions::Phase;
    ///
    /// let session = |i: usize| Session {
    ///     name: format!("riff-issue-{i}-1a2b3c4d"),
    ///     item: format!("issue-{i}"),
    ///     phase: Phase::Works,
    ///     last: "x".repeat(200),
    /// };
    /// let facts = Facts { sessions: (0..100).map(session).collect(), ..Facts::default() };
    /// let line = facts.line();
    /// assert!(line.chars().count() <= riff_core::wire::MACHINE_CHARS);
    /// let back = Facts::parse(&line).unwrap();
    /// assert_eq!(back.runs(), 100);
    /// assert!(back.more > 0);
    /// assert!(back.sessions.iter().all(|s| s.last.chars().count() == LAST_CHARS));
    /// ```
    pub fn line(&self) -> String {
        let mut facts = self.clone();
        for session in &mut facts.sessions {
            if session.last.chars().count() > LAST_CHARS {
                let cut: String = session.last.chars().take(LAST_CHARS - 1).collect();
                session.last = format!("{cut}…");
            }
        }
        let text = |facts: &Facts| {
            let json = serde_json::to_string(facts).unwrap_or_else(|_| "{}".into());
            format!("{MARK}{json}")
        };
        loop {
            let line = text(&facts);
            if line.chars().count() <= MACHINE_CHARS {
                return line;
            }
            if facts.sessions.pop().is_some() {
                facts.more += 1;
            } else if facts.waiting.pop().is_none() && facts.waves.pop().is_none() {
                return line;
            }
        }
    }

    /// The facts in the numbers of a session, or `None` for other
    /// numbers.
    pub fn parse(numbers: &str) -> Option<Facts> {
        serde_json::from_str(numbers.strip_prefix(MARK)?).ok()
    }
}

/// The facts of the daemon of one host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Host {
    /// The user of the daemon.
    pub user: String,
    /// The host.
    pub host: String,
    /// The seconds since the daemon sent them.
    pub age_secs: u64,
    /// The facts.
    pub facts: Facts,
}

/// The newest facts of each daemon in `sessions`, by user and host
/// (01M57D3F7FXTPZB3M72Z1DQ1T7). A daemon sends the same facts in the
/// session of each repository that it serves: the youngest wins. Facts
/// older than [`NUMBERS_TTL`] count as none.
///
/// ```
/// use riff_client::daemons::{Facts, hosts};
/// use riff_core::wire::{MachineInfo, SessionInfo};
///
/// let session = |uri: &str, facts: &Facts, age_secs: u64| -> SessionInfo {
///     let mut s: SessionInfo = serde_json::from_value(serde_json::json!({ "uri": uri, "live": true })).unwrap();
///     s.machine = Some(MachineInfo { numbers: facts.line(), age_secs });
///     s
/// };
/// let a = Facts { limit: 2, ..Facts::default() };
/// let b = Facts { limit: 5, ..Facts::default() };
/// let sessions = [
///     session("riff://mike@a/o/r?session=d1", &a, 40),
///     session("riff://mike@a/o/s?session=d2", &Facts { limit: 3, ..a.clone() }, 10),
///     session("riff://mike@b/o/r?session=d3", &b, 5),
///     session("riff://mike@c/o/r?session=d4", &b, 181),
/// ];
/// let found = hosts(&sessions);
/// let seen: Vec<(&str, u32)> = found.iter().map(|h| (h.host.as_str(), h.facts.limit)).collect();
/// assert_eq!(seen, [("a", 3), ("b", 5)]);
/// ```
pub fn hosts<'a>(sessions: impl IntoIterator<Item = &'a SessionInfo>) -> Vec<Host> {
    let mut found: Vec<Host> = Vec::new();
    for s in sessions.into_iter().filter(|s| s.live) {
        let Some(machine) = &s.machine else { continue };
        if machine.age_secs > NUMBERS_TTL.as_secs() {
            continue;
        }
        let Some(facts) = Facts::parse(&machine.numbers) else {
            continue;
        };
        let host = Host {
            user: s.uri.who().user().to_owned(),
            host: s.uri.place().host().to_owned(),
            age_secs: machine.age_secs,
            facts,
        };
        match found
            .iter_mut()
            .find(|h| h.user == host.user && h.host == host.host)
        {
            Some(old) if old.age_secs > host.age_secs => *old = host,
            Some(_) => {}
            None => found.push(host),
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_facts_are_plain_data_with_serde() {
        let facts = Facts {
            build: "3.1.0".into(),
            limit: 4,
            floor_gb: 8,
            sessions: vec![Session {
                name: "riff-issue-12-1a2b3c4d".into(),
                item: "issue-12".into(),
                phase: Phase::Ends,
                last: "end: success".into(),
            }],
            more: 0,
            waiting: vec!["riff-issue-13-2b3c4d5e".into()],
            waves: vec![Wave {
                repo: "o/r".into(),
                word: "started issue-12".into(),
            }],
            machine: Some(Machine {
                threads: 16,
                mhz: 4500,
                now_mhz: 4400,
                mem_gb: 32,
                avail_gb: 24,
                load: 1.5,
            }),
            disk: Some(Disk {
                free_gb: 200,
                total_gb: 455,
            }),
            podman: Some("5.2.1".into()),
            image: Some(Image {
                name: "ghcr.io/o/riff-session".into(),
                here: true,
            }),
            riff: Sign::On,
            gh: Sign::Off,
            claude: Sign::Left { minutes: 90 },
        };
        assert_eq!(Facts::parse(&facts.line()), Some(facts.clone()));
        assert!(!facts.line().contains('\n'));
    }

    #[test]
    fn a_line_with_no_room_lists_fewer_starts_that_wait() {
        let facts = Facts {
            waiting: (0..400)
                .map(|i| format!("riff-issue-{i}-00000000"))
                .collect(),
            ..Facts::default()
        };
        let line = facts.line();
        assert!(line.chars().count() <= MACHINE_CHARS);
        assert!(Facts::parse(&line).unwrap().waiting.len() < 400);
    }
}
