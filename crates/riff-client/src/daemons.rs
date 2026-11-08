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
//! The facts carry the [`Parts`] of the host in each repository that its
//! daemon serves: `workers`, `lead`, both or none (#1643). `riff who`
//! shows them as the line `parts`. A daemon of an older build sends none.
//! They carry the [`Caps`] of each new container of the host too: the
//! Machines tab of the TUI shows them.
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

use std::collections::BTreeMap;

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

/// The hex digits of a short digest ([`short`]).
pub const SHORT_DIGEST: usize = 12;

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

/// The long-lived token of `claude setup-token` that the daemon gives
/// each container session (01M58B7HA8CRCW0MJ17QP3R5QG). The facts never
/// hold its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Token {
    /// The daemon did not say: it is older.
    #[default]
    Unknown,
    /// No token is set: the daemon binds the sign-in file of `claude`.
    NotSet,
    /// A token is set.
    Set,
    /// The OS keyring is locked, or does not answer: the daemon cannot
    /// read the token, and starts no session.
    Locked,
    /// A token is set, and `claude` refused it.
    Refused,
}

impl Token {
    /// The word of the state.
    ///
    /// ```
    /// use riff_client::daemons::Token;
    ///
    /// assert_eq!(Token::Set.word(), "set");
    /// assert_eq!(Token::NotSet.word(), "not set");
    /// assert_eq!(Token::Refused.word(), "refused");
    /// assert_eq!(Token::Locked.word(), "keyring locked");
    /// assert_eq!(Token::Unknown.word(), "?");
    /// ```
    pub fn word(self) -> &'static str {
        match self {
            Token::Unknown => "?",
            Token::NotSet => "not set",
            Token::Set => "set",
            Token::Refused => "refused",
            Token::Locked => "keyring locked",
        }
    }

    /// True when a token is set, also a refused one: the sessions do not
    /// use the sign-in file of `claude`.
    ///
    /// ```
    /// use riff_client::daemons::Token;
    ///
    /// assert!(Token::Set.set() && Token::Refused.set() && Token::Locked.set());
    /// assert!(!Token::NotSet.set() && !Token::Unknown.set());
    /// ```
    pub fn set(self) -> bool {
        matches!(self, Token::Set | Token::Refused | Token::Locked)
    }

    /// True when the person must act: no token, or a refused one.
    ///
    /// ```
    /// use riff_client::daemons::Token;
    ///
    /// assert!(Token::NotSet.warns() && Token::Refused.warns() && Token::Locked.warns());
    /// assert!(!Token::Set.warns() && !Token::Unknown.warns());
    /// ```
    pub fn warns(self) -> bool {
        matches!(self, Token::NotSet | Token::Refused | Token::Locked)
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
    /// The short digest ([`short`]) of the image that its container
    /// runs, or empty when podman did not say.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub digest: String,
}

/// A wave that the daemon runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wave {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// What the wave did last.
    pub word: String,
    /// The merged pin of the repository, or `None` when its main clone
    /// has no good merged pin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<Image>,
    /// The verifies that wait for the merge of another pull request with
    /// a file in common, by issue, with the number of that pull request
    /// (#1710). `riff wave list` shows them ([`crate::waves::Board`]).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub waits: BTreeMap<u64, u64>,
}

/// What a host does in one repository: it gives workers (the daemon runs
/// the wave), it runs the lead of a person, both, or none (#1643).
///
/// ```
/// use riff_client::daemons::Parts;
///
/// assert_eq!(Parts { workers: true, lead: false }.word(), "workers");
/// assert_eq!(Parts { workers: false, lead: true }.word(), "lead");
/// assert_eq!(Parts { workers: true, lead: true }.word(), "workers and lead");
/// assert!(Parts::default().none());
/// assert_eq!(Parts::default().word(), "none");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Parts {
    /// The daemon of the host runs the wave of the repository: the work
    /// items and the verifies.
    #[serde(default)]
    pub workers: bool,
    /// The host runs the lead of a person in the repository.
    #[serde(default)]
    pub lead: bool,
}

impl Parts {
    /// True when the host has no part.
    pub fn none(self) -> bool {
        !self.workers && !self.lead
    }

    /// The parts in words.
    pub fn word(self) -> &'static str {
        match (self.workers, self.lead) {
            (true, true) => "workers and lead",
            (true, false) => "workers",
            (false, true) => "lead",
            (false, false) => "none",
        }
    }
}

/// The parts of a host in one repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoParts {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// The parts of the host in it.
    #[serde(flatten)]
    pub parts: Parts,
}

/// The real-session test that the daemon runs (01M5QVBTXE8YRY44A1C1VP71QS).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Test {
    /// The tag of the release under test.
    pub tag: String,
    /// The commit that the test runs on.
    pub commit: String,
    /// The seconds that the test has run, at the time of the status.
    pub secs: u64,
}

/// The session image that the merged pin of a wave names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    /// The short digest ([`short`]) of the pinned image.
    pub digest: String,
    /// True when podman has the pinned image on the host.
    pub here: bool,
}

/// The caps of each new container of a host: `riff daemon limit
/// --cpus N --memory GB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caps {
    /// The CPUs of each new container.
    pub cpus: u32,
    /// The memory cap of each new container, in GB.
    pub memory_gb: u64,
}

/// The short form of a digest: the first [`SHORT_DIGEST`] hex digits
/// after `sha256:`. A text with no `sha256:` is cut the same way.
///
/// ```
/// use riff_client::daemons::short;
///
/// assert_eq!(short(&format!("sha256:{}", "0123456789abcdef".repeat(4))), "0123456789ab");
/// assert_eq!(short("0123456789abcdef"), "0123456789ab");
/// assert_eq!(short("sha256:abc"), "abc");
/// assert_eq!(short(""), "");
/// ```
pub fn short(digest: &str) -> String {
    let hex = digest.strip_prefix("sha256:").unwrap_or(digest);
    hex.chars().take(SHORT_DIGEST).collect()
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
    /// The parts of the host in each repository that its daemon serves
    /// (#1643).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<RepoParts>,
    /// The real-session test that the daemon runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<Test>,
    /// The hardware threads, the memory and the load of the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<Machine>,
    /// The disk of the session folders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk: Option<Disk>,
    /// The caps of each new container. A daemon of an older build sends
    /// none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caps: Option<Caps>,
    /// The version of podman, or `None` with no podman.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub podman: Option<String>,
    /// The sign-in of riff to its riff-server.
    #[serde(default)]
    pub riff: Sign,
    /// The sign-in of `gh`.
    #[serde(default)]
    pub gh: Sign,
    /// The sign-in of `claude`.
    #[serde(default)]
    pub claude: Sign,
    /// The long-lived token of `claude setup-token`.
    #[serde(default)]
    pub claude_token: Token,
    /// True when the host updates riff by itself (`update.auto`). A daemon
    /// of an older build sends none (01M6WFNDS76NESPXHMQPGHHQSS).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto: Option<bool>,
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
    ///     digest: "0123456789ab".into(),
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
            } else if facts.waiting.pop().is_none()
                && facts.waves.pop().is_none()
                && facts.parts.pop().is_none()
            {
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
                digest: "0123456789ab".into(),
            }],
            more: 0,
            waiting: vec!["riff-issue-13-2b3c4d5e".into()],
            waves: vec![Wave {
                repo: "o/r".into(),
                word: "started issue-12".into(),
                image: Some(Image {
                    digest: "0123456789ab".into(),
                    here: true,
                }),
                waits: BTreeMap::from([(13, 41)]),
            }],
            parts: vec![RepoParts {
                repo: "o/r".into(),
                parts: Parts {
                    workers: true,
                    lead: false,
                },
            }],
            test: Some(Test {
                tag: "v4.1.1".into(),
                commit: "90e36bfd".into(),
                secs: 185,
            }),
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
            caps: Some(Caps {
                cpus: 12,
                memory_gb: 16,
            }),
            podman: Some("5.2.1".into()),
            riff: Sign::On,
            gh: Sign::Off,
            claude: Sign::Left { minutes: 90 },
            claude_token: Token::Set,
            auto: Some(true),
        };
        assert_eq!(Facts::parse(&facts.line()), Some(facts.clone()));
        assert!(!facts.line().contains('\n'));
    }

    /// 01M6WFNDS76NESPXHMQPGHHQSS: the facts carry `update.auto` of the
    /// host. An older daemon sends none.
    #[test]
    fn the_facts_carry_update_auto_and_an_older_daemon_sends_none() {
        let old = r#"daemon facts: {"build":"4.12.0","limit":8,"floor_gb":8}"#;
        assert_eq!(Facts::parse(old).unwrap().auto, None);
        let off = Facts {
            auto: Some(false),
            ..Facts::default()
        };
        assert!(off.line().contains(r#""auto":false"#), "{}", off.line());
        assert_eq!(Facts::parse(&off.line()).unwrap().auto, Some(false));
        assert!(!Facts::default().line().contains("auto"));
    }

    #[test]
    fn the_facts_of_a_3_2_0_daemon_give_the_threads() {
        let old = r#"daemon facts: {"build":"3.2.0","limit":8,"floor_gb":8,"machine":{"cores":32,"mhz":5883,"now_mhz":3848,"mem_gb":124,"avail_gb":117,"load":0.1}}"#;
        let facts = Facts::parse(old).unwrap();
        let machine = facts.machine.unwrap();
        assert_eq!(machine.threads, 32);
        assert_eq!((machine.mhz, machine.avail_gb), (5883, 117));
        // An old daemon tells no caps.
        assert_eq!(facts.caps, None);
        assert!(facts.line().contains(r#""cores":32"#));
    }

    /// An old daemon sent the image of one repository for the host, and no
    /// digest: the new facts read it with no image and no digest.
    #[test]
    fn the_facts_of_an_old_daemon_have_no_wave_image_and_no_digest() {
        let old = r#"daemon facts: {"build":"4.3.0","limit":8,"floor_gb":8,"sessions":[{"name":"riff-issue-12-1a2b3c4d","item":"issue-12","phase":"works"}],"waves":[{"repo":"o/r","word":"waits"}],"image":{"name":"ghcr.io/o/i","here":true}}"#;
        let facts = Facts::parse(old).unwrap();
        assert_eq!(facts.waves[0].image, None);
        assert_eq!(facts.sessions[0].digest, "");
        // A fact with no digest adds nothing to the line.
        assert!(!facts.line().contains("digest"));
        assert!(!facts.line().contains("image"));
    }

    /// #1643: the facts of an older daemon have no parts, and the parts
    /// round-trip with the line.
    #[test]
    fn the_parts_come_back_from_the_line_and_an_older_daemon_sends_none() {
        let old = r#"daemon facts: {"build":"4.10.0","limit":8,"floor_gb":8}"#;
        assert!(Facts::parse(old).unwrap().parts.is_empty());
        let facts = Facts {
            parts: vec![
                RepoParts {
                    repo: "o/r".into(),
                    parts: Parts {
                        workers: true,
                        lead: true,
                    },
                },
                RepoParts {
                    repo: "o/s".into(),
                    parts: Parts {
                        workers: false,
                        lead: true,
                    },
                },
            ],
            ..Facts::default()
        };
        let line = facts.line();
        assert!(line.contains(r#""parts":[{"repo":"o/r","workers":true,"lead":true}"#));
        assert_eq!(Facts::parse(&line), Some(facts));
        assert!(!Facts::default().line().contains("parts"));
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
