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
//! daemon serves: `workers`, `lead`, both or none (#1643). `riff who` shows
//! them as the line `parts`. A daemon of an older build sends none. They
//! carry the [`Leaving`] of each repository where the host takes no new claim
//! and still runs sessions, for example after a `riff host leave`
//! (01M6WJYQJ58MHQF1XPXXZW2ET6): `riff who` and the Machines tab of the TUI
//! show it. They carry the [`Caps`] of each new container of the host too:
//! the Machines tab shows them. They carry the [`Release`] of each watch of a
//! deploy or of a check that the daemon follows (01M6XTS7RGXB30MFM6DZ7PVZ75),
//! and of each watch of a wave from the cut to the check
//! (01M75R6FPVZA3JA5C48PQ26A85): `riff who` and the header of the TUI make
//! the line of the open release from the facts of each host ([`release`]).
//! With no open release, they
//! make the line of a wave with no open item whose release waits for its
//! cut ([`Complete`], [`complete`], 01M75N6YNHQQHGS0VGKTKSNJTR).
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
    /// The verifies and the rebases that wait for another pull request whose
    /// head conflicts with theirs, by issue, with the number of that pull
    /// request (#1710, #1801, #1895). `riff wave list` and the Waves tab show them
    /// ([`crate::waves::Board`]). The keys are text on the wire
    /// ([`riff_core::number_keys`]).
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        with = "riff_core::number_keys"
    )]
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

/// A repository where the host takes no new claim, and still runs
/// sessions of it: they go on to their end after a `riff host leave`
/// (01M6WJYQJ58MHQF1XPXXZW2ET6). A leaving with no session is a
/// repository that the daemon stopped serving at the look before its
/// last one.
///
/// ```
/// use riff_client::daemons::Leaving;
///
/// let leaving = Leaving { repo: "o/r".into(), sessions: vec!["riff-issue-12-1a2b3c4d".into()] };
/// assert!(!leaving.left());
/// assert!(Leaving { sessions: vec![], ..leaving }.left());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leaving {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// The names of the containers of its sessions that still run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<String>,
}

impl Leaving {
    /// True when no session of the repository runs any more: the host
    /// left it.
    pub fn left(&self) -> bool {
        self.sessions.is_empty()
    }
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

/// A step of a release that a watch of the daemon follows: the watch of the
/// wave reads each step from the cut to the check
/// (01M75R6FPVZA3JA5C48PQ26A85), and the watch of the deploy, then the watch
/// of the check of the lead, read the steps from the wait for the approve
/// (01M6XTS7RGXB30MFM6DZ7PVZ75). The order of the variants is the order of
/// the steps. No step is a step of a release pull request
/// (01M76SBPK34YSRSFCC97H8SFH7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseStep {
    /// The last commit of `main` has its release build and a passed release
    /// test: the lead starts the real-session test next.
    Merged,
    /// The real-session test runs.
    TestRuns,
    /// The real-session test failed: the lead files the bug next.
    TestFailed,
    /// The real-session test passed: the lead cuts the release next.
    Tested,
    /// The tag and the GitHub release exist: the jobs of the CI run of the
    /// tag before the job `Deploy` run.
    Published,
    /// The job `Deploy` waits for a person.
    Waits,
    /// The job `Deploy` runs.
    Runs,
    /// The job `Deploy` passed: the lead checks the release next.
    Passed,
    /// The job `Deploy` failed: the lead checks next, and riff files the
    /// issue.
    DeployFailed,
    /// A check after the deploy failed.
    CheckFailed,
}

/// The release that a watch of the daemon follows, at its step
/// (01M6XTS7RGXB30MFM6DZ7PVZ75, 01M75R6FPVZA3JA5C48PQ26A85).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// The tag of the release, `vX.Y.Z`.
    pub tag: String,
    /// The step that the watch saw last.
    pub step: ReleaseStep,
}

/// The open release of the riff in the facts of `hosts`
/// (01M6XTS7RGXB30MFM6DZ7PVZ75). Of the releases of one repository, the
/// newest tag wins, at the latest step that a host saw. Of more repositories,
/// the first by name wins. `None` when no watch of a host follows a release.
///
/// ```
/// use riff_client::daemons::{Facts, Host, Release, ReleaseStep, release};
///
/// let host = |name: &str, releases: Vec<Release>| Host {
///     user: "mike".into(),
///     host: name.into(),
///     age_secs: 5,
///     facts: Facts { releases, ..Facts::default() },
/// };
/// let of = |repo: &str, tag: &str, step| Release { repo: repo.into(), tag: tag.into(), step };
/// assert_eq!(release(&[host("a", vec![])]), None);
/// // The watch of the person's host saw the run first.
/// let hosts = [
///     host("a", vec![of("o/r", "v3.3.0", ReleaseStep::Waits)]),
///     host("b", vec![of("o/r", "v3.3.0", ReleaseStep::Runs)]),
/// ];
/// assert_eq!(release(&hosts), Some(of("o/r", "v3.3.0", ReleaseStep::Runs)));
/// // The watch of the wave of one host read the stage of the release
/// // before the watch of the wave of another host (01M75R6FPVZA3JA5C48PQ26A85).
/// let hosts = [
///     host("a", vec![of("o/r", "v3.3.0", ReleaseStep::TestRuns)]),
///     host("b", vec![of("o/r", "v3.3.0", ReleaseStep::Merged)]),
/// ];
/// assert_eq!(release(&hosts), Some(of("o/r", "v3.3.0", ReleaseStep::TestRuns)));
/// // A fix forward: the newer tag wins, also at an earlier step.
/// let hosts = [
///     host("a", vec![of("o/r", "v3.3.0", ReleaseStep::CheckFailed)]),
///     host("b", vec![of("o/r", "v3.3.1", ReleaseStep::Waits), of("o/q", "v1.0.0", ReleaseStep::Runs)]),
/// ];
/// assert_eq!(release(&hosts), Some(of("o/q", "v1.0.0", ReleaseStep::Runs)));
/// assert_eq!(release(&hosts[..1]), Some(of("o/r", "v3.3.0", ReleaseStep::CheckFailed)));
/// let fixed = [host("b", vec![of("o/r", "v3.3.1", ReleaseStep::Waits)]), hosts[0].clone()];
/// assert_eq!(release(&fixed), Some(of("o/r", "v3.3.1", ReleaseStep::Waits)));
/// ```
pub fn release(hosts: &[Host]) -> Option<Release> {
    let mut found: Option<&Release> = None;
    for release in hosts.iter().flat_map(|h| &h.facts.releases) {
        let better = match found {
            None => true,
            Some(old) if old.repo != release.repo => release.repo < old.repo,
            Some(old) if old.tag != release.tag => newer(&release.tag, &old.tag),
            Some(old) => release.step > old.step,
        };
        if better {
            found = Some(release);
        }
    }
    found.cloned()
}

/// Why the cut of the release of a complete wave waits
/// (01M75N6YNHQQHGS0VGKTKSNJTR): the full check of the head of `main`, as the
/// watch of the wave read it last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CutWaits {
    /// The full check of `main` runs.
    MainRuns,
    /// The full check of `main` failed.
    MainFailed,
    /// The full check of `main` passed: the lead cuts the release next.
    Lead,
}

/// The current wave of a repository with a release cycle, when it has no
/// open item and no release issue: its release waits for its cut
/// (01M75N6YNHQQHGS0VGKTKSNJTR). The watch of the wave of the daemon saw it
/// at its last look.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Complete {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// The number of the wave.
    pub wave: u64,
    /// Why the cut waits, when the watch knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waits: Option<CutWaits>,
}

/// The complete wave of the riff in the facts of `hosts`
/// (01M75N6YNHQQHGS0VGKTKSNJTR). Of more repositories, the first by name
/// wins. Of one repository, the higher wave wins, and of one wave, the first
/// host that knows why the cut waits. `None` when no watch of a host sees a
/// complete wave.
///
/// ```
/// use riff_client::daemons::{Complete, CutWaits, Facts, Host, complete};
///
/// let host = |name: &str, complete: Vec<Complete>| Host {
///     user: "mike".into(),
///     host: name.into(),
///     age_secs: 5,
///     facts: Facts { complete, ..Facts::default() },
/// };
/// let of = |repo: &str, wave, waits| Complete { repo: repo.into(), wave, waits };
/// assert_eq!(complete(&[host("a", vec![])]), None);
/// let hosts = [
///     host("a", vec![of("o/r", 41, None)]),
///     host("b", vec![of("o/r", 41, Some(CutWaits::MainFailed))]),
/// ];
/// assert_eq!(complete(&hosts), Some(of("o/r", 41, Some(CutWaits::MainFailed))));
/// let hosts = [
///     host("a", vec![of("o/r", 42, None)]),
///     host("b", vec![of("o/r", 41, Some(CutWaits::Lead)), of("o/q", 7, None)]),
/// ];
/// assert_eq!(complete(&hosts), Some(of("o/q", 7, None)));
/// assert_eq!(complete(&hosts[..1]), Some(of("o/r", 42, None)));
/// ```
pub fn complete(hosts: &[Host]) -> Option<Complete> {
    let mut found: Option<&Complete> = None;
    for complete in hosts.iter().flat_map(|h| &h.facts.complete) {
        let better = match found {
            None => true,
            Some(old) if old.repo != complete.repo => complete.repo < old.repo,
            Some(old) if old.wave != complete.wave => complete.wave > old.wave,
            Some(old) => old.waits.is_none() && complete.waits.is_some(),
        };
        if better {
            found = Some(complete);
        }
    }
    found.cloned()
}

/// True when the tag `a` names a newer release than the tag `b`. A tag
/// with no version `vX.Y.Z` counts by its text.
fn newer(a: &str, b: &str) -> bool {
    let version = |tag: &str| {
        tag.strip_prefix('v')
            .and_then(|v| v.parse::<riff_core::build::Semver>().ok())
    };
    match (version(a), version(b)) {
        (Some(a), Some(b)) => a > b,
        _ => a > b,
    }
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
    /// The repositories where the host takes no new claim and still runs
    /// sessions (01M6WJYQJ58MHQF1XPXXZW2ET6).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leaving: Vec<Leaving>,
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
    /// The release of each watch of a deploy or of a check that the daemon
    /// follows, at its step (01M6XTS7RGXB30MFM6DZ7PVZ75), and of each watch
    /// of a wave from the cut to the check (01M75R6FPVZA3JA5C48PQ26A85):
    /// `riff who` and the TUI make the line of the release from them
    /// ([`release`]). A daemon of an older build sends none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub releases: Vec<Release>,
    /// The current wave of each watch of a wave of the daemon that has no
    /// open item, and whose release waits for its cut
    /// (01M75N6YNHQQHGS0VGKTKSNJTR): `riff who` and the TUI make the line
    /// of the wave from them ([`complete`]). A daemon of an older build
    /// sends none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub complete: Vec<Complete>,
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
                && facts.leaving.pop().is_none()
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

/// The verifies and the rebases of the waves of `repo` that wait for
/// another pull request ([`Wave::waits`]), from the daemons of `hosts`
/// ([`hosts`]), by issue.
///
/// ```
/// use std::collections::BTreeMap;
/// use riff_client::daemons::{Facts, Host, Wave, waits};
///
/// let host = |repo: &str, waits: BTreeMap<u64, u64>| Host {
///     user: "mike".into(),
///     host: "thelio".into(),
///     age_secs: 5,
///     facts: Facts {
///         waves: vec![Wave { repo: repo.into(), word: String::new(), image: None, waits }],
///         ..Facts::default()
///     },
/// };
/// let hosts = [host("o/r", BTreeMap::from([(15, 54)])), host("o/s", BTreeMap::from([(14, 60)]))];
/// assert_eq!(waits(&hosts, "o/r"), BTreeMap::from([(15, 54)]));
/// assert!(waits(&hosts, "o/t").is_empty());
/// ```
pub fn waits(hosts: &[Host], repo: &str) -> BTreeMap<u64, u64> {
    hosts
        .iter()
        .flat_map(|h| h.facts.waves.iter())
        .filter(|w| w.repo == repo)
        .flat_map(|w| w.waits.iter().map(|(n, pull)| (*n, *pull)))
        .collect()
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
            leaving: vec![Leaving {
                repo: "o/s".into(),
                sessions: vec!["riff-issue-14-3c4d5e6f".into()],
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
            releases: vec![Release {
                repo: "o/r".into(),
                tag: "v4.1.1".into(),
                step: ReleaseStep::CheckFailed,
            }],
            complete: vec![Complete {
                repo: "o/q".into(),
                wave: 41,
                waits: Some(CutWaits::MainFailed),
            }],
        };
        assert!(facts.line().contains(r#""step":"check_failed""#));
        assert!(facts.line().contains(r#""waits":"main_failed""#));
        assert_eq!(Facts::parse(&facts.line()), Some(facts.clone()));
        assert!(!facts.line().contains('\n'));
    }

    /// 01M74BVSFDAE67FA1PRYK7E2PN, #1855: the waits of a wave read again
    /// from the line of the facts, and also in a tagged enum, where serde
    /// gets each key as text.
    #[test]
    fn the_waits_of_a_wave_read_again_also_in_a_tagged_enum() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        #[serde(tag = "say", rename_all = "snake_case")]
        enum Said {
            Wave(Wave),
        }
        let wave = Wave {
            repo: "o/r".into(),
            word: "no free work now".into(),
            image: None,
            waits: BTreeMap::from([(724, 728), (725, 728), (730, 731)]),
        };
        let facts = Facts {
            waves: vec![wave.clone()],
            ..Facts::default()
        };
        let line = facts.line();
        assert!(
            line.contains(r#""waits":{"724":728,"725":728,"730":731}"#),
            "{line}"
        );
        assert_eq!(Facts::parse(&line), Some(facts));
        let said = Said::Wave(wave);
        let line = serde_json::to_string(&said).unwrap();
        assert_eq!(serde_json::from_str::<Said>(&line).unwrap(), said);
    }

    /// 01M75R6FPVZA3JA5C48PQ26A85: the steps from the cut come before the
    /// steps of the deploy, in the order of the cycle, and each step reads
    /// again from the line.
    #[test]
    fn the_steps_from_the_cut_come_before_the_deploy_and_read_again() {
        use ReleaseStep::*;
        let steps = [
            Merged,
            TestRuns,
            TestFailed,
            Tested,
            Published,
            Waits,
            Runs,
            Passed,
            DeployFailed,
            CheckFailed,
        ];
        assert!(steps.windows(2).all(|w| w[0] < w[1]));
        let facts = Facts {
            releases: steps
                .iter()
                .map(|&step| Release {
                    repo: "o/r".into(),
                    tag: "v4.17.0".into(),
                    step,
                })
                .collect(),
            ..Facts::default()
        };
        let line = facts.line();
        assert!(line.contains(r#""step":"merged""#), "{line}");
        assert!(line.contains(r#""step":"test_failed""#), "{line}");
        assert_eq!(Facts::parse(&line), Some(facts));
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

    /// 01M6WJYQJ58MHQF1XPXXZW2ET6: the leaving of each repository
    /// round-trips with the line, a left repository has no list of
    /// sessions, and an older daemon sends none.
    #[test]
    fn the_leaving_comes_back_from_the_line_and_an_older_daemon_sends_none() {
        let old = r#"daemon facts: {"build":"4.12.0","limit":8,"floor_gb":8}"#;
        assert!(Facts::parse(old).unwrap().leaving.is_empty());
        let facts = Facts {
            leaving: vec![
                Leaving {
                    repo: "o/r".into(),
                    sessions: vec!["riff-issue-12-1a2b3c4d".into()],
                },
                Leaving {
                    repo: "o/s".into(),
                    sessions: vec![],
                },
            ],
            ..Facts::default()
        };
        let line = facts.line();
        assert!(
            line.contains(
                r#""leaving":[{"repo":"o/r","sessions":["riff-issue-12-1a2b3c4d"]},{"repo":"o/s"}]"#
            ),
            "{line}"
        );
        assert_eq!(Facts::parse(&line), Some(facts));
        assert!(!Facts::default().line().contains("leaving"));
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
