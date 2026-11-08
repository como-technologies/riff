//! The Machines tab of the TUI (#1639, #1702): the facts of each host,
//! the detail of the selected host, the link and riff-server, the limit
//! of a host, and the filter of the hosts.
//!
//! # Design
//!
//! ```text
//!  1 Sessions  2 Chat•2  3 Waves•  [4 Machines]  5 Devices  6 Reports
//!  riff running · owner mike · build 10df8a4 · link ok 2s
//!  3 hosts · 1 at its limit · 1 low on memory
//!      HOST       WORK  LOAD     MEM FREE    DISK FREE   USED  WAITS SEEN
//!  > ! thelio     3/3   19.0/32  2/124 GB !  95/455 GB   79%   1     0s
//!      pangolin   2/4   3.1/16   14/32 GB    90/200 GB   55%   0     12s
//!      strata-ci  0/2   0.4/8    30/64 GB    140/500 GB  72%   0     28s
//!
//! ──thelio · riff daemon pid 4242, up 2d ─────────────────────────────────────────
//!  cpu 32x5883MHz (now 4100MHz), mem 124GB, 2GB available, load 18.96
//!  limit 3 sessions and a floor of 8 GB · 1 start waits · 1 job
//!  at its limit: 3 of 3 sessions live, a new start waits for room
//!  low on memory: the daemon starts no session until 8 GB are free
//!  parts como-technologies/riff workers and lead
//!  wave como-technologies/riff: started issue-1594
//!  sessions: 5b1e2a90 works · 9c0d1e2f works · 3a3f8d5d ends
//!
//! ──the link and the server ──────────────────────────────────────────────────────
//!  link ok: 118 ms · 1 reconnect · riff-server build 10df8a4, same as yours
//!  GitHub: 4,210 of 5,000 points left · riff spent 310, others 480, in the hour
//!
//!
//!
//!  ↑↓ select  Enter sessions of the host  l limit  r read again  ? help  q quit
//! ```
//!
//! The rows come from the facts that the riff daemon of each host sends
//! to riff-server each 30 seconds ([`riff_client::daemons::hosts`], in
//! the reply of `who`), as in `riff who` and `riff top`. Facts older
//! than 3 minutes count as none (01M6VFCPK2PSDTHHE5TSY3XP5N). The row of
//! this host comes from the socket of its daemon at each look
//! ([`here`], [`Here`], 01M6VFD09QN71SWMPP8YXCRAEK), so it is never 30
//! seconds old. The facts that the socket does not tell, the version
//! of podman and the sign-ins, come from the facts that the daemon sent.
//!
//! ```mermaid
//! flowchart LR
//!     D["the riff daemon of each host"] -- "each 30 s: its facts" --> S["riff-server"]
//!     S -- "who" --> R["rows: a Row for each host"]
//!     H["the socket of the daemon of this host"] -- "status, at each look" --> R
//!     R --> F["Fault::of: the faults, the order"]
//!     F --> B["body: the rows, the detail,<br/>the link and the server"]
//! ```
//!
//! A host with a fault ([`Fault`], 01M6VFD0CAS71DNBGE7K56JJ2H) gets `!`
//! and comes first. The first line counts the hosts and each fault.
//! Under the rows comes the detail of the selected host
//! (01M6VFD0EXECEEVNN969SAPV4C), and at the end the lines of the link
//! and the server (01M6VFD0HGCVMYNNC3H4788W9X). The narrow layout has no
//! detail. When the hosts do not fit, the rows scroll with the
//! selection, and the first line shows the place
//! (01M6VFD0M3RHT488D0RQ84RVMC). The column `USED` is the used part of
//! the disk, rounded to the nearest percent
//! ([`riff_client::machines::Disk::used_percent`],
//! 01M6VJQPG6RM3Q4QAJEHFJ6RQK).
//!
//! # A host that leaves a repository
//!
//! After `riff leave`, a host takes no new claim for the repository, and
//! its sessions there go on to their end. Its daemon names the
//! repository and those sessions in its facts
//! ([`riff_client::daemons::Leaving`], 01M6WJYQJ58MHQF1XPXXZW2ET6). The
//! row of the host gets a line under it for each repository that it
//! leaves ([`marks`], 01M6WJYQN2WY5NH3DVDBAAY8ST), with the short name of
//! the repository: `leaving riff: 3 to end`, and `left riff` at the look
//! after the last session. A host that gives workers to no repository
//! gets `takes no work`. The detail lists the parts of the host by
//! repository, and the sessions of each repository that it leaves. A
//! mark is no fault: the order of the rows stays. When the hosts do not
//! fit, the rows scroll by lines, so the selected row shows with its
//! marks. The screen after `riff leave --host pangolin`, with pangolin
//! selected:
//!
//! ```text
//!  3 hosts · 1 at its limit · 1 low on memory
//!      HOST       WORK  LOAD     MEM FREE    DISK FREE   USED  WAITS SEEN
//!    ! thelio     3/3   19.0/32  2/124 GB !  95/455 GB   79%   1     0s
//!  >   pangolin   3/4   3.1/16   14/32 GB    90/200 GB   55%   0     12s
//!                 leaving riff: 3 to end
//!      strata-ci  0/2   0.4/8    30/64 GB    140/500 GB  72%   0     28s
//!
//! ──pangolin · riff daemon build d2ae080 ─────────────────────────────────────────
//!  cpu 16x4500MHz (now 4400MHz), mem 32GB, 14GB available, load 3.10
//!  limit 4 sessions and a floor of 8 GB · 0 starts wait
//!  parts n8behavior/sens-o-matic workers
//!  leaving como-technologies/riff: 3 sessions go on to their end
//!    issue-1610 works · issue-1611 works · issue-1612 ends
//!  sessions: 7a8b9c0d works · 1e2f3a4b works · 5c6d7e8f ends
//! ```
//!
//! # The wide rows
//!
//! From [`view::WIDER`] columns, a row adds two columns
//! (01M6VJQGSMNAJ82GX0YQ2YAHKC): `CLOCK`, the clock now against the
//! most clock of a core, and `POOL`, the CPUs and the memory of each new
//! container of the host ([`riff_client::daemons::Caps`], `riff daemon
//! limit --cpus N --memory GB`). The key line adds `/ filter`. The rows
//! at 132 columns:
//!
//! ```text
//!      HOST       WORK  LOAD     CLOCK          MEM FREE    DISK FREE   USED  POOL          WAITS SEEN
//!  > ! thelio     3/3   19.0/32  4100/5883 MHz  2/124 GB !  95/455 GB   79%   8 CPU 8 GB    1     0s
//!      pangolin   2/4   3.1/16   4400/4500 MHz  14/32 GB    90/200 GB   55%   4 CPU 8 GB    0     12s
//!      strata-ci  0/2   0.4/8    1200/3600 MHz  30/64 GB    140/500 GB  72%   2 CPU 16 GB   0     28s
//! ```
//!
//! # The filter
//!
//! `/` filters the hosts ([`Filter`], 01M6VJQPJXKHKJ8WT1CAQMN1YR). Each
//! word must match a part of a word of the host ([`matches()`]): its user,
//! its host, the repository of a wave, the item of a session, or a fault
//! ([`Fault::word`]). `KEY:WORD` matches only the words of [`KEYS`]. While
//! the person types, the note line shows the filter, and each character
//! is a letter of it ([`crate::tui::State::on_key`]). The first line
//! counts the hosts that match against all, and ends with the filter:
//!
//! ```text
//!  1 of 3 hosts · /pang
//!      HOST       WORK  LOAD     MEM FREE    DISK FREE   USED  WAITS SEEN
//!  >   pangolin   2/4   3.1/16   14/32 GB    90/200 GB   55%   0     12s
//! ```
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> None
//!     None --> Typing: /
//!     Typing --> Typing: a character, Backspace
//!     Typing --> Kept: Enter
//!     Typing --> None: Esc
//!     Kept --> Typing: /
//!     Kept --> None: Esc
//! ```
//!
//! # The keys
//!
//! [`on_key`] takes the keys of the tab (01M6VFD0M3RHT488D0RQ84RVMC):
//!
//! | Key | Does |
//! |---|---|
//! | `↑` `↓` `j` `k`, the wheel | Select the host above or below. |
//! | `PgUp` `PgDn` | Move [`PAGE`] hosts. |
//! | `g` `G` | Select the first or the last host. |
//! | A click on a row | Select the host. |
//! | `Enter` | Open the Sessions tab with the filter `host:HOST`. |
//! | `l` | Set the limit of the host. |
//! | `/` | Filter the hosts. `Enter` keeps the filter, `Esc` clears it. |
//! | `Esc` | Clear the filter. |
//!
//! `l` asks for the most live sessions of the selected host in the
//! note line ([`Typing`], 01M6VFD0PPXDNH3NKD3YE4W9SB). `Enter` asks
//! `y` or `n` ([`super::Question`]), and `y` gives [`Action::Limit`] to
//! the loop. The loop runs [`set_limit`], the code of
//! `riff daemon limit --sessions N`: for this host in the folder of its
//! daemon ([`crate::room::set`]), and for another host through
//! riff-server, as with `--host HOST` ([`crate::daemon::remote::ask`]).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as the person
//!     participant T as the Machines tab
//!     participant L as the loop
//!     P->>T: l
//!     T-->>P: The most live sessions of thelio (now 3):
//!     P->>T: 4, Enter
//!     T-->>P: Set the limit of thelio to 4 sessions? y/N
//!     P->>T: y
//!     T->>L: Action::Limit
//!     L->>L: set_limit: riff daemon limit --sessions 4
//!     L-->>P: the reply in the note line, and a new look
//! ```

use std::time::Duration;

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use riff_client::api::Api;
use riff_client::daemons::{self, Facts, Sign, Token};
use riff_client::machines::{Disk, Machine};
use riff_core::build::Build;
use riff_core::name::SessionUri;

use super::view::{self, ACTIVE, DIM, ERROR, GOOD, Hit, Layout, Placed, WARNING};
use super::{Action, Key, Look, Mark, Question, State, Tab};
use crate::text::{self, safe};

/// The longest wait for the status of the daemon of this host.
pub const DAEMON_WAIT: Duration = Duration::from_secs(2);

/// The hosts that `PgUp` and `PgDn` move.
pub const PAGE: usize = 10;

/// The most digits of the limit that the person types.
pub const MOST_DIGITS: usize = 4;

/// The first line of the tab with no host.
pub const NO_HOSTS: &str = "No riff daemon sent the facts of its host in the last 3 minutes.";

/// The line under [`NO_HOSTS`].
pub const ADD_HOST: &str =
    "To add a host, run riff join in the main clone of the repository on that host.";

/// This host at a look (01M6VFD09QN71SWMPP8YXCRAEK): the user and the
/// host of the TUI, and what its riff daemon tells on its socket.
#[derive(Debug, Clone, PartialEq)]
pub struct Here {
    /// The user of the TUI.
    pub user: String,
    /// The host of the TUI.
    pub host: String,
    /// The status of the daemon of this host, or `None` when no daemon
    /// answers.
    pub daemon: Option<crate::daemon::Status>,
    /// The numbers of this machine, read with the status.
    pub machine: Option<Machine>,
    /// The disk of the folder of the daemon, read with the status.
    pub disk: Option<Disk>,
}

/// This host for `me` (01M6VFD09QN71SWMPP8YXCRAEK): the status of the
/// daemon in [`crate::daemon::dir`], within [`DAEMON_WAIT`], and the
/// numbers and the disk of this machine. With no socket of a daemon, it
/// asks nothing and reads no numbers. The reads of the machine run on a
/// thread for blocking work.
pub async fn here(me: &SessionUri) -> Here {
    let dir = crate::daemon::dir()
        .ok()
        .filter(|dir| crate::daemon::socket(dir).exists());
    let daemon = match &dir {
        Some(dir) => crate::daemon::status_within(dir, DAEMON_WAIT).await.ok(),
        None => None,
    };
    let (machine, disk) = match (&daemon, dir) {
        (Some(_), Some(dir)) => tokio::task::spawn_blocking(move || {
            (Some(crate::machine::here()), crate::disk::here(&dir))
        })
        .await
        .unwrap_or_default(),
        _ => (None, None),
    };
    Here {
        user: me.who().user().to_owned(),
        host: me.place().host().to_owned(),
        daemon,
        machine,
        disk,
    }
}

/// The name of a host in the Machines tab: its user and its host.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name {
    pub user: String,
    pub host: String,
}

/// A fault of a host (01M6VFD0CAS71DNBGE7K56JJ2H), in the order of the
/// first line of the tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Fault {
    /// Its live sessions are at its limit: a new start waits for room.
    Limit,
    /// Its free memory is under its floor: the daemon starts no session.
    Memory,
    /// Less than [`riff_client::machines::TIGHT_PERCENT`] of its disk
    /// is free.
    Disk,
    /// A sign-in stops its sessions ([`stopping_sign_ins`]).
    SignIn,
    /// It tells no numbers of its machine.
    Numbers,
}

impl Fault {
    /// Each fault, in the order of the first line.
    pub const ALL: [Fault; 5] = [
        Fault::Limit,
        Fault::Memory,
        Fault::Disk,
        Fault::SignIn,
        Fault::Numbers,
    ];

    /// The faults of a host with the facts `facts`
    /// (01M6VFD0CAS71DNBGE7K56JJ2H). A limit of 0 is no fault: the
    /// person chose that the host starts no session.
    ///
    /// ```
    /// use riff::tui::machines::Fault;
    /// use riff_client::daemons::{Facts, Session, Sign};
    /// use riff_client::machines::{Disk, Machine};
    /// use riff_client::sessions::Phase;
    ///
    /// let machine = Machine { threads: 16, mhz: 4500, now_mhz: 4400, mem_gb: 32, avail_gb: 24, load: 1.5 };
    /// let fine = Facts {
    ///     limit: 2,
    ///     floor_gb: 8,
    ///     machine: Some(machine),
    ///     disk: Some(Disk { free_gb: 200, total_gb: 455 }),
    ///     ..Facts::default()
    /// };
    /// assert_eq!(Fault::of(&fine), []);
    /// let session = Session {
    ///     name: "riff-issue-12-1a2b3c4d".into(),
    ///     item: "issue-12".into(),
    ///     phase: Phase::Works,
    ///     last: String::new(),
    ///     digest: String::new(),
    /// };
    /// let full = Facts { sessions: vec![session.clone(), session], ..fine.clone() };
    /// assert_eq!(Fault::of(&full), [Fault::Limit]);
    /// let low = Facts { machine: Some(Machine { avail_gb: 7, ..machine }), ..fine.clone() };
    /// assert_eq!(Fault::of(&low), [Fault::Memory]);
    /// let tight = Facts { disk: Some(Disk { free_gb: 60, total_gb: 455 }), ..fine.clone() };
    /// assert_eq!(Fault::of(&tight), [Fault::Disk]);
    /// let off = Facts { gh: Sign::Off, ..fine.clone() };
    /// assert_eq!(Fault::of(&off), [Fault::SignIn]);
    /// assert_eq!(Fault::of(&Facts { machine: None, ..fine.clone() }), [Fault::Numbers]);
    /// // A limit of 0 is a choice of the person.
    /// assert_eq!(Fault::of(&Facts { limit: 0, ..fine }), []);
    /// ```
    pub fn of(facts: &Facts) -> Vec<Fault> {
        let mut faults = Vec::new();
        let limit = usize::try_from(facts.limit).unwrap_or(usize::MAX);
        if limit > 0 && facts.runs() >= limit {
            faults.push(Fault::Limit);
        }
        let floor = u32::try_from(facts.floor_gb).unwrap_or(u32::MAX);
        if facts.machine.is_some_and(|m| m.low(floor)) {
            faults.push(Fault::Memory);
        }
        if facts.disk.is_some_and(|d| d.tight()) {
            faults.push(Fault::Disk);
        }
        if !stopping_sign_ins(facts).is_empty() {
            faults.push(Fault::SignIn);
        }
        if facts.machine.is_none() {
            faults.push(Fault::Numbers);
        }
        faults
    }

    /// The count of `n` hosts with the fault, for the first line of the
    /// tab.
    ///
    /// ```
    /// use riff::tui::machines::Fault;
    ///
    /// let one: Vec<String> = Fault::ALL.iter().map(|f| f.count(1)).collect();
    /// assert_eq!(
    ///     one,
    ///     ["1 at its limit", "1 low on memory", "1 tight on disk", "1 needs a sign-in", "1 tells no numbers"]
    /// );
    /// let two: Vec<String> = Fault::ALL.iter().map(|f| f.count(2)).collect();
    /// assert_eq!(
    ///     two,
    ///     ["2 at their limit", "2 low on memory", "2 tight on disk", "2 need a sign-in", "2 tell no numbers"]
    /// );
    /// ```
    pub fn count(self, n: usize) -> String {
        let one = n == 1;
        match self {
            Fault::Limit => format!("{n} at {} limit", if one { "its" } else { "their" }),
            Fault::Memory => format!("{n} low on memory"),
            Fault::Disk => format!("{n} tight on disk"),
            Fault::SignIn => format!("{n} {} a sign-in", if one { "needs" } else { "need" }),
            Fault::Numbers => format!("{n} {} no numbers", if one { "tells" } else { "tell" }),
        }
    }

    /// The words of the fault that the filter matches with the key
    /// `state` (01M6VJQPJXKHKJ8WT1CAQMN1YR).
    ///
    /// ```
    /// use riff::tui::machines::Fault;
    ///
    /// let words: Vec<&str> = Fault::ALL.iter().map(|f| f.word()).collect();
    /// assert_eq!(
    ///     words,
    ///     ["at its limit", "low on memory", "tight on disk", "needs a sign-in", "tells no numbers"]
    /// );
    /// ```
    pub fn word(self) -> &'static str {
        match self {
            Fault::Limit => "at its limit",
            Fault::Memory => "low on memory",
            Fault::Disk => "tight on disk",
            Fault::SignIn => "needs a sign-in",
            Fault::Numbers => "tells no numbers",
        }
    }

    /// The line of the fault in the detail of a host with the facts
    /// `facts` (01M6VFD0EXECEEVNN969SAPV4C).
    pub fn line(self, facts: &Facts) -> String {
        match self {
            Fault::Limit => format!(
                "at its limit: {} of {} sessions live, a new start waits for room",
                facts.runs(),
                facts.limit
            ),
            Fault::Memory => format!(
                "low on memory: the daemon starts no session until {} GB are free",
                facts.floor_gb
            ),
            Fault::Disk => match facts.disk {
                Some(d) => format!("tight on disk: {} of {} GB free", d.free_gb, d.total_gb),
                None => "tight on disk".to_owned(),
            },
            Fault::SignIn => format!("needs a sign-in: {}", stopping_sign_ins(facts).join(", ")),
            Fault::Numbers => "no numbers: the daemon tells no numbers of its machine".to_owned(),
        }
    }
}

/// The sign-ins of a host that stop its sessions
/// (01M6VFD0CAS71DNBGE7K56JJ2H), in the words of `riff who`: the sign-in
/// of riff or of `gh` that is off, the sign-in of `claude` with less
/// than 2 hours left and no token, and a token that `claude` refused.
/// A sign-in that the daemon cannot read stops nothing.
///
/// ```
/// use riff::tui::machines::stopping_sign_ins;
/// use riff_client::daemons::{Facts, Sign, Token};
///
/// let fine = Facts { riff: Sign::On, gh: Sign::On, claude: Sign::Left { minutes: 300 }, ..Facts::default() };
/// assert!(stopping_sign_ins(&fine).is_empty());
/// let short = Facts { claude: Sign::Left { minutes: 90 }, ..fine.clone() };
/// assert_eq!(stopping_sign_ins(&short), ["claude 1h30m left"]);
/// // A token: the minutes of claude stop no start.
/// let token = Facts { claude_token: Token::Set, ..short.clone() };
/// assert!(stopping_sign_ins(&token).is_empty());
/// let refused = Facts { claude_token: Token::Refused, ..short };
/// assert_eq!(stopping_sign_ins(&refused), ["claude token refused"]);
/// let off = Facts { riff: Sign::Off, gh: Sign::Off, ..fine };
/// assert_eq!(stopping_sign_ins(&off), ["riff off", "gh off"]);
/// assert!(stopping_sign_ins(&Facts::default()).is_empty());
/// ```
pub fn stopping_sign_ins(facts: &Facts) -> Vec<String> {
    let mut stops = Vec::new();
    for (name, sign) in [("riff", &facts.riff), ("gh", &facts.gh)] {
        if *sign == Sign::Off {
            stops.push(format!("{name} {}", text::sign_word(sign)));
        }
    }
    if facts.claude.short() && !facts.claude_token.set() {
        stops.push(format!("claude {}", text::sign_word(&facts.claude)));
    }
    if facts.claude_token == Token::Refused {
        stops.push(format!("claude token {}", facts.claude_token.word()));
    }
    stops
}

/// The marks under the row of a host with the facts `facts`
/// (01M6WJYQN2WY5NH3DVDBAAY8ST), a line each: `leaving riff: 3 to end`
/// for each repository where the host takes no new claim and still runs
/// sessions, with the short name of the repository, `left riff` when no
/// session of it runs any more, and `takes no work` when the host gives
/// workers to no repository. A mark is no fault: it does not change the
/// order of the rows.
///
/// ```
/// use riff::tui::machines::marks;
/// use riff_client::daemons::{Facts, Leaving, Parts, RepoParts};
///
/// let workers = Parts { workers: true, lead: false };
/// let gives = Facts {
///     parts: vec![RepoParts { repo: "n8behavior/sens-o-matic".into(), parts: workers }],
///     ..Facts::default()
/// };
/// assert!(marks(&gives).is_empty());
/// let riff = |n: usize| Leaving {
///     repo: "como-technologies/riff".into(),
///     sessions: vec!["riff-issue-12-1a2b3c4d".into(); n],
/// };
/// let leaving = Facts { leaving: vec![riff(3)], ..gives.clone() };
/// assert_eq!(marks(&leaving), ["leaving riff: 3 to end"]);
/// let left = Facts { leaving: vec![riff(0)], ..gives };
/// assert_eq!(marks(&left), ["left riff"]);
/// // A host with no part workers takes no work: one with no part at
/// // all, and one that runs a lead only.
/// assert_eq!(
///     marks(&Facts { parts: vec![], ..leaving }),
///     ["leaving riff: 3 to end", "takes no work"]
/// );
/// let lead = Parts { workers: false, lead: true };
/// let lead = Facts { parts: vec![RepoParts { repo: "o/r".into(), parts: lead }], ..Facts::default() };
/// assert_eq!(marks(&lead), ["takes no work"]);
/// ```
pub fn marks(facts: &Facts) -> Vec<String> {
    let mut marks: Vec<String> = facts
        .leaving
        .iter()
        .map(|leaving| {
            let repo = safe(super::waves::short_repo(&leaving.repo));
            if leaving.left() {
                format!("left {repo}")
            } else {
                format!("leaving {repo}: {} to end", leaving.sessions.len())
            }
        })
        .collect();
    if !facts.parts.iter().any(|p| p.parts.workers) {
        marks.push("takes no work".to_owned());
    }
    marks
}

/// The session of `facts` with the container `name`, for the detail of
/// a host (01M6WJYQN2WY5NH3DVDBAAY8ST): its item and its phase, or the
/// name when the facts do not list the session.
fn session_word(facts: &Facts, name: &str) -> String {
    match facts.sessions.iter().find(|s| s.name == name) {
        Some(s) => format!("{} {}", safe(&s.item), text::phase_word(s.phase)),
        None => safe(name),
    }
}

/// The process of the daemon of this host, from its socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Process {
    /// Its pid.
    pub pid: u32,
    /// Its start, in milliseconds since the epoch.
    pub since_ms: i64,
    /// The jobs of its waves.
    pub jobs: usize,
}

/// One host of the Machines tab (01M6VFCPK2PSDTHHE5TSY3XP5N).
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The user and the host.
    pub name: Name,
    /// The seconds since its daemon sent the facts. 0 for this host.
    pub age_secs: u64,
    /// The facts of its daemon.
    pub facts: Facts,
    /// The process of its daemon: only for this host.
    pub process: Option<Process>,
    /// Its faults ([`Fault::of`]).
    pub faults: Vec<Fault>,
}

impl Row {
    fn new(name: Name, age_secs: u64, facts: Facts, process: Option<Process>) -> Row {
        Row {
            faults: Fault::of(&facts),
            name,
            age_secs,
            facts,
            process,
        }
    }

    /// The words of the host that a word of the filter can match, each
    /// with its key of [`KEYS`] (01M6VJQPJXKHKJ8WT1CAQMN1YR): its user,
    /// its host, the repository of each wave, the item of each session,
    /// and each fault.
    pub fn words(&self) -> Vec<(&'static str, String)> {
        let mut words = vec![
            ("user", self.name.user.clone()),
            ("host", self.name.host.clone()),
        ];
        words.extend(self.facts.waves.iter().map(|w| ("repo", w.repo.clone())));
        words.extend(self.facts.sessions.iter().map(|s| ("item", s.item.clone())));
        words.extend(self.faults.iter().map(|f| ("state", f.word().to_owned())));
        words
    }
}

/// The keys of a word of the filter: `KEY:WORD` matches only the words
/// of the host with that key (01M6VJQPJXKHKJ8WT1CAQMN1YR).
pub const KEYS: [&str; 5] = ["user", "host", "repo", "item", "state"];

/// True when the word `word` of the filter matches one of `words`
/// (01M6VJQPJXKHKJ8WT1CAQMN1YR): a word with no key of [`KEYS`] matches
/// each word, `host:thelio` only a host. The match is a part of the
/// word, with no case.
///
/// ```
/// use riff::tui::machines::matches;
///
/// let words = [("host", "thelio".to_owned()), ("item", "issue-1594".to_owned())];
/// assert!(matches("thel", &words));
/// assert!(matches("THELIO", &words));
/// assert!(matches("host:thelio", &words));
/// assert!(matches("1594", &words));
/// assert!(!matches("user:thelio", &words));
/// assert!(!matches("pangolin", &words));
/// // A key that is not in KEYS is a part of the word.
/// assert!(!matches("os:thelio", &words));
/// ```
pub fn matches(word: &str, words: &[(&str, String)]) -> bool {
    let word = word.to_lowercase();
    let (key, value) = match word.split_once(':') {
        Some((key, value)) if KEYS.contains(&key) => (Some(key), value),
        _ => (None, word.as_str()),
    };
    words
        .iter()
        .filter(|(k, _)| key.is_none_or(|key| key == *k))
        .any(|(_, w)| w.to_lowercase().contains(value))
}

/// The filter of the Machines tab (01M6VJQPJXKHKJ8WT1CAQMN1YR): its
/// words, and true while the person types them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// The words. Each word must match.
    pub words: String,
    /// True while the person types the words.
    pub typing: bool,
}

impl Filter {
    /// True when the filter has a word.
    pub fn is_set(&self) -> bool {
        !self.words.trim().is_empty()
    }

    /// True when each word of the filter matches a word of `row`.
    ///
    /// ```
    /// use riff::tui::machines::{Fault, Filter, Name, Row};
    /// use riff_client::daemons::Facts;
    ///
    /// let row = Row {
    ///     name: Name { user: "mike".into(), host: "thelio".into() },
    ///     age_secs: 0,
    ///     facts: Facts::default(),
    ///     process: None,
    ///     faults: vec![Fault::Numbers],
    /// };
    /// let filter = |words: &str| Filter { words: words.into(), typing: false };
    /// assert!(filter("thel mike").shows(&row));
    /// assert!(filter("state:numbers").shows(&row));
    /// assert!(!filter("thel ann").shows(&row));
    /// assert!(!filter("host:mike").shows(&row));
    /// // No word shows each host.
    /// assert!(filter("").shows(&row) && !filter("  ").is_set());
    /// assert!(filter("thel").is_set());
    /// ```
    pub fn shows(&self, row: &Row) -> bool {
        let words = row.words();
        self.words.split_whitespace().all(|w| matches(w, &words))
    }

    /// The note line while the person types: `/`, the words and a
    /// cursor.
    ///
    /// ```
    /// use riff::tui::machines::Filter;
    ///
    /// let filter = Filter { words: "host:thel".into(), typing: true };
    /// assert_eq!(filter.line(), "/host:thel█");
    /// ```
    pub fn line(&self) -> String {
        format!("/{}█", self.words)
    }
}

/// The rows of `look` that the filter of the Machines tab shows, in the
/// order of [`rows`] (01M6VJQPJXKHKJ8WT1CAQMN1YR).
pub fn shown(state: &State, look: &Look) -> Vec<Row> {
    rows(look)
        .into_iter()
        .filter(|row| state.host_filter().shows(row))
        .collect()
}

/// The hosts of the look `look`, in the order of the tab
/// (01M6VFCPK2PSDTHHE5TSY3XP5N, 01M6VFD09QN71SWMPP8YXCRAEK,
/// 01M6VFD0CAS71DNBGE7K56JJ2H): the facts of each daemon in `who`, and
/// the row of this host from the status of its daemon when one
/// answered. A host with a fault comes first, then the hosts by name.
pub fn rows(look: &Look) -> Vec<Row> {
    let mut found: Vec<Row> = daemons::hosts(&look.who.sessions)
        .into_iter()
        .map(|h| {
            let name = Name {
                user: h.user,
                host: h.host,
            };
            Row::new(name, h.age_secs, h.facts, None)
        })
        .collect();
    if let Some(here) = &look.here
        && let Some(status) = &here.daemon
    {
        let name = Name {
            user: here.user.clone(),
            host: here.host.clone(),
        };
        let sent = found
            .iter()
            .position(|row| row.name == name)
            .map(|at| found.remove(at).facts);
        let process = Process {
            pid: status.pid,
            since_ms: status.since_ms,
            jobs: status.jobs,
        };
        found.push(Row::new(
            name,
            0,
            of_here(status, here, sent),
            Some(process),
        ));
    }
    found.sort_by(|a, b| {
        let key = |r: &Row| {
            (
                !r.faults.is_empty(),
                r.name.host.clone(),
                r.name.user.clone(),
            )
        };
        let (a, b) = (key(a), key(b));
        b.0.cmp(&a.0).then_with(|| (&a.1, &a.2).cmp(&(&b.1, &b.2)))
    });
    found
}

/// The facts of this host: the status of its daemon, and the numbers
/// and the disk of the host (01M6VFD09QN71SWMPP8YXCRAEK). The facts that
/// the socket does not tell come from `sent`, the facts that the daemon
/// sent to riff-server: the version of podman, the sign-ins, and the
/// session image of each wave.
fn of_here(status: &crate::daemon::Status, here: &Here, sent: Option<Facts>) -> Facts {
    let mut facts = crate::daemon::facts::of_status(status);
    facts.machine = here.machine;
    facts.disk = here.disk;
    if let Some(sent) = sent {
        facts.podman = sent.podman;
        facts.riff = sent.riff;
        facts.gh = sent.gh;
        facts.claude = sent.claude;
        for wave in &mut facts.waves {
            wave.image = sent
                .waves
                .iter()
                .find(|w| w.repo == wave.repo)
                .and_then(|w| w.image.clone());
        }
    }
    facts
}

/// The index of the selected host in `rows`: the first host when the
/// person selected none, or when the selected host is gone.
pub fn selected(state: &State, rows: &[Row]) -> usize {
    state
        .host
        .as_ref()
        .and_then(|name| rows.iter().position(|row| &row.name == name))
        .unwrap_or(0)
}

/// What the person types in the note line: the most live sessions of a
/// host (01M6VFD0PPXDNH3NKD3YE4W9SB).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Typing {
    /// The host.
    pub host: String,
    /// Its limit now.
    pub now: u32,
    /// The digits so far.
    pub typed: String,
}

impl Typing {
    /// The note line while the person types: the question, the digits
    /// and a cursor.
    ///
    /// ```
    /// use riff::tui::machines::Typing;
    ///
    /// let typing = Typing { host: "thelio".into(), now: 3, typed: "4".into() };
    /// assert_eq!(typing.line(), "The most live sessions of thelio (now 3): 4█");
    /// ```
    pub fn line(&self) -> String {
        format!(
            "The most live sessions of {} (now {}): {}█",
            safe(&self.host),
            self.now,
            self.typed
        )
    }

    /// The note when the person sets nothing.
    fn stays(&self) -> String {
        format!(
            "The limit of {} stays {} {}.",
            safe(&self.host),
            self.now,
            sessions_word(self.now)
        )
    }
}

/// `session` or `sessions`, for `n`.
fn sessions_word(n: u32) -> &'static str {
    if n == 1 { "session" } else { "sessions" }
}

/// Takes the key `key` while the person types the limit `typing`
/// (01M6VFD0PPXDNH3NKD3YE4W9SB): a digit, also the digits of the tabs,
/// `Backspace`, `Enter` to ask `y` or `n`, and `Esc` to set nothing. `q`
/// and `Ctrl-C` end the TUI.
///
/// ```
/// use riff::tui::{Action, Key, Look, State, Tab};
/// use riff_client::daemons::Facts;
/// use riff_core::wire::{MachineInfo, SessionInfo};
/// use riff_core::build::Build;
///
/// let mut host: SessionInfo = serde_json::from_value(serde_json::json!({
///     "uri": "riff://mike@thelio/o/r?session=d1", "live": true,
/// }))?;
/// let facts = Facts { limit: 3, ..Facts::default() };
/// host.machine = Some(MachineInfo { numbers: facts.line(), age_secs: 5 });
/// let mut look = Look::default();
/// look.who.sessions = vec![host];
/// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
/// state.looked(look, 0);
/// state.on_key(Key::Open(Tab::Machines));
/// state.on_key(Key::Char('l'));
/// assert_eq!(state.asked().as_deref(), Some("The most live sessions of thelio (now 3): █"));
/// // The digits of the tabs are digits here, and a letter is nothing.
/// for key in [Key::Char('1'), Key::Char('2'), Key::Backspace, Key::Char('j'), Key::Char('0')] {
///     state.on_key(key);
/// }
/// assert_eq!(state.tab(), Tab::Machines);
/// assert_eq!(state.on_key(Key::Enter), None);
/// assert_eq!(state.asked().as_deref(), Some("Set the limit of thelio to 10 sessions? y/N"));
/// assert_eq!(
///     state.on_key(Key::Char('y')),
///     Some(Action::Limit { host: "thelio".into(), sessions: 10 })
/// );
/// // Esc sets nothing.
/// state.on_key(Key::Char('l'));
/// state.on_key(Key::Esc);
/// assert_eq!(state.asked(), None);
/// assert_eq!(state.shown_note(), Some("The limit of thelio stays 3 sessions."));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn typed(state: &mut State, mut typing: Typing, key: Key) -> Option<Action> {
    let digit = match key {
        Key::Char(c) if c.is_ascii_digit() => Some(c),
        _ => None,
    };
    match key {
        Key::Quit | Key::Char('q') => return Some(Action::Quit),
        Key::Esc => state.note(typing.stays()),
        Key::Enter => match typing.typed.parse::<u32>() {
            Ok(sessions) => state.ask(Question {
                text: format!(
                    "Set the limit of {} to {sessions} {}?",
                    safe(&typing.host),
                    sessions_word(sessions)
                ),
                no: typing.stays(),
                yes: Action::Limit {
                    host: typing.host,
                    sessions,
                },
            }),
            Err(_) => state.note(typing.stays()),
        },
        Key::Backspace => {
            typing.typed.pop();
            state.typing = Some(typing);
        }
        _ => {
            if let Some(digit) = digit
                && typing.typed.len() < MOST_DIGITS
            {
                typing.typed.push(digit);
            }
            state.typing = Some(typing);
        }
    }
    None
}

/// Takes the key `key` while the person types the filter of the tab
/// (01M6VJQPJXKHKJ8WT1CAQMN1YR): a character, `Backspace`, `Enter` to
/// keep the filter, and `Esc` to clear it. `Ctrl-C` ends the TUI.
///
/// ```
/// use riff::tui::{Action, Key, State, Tab};
/// use riff_core::build::Build;
///
/// let mut state = State::new(Build::this(), "riff://mike@thelio".parse()?);
/// state.on_key(Key::Open(Tab::Machines));
/// state.on_key(Key::Char('/'));
/// for c in "pang q1".chars() {
///     state.on_key(Key::Char(c));
/// }
/// state.on_key(Key::Backspace);
/// assert_eq!(state.asked().as_deref(), Some("/pang q█"));
/// assert_eq!(state.tab(), Tab::Machines);
/// // Enter keeps the filter.
/// state.on_key(Key::Enter);
/// assert_eq!(state.host_filter().words, "pang q");
/// assert_eq!(state.asked(), None);
/// // `/` types on, and Esc clears the filter.
/// state.on_key(Key::Char('/'));
/// assert_eq!(state.asked().as_deref(), Some("/pang q█"));
/// state.on_key(Key::Esc);
/// assert!(!state.host_filter().is_set());
/// assert_eq!(state.asked(), None);
/// // Esc also clears a kept filter.
/// for key in [Key::Char('/'), Key::Char('x'), Key::Enter, Key::Esc] {
///     state.on_key(key);
/// }
/// assert!(!state.host_filter().is_set());
/// state.on_key(Key::Char('/'));
/// assert_eq!(state.on_key(Key::Quit), Some(Action::Quit));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn filter_typed(state: &mut State, key: Key) -> Option<Action> {
    let filter = &mut state.host_filter;
    match key {
        Key::Quit => return Some(Action::Quit),
        Key::Char(c) => filter.words.push(c),
        Key::Backspace => {
            filter.words.pop();
        }
        Key::Enter => filter.typing = false,
        Key::Esc => *filter = Filter::default(),
        _ => {}
    }
    None
}

/// Takes the key `key` of the Machines tab (01M6VFD0M3RHT488D0RQ84RVMC,
/// 01M6VFD0PPXDNH3NKD3YE4W9SB, 01M6VJQPJXKHKJ8WT1CAQMN1YR). `/` types
/// the filter, and `Esc` clears it. With no host that shows, no other
/// key acts.
pub fn on_key(state: &mut State, key: Key) -> Option<Action> {
    match key {
        Key::Char('/') => {
            state.host_filter.typing = true;
            return None;
        }
        Key::Esc => {
            state.host_filter = Filter::default();
            return None;
        }
        _ => {}
    }
    let rows = state
        .last_look()
        .map(|(look, _)| shown(state, look))
        .unwrap_or_default();
    let last = rows.len().checked_sub(1)?;
    let at = selected(state, &rows);
    let to = match key {
        Key::Up => at.saturating_sub(1),
        Key::Down => (at + 1).min(last),
        Key::PageUp => at.saturating_sub(PAGE),
        Key::PageDown => (at + PAGE).min(last),
        Key::First => 0,
        Key::Last => last,
        Key::Row(row) => row.min(last),
        Key::Enter => {
            let host = safe(&rows[at].name.host);
            state.sessions.set_filter(format!("host:{host}"));
            state.open(Tab::Sessions);
            state.note(format!("The filter of the Sessions tab: host:{host}."));
            return None;
        }
        Key::Char('l') => {
            ask_limit(state, &rows[at]);
            return None;
        }
        _ => return None,
    };
    state.host = Some(rows[to].name.clone());
    None
}

/// Asks for the limit of the host of `row` in the note line. In the TUI,
/// only the user of a host sets its limit (01M6VFD0PPXDNH3NKD3YE4W9SB).
fn ask_limit(state: &mut State, row: &Row) {
    let me = state
        .last_look()
        .and_then(|(look, _)| look.here.as_ref())
        .map(|here| here.user.clone());
    if me.is_some_and(|user| user != row.name.user) {
        state.note(format!(
            "Only {} sets the limit of {}.",
            safe(&row.name.user),
            safe(&row.name.host)
        ));
        return;
    }
    state.typing = Some(Typing {
        host: row.name.host.clone(),
        now: row.facts.limit,
        typed: String::new(),
    });
}

/// The keys of the tab on the key line of `layout`, before the keys of
/// the frame: the text of the key, its word, and the key of a click.
/// From [`view::WIDER`] columns, `wider`, the line names `/` too
/// (01M6VJQPJXKHKJ8WT1CAQMN1YR).
pub fn keys(layout: Layout, wider: bool) -> Vec<(&'static str, &'static str, Option<Key>)> {
    let mut keys = vec![("↑↓", "select", None)];
    if layout == Layout::Wide {
        keys.push(("Enter", "sessions of the host", Some(Key::Enter)));
    }
    keys.push(("l", "limit", Some(Key::Char('l'))));
    if wider {
        keys.push(("/", "filter", Some(Key::Char('/'))));
    }
    keys
}

/// The keys of the key line while the person types the filter
/// (01M6VJQPJXKHKJ8WT1CAQMN1YR): the line has no other key.
pub fn filter_keys() -> Vec<(&'static str, &'static str, Option<Key>)> {
    vec![
        ("Enter", "keep the filter", Some(Key::Enter)),
        ("Esc", "clear it", Some(Key::Esc)),
    ]
}

/// The keys of the tab in the help popup, before the keys of the frame.
pub fn help_keys() -> Vec<(&'static str, &'static str)> {
    vec![
        ("↑ ↓ j k", "select a host"),
        ("PgUp PgDn", "move 10 hosts"),
        ("g G", "the first or last host"),
        ("Enter", "show its sessions"),
        ("l", "set its limit"),
        ("/", "filter: Enter keeps it, Esc clears it"),
        ("y, n", "answer a question"),
    ]
}

/// A column of the rows of the hosts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    /// The host, or `USER@HOST`.
    Host,
    /// The live sessions against the limit.
    Work,
    /// The 1-minute load against the hardware threads.
    Load,
    /// The clock now against the most clock of a core, in MHz.
    Clock,
    /// The free memory against the memory, in GB.
    Memory,
    /// The same, with no unit: the narrow layout.
    MemoryGb,
    /// The free disk against the disk, in GB.
    Disk,
    /// The used part of the disk.
    Used,
    /// The CPUs and the memory of each new container.
    Pool,
    /// The starts that wait for room.
    Waits,
    /// The age of the facts.
    Seen,
}

impl Column {
    /// The head of the column.
    fn head(self) -> &'static str {
        match self {
            Column::Host => "HOST",
            Column::Work => "WORK",
            Column::Load => "LOAD",
            Column::Clock => "CLOCK",
            Column::Memory => "MEM FREE",
            Column::MemoryGb => "MEM GB",
            Column::Disk => "DISK FREE",
            Column::Used => "USED",
            Column::Pool => "POOL",
            Column::Waits => "WAITS",
            Column::Seen => "SEEN",
        }
    }
}

/// The columns of the wide layout, and the width of each. The last
/// column has the rest of the row.
const WIDE_COLUMNS: [(Column, usize); 8] = [
    (Column::Host, 11),
    (Column::Work, 6),
    (Column::Load, 9),
    (Column::Memory, 12),
    (Column::Disk, 12),
    (Column::Used, 6),
    (Column::Waits, 6),
    (Column::Seen, 0),
];

/// The columns from [`view::WIDER`] columns (01M6VJQGSMNAJ82GX0YQ2YAHKC):
/// the wide ones, with the clock and the pool.
const WIDER_COLUMNS: [(Column, usize); 10] = [
    (Column::Host, 11),
    (Column::Work, 6),
    (Column::Load, 9),
    (Column::Clock, 15),
    (Column::Memory, 12),
    (Column::Disk, 12),
    (Column::Used, 6),
    (Column::Pool, 14),
    (Column::Waits, 6),
    (Column::Seen, 0),
];

/// The columns of the narrow layout.
const NARROW_COLUMNS: [(Column, usize); 4] = [
    (Column::Host, 11),
    (Column::Work, 6),
    (Column::Load, 8),
    (Column::MemoryGb, 0),
];

/// `text` in a column of `width` characters, with one space after it at
/// least. A longer text ends with `…`. A width of 0 keeps the text.
fn fit(text: &str, width: usize) -> String {
    if width == 0 {
        return text.to_owned();
    }
    if text.chars().count() < width {
        return format!("{text:<width$}");
    }
    let cut: String = text.chars().take(width.saturating_sub(2)).collect();
    format!("{cut}… ")
}

/// The cell of `row` with the name `name` in the column `column`, with
/// its style. A number over its limit has the warning style
/// (01M6VFCPK2PSDTHHE5TSY3XP5N). A fact that the host does not tell is
/// `-`.
fn cell(row: &Row, name: &str, column: Column) -> (String, Style) {
    let facts = &row.facts;
    let warn = |over: bool| if over { WARNING } else { Style::new() };
    let plain = |text: String| (text, Style::new());
    let none = || plain("-".to_owned());
    let mark = |over: bool| if over { " !" } else { "" };
    let low = row.faults.contains(&Fault::Memory);
    let tight = row.faults.contains(&Fault::Disk);
    match column {
        Column::Host => plain(name.to_owned()),
        Column::Work => (
            format!("{}/{}", facts.runs(), facts.limit),
            warn(row.faults.contains(&Fault::Limit)),
        ),
        Column::Load => facts.machine.map_or_else(none, |m| {
            (format!("{:.1}/{}", m.load, m.threads), warn(m.busy()))
        }),
        Column::Clock => facts
            .machine
            .map_or_else(none, |m| plain(format!("{}/{} MHz", m.now_mhz, m.mhz))),
        Column::Memory | Column::MemoryGb => facts.machine.map_or_else(none, |m| {
            let unit = if column == Column::Memory { " GB" } else { "" };
            (
                format!("{}/{}{unit}{}", m.avail_gb, m.mem_gb, mark(low)),
                warn(low),
            )
        }),
        Column::Disk => facts.disk.map_or_else(none, |d| {
            (
                format!("{}/{} GB{}", d.free_gb, d.total_gb, mark(tight)),
                warn(tight),
            )
        }),
        Column::Used => facts
            .disk
            .map_or_else(none, |d| plain(format!("{}%", d.used_percent()))),
        Column::Pool => facts.caps.map_or_else(none, |c| {
            plain(format!("{} CPU {} GB", c.cpus, c.memory_gb))
        }),
        Column::Waits => plain(facts.waiting.len().to_string()),
        Column::Seen => plain(text::ago(row.age_secs)),
    }
}

/// The line of the cells of `row` with the name `name` in the columns
/// `columns`, after the lead-in `pre`.
fn columns_line(
    pre: Vec<Span<'static>>,
    row: &Row,
    name: &str,
    columns: &[(Column, usize)],
) -> Line<'static> {
    let mut spans = pre;
    for (column, width) in columns {
        let (text, style) = cell(row, name, *column);
        spans.push(Span::styled(fit(&text, *width), style));
    }
    Line::from(spans)
}

/// The line of the heads of `columns`, after the lead-in.
fn heads_line(columns: &[(Column, usize)]) -> Line<'static> {
    let mut spans = vec![Span::raw("    ")];
    for (column, width) in columns {
        spans.push(Span::raw(fit(column.head(), *width)));
    }
    Line::from(spans).style(DIM)
}

/// The name of the host of `row` among the hosts `all`: the host, or
/// `USER@HOST` for a host with the same name as the host of another
/// user.
fn name_of(all: &[Row], row: &Row) -> String {
    let twin = all
        .iter()
        .any(|other| other.name.host == row.name.host && other.name.user != row.name.user);
    if twin {
        format!("{}@{}", safe(&row.name.user), safe(&row.name.host))
    } else {
        safe(&row.name.host)
    }
}

/// The first line of the tab (01M6VFD0CAS71DNBGE7K56JJ2H,
/// 01M6VFD0M3RHT488D0RQ84RVMC, 01M6VJQPJXKHKJ8WT1CAQMN1YR): the count of
/// the hosts and of each fault. The narrow layout counts the hosts with
/// a fault. With a filter, it counts the hosts that match, `rows`,
/// against all the hosts, `all`, and ends with the filter. When the
/// rows scroll, it shows the place of the rows that show.
fn summary(
    rows: &[Row],
    all: usize,
    filter: &Filter,
    wide: bool,
    shown: std::ops::Range<usize>,
) -> String {
    let n = rows.len();
    let hosts = if all == 1 { "host" } else { "hosts" };
    let mut parts = vec![if filter.is_set() {
        format!("{n} of {all} {hosts}")
    } else {
        format!("{n} {hosts}")
    }];
    if wide {
        for fault in Fault::ALL {
            let count = rows.iter().filter(|r| r.faults.contains(&fault)).count();
            if count > 0 {
                parts.push(fault.count(count));
            }
        }
    } else {
        let faulty = rows.iter().filter(|r| !r.faults.is_empty()).count();
        if faulty > 0 {
            let verb = if faulty == 1 { "needs" } else { "need" };
            parts.push(format!("{faulty} {verb} a look"));
        }
    }
    if !shown.is_empty() && shown.len() < n {
        parts.push(format!("{}-{} of {n}", shown.start + 1, shown.end));
    }
    if filter.is_set() {
        parts.push(format!("/{}", safe(filter.words.trim())));
    }
    parts.join(" · ")
}

/// The title of the detail of `row` at `now_ms` (01M6VFD0EXECEEVNN969SAPV4C):
/// the pid of the daemon and the time since its start for this host,
/// else the build of its daemon.
fn detail_title(row: &Row, now_ms: i64) -> String {
    let host = safe(&row.name.host);
    match row.process {
        Some(p) => {
            let up = u64::try_from((now_ms - p.since_ms) / 1000).unwrap_or(0);
            format!("{host} · riff daemon pid {}, up {}", p.pid, text::ago(up))
        }
        None => format!(
            "{host} · riff daemon build {}",
            daemon_build(&row.facts.build)
        ),
    }
}

/// The short build of a daemon: the first 7 characters of its ID.
///
/// ```
/// # use riff::tui::machines::daemon_build;
/// assert_eq!(daemon_build("0.1.0 10df8a4c0ffe 2026-11-06T12:00:00Z"), "10df8a4");
/// assert_eq!(daemon_build("odd"), "odd");
/// assert_eq!(daemon_build(""), "?");
/// ```
pub fn daemon_build(build: &str) -> String {
    match build.parse::<Build>() {
        Ok(build) => view::short(&build.id),
        Err(_) => build
            .split_whitespace()
            .next()
            .map_or_else(|| "?".to_owned(), safe),
    }
}

/// The lines of the detail of `row` (01M6VFD0EXECEEVNN969SAPV4C): the
/// numbers of its machine, its limit and its floor with the starts that
/// wait, a line for each fault, its parts by repository, each repository
/// that it left with the sessions that still run
/// (01M6WJYQN2WY5NH3DVDBAAY8ST), its waves and its sessions.
fn detail_lines(row: &Row) -> Vec<Line<'static>> {
    let facts = &row.facts;
    let mut lines = Vec::new();
    if let Some(m) = facts.machine {
        lines.push(Line::raw(m.to_string()));
    }
    let waits = match facts.waiting.len() {
        1 => "1 start waits".to_owned(),
        n => format!("{n} starts wait"),
    };
    let mut limit = format!(
        "limit {} {} and a floor of {} GB · {waits}",
        facts.limit,
        sessions_word(facts.limit),
        facts.floor_gb
    );
    if let Some(p) = row.process {
        let jobs = if p.jobs == 1 { "job" } else { "jobs" };
        limit.push_str(&format!(" · {} {jobs}", p.jobs));
    }
    lines.push(Line::raw(limit));
    for fault in &row.faults {
        lines.push(Line::styled(fault.line(facts), WARNING));
    }
    lines.push(Line::raw(if facts.parts.is_empty() {
        "parts none".to_owned()
    } else {
        text::parts_line(&facts.parts)
    }));
    for leaving in &facts.leaving {
        lines.push(Line::raw(text::leaving_line(leaving)));
        if !leaving.left() {
            let sessions: Vec<String> = leaving
                .sessions
                .iter()
                .map(|name| session_word(facts, name))
                .collect();
            lines.push(Line::raw(format!("  {}", sessions.join(" · "))));
        }
    }
    for wave in &facts.waves {
        lines.push(Line::raw(format!(
            "wave {}: {}",
            safe(&wave.repo),
            safe(&wave.word)
        )));
    }
    let mut sessions: Vec<String> = facts
        .sessions
        .iter()
        .map(|s| {
            // The name of a container ends with the short session ID.
            let id = s.name.rsplit('-').next().unwrap_or(&s.name);
            format!("{} {}", safe(id), text::phase_word(s.phase))
        })
        .collect();
    if facts.more > 0 {
        sessions.push(format!("{} more", facts.more));
    }
    if sessions.is_empty() {
        sessions.push("none".to_owned());
    }
    lines.push(Line::raw(format!("sessions: {}", sessions.join(" · "))));
    lines
}

/// The lines of the link and the server (01M6VFD0HGCVMYNNC3H4788W9X):
/// the link with the time of the call `who`, the reconnects, the build
/// of riff-server against the build of this riff, and the GitHub points.
/// The narrow layout puts the build on a line of its own, and the
/// points that are left only.
fn link_lines(
    state: &State,
    look: &Look,
    at_ms: i64,
    wide: bool,
    now_ms: i64,
) -> Vec<Line<'static>> {
    let link = if state.link_down() {
        let secs = u64::try_from((now_ms - at_ms) / 1000).unwrap_or(0);
        Span::styled(
            format!("link down: no good look for {}", text::ago(secs)),
            ERROR,
        )
    } else {
        match look.call {
            Some(call) => Span::styled(format!("link ok: {} ms", call.as_millis()), GOOD),
            None => Span::styled("link ok", GOOD),
        }
    };
    let reconnects = match look.reconnects {
        1 => "1 reconnect".to_owned(),
        n => format!("{n} reconnects"),
    };
    let build = state.build();
    let server = match &look.server {
        Some(server) if server.matches(build) => Span::raw(format!(
            "riff-server {}{}, same as yours",
            if wide { "build " } else { "" },
            view::short(&server.id)
        )),
        Some(server) => Span::styled(
            format!(
                "riff-server {}{}, yours is {}",
                if wide { "build " } else { "" },
                view::short(&server.id),
                view::short(&build.id)
            ),
            WARNING,
        ),
        None => Span::raw("riff-server build ?"),
    };
    let github = Line::raw(github_line(look, wide));
    let dot = || Span::raw(" · ");
    if wide {
        vec![
            Line::from(vec![link, dot(), Span::raw(reconnects), dot(), server]),
            github,
        ]
    } else {
        vec![
            Line::from(vec![link, dot(), Span::raw(reconnects)]),
            Line::from(server),
            github,
        ]
    }
}

/// The line of the GitHub points (01M6VFD0HGCVMYNNC3H4788W9X), from the
/// daemon of this host: the points that are left, and the points of riff
/// and of the other callers in the last hour. The other callers show
/// only when riff knows their points.
fn github_line(look: &Look, wide: bool) -> String {
    let Some(status) = look.here.as_ref().and_then(|here| here.daemon.as_ref()) else {
        return if wide {
            "GitHub: no riff daemon here counts the points".to_owned()
        } else {
            "GitHub: no riff daemon here".to_owned()
        };
    };
    let shown = &status.github;
    let points = text::grouped(shown.total().1.into());
    let spent = match shown.other.filter(|other| !other.counted_more()) {
        Some(other) => format!(
            "riff spent {points}, others {}, in the hour",
            text::grouped(other.points)
        ),
        None => format!("riff spent {points} in the hour"),
    };
    let left = shown.left.as_ref().map(|left| {
        format!(
            "{} of {} points left",
            text::grouped(left.remaining),
            text::grouped(left.limit)
        )
    });
    match (left, wide) {
        (Some(left), true) => format!("GitHub: {left} · {spent}"),
        (Some(left), false) => format!("GitHub: {left}"),
        (None, _) => format!("GitHub: {spent}"),
    }
}

/// A rule over the whole width with `title`: `──TITLE ───…`.
fn rule(title: &str, width: u16) -> Line<'static> {
    let head = format!("──{title} ");
    let rest = usize::from(width).saturating_sub(head.chars().count());
    Line::styled(format!("{head}{}", "─".repeat(rest)), DIM)
}

/// The body of the Machines tab in the rows of `area`, in `layout`, at
/// the time `now_ms` (01M6VAZKNXZV1RTG7MT0GMHKT3): its rows, and the
/// place of each host that a click selects. It reads only its arguments.
/// The rows of the hosts, the detail and the lines of the link share
/// the rows of the area (01M6VFD0EXECEEVNN969SAPV4C): the hosts get at
/// least half of the rest, and the detail loses its last lines first.
/// The rows are the hosts that the filter shows, with the clock and the
/// pool from [`view::WIDER`] columns (01M6VJQGSMNAJ82GX0YQ2YAHKC,
/// 01M6VJQPJXKHKJ8WT1CAQMN1YR).
pub fn body(state: &State, area: Rect, layout: Layout, now_ms: i64) -> (Vec<Placed>, Vec<Hit>) {
    let wide = layout == Layout::Wide;
    let margin = u16::from(wide);
    let mut placed = Vec::new();
    let mut hits = Vec::new();
    let Some((look, at_ms)) = state.last_look() else {
        return (placed, hits);
    };
    let total = usize::from(area.height);
    // Each line goes on the row after the last one.
    let next_row = |placed: &[Placed]| area.y + u16::try_from(placed.len()).unwrap_or(u16::MAX);
    let push = |line: Line<'static>, full: bool, placed: &mut Vec<Placed>| {
        let (x, w) = if full {
            (area.x, area.width)
        } else {
            (area.x + margin, area.width - margin)
        };
        placed.push(Placed {
            area: Rect::new(x, next_row(placed), w, 1),
            line,
        });
    };
    let link = link_lines(state, look, at_ms, wide, now_ms);
    let all = rows(look);
    if all.is_empty() {
        let width = usize::from((area.width - margin).max(1));
        let mut lines: Vec<Line<'static>> = Vec::new();
        for text in [NO_HOSTS, ADD_HOST] {
            lines.extend(
                textwrap::wrap(text, width)
                    .into_iter()
                    .map(|line| Line::raw(line.into_owned())),
            );
        }
        let room = total.saturating_sub(lines.len());
        for line in lines.into_iter().take(total) {
            push(line, false, &mut placed);
        }
        if room >= link.len() + 2 {
            push(Line::raw(""), false, &mut placed);
            push(
                rule("the link and the server", area.width),
                true,
                &mut placed,
            );
            for line in link {
                push(line, false, &mut placed);
            }
        }
        return (placed, hits);
    }

    let filter = state.host_filter();
    let rows: Vec<Row> = all
        .iter()
        .filter(|row| filter.shows(row))
        .cloned()
        .collect();

    // The first line and the head of the columns.
    let mut room = total.saturating_sub(2);
    let link_rows = if room > link.len() + 2 {
        link.len() + 2
    } else {
        0
    };
    room -= link_rows;
    let at = selected(state, &rows);
    let detail = match rows.get(at) {
        Some(row) if wide => detail_lines(row),
        _ => Vec::new(),
    };
    let detail_rows = if detail.is_empty() {
        0
    } else {
        detail.len() + 2
    };
    let (shown, list) = window(&rows, at, room.saturating_sub(detail_rows.min(room / 2)));
    let columns: &[(Column, usize)] = match layout {
        Layout::Wide if area.width >= view::WIDER => &WIDER_COLUMNS,
        Layout::Wide => &WIDE_COLUMNS,
        _ => &NARROW_COLUMNS,
    };

    push(
        Line::raw(summary(&rows, all.len(), filter, wide, shown.clone())),
        false,
        &mut placed,
    );
    push(heads_line(columns), false, &mut placed);
    // The marks of a row go under it, in the column after the host.
    let indent = 4 + columns.first().map_or(0, |(_, width)| *width);
    let mut lines_left = list;
    for index in shown {
        let row = &rows[index];
        let choice = if index == at {
            Span::styled("> ", ACTIVE)
        } else {
            Span::raw("  ")
        };
        let mark = if row.faults.is_empty() {
            Span::raw("  ")
        } else {
            Span::styled(
                format!("{} ", Mark::Blocked.symbol()),
                Mark::Blocked.style(),
            )
        };
        let line = columns_line(vec![choice, mark], row, &name_of(&all, row), columns);
        let under = marks(&row.facts)
            .into_iter()
            .map(|mark| Line::raw(format!("{:indent$}{mark}", "")));
        for line in std::iter::once(line).chain(under).take(lines_left) {
            hits.push(Hit {
                area: Rect::new(area.x, next_row(&placed), area.width, 1),
                key: Key::Row(index),
            });
            push(line, false, &mut placed);
            lines_left -= 1;
        }
    }
    // The detail gets the rows that the hosts leave: a blank row, the
    // title, and as many lines as fit.
    let detail_room = room - list;
    if detail_rows > 0 && detail_room >= 3 {
        push(Line::raw(""), false, &mut placed);
        push(
            rule(&detail_title(&rows[at], now_ms), area.width),
            true,
            &mut placed,
        );
        for line in detail.into_iter().take(detail_room - 2) {
            push(line, false, &mut placed);
        }
    }
    if link_rows > 0 {
        push(Line::raw(""), false, &mut placed);
        push(
            rule("the link and the server", area.width),
            true,
            &mut placed,
        );
        for line in link {
            push(line, false, &mut placed);
        }
    }
    (placed, hits)
}

/// The rows of `rows` that show in `lines` lines with the selected row
/// `at`, and the lines that they take (01M6VFD0M3RHT488D0RQ84RVMC): a row
/// takes a line, and a line for each of its [`marks`]
/// (01M6WJYQN2WY5NH3DVDBAAY8ST). The rows scroll with the selection, so
/// the selected row and its marks show. The last row that shows can lose
/// its last marks.
fn window(rows: &[Row], at: usize, lines: usize) -> (std::ops::Range<usize>, usize) {
    let height = |row: &Row| 1 + marks(&row.facts).len();
    let mut first = 0;
    while first < at && rows[first..=at].iter().map(height).sum::<usize>() > lines {
        first += 1;
    }
    let (mut end, mut used) = (first, 0);
    while end < rows.len() && used < lines {
        used += height(&rows[end]);
        end += 1;
    }
    (first..end, used.min(lines))
}

/// The note when the loop starts [`set_limit`].
///
/// ```
/// assert_eq!(
///     riff::tui::machines::setting("thelio", 4),
///     "riff asked for the limit of 4 sessions on thelio, and waits for the reply."
/// );
/// ```
pub fn setting(host: &str, sessions: u32) -> String {
    format!(
        "riff asked for the limit of {sessions} {} on {}, and waits for the reply.",
        sessions_word(sessions),
        safe(host)
    )
}

/// Sets the most live sessions of `host` to `sessions` as `me` at `api`
/// (01M6VFD0PPXDNH3NKD3YE4W9SB), with the code of `riff daemon limit
/// --sessions N`: for the host of `me`, in the folder of its daemon
/// ([`crate::room::set`]), and for another host through riff-server, as
/// with `--host HOST` ([`crate::daemon::remote::ask`]). It gives the
/// note: the first line of the reply, or of the error.
pub async fn set_limit(api: &Api, me: &SessionUri, host: &str, sessions: u32) -> String {
    use crate::daemon::remote;
    let change = crate::room::Change {
        sessions: Some(sessions),
        ..crate::room::Change::default()
    };
    let done = if host == me.place().host() {
        crate::daemon::dir()
            .and_then(|dir| crate::room::set(&dir, change))
            .map(|limit| text::daemon_limit(&limit))
    } else {
        let request = remote::Request::Limit(change);
        remote::ask(api, me, host, &request, remote::REPLY_WAIT).await
    };
    let first = |text: &str| safe(text.lines().next().unwrap_or_default());
    match done {
        Ok(reply) => format!("{}: {}", safe(host), first(&reply)),
        Err(e) => format!(
            "riff cannot set the limit of {}: {}",
            safe(host),
            first(&format!("{e:#}"))
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::view::{compute_view, render};
    use crate::tui::{News, text_block};
    use ratatui::buffer::Buffer;
    use riff_client::daemons::{Caps, Leaving, Parts, RepoParts, Session, Wave};
    use riff_client::sessions::Phase;
    use riff_core::wire::{MachineInfo, RiffOwner, SessionInfo};

    /// The time of the screen tests.
    const NOW: i64 = 1_000_000_000;

    fn build(id: &str) -> Build {
        format!("0.1.0 {id} 2026-11-06T12:00:00Z").parse().unwrap()
    }

    /// The session of the daemon of `user` on `host` in `who`, with the
    /// facts `facts`, sent `age` seconds ago.
    fn daemon(user: &str, host: &str, facts: &Facts, age: u64) -> SessionInfo {
        let mut s: SessionInfo = serde_json::from_value(serde_json::json!({
            "uri": format!("riff://{user}@{host}/como-technologies/riff?session=d-{host}"),
            "live": true,
        }))
        .unwrap();
        s.machine = Some(MachineInfo {
            numbers: facts.line(),
            age_secs: age,
        });
        s
    }

    fn machine(threads: u16, mem_gb: u32, avail_gb: u32, load: f32) -> Machine {
        Machine {
            threads,
            mhz: 5883,
            now_mhz: 4100,
            mem_gb,
            avail_gb,
            load,
        }
    }

    fn session(id: &str, item: &str, phase: Phase) -> Session {
        Session {
            name: format!("riff-{item}-{id}"),
            item: item.into(),
            phase,
            last: String::new(),
            digest: String::new(),
        }
    }

    /// The part `workers` in `repo`.
    fn workers(repo: &str) -> RepoParts {
        RepoParts {
            repo: repo.into(),
            parts: Parts {
                workers: true,
                lead: false,
            },
        }
    }

    /// Facts of a fine host: the part workers in como-technologies/riff,
    /// podman, each sign-in on, and caps of 8 CPUs and 8 GB.
    fn fine(limit: u32, machine: Machine, disk: Disk) -> Facts {
        Facts {
            build: "0.1.0 d2ae0801c0de 2026-11-06T12:00:00Z".into(),
            limit,
            floor_gb: 8,
            parts: vec![workers("como-technologies/riff")],
            machine: Some(machine),
            disk: Some(disk),
            caps: Some(Caps {
                cpus: 8,
                memory_gb: 8,
            }),
            podman: Some("5.2.1".into()),
            riff: Sign::On,
            gh: Sign::On,
            claude: Sign::Left { minutes: 300 },
            ..Facts::default()
        }
    }

    /// The status of the daemon of thelio on its socket: pid 4242, up 2
    /// days, a limit of 3 sessions, three sessions and a lead, one start
    /// that waits, one wave and one job, the parts workers and lead, and
    /// the GitHub points.
    fn thelio_status() -> crate::daemon::Status {
        let live = |name: &str, item: &str, phase: &str| {
            serde_json::json!({
                "name": name, "session": "s", "item": item, "image": "i",
                "since_ms": 0, "phase": phase,
            })
        };
        let mut status: crate::daemon::Status = serde_json::from_value(serde_json::json!({
            "pid": 4242,
            "build": "0.1.0 10df8a4c0ffe 2026-11-06T12:00:00Z",
            "since_ms": NOW - 2 * 86_400_000,
            "dir": "/d",
            "sessions": [
                live("riff-issue-1590-5b1e2a90", "issue-1590", "works"),
                live("riff-issue-1594-9c0d1e2f", "issue-1594", "works"),
                live("riff-issue-994-3a3f8d5d", "issue-994", "ends"),
                live("riff-lead-como-technologies-riff", "lead", "terminal"),
            ],
            "limit": {"sessions": 3, "floor_gb": 8, "cpus": 8, "memory_gb": 8},
            "waiting": ["riff-issue-1601-1c2d3e4f"],
            "waves": [{"repo": "como-technologies/riff", "word": "started issue-1594"}],
            "parts": [{"repo": "como-technologies/riff", "workers": true, "lead": true}],
            "jobs": 1,
        }))
        .unwrap();
        status.github = crate::ghcalls::Shown {
            rows: vec![crate::ghcalls::Row {
                caller: "look".into(),
                kind: "pr list".into(),
                calls: 12,
                points: 310,
            }],
            left: Some(crate::ghcalls::Left {
                remaining: 4210,
                reset_at: "2026-11-06T13:00:00Z".into(),
                read_ms: NOW,
                limit: 5000,
            }),
            other: Some(crate::ghcalls::Other {
                points: 480,
                used: 790,
                counted: 310,
                since_ms: 0,
                read_ms: NOW,
            }),
            ..crate::ghcalls::Shown::default()
        };
        status
    }

    /// The state of the mock-up of the Machines tab of #994: mike on
    /// thelio, whose daemon answers on its socket, and the daemons of
    /// pangolin and strata-ci in `who`. Thelio is at its limit and low
    /// on memory. The link is good, and riff-server has the same build.
    fn mock_up() -> State {
        let mut state = State::new(build("10df8a4c0ffe"), "riff://mike@thelio".parse().unwrap());
        let mut look = Look::default();
        look.who.owner = RiffOwner::Owner {
            user: "mike".into(),
            email: "m@x.io".into(),
        };
        look.server = Some(build("10df8a4c0ffe"));
        look.call = Some(Duration::from_millis(118));
        look.reconnects = 1;
        let pangolin = Facts {
            sessions: vec![
                session("7a8b9c0d", "issue-1610", Phase::Works),
                session("1e2f3a4b", "issue-1611", Phase::Works),
            ],
            caps: Some(Caps {
                cpus: 4,
                memory_gb: 8,
            }),
            ..fine(
                4,
                Machine {
                    mhz: 4500,
                    now_mhz: 4400,
                    ..machine(16, 32, 14, 3.1)
                },
                Disk {
                    free_gb: 90,
                    total_gb: 200,
                },
            )
        };
        let strata = Facts {
            caps: Some(Caps {
                cpus: 2,
                memory_gb: 16,
            }),
            ..fine(
                2,
                Machine {
                    mhz: 3600,
                    now_mhz: 1200,
                    ..machine(8, 64, 30, 0.4)
                },
                Disk {
                    free_gb: 140,
                    total_gb: 500,
                },
            )
        };
        // The facts that thelio sent: the socket replaces them.
        let thelio = Facts {
            waves: vec![Wave {
                repo: "como-technologies/riff".into(),
                word: "waits".into(),
                image: None,
                waits: Default::default(),
            }],
            ..fine(
                3,
                machine(32, 124, 100, 0.5),
                Disk {
                    free_gb: 400,
                    total_gb: 455,
                },
            )
        };
        look.who.sessions = vec![
            daemon("mike", "strata-ci", &strata, 28),
            daemon("mike", "thelio", &thelio, 20),
            daemon("mike", "pangolin", &pangolin, 12),
        ];
        look.here = Some(Here {
            user: "mike".into(),
            host: "thelio".into(),
            daemon: Some(thelio_status()),
            machine: Some(Machine {
                threads: 32,
                mhz: 5883,
                now_mhz: 4100,
                mem_gb: 124,
                avail_gb: 2,
                load: 18.96,
            }),
            disk: Some(Disk {
                free_gb: 95,
                total_gb: 455,
            }),
        });
        state.looked(look, NOW - 2_000);
        state.news(Tab::Chat, News::Lines(2));
        state.news(Tab::Waves, News::Dot);
        state.on_key(Key::Open(Tab::Machines));
        state
    }

    /// The rows of `state` drawn into a `Buffer` of `width` x `height`,
    /// with no spaces at their ends.
    fn drawn(state: &State, width: u16, height: u16) -> Vec<String> {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        render(&compute_view(state, area, NOW), &mut buffer);
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    /// A rule of `width` columns with `title`.
    fn ruled(title: &str, width: usize) -> String {
        let head = format!("──{title} ");
        let rest = width - head.chars().count();
        format!("{head}{}", "─".repeat(rest))
    }

    /// The state of [`mock_up`] with the filter `pang` kept: `/`, the
    /// word, then Enter.
    fn filtered() -> State {
        let mut state = mock_up();
        state.on_key(Key::Char('/'));
        for c in "pang".chars() {
            state.on_key(Key::Char(c));
        }
        state.on_key(Key::Enter);
        state
    }

    /// The mock-up in the rustdoc of this module and in the book is the
    /// screen of [`mock_up`]: the book shows the body only. The rows at
    /// 132 columns and the rows of the filter `pang` are the screens too.
    #[test]
    fn the_rustdoc_and_the_book_show_the_screen() {
        let screen = drawn(&mock_up(), 80, 24);
        let wider = drawn(&mock_up(), 132, 24);
        let filter = drawn(&filtered(), 80, 24);
        let leaves = drawn(&leaving(), 80, 24);
        let doc = include_str!("machines.rs");
        assert_eq!(text_block(doc, "//! # Design", "//! "), screen);
        assert_eq!(
            text_block(doc, "//! # A host that leaves a repository", "//! "),
            leaves[2..16]
        );
        assert_eq!(text_block(doc, "//! # The wide rows", "//! "), wider[3..7]);
        assert_eq!(text_block(doc, "//! # The filter", "//! "), filter[2..5]);
        let book = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/src/how-it-works.md"
        ))
        .unwrap();
        let block = |after: &str| text_block(&book, after, "");
        assert_eq!(block("#### See the hosts in the TUI"), screen[2..20]);
        assert_eq!(block("After `riff leave --host pangolin`"), leaves[2..16]);
        assert_eq!(block("In a terminal of 120 columns or more"), wider[3..7]);
        assert_eq!(block("#### Filter the hosts in the TUI"), filter[2..5]);
    }

    /// 01M6VJQGSMNAJ82GX0YQ2YAHKC: at 132 x 24, the rows add the clock
    /// and the pool, and the key line adds `/ filter`. The detail and the
    /// lines of the link are the ones of 80 columns.
    #[test]
    fn the_machines_tab_at_132_x_24_adds_the_clock_and_the_pool() {
        let state = mock_up();
        let mut want: Vec<String> = [
            " 1 Sessions  2 Chat•2  3 Waves•  [4 Machines]  5 Devices  6 Reports",
            " riff running · owner mike · build 10df8a4 · link ok 2s",
            " 3 hosts · 1 at its limit · 1 low on memory",
            "     HOST       WORK  LOAD     CLOCK          MEM FREE    DISK FREE   USED  POOL          WAITS SEEN",
            " > ! thelio     3/3   19.0/32  4100/5883 MHz  2/124 GB !  95/455 GB   79%   8 CPU 8 GB    1     0s",
            "     pangolin   2/4   3.1/16   4400/4500 MHz  14/32 GB    90/200 GB   55%   4 CPU 8 GB    0     12s",
            "     strata-ci  0/2   0.4/8    1200/3600 MHz  30/64 GB    140/500 GB  72%   2 CPU 16 GB   0     28s",
            "",
        ]
        .map(String::from)
        .to_vec();
        want.push(ruled("thelio · riff daemon pid 4242, up 2d", 132));
        let narrow = drawn(&state, 80, 24);
        want.extend(narrow[9..17].iter().cloned());
        want.push(ruled("the link and the server", 132));
        want.extend(narrow[18..20].iter().cloned());
        want.resize(23, String::new());
        want.push(
            " ↑↓ select  Enter sessions of the host  l limit  / filter  r read again  ? help  q quit"
                .into(),
        );
        assert_eq!(drawn(&state, 132, 24), want);
        // From 120 columns: 119 columns have no clock.
        assert_eq!(drawn(&state, 120, 24)[3], want[3]);
        assert_eq!(drawn(&state, 119, 24)[3], narrow[3]);
        // A click on `/ filter` types the filter.
        let view = compute_view(&state, Rect::new(0, 0, 132, 24), NOW);
        let keys = &want[23];
        let at = u16::try_from(keys.chars().take_while(|c| *c != '/').count()).unwrap();
        assert_eq!(view.hit(at, 23), Some(Key::Char('/')));
    }

    /// 01M6VJQGSMNAJ82GX0YQ2YAHKC: a host that tells no numbers and no
    /// caps shows `-` for the clock and the pool.
    #[test]
    fn a_host_that_tells_no_clock_and_no_caps_shows_a_dash() {
        let mut state = State::new(build("10df8a4c0ffe"), "riff://mike@thelio".parse().unwrap());
        let mut look = Look::default();
        let silent = Facts {
            machine: None,
            caps: None,
            ..fine(
                2,
                machine(8, 64, 30, 0.4),
                Disk {
                    free_gb: 140,
                    total_gb: 500,
                },
            )
        };
        look.who.sessions = vec![daemon("mike", "m", &silent, 5)];
        state.looked(look, NOW);
        state.on_key(Key::Open(Tab::Machines));
        assert_eq!(
            drawn(&state, 132, 24)[4],
            " > ! m          0/2   -        -              -           140/500 GB  72%   -             0     5s"
        );
    }

    /// 01M6VJQPG6RM3Q4QAJEHFJ6RQK: `USED` is the used part of the disk,
    /// rounded to the nearest percent: 95 of 455 GB free is 79%, not the
    /// 80% of 100 less the free part rounded down.
    #[test]
    fn the_use_of_95_of_455_gb_free_is_79_percent() {
        let state = mock_up();
        let (look, _) = state.last_look().unwrap();
        let found = rows(look);
        let thelio = found.iter().find(|r| r.name.host == "thelio").unwrap();
        assert_eq!(
            thelio.facts.disk,
            Some(Disk {
                free_gb: 95,
                total_gb: 455
            })
        );
        assert_eq!(cell(thelio, "thelio", Column::Used).0, "79%");
        let at = |free_gb, total_gb| {
            let row = Row {
                facts: Facts {
                    disk: Some(Disk { free_gb, total_gb }),
                    ..thelio.facts.clone()
                },
                ..thelio.clone()
            };
            cell(&row, "thelio", Column::Used).0
        };
        assert_eq!(at(400, 455), "12%");
        assert_eq!(at(0, 455), "100%");
        assert_eq!(at(455, 455), "0%");
    }

    /// 01M6VJQPJXKHKJ8WT1CAQMN1YR: `/` with a word shows only the hosts
    /// that match. The note line shows the filter while the person types
    /// it, Enter keeps it, and the first line counts the hosts that match
    /// and shows the filter. Esc shows each host again.
    #[test]
    fn the_filter_shows_the_hosts_that_match() {
        let mut state = mock_up();
        state.on_key(Key::Char('/'));
        // `g` is a letter of the filter, not the first host.
        for c in "pang".chars() {
            state.on_key(Key::Char(c));
        }
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[2], " 1 of 3 hosts · /pang");
        assert!(rows[4].starts_with(" >   pangolin   2/4 "), "{rows:#?}");
        assert_eq!(rows[5], "");
        assert_eq!(rows[6], ruled("pangolin · riff daemon build d2ae080", 80));
        assert_eq!(rows[22], " /pang█");
        assert_eq!(rows[23], " Enter keep the filter  Esc clear it");
        // A click on `Esc clear it` presses Esc.
        let view = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
        assert_eq!(view.hit(25, 23), Some(Key::Esc));
        assert_eq!(view.hit(5, 23), Some(Key::Enter));
        state.on_key(Key::Enter);
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[2], " 1 of 3 hosts · /pang");
        assert_eq!(rows[22], "");
        assert_eq!(
            rows[23],
            " ↑↓ select  Enter sessions of the host  l limit  r read again  ? help  q quit"
        );
        // The keys act on the hosts that show.
        state.on_key(Key::Last);
        state.on_key(Key::Enter);
        assert_eq!(state.choices().filter(), "host:pangolin");
        state.on_key(Key::Open(Tab::Machines));
        assert_eq!(drawn(&state, 40, 24)[2], "1 of 3 hosts · /pang");
        // Esc shows each host again.
        state.on_key(Key::Esc);
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[2], " 3 hosts · 1 at its limit · 1 low on memory");
        assert!(rows[5].starts_with(" >   pangolin "), "{rows:#?}");
    }

    /// 01M6VJQPJXKHKJ8WT1CAQMN1YR: each word must match, a key matches
    /// only its facts, and a filter with no host that matches shows no
    /// row and no detail.
    #[test]
    fn each_word_and_each_key_of_the_filter_must_match() {
        let shows = |words: &str| {
            let mut state = mock_up();
            state.host_filter.words = words.into();
            let (look, _) = state.last_look().unwrap();
            shown(&state, look)
                .into_iter()
                .map(|row| row.name.host)
                .collect::<Vec<_>>()
        };
        assert_eq!(shows("state:memory"), ["thelio"]);
        assert_eq!(shows("item:1610"), ["pangolin"]);
        assert_eq!(shows("repo:riff"), ["thelio"]);
        assert_eq!(shows("user:mike"), ["thelio", "pangolin", "strata-ci"]);
        assert_eq!(shows("MIKE ci"), ["strata-ci"]);
        assert_eq!(shows("host:mike"), Vec::<String>::new());
        assert_eq!(shows("i"), ["thelio", "pangolin", "strata-ci"]);
        let mut state = mock_up();
        state.on_key(Key::Char('/'));
        state.on_key(Key::Char('x'));
        state.on_key(Key::Enter);
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[2], " 0 of 3 hosts · /x");
        assert_eq!(rows[4], "");
        assert_eq!(rows[5], ruled("the link and the server", 80));
        // No key acts on a host, and `l` asks nothing.
        assert_eq!(state.on_key(Key::Char('l')), None);
        assert_eq!(state.on_key(Key::Enter), None);
        assert_eq!((state.tab(), state.asked()), (Tab::Machines, None));
        // `/` types on: Backspace, then a new word.
        state.on_key(Key::Char('/'));
        state.on_key(Key::Backspace);
        for c in "strata".chars() {
            state.on_key(Key::Char(c));
        }
        state.on_key(Key::Enter);
        assert_eq!(drawn(&state, 80, 24)[2], " 1 of 3 hosts · /strata");
    }

    /// 01M6VJQPJXKHKJ8WT1CAQMN1YR: Esc closes the help popup first, and
    /// keeps the filter. Then Esc clears the filter.
    #[test]
    fn esc_closes_the_help_first_and_then_clears_the_filter() {
        let mut state = filtered();
        state.on_key(Key::Help);
        assert!(state.help_shown());
        state.on_key(Key::Esc);
        assert!(!state.help_shown());
        assert!(state.host_filter().is_set());
        state.on_key(Key::Esc);
        assert!(!state.host_filter().is_set());
        // The help names `/`.
        state.on_key(Key::Help);
        let text = drawn(&state, 80, 24).join("\n");
        assert!(
            text.contains("filter: Enter keeps it, Esc clears it"),
            "{text}"
        );
    }

    /// The mock-up of the Machines tab of #994 at 80 x 24
    /// (01M6VAZKNXZV1RTG7MT0GMHKT3, 01M6VFCPK2PSDTHHE5TSY3XP5N,
    /// 01M6VFD09QN71SWMPP8YXCRAEK, 01M6VFD0CAS71DNBGE7K56JJ2H,
    /// 01M6VFD0EXECEEVNN969SAPV4C, 01M6VFD0HGCVMYNNC3H4788W9X). The row of
    /// thelio comes from its socket: its sessions, its start that waits,
    /// its memory and its disk are not the ones that it sent.
    #[test]
    fn the_machines_tab_at_80_x_24_is_the_mock_up() {
        let state = mock_up();
        let mut want: Vec<String> = [
            " 1 Sessions  2 Chat•2  3 Waves•  [4 Machines]  5 Devices  6 Reports",
            " riff running · owner mike · build 10df8a4 · link ok 2s",
            " 3 hosts · 1 at its limit · 1 low on memory",
            "     HOST       WORK  LOAD     MEM FREE    DISK FREE   USED  WAITS SEEN",
            " > ! thelio     3/3   19.0/32  2/124 GB !  95/455 GB   79%   1     0s",
            "     pangolin   2/4   3.1/16   14/32 GB    90/200 GB   55%   0     12s",
            "     strata-ci  0/2   0.4/8    30/64 GB    140/500 GB  72%   0     28s",
            "",
        ]
        .map(String::from)
        .to_vec();
        want.push(ruled("thelio · riff daemon pid 4242, up 2d", 80));
        want.extend(
            [
                " cpu 32x5883MHz (now 4100MHz), mem 124GB, 2GB available, load 18.96",
                " limit 3 sessions and a floor of 8 GB · 1 start waits · 1 job",
                " at its limit: 3 of 3 sessions live, a new start waits for room",
                " low on memory: the daemon starts no session until 8 GB are free",
                " parts como-technologies/riff workers and lead",
                " wave como-technologies/riff: started issue-1594",
                " sessions: 5b1e2a90 works · 9c0d1e2f works · 3a3f8d5d ends",
                "",
            ]
            .map(String::from),
        );
        want.push(ruled("the link and the server", 80));
        want.extend(
            [
                " link ok: 118 ms · 1 reconnect · riff-server build 10df8a4, same as yours",
                " GitHub: 4,210 of 5,000 points left · riff spent 310, others 480, in the hour",
            ]
            .map(String::from),
        );
        want.resize(23, String::new());
        want.push(
            " ↑↓ select  Enter sessions of the host  l limit  r read again  ? help  q quit".into(),
        );
        assert_eq!(drawn(&state, 80, 24), want);
    }

    /// The narrow layout of the Machines tab at 40 x 24
    /// (01M6VAZKK4KHCDCX4RKG9B95D7, 01M6VFD0EXECEEVNN969SAPV4C): fewer
    /// columns, the hosts with a fault counted as one, no detail, and
    /// the build of riff-server on a line of its own.
    #[test]
    fn the_machines_tab_at_40_x_24_is_the_narrow_mock_up() {
        let state = mock_up();
        let mut want: Vec<String> = [
            "‹ 4/6 Machines ›",
            "riff running · link ok 2s",
            "3 hosts · 1 needs a look",
            "    HOST       WORK  LOAD    MEM GB",
            "> ! thelio     3/3   19.0/32 2/124 !",
            "    pangolin   2/4   3.1/16  14/32",
            "    strata-ci  0/2   0.4/8   30/64",
            "",
        ]
        .map(String::from)
        .to_vec();
        want.push(ruled("the link and the server", 40));
        want.extend(
            [
                "link ok: 118 ms · 1 reconnect",
                "riff-server 10df8a4, same as yours",
                "GitHub: 4,210 of 5,000 points left",
            ]
            .map(String::from),
        );
        want.resize(23, String::new());
        want.push("↑↓ select  l limit  ? help  q quit".into());
        assert_eq!(drawn(&state, 40, 24), want);
    }

    /// The state of [`mock_up`] after `riff leave --host pangolin` in
    /// como-technologies/riff (01M6WJYQN2WY5NH3DVDBAAY8ST): pangolin gives
    /// workers to n8behavior/sens-o-matic only, and its 3 sessions of
    /// como-technologies/riff go on to their end. Pangolin is selected.
    fn leaving() -> State {
        let mut state = mock_up();
        let (look, at) = state.last_look().unwrap();
        let mut look = look.clone();
        let sent = look
            .who
            .sessions
            .iter_mut()
            .find(|s| s.uri.place().host() == "pangolin")
            .unwrap();
        let mut facts = Facts::parse(&sent.machine.as_ref().unwrap().numbers).unwrap();
        facts
            .sessions
            .push(session("5c6d7e8f", "issue-1612", Phase::Ends));
        facts.parts = vec![workers("n8behavior/sens-o-matic")];
        facts.leaving = vec![Leaving {
            repo: "como-technologies/riff".into(),
            sessions: facts.sessions.iter().map(|s| s.name.clone()).collect(),
        }];
        *sent = daemon("mike", "pangolin", &facts, 12);
        state.looked(look, at);
        state.on_key(Key::Down);
        state
    }

    /// 01M6WJYQN2WY5NH3DVDBAAY8ST: at 80 x 24, a host that left a
    /// repository with 3 sessions shows `leaving riff: 3 to end` under its
    /// row. Its detail lists its parts, and the sessions that go on to
    /// their end. Leaving is no fault: the order of the rows and the count
    /// of the faults stay.
    #[test]
    fn a_host_that_leaves_a_repository_at_80_x_24_shows_its_sessions_to_end() {
        let rows = drawn(&leaving(), 80, 24);
        let want: Vec<String> = [
            " 3 hosts · 1 at its limit · 1 low on memory",
            "     HOST       WORK  LOAD     MEM FREE    DISK FREE   USED  WAITS SEEN",
            "   ! thelio     3/3   19.0/32  2/124 GB !  95/455 GB   79%   1     0s",
            " >   pangolin   3/4   3.1/16   14/32 GB    90/200 GB   55%   0     12s",
            "                leaving riff: 3 to end",
            "     strata-ci  0/2   0.4/8    30/64 GB    140/500 GB  72%   0     28s",
            "",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(rows[2..9], want);
        assert_eq!(rows[9], ruled("pangolin · riff daemon build d2ae080", 80));
        assert_eq!(
            rows[12..16],
            [
                " parts n8behavior/sens-o-matic workers",
                " leaving como-technologies/riff: 3 sessions go on to their end",
                "   issue-1610 works · issue-1611 works · issue-1612 ends",
                " sessions: 7a8b9c0d works · 1e2f3a4b works · 5c6d7e8f ends",
            ]
        );
        // A click on the mark selects the host of the row.
        let mut state = leaving();
        state.on_key(Key::First);
        let view = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
        assert_eq!(view.hit(20, 6), Some(Key::Row(1)));
        assert_eq!(view.hit(20, 7), Some(Key::Row(2)));
    }

    /// 01M6WJYQN2WY5NH3DVDBAAY8ST: at 40 x 24, the narrow layout shows the
    /// mark under the row too.
    #[test]
    fn a_host_that_leaves_a_repository_at_40_x_24_shows_the_mark() {
        let rows = drawn(&leaving(), 40, 24);
        assert_eq!(
            rows[2..9],
            [
                "3 hosts · 1 needs a look",
                "    HOST       WORK  LOAD    MEM GB",
                "  ! thelio     3/3   19.0/32 2/124 !",
                ">   pangolin   3/4   3.1/16  14/32",
                "               leaving riff: 3 to end",
                "    strata-ci  0/2   0.4/8   30/64",
                "",
            ]
        );
    }

    /// 01M6WJYQN2WY5NH3DVDBAAY8ST: when the last session ends, the host
    /// shows `left riff` until the next look. A host that gives workers to
    /// no repository shows `takes no work`, under the marks of the
    /// repositories that it left.
    #[test]
    fn a_host_that_left_shows_left_and_a_host_with_no_workers_takes_no_work() {
        let row = |facts: Facts| {
            let mut state = leaving();
            let (look, at) = state.last_look().unwrap();
            let mut look = look.clone();
            let sent = look
                .who
                .sessions
                .iter_mut()
                .find(|s| s.uri.place().host() == "pangolin")
                .unwrap();
            *sent = daemon("mike", "pangolin", &facts, 12);
            state.looked(look, at);
            drawn(&state, 80, 24)
        };
        let (look, _) = leaving()
            .last_look()
            .map(|(l, at)| (l.clone(), at))
            .unwrap();
        let facts = rows(&look)
            .into_iter()
            .find(|r| r.name.host == "pangolin")
            .unwrap()
            .facts;
        let mut left = facts.clone();
        left.leaving[0].sessions.clear();
        left.sessions.clear();
        let rows = row(left.clone());
        assert_eq!(rows[6], "                left riff");
        assert!(
            rows.contains(&" left como-technologies/riff".to_owned()),
            "{rows:#?}"
        );
        let none = Facts {
            parts: vec![],
            ..left
        };
        let rows = row(none);
        assert_eq!(
            rows[6..8],
            ["                left riff", "                takes no work"]
        );
        assert!(rows.contains(&" parts none".to_owned()), "{rows:#?}");
        assert!(rows[8].starts_with("     strata-ci "), "{rows:#?}");
    }

    /// 01M6WJYQN2WY5NH3DVDBAAY8ST, 01M6VFD0M3RHT488D0RQ84RVMC: when the
    /// hosts do not fit, the rows scroll by lines: the selected row and its
    /// marks show, and the first line shows the place of the rows.
    #[test]
    fn the_rows_scroll_by_lines_with_the_marks() {
        let row = |host: &str, marks: usize| {
            let leaving = (0..marks)
                .map(|i| Leaving {
                    repo: format!("o/r{i}"),
                    sessions: vec!["riff-issue-1-1a2b3c4d".into()],
                })
                .collect();
            let facts = Facts {
                parts: vec![workers("o/w")],
                leaving,
                ..Facts::default()
            };
            let name = Name {
                user: "mike".into(),
                host: host.into(),
            };
            Row::new(name, 0, facts, None)
        };
        let rows = [row("a", 0), row("b", 2), row("c", 0), row("d", 1)];
        // All fit in 7 lines.
        assert_eq!(window(&rows, 0, 7), (0..4, 7));
        // In 4 lines, from the first row: a, b and its 2 marks.
        assert_eq!(window(&rows, 0, 4), (0..2, 4));
        // d and its mark show at the end.
        assert_eq!(window(&rows, 3, 4), (2..4, 3));
        // c: b loses its place, and d loses its mark.
        assert_eq!(window(&rows, 2, 2), (2..4, 2));
        // A row that does not fit loses its marks.
        assert_eq!(window(&rows, 1, 2), (1..2, 2));
        assert_eq!(window(&rows, 1, 0), (1..1, 0));
    }

    /// 01M6VFD0CAS71DNBGE7K56JJ2H: the marks of the cells of a host over
    /// its limits, and of a host with a fault.
    #[test]
    fn a_number_over_its_limit_has_the_warning_style() {
        let state = mock_up();
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);
        render(&compute_view(&state, area, NOW), &mut buffer);
        let style = |x: u16, y: u16| buffer[(x, y)].fg;
        // The `!` of thelio is red; its work and its memory are yellow.
        assert_eq!(style(3, 4), ratatui::style::Color::Red);
        assert_eq!(style(16, 4), ratatui::style::Color::Yellow);
        assert_eq!(style(31, 4), ratatui::style::Color::Yellow);
        // Pangolin has none.
        assert_eq!(style(16, 5), ratatui::style::Color::Reset);
    }

    /// 01M6VFD09QN71SWMPP8YXCRAEK: with no daemon on this host, the row
    /// of this host comes from riff-server, and the line of GitHub says
    /// that no daemon counts the points.
    #[test]
    fn with_no_daemon_here_the_row_of_this_host_comes_from_riff_server() {
        let mut state = mock_up();
        let (look, at) = state.last_look().unwrap();
        let mut look = look.clone();
        look.here.as_mut().unwrap().daemon = None;
        state.looked(look, at);
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[2], " 3 hosts");
        assert_eq!(
            rows[6],
            "     thelio     0/3   0.5/32   100/124 GB  400/455 GB  12%   0     20s"
        );
        assert_eq!(rows[8], ruled("pangolin · riff daemon build d2ae080", 80));
        assert!(
            rows.contains(&" GitHub: no riff daemon here counts the points".to_owned()),
            "{rows:#?}"
        );
        assert!(drawn(&state, 40, 24).contains(&"GitHub: no riff daemon here".to_owned()));
    }

    /// 01M6VFD09QN71SWMPP8YXCRAEK: the row of this host keeps the facts
    /// that the socket does not tell: podman, the sign-ins and the image
    /// of each wave.
    #[test]
    fn the_row_of_this_host_keeps_the_sign_ins_that_its_daemon_sent() {
        let state = mock_up();
        let (look, _) = state.last_look().unwrap();
        let mut look = look.clone();
        let mut sent = fine(
            3,
            machine(32, 124, 100, 0.5),
            Disk {
                free_gb: 400,
                total_gb: 455,
            },
        );
        sent.gh = Sign::Off;
        sent.waves = vec![Wave {
            repo: "como-technologies/riff".into(),
            word: "waits".into(),
            image: Some(daemons::Image {
                digest: "0123456789ab".into(),
                here: true,
            }),
            waits: Default::default(),
        }];
        look.who.sessions[1] = daemon("mike", "thelio", &sent, 20);
        let found = rows(&look);
        let thelio = found.iter().find(|r| r.name.host == "thelio").unwrap();
        assert_eq!(thelio.age_secs, 0);
        assert_eq!(thelio.facts.gh, Sign::Off);
        assert_eq!(thelio.facts.podman.as_deref(), Some("5.2.1"));
        assert_eq!(thelio.facts.waves[0].word, "started issue-1594");
        assert!(thelio.facts.waves[0].image.is_some());
        assert_eq!(thelio.faults, [Fault::Limit, Fault::Memory, Fault::SignIn]);
        assert_eq!(Fault::SignIn.line(&thelio.facts), "needs a sign-in: gh off");
        // One row for thelio.
        assert_eq!(found.len(), 3);
    }

    /// 01M6VFD0CAS71DNBGE7K56JJ2H: the hosts with a fault come first,
    /// then the hosts by name, and the first line counts each fault. A
    /// host of two users shows the user.
    #[test]
    fn the_hosts_with_a_fault_come_first() {
        let mut state = State::new(build("10df8a4c0ffe"), "riff://mike@thelio".parse().unwrap());
        let mut look = Look::default();
        let ok = fine(
            2,
            machine(8, 64, 30, 0.4),
            Disk {
                free_gb: 140,
                total_gb: 500,
            },
        );
        let tight = Facts {
            disk: Some(Disk {
                free_gb: 10,
                total_gb: 500,
            }),
            ..ok.clone()
        };
        let silent = Facts {
            machine: None,
            ..ok.clone()
        };
        look.who.sessions = vec![
            daemon("mike", "a", &ok, 5),
            daemon("mike", "z", &tight, 5),
            daemon("mike", "m", &silent, 5),
            daemon("ann", "a", &tight, 5),
        ];
        let found = rows(&look);
        let order: Vec<(&str, &str)> = found
            .iter()
            .map(|r| (r.name.user.as_str(), r.name.host.as_str()))
            .collect();
        assert_eq!(
            order,
            [("ann", "a"), ("mike", "m"), ("mike", "z"), ("mike", "a")]
        );
        state.looked(look, NOW);
        state.on_key(Key::Open(Tab::Machines));
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[2], " 4 hosts · 2 tight on disk · 1 tells no numbers");
        assert!(rows[4].starts_with(" > ! ann@a      "), "{rows:#?}");
        assert!(
            rows[5].starts_with("   ! m          0/2   -        -           "),
            "{rows:#?}"
        );
        assert!(rows[7].starts_with("     mike@a     "), "{rows:#?}");
        assert!(
            rows.contains(&" tight on disk: 10 of 500 GB free".to_owned()),
            "{rows:#?}"
        );
        assert_eq!(drawn(&state, 40, 24)[2], "4 hosts · 3 need a look");
    }

    /// 01M6VFD0M3RHT488D0RQ84RVMC: the keys select a host, a click on a
    /// row selects it, and `Enter` opens the Sessions tab with the
    /// filter of the host.
    #[test]
    fn the_keys_and_a_click_select_a_host_and_enter_opens_its_sessions() {
        let mut state = mock_up();
        let host = |state: &State| {
            let (look, _) = state.last_look().unwrap();
            let rows = rows(look);
            rows[selected(state, &rows)].name.host.clone()
        };
        assert_eq!(host(&state), "thelio");
        state.on_key(Key::Down);
        assert_eq!(host(&state), "pangolin");
        assert!(drawn(&state, 80, 24)[5].starts_with(" > "));
        assert_eq!(
            drawn(&state, 80, 24)[8],
            ruled("pangolin · riff daemon build d2ae080", 80)
        );
        state.on_key(Key::Last);
        assert_eq!(host(&state), "strata-ci");
        state.on_key(Key::Down);
        assert_eq!(host(&state), "strata-ci");
        state.on_key(Key::PageUp);
        assert_eq!(host(&state), "thelio");
        state.on_key(Key::Up);
        assert_eq!(host(&state), "thelio");
        state.on_key(Key::PageDown);
        assert_eq!(host(&state), "strata-ci");
        state.on_key(Key::First);
        assert_eq!(host(&state), "thelio");
        // `j` `k` `G` `g` are the same as `↓` `↑` `End` `Home`.
        state.on_key(Key::Char('j'));
        assert_eq!(host(&state), "pangolin");
        state.on_key(Key::Char('k'));
        assert_eq!(host(&state), "thelio");
        state.on_key(Key::Char('G'));
        assert_eq!(host(&state), "strata-ci");
        state.on_key(Key::Char('g'));
        assert_eq!(host(&state), "thelio");
        // A click on the row of pangolin.
        let view = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
        assert_eq!(view.hit(30, 5), Some(Key::Row(1)));
        assert_eq!(view.hit(30, 3), None);
        state.on_click(&view, 30, 5, false);
        assert_eq!(host(&state), "pangolin");
        // A click on `Enter sessions of the host` of the key line.
        assert_eq!(view.hit(14, 23), Some(Key::Enter));
        assert_eq!(view.hit(42, 23), Some(Key::Char('l')));
        assert_eq!(state.on_key(Key::Enter), None);
        assert_eq!(state.tab(), Tab::Sessions);
        assert_eq!(state.choices().filter(), "host:pangolin");
        assert_eq!(
            state.shown_note(),
            Some("The filter of the Sessions tab: host:pangolin.")
        );
        // The selection stays after the jump.
        state.on_key(Key::Open(Tab::Machines));
        assert_eq!(host(&state), "pangolin");
    }

    /// 01M6VFD0M3RHT488D0RQ84RVMC: when the hosts do not fit, the rows
    /// follow the selection, and the first line shows the place.
    #[test]
    fn many_hosts_scroll_with_the_selection() {
        let mut state = State::new(build("10df8a4c0ffe"), "riff://mike@thelio".parse().unwrap());
        let mut look = Look::default();
        let ok = fine(
            2,
            machine(8, 64, 30, 0.4),
            Disk {
                free_gb: 140,
                total_gb: 500,
            },
        );
        look.who.sessions = (0..50)
            .map(|i| daemon("mike", &format!("host-{i:02}"), &ok, 5))
            .collect();
        state.looked(look, NOW);
        state.on_key(Key::Open(Tab::Machines));
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[2], " 50 hosts · 1-8 of 50");
        assert!(rows[4].starts_with(" >   host-00"), "{rows:#?}");
        assert!(rows[11].starts_with("     host-07"), "{rows:#?}");
        assert_eq!(rows[13], ruled("host-00 · riff daemon build d2ae080", 80));
        state.on_key(Key::Row(30));
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[2], " 50 hosts · 24-31 of 50");
        assert!(rows[11].starts_with(" >   host-30"), "{rows:#?}");
        let view = compute_view(&state, Rect::new(0, 0, 80, 24), NOW);
        assert_eq!(view.hit(5, 4), Some(Key::Row(23)));
        state.on_key(Key::Last);
        assert_eq!(drawn(&state, 80, 24)[2], " 50 hosts · 43-50 of 50");
        // The narrow layout has no detail: more hosts fit.
        assert_eq!(drawn(&state, 40, 24)[2], "50 hosts · 38-50 of 50");
    }

    /// 01M6VFD0PPXDNH3NKD3YE4W9SB: `l` on the host of another user sets
    /// nothing, and says why.
    #[test]
    fn only_the_user_of_a_host_sets_its_limit() {
        let mut state = mock_up();
        let (look, at) = state.last_look().unwrap();
        let mut look = look.clone();
        let ann = fine(
            2,
            machine(8, 64, 30, 0.4),
            Disk {
                free_gb: 140,
                total_gb: 500,
            },
        );
        look.who.sessions.push(daemon("ann", "heron", &ann, 5));
        state.looked(look, at);
        // Thelio, then heron.
        state.on_key(Key::Row(1));
        assert_eq!(state.on_key(Key::Char('l')), None);
        assert_eq!(state.asked(), None);
        assert_eq!(
            state.shown_note(),
            Some("Only ann sets the limit of heron.")
        );
        // The host of mike.
        state.on_key(Key::First);
        state.on_key(Key::Char('l'));
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[22], " The most live sessions of thelio (now 3): █");
        state.on_key(Key::Char('4'));
        state.on_key(Key::Enter);
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[22], " Set the limit of thelio to 4 sessions? y/N");
        // `n` sets nothing.
        assert_eq!(state.on_key(Key::Char('n')), None);
        assert_eq!(
            drawn(&state, 80, 24)[22],
            " The limit of thelio stays 3 sessions."
        );
        // An empty answer sets nothing.
        state.on_key(Key::Char('l'));
        state.on_key(Key::Char('x'));
        assert_eq!(
            state.asked().as_deref(),
            Some("The most live sessions of thelio (now 3): █")
        );
        state.on_key(Key::Enter);
        assert_eq!(state.asked(), None);
        // At most 4 digits, and 1 session is one.
        state.on_key(Key::Char('l'));
        for _ in 0..6 {
            state.on_key(Key::Char('1'));
        }
        assert_eq!(
            state.asked().as_deref(),
            Some("The most live sessions of thelio (now 3): 1111█")
        );
        state.on_key(Key::Backspace);
        state.on_key(Key::Backspace);
        state.on_key(Key::Backspace);
        state.on_key(Key::Enter);
        assert_eq!(
            state.asked().as_deref(),
            Some("Set the limit of thelio to 1 session? y/N")
        );
        assert_eq!(
            state.on_key(Key::Char('y')),
            Some(Action::Limit {
                host: "thelio".into(),
                sessions: 1
            })
        );
    }

    /// 01M6VFD0HGCVMYNNC3H4788W9X: the link down, another build of
    /// riff-server, and no reading of the points left.
    #[test]
    fn the_lines_of_the_link_show_a_fault_and_another_build() {
        let mut state = mock_up();
        let (look, _) = state.last_look().unwrap();
        let mut look = look.clone();
        look.server = Some(build("3f2a91c00000"));
        look.reconnects = 0;
        let status = look.here.as_mut().unwrap().daemon.as_mut().unwrap();
        status.github.left = None;
        status.github.other = None;
        state.looked(look, NOW - 41_000);
        state.failed("riff: no good look since 21:35:07: cannot reach riff-server");
        let rows = drawn(&state, 80, 24);
        assert_eq!(
            rows[18],
            " link down: no good look for 41s · 0 reconnects · riff-server build 3f2a91c, yours is 10df8a4"
                .chars()
                .take(80)
                .collect::<String>()
        );
        assert_eq!(rows[19], " GitHub: riff spent 310 in the hour");
        let narrow = drawn(&state, 40, 24);
        assert_eq!(narrow[10], "riff-server 3f2a91c, yours is 10df8a4");
    }

    /// With no host: the line that says how to add one, and the lines of
    /// the link. No key acts.
    #[test]
    fn with_no_host_the_tab_says_how_to_add_one() {
        let mut state = State::new(build("10df8a4c0ffe"), "riff://mike@thelio".parse().unwrap());
        state.looked(Look::default(), NOW);
        state.on_key(Key::Open(Tab::Machines));
        let rows = drawn(&state, 80, 24);
        assert_eq!(rows[2], format!(" {NO_HOSTS}"));
        assert_eq!(
            rows[3],
            " To add a host, run riff join in the main clone of the repository on that host."
        );
        assert_eq!(rows[5], ruled("the link and the server", 80));
        assert_eq!(state.on_key(Key::Char('l')), None);
        assert_eq!(state.on_key(Key::Enter), None);
        assert_eq!(state.tab(), Tab::Machines);
        assert_eq!(state.asked(), None);
    }

    /// The detail of a host loses its last lines first in a short
    /// terminal, and the help popup has the keys of the tab.
    #[test]
    fn a_short_terminal_cuts_the_detail_and_the_help_has_the_keys_of_the_tab() {
        let mut state = mock_up();
        let short = drawn(&state, 80, 16);
        assert_eq!(short[8], ruled("thelio · riff daemon pid 4242, up 2d", 80));
        assert_eq!(
            short[9],
            " cpu 32x5883MHz (now 4100MHz), mem 124GB, 2GB available, load 18.96"
        );
        assert_eq!(short[10], "");
        assert_eq!(short[11], ruled("the link and the server", 80));
        state.on_key(Key::Help);
        let text = drawn(&state, 80, 24).join("\n");
        assert!(text.contains("Keys of the Machines tab"), "{text}");
        for (key, does) in super::help_keys() {
            assert!(text.contains(key) && text.contains(does), "{key}: {text}");
        }
        // The popup takes no key of the tab.
        state.on_key(Key::Down);
        state.on_key(Key::Esc);
        let (look, _) = state.last_look().unwrap();
        assert_eq!(selected(&state, &rows(look)), 0);
    }
}
