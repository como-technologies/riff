//! The sessions model: the tree of `riff top` and of the Sessions tab of
//! the TUI, in one model (01M56Y8KAPHJMSK89ZW1PA8B0E,
//! 01M56Y8KD05JH2J7AMK636WGT7).
//!
//! # Design
//!
//! [`Sessions`] holds two kinds of session, as plain data with serde:
//!
//! - a session of riff-server: a [`SessionInfo`] of `who`;
//! - a container session of the riff daemon of this machine: a [`Live`]
//!   of its status. The socket of the daemon stays in the UI: a terminal
//!   UI gives the container sessions of its daemon to [`Sessions::new`]
//!   (01M54216S51X5VBXN3YAERW6J6).
//!
//! A container session that is also a session of riff-server, with the
//! same session ID, is one [`Entry`] with both: the facts of the server
//! and the state of its container ([`Entry::phase`]).
//!
//! ```mermaid
//! flowchart LR
//!     S["riff-server: who"] -- "SessionInfo" --> M["Sessions"]
//!     D["the riff daemon of this machine:<br/>its status, through the UI"] -- "Live" --> M
//!     M -- "Sessions::on_server, Sessions::tree" --> T["riff top: the tree"]
//!     M -- "Sessions::entries" --> V["the Sessions tab of the TUI: the tree"]
//! ```
//!
//! `riff top` gives the sessions of riff-server and no container
//! session: it asks no riff daemon (01M53NGW6F5HP1DZ9QRGP50MGT).
//! The Sessions tab of the TUI gives both: the sessions of riff-server
//! and the container sessions of the daemon of its machine. It draws each
//! entry.
//!
//! The model gives two shapes of the tree. The tree of `riff top` groups
//! the sessions by project (01M77F8JJ8WC0TSCYP3ARJNQGS, [`By::Project`]):
//! project, person, host, session, and last the row [`NO_PROJECT`] with
//! the sessions in no project: person, host, repository, session.
//! [`By::Person`] puts the person first: person, host, repository,
//! session (01M3NT4M5D36KTZ5XZMDP6QFQT, 01M42KHN33M4K13GKTX2WM6CMM,
//! [`By::top_levels`]). [`Show`] picks a part of the riff
//! (01M42KHNCBMBCT3TFBYWE339H5). [`Sessions::tree`] makes the rows, and
//! [`Sessions::people`] the person of each row. The tree of the Sessions
//! tab has three levels: repository, person, session, with the host as a
//! column of each session row ([`By::tab_levels`]). [`By::Person`] puts
//! the person first there.
//!
//! ```mermaid
//! flowchart TD
//!     W["who: the sessions and the projects"] --> T{"Sessions::tree"}
//!     T -- "a project" --> P["o/r › person › host › session"]
//!     T -- "no project" --> N["no project › person › host › repository › session"]
//! ```
//!
//! ```
//! use riff_client::sessions::{Live, Phase, Sessions};
//! use riff_core::wire::SessionInfo;
//!
//! let info = |uri: &str| -> SessionInfo {
//!     serde_json::from_value(serde_json::json!({ "uri": uri, "live": true })).unwrap()
//! };
//! let live = |name: &str, session: &str| Live {
//!     name: name.into(),
//!     session: session.into(),
//!     item: "issue-12".into(),
//!     image: "riff-session".into(),
//!     since_ms: 0,
//!     digest: String::new(),
//!     phase: Phase::Works,
//!     last: String::new(),
//!     lead: false,
//!     cause: None,
//! };
//! let sessions = Sessions::new(
//!     vec![
//!         info("riff://mike@thelio/o/r?session=aaaa1111&lead=true"),
//!         info("riff://mike@thelio/o/r?session=bbbb2222&claim=issue-12"),
//!     ],
//!     vec![live("riff-issue-12-bbbb2222", "bbbb2222"), live("riff-issue-13-cccc3333", "cccc3333")],
//! );
//! // A session of the server, one on the server and in a container, and
//! // a container only.
//! assert_eq!(sessions.entries.len(), 3);
//! assert_eq!(sessions.entries[1].phase(), Some(Phase::Works));
//! assert_eq!(sessions.on_server().count(), 2);
//! assert_eq!(sessions.containers().count(), 2);
//! ```

use std::collections::BTreeMap;

use riff_core::build::Build;
use riff_core::name::ThreadName;
use riff_core::wire::{Person, PersonRole, SessionInfo, SessionState};
use serde::{Deserialize, Serialize};

/// The key of the row of the sessions in no project, in the tree of
/// `riff top` and in the table of `riff who` (01M77F8JJ8WC0TSCYP3ARJNQGS,
/// 01M77F8JN1SE2H4DZBZH1YX7WD). No `OWNER/REPO` has a space.
pub const NO_PROJECT: &str = "no project";

/// The project of the session `s`: its repository, `OWNER/REPO`, when
/// `projects` has it. `None` for a repository that is no project, and
/// outside git.
///
/// ```
/// use riff_client::sessions::project;
/// use riff_core::wire::SessionInfo;
///
/// let info = |uri: &str| -> SessionInfo {
///     serde_json::from_value(serde_json::json!({ "uri": uri, "live": true })).unwrap()
/// };
/// let projects = ["o/r".parse()?];
/// assert_eq!(project(&info("riff://mike@thelio/o/r?session=a1"), &projects).as_deref(), Some("o/r"));
/// assert_eq!(project(&info("riff://mike@thelio/local/theme?session=b2"), &projects), None);
/// assert_eq!(project(&info("riff://mike@thelio?session=c3"), &projects), None);
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn project(s: &SessionInfo, projects: &[ThreadName]) -> Option<String> {
    s.uri
        .place()
        .default_thread()
        .filter(|thread| projects.contains(thread))
        .map(|thread| thread.to_string())
}

/// What a container session does now, as `riff daemon status` shows it
/// (01M501PADKZNRHBZD9XPMJGW06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// A turn of `claude` runs.
    #[default]
    Works,
    /// After the result of the turn: `claude` ends, or waits for its
    /// background tasks.
    Ends,
    /// A takeover waits for the end of the turn.
    Holds,
    /// `claude` ended for a takeover: a person has the session.
    Taken,
    /// A session with a terminal: the lead.
    Terminal,
}

/// A container session that the riff daemon of a machine owns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Live {
    /// The name of its container.
    pub name: String,
    /// The riff session ID.
    pub session: String,
    /// The work item.
    pub item: String,
    /// The image.
    pub image: String,
    /// The short digest of the image that the container runs, from podman,
    /// or empty before the daemon asks podman.
    #[serde(default)]
    pub digest: String,
    /// The start, in milliseconds since the epoch.
    pub since_ms: i64,
    /// What the session does now.
    #[serde(default)]
    pub phase: Phase,
    /// The last step of the session in a few words: the last line that
    /// `riff run` shows of its events.
    #[serde(default)]
    pub last: String,
    /// True for the lead: the session with a terminal. It does not keep the
    /// daemon on an old riff (an old daemon gives false).
    #[serde(default)]
    pub lead: bool,
    /// What started the lead (#2099). `None` for a session that is not
    /// the lead, and for a lead of an old daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<LeadCause>,
}

/// What started a lead (#2099, 01M7BNP4ZYJCCEK7ZHCJ3EX3SJ): the line of the
/// start in the log of the riff daemon and the line of the lead in `riff
/// daemon status` name it.
///
/// ```
/// use riff_client::sessions::LeadCause;
///
/// let by = "riff://mike@pangolin/o/r".parse()?;
/// let cause = LeadCause::Command { command: "riff lead move".into(), by: Box::new(by) };
/// let json = serde_json::to_value(&cause)?;
/// assert_eq!(json["cause"], "command");
/// assert_eq!(serde_json::from_value::<LeadCause>(json)?, cause);
/// let loop_cause = serde_json::to_value(LeadCause::LeadLoop)?;
/// assert_eq!(loop_cause, serde_json::json!({ "cause": "lead_loop" }));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cause", rename_all = "snake_case")]
pub enum LeadCause {
    /// The start of the daemon: the lead had no container, for example
    /// after a reboot.
    DaemonStart,
    /// The lead loop of the daemon: no host held the claim lead.
    LeadLoop,
    /// A start again after a fault: the container of the lead ended by
    /// itself, for example with `/exit` or a crash.
    Fault,
    /// An update of riff: the lead starts again on the new riff.
    Update,
    /// A command of a person.
    Command {
        /// The command, for example `riff lead move`.
        command: String,
        /// Who ran it: the person, or the lead of the person.
        by: Box<riff_core::name::SessionUri>,
    },
}

/// One session of the model: a session of riff-server, a container
/// session, or both with one session ID (01M56Y8KD05JH2J7AMK636WGT7).
/// Each entry has at least one of the two.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The session on riff-server.
    pub server: Option<SessionInfo>,
    /// The container session of the riff daemon of this machine.
    pub container: Option<Live>,
}

impl Entry {
    /// The session ID: of the server session, else of the container.
    pub fn id(&self) -> Option<&str> {
        match (&self.server, &self.container) {
            (Some(s), _) => s.uri.who().session(),
            (None, Some(c)) => Some(&c.session),
            (None, None) => None,
        }
    }

    /// The state of the container, for a session in a container.
    pub fn phase(&self) -> Option<Phase> {
        self.container.as_ref().map(|c| c.phase)
    }
}

/// The sessions model: the sessions of riff-server and the container
/// sessions of the riff daemon of this machine, in one list
/// (01M5421633BKPPMANYRVZ76BVP, 01M56Y8KD05JH2J7AMK636WGT7).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sessions {
    /// Each session: the sessions of the server in the order of `who`,
    /// then each container session that is not on the server, in the
    /// order of the daemon.
    pub entries: Vec<Entry>,
}

impl Sessions {
    /// The model of `server`, the sessions of `who`, and `containers`,
    /// the container sessions of the daemon. A container session with
    /// the session ID of a server session joins its entry.
    pub fn new(server: Vec<SessionInfo>, containers: Vec<Live>) -> Sessions {
        let mut entries: Vec<Entry> = server
            .into_iter()
            .map(|s| Entry {
                server: Some(s),
                container: None,
            })
            .collect();
        for live in containers {
            let on_server = entries.iter_mut().find(|e| {
                e.container.is_none()
                    && e.server.as_ref().and_then(|s| s.uri.who().session())
                        == Some(live.session.as_str())
            });
            match on_server {
                Some(entry) => entry.container = Some(live),
                None => entries.push(Entry {
                    server: None,
                    container: Some(live),
                }),
            }
        }
        Sessions { entries }
    }

    /// Each session on riff-server, in the order of `who`.
    pub fn on_server(&self) -> impl Iterator<Item = &SessionInfo> {
        self.entries.iter().filter_map(|e| e.server.as_ref())
    }

    /// Each container session.
    pub fn containers(&self) -> impl Iterator<Item = &Live> {
        self.entries.iter().filter_map(|e| e.container.as_ref())
    }

    /// Each person by USER: each member of `members`, the members of
    /// `who`, and each user with a session on the server. A user that
    /// `members` does not list, as in a riff with no sign-in, gets the
    /// time from its sessions, and the role of a member.
    ///
    /// ```
    /// use riff_client::sessions::Sessions;
    /// use riff_core::wire::{Person, PersonRole, SessionInfo};
    ///
    /// let s: SessionInfo = serde_json::from_value(serde_json::json!({
    ///     "uri": "riff://mike@thelio/o/r?session=a1", "live": false, "idle_secs": 300,
    /// }))?;
    /// let sessions = Sessions::new(vec![s], vec![]);
    /// let ann = Person { user: "ann".into(), role: PersonRole::Admin, live: true, seen_secs: Some(0) };
    /// let people = sessions.people(&[ann]);
    /// assert_eq!(people["ann"].role, PersonRole::Admin);
    /// assert_eq!((people["mike"].role, people["mike"].seen_secs), (PersonRole::Member, Some(300)));
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    pub fn people(&self, members: &[Person]) -> BTreeMap<String, PersonRow> {
        let mut people: BTreeMap<String, PersonRow> = members
            .iter()
            .map(|p| {
                let row = PersonRow {
                    role: p.role,
                    live: p.live,
                    seen_secs: p.seen_secs,
                };
                (p.user.clone(), row)
            })
            .collect();
        for s in self.on_server() {
            let user = s.uri.who().user();
            if members.iter().any(|p| p.user == user) {
                continue;
            }
            let row = people.entry(user.to_owned()).or_insert(PersonRow {
                role: PersonRole::Member,
                live: false,
                seen_secs: None,
            });
            row.live |= s.live;
            row.seen_secs = Some(row.seen_secs.map_or(s.idle_secs, |t| t.min(s.idle_secs)));
        }
        people
    }

    /// The rows of the tree of `riff top`: the sessions on the server
    /// that `show` shows and that have a session ID, by the levels of
    /// [`Show::by`]. With [`By::Person`], each person of `people` that
    /// `show` shows gets a row, also with no session.
    ///
    /// With [`By::Project`] (01M77F8JJ8WC0TSCYP3ARJNQGS), each project of
    /// `projects` with a session gets a row at the top, a
    /// [`Level::Repo`] row with its `OWNER/REPO`. The sessions in no
    /// project come last, in the row [`NO_PROJECT`], by the levels of
    /// [`By::Person`]. Then each person of `people` that `show` shows and
    /// that has no session gets a row.
    ///
    /// ```
    /// use riff_client::sessions::{By, Level, NO_PROJECT, Sessions, Show};
    /// use riff_core::wire::SessionInfo;
    ///
    /// let info = |uri: &str| -> SessionInfo {
    ///     serde_json::from_value(serde_json::json!({ "uri": uri, "live": true })).unwrap()
    /// };
    /// let sessions = Sessions::new(
    ///     vec![
    ///         info("riff://mike@thelio/o/r?session=b2"),
    ///         info("riff://mike@pangolin/o/r?session=a1"),
    ///         info("riff://ann@heron/o/s?session=c3"),
    ///         // A person on the command line has no row.
    ///         info("riff://ann@heron"),
    ///     ],
    ///     vec![],
    /// );
    /// let people = sessions.people(&[]);
    /// let projects = ["o/r".parse()?];
    /// let tree = sessions.tree(&Show::default(), &people, &projects);
    /// let keys: Vec<&str> = tree.iter().map(|g| g.key.as_str()).collect();
    /// assert_eq!(keys, ["ann", "mike"]);
    /// let hosts: Vec<&str> = tree[1].kids.iter().map(|g| g.key.as_str()).collect();
    /// assert_eq!(hosts, ["pangolin", "thelio"]);
    /// assert_eq!(tree[1].kids[0].kids[0].level, Level::Repo);
    ///
    /// let by_repo = Show { by: By::Repo, ..Show::default() };
    /// let tree = sessions.tree(&by_repo, &people, &projects);
    /// assert_eq!(tree.iter().map(|g| g.key.as_str()).collect::<Vec<_>>(), ["o/r", "o/s"]);
    ///
    /// // By project: o/s is no project.
    /// let by_project = Show { by: By::Project, ..Show::default() };
    /// let tree = sessions.tree(&by_project, &people, &projects);
    /// assert_eq!(tree.iter().map(|g| g.key.as_str()).collect::<Vec<_>>(), ["o/r", NO_PROJECT]);
    /// assert_eq!(tree[0].kids[0].key, "mike");
    /// let repo = &tree[1].kids[0].kids[0].kids[0];
    /// assert_eq!((repo.level, repo.key.as_str()), (Level::Repo, "o/s"));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn tree(
        &self,
        show: &Show,
        people: &BTreeMap<String, PersonRow>,
        projects: &[ThreadName],
    ) -> Vec<Group<'_>> {
        // A person on the command line has no session, and no row.
        let agents: Vec<&SessionInfo> = self
            .on_server()
            .filter(|s| show.shows(s))
            .filter(|s| s.uri.who().session().is_some())
            .collect();
        let levels = show.by.top_levels();
        let away = |user: &String| Group {
            level: Level::Person,
            key: user.clone(),
            sessions: Vec::new(),
            kids: Vec::new(),
        };
        match show.by {
            By::Person => {
                let mut tops = groups(&agents, &levels);
                for user in people.keys().filter(|u| show.shows_person(u)) {
                    if !tops.iter().any(|g| &g.key == user) {
                        tops.push(away(user));
                    }
                }
                tops.sort_by(|a, b| a.key.cmp(&b.key));
                tops
            }
            By::Repo => groups(&agents, &levels),
            By::Project => {
                let (inside, mut outside): (Vec<&SessionInfo>, Vec<&SessionInfo>) =
                    agents.iter().partition(|s| project(s, projects).is_some());
                let mut tops = groups(&inside, &levels);
                if !outside.is_empty() {
                    in_order(&mut outside);
                    tops.push(Group {
                        level: Level::Repo,
                        key: NO_PROJECT.to_owned(),
                        kids: groups(&outside, &By::Person.top_levels()),
                        sessions: outside,
                    });
                }
                for user in people.keys().filter(|u| show.shows_person(u)) {
                    if !agents.iter().any(|s| s.uri.who().user() == user) {
                        tops.push(away(user));
                    }
                }
                tops
            }
        }
    }
}

/// The state of `s`. riff fills the state before it shows a session
/// ([`SessionInfo::fill_state`]); a session with none is `offline`.
pub fn state(s: &SessionInfo) -> SessionState {
    s.state.unwrap_or_default()
}

/// True when the state of `s` is `blocked`.
pub fn blocked(s: &SessionInfo) -> bool {
    state(s) == SessionState::Blocked
}

/// True when the session `s` runs an old riff: the file of its riff was
/// replaced, or its build is older than `server`, the build of the
/// server (01M514C142K5BM2746F1D9Y448). A session whose keep-alive named
/// no riff, as from an older riff, is not out of date.
///
/// ```
/// use riff_core::build::Build;
/// use riff_core::wire::{RiffBinary, SessionInfo};
///
/// let json = r#"{"uri":"riff://mike@thelio/o/r?session=w1","live":true}"#;
/// let mut s: SessionInfo = serde_json::from_str(json).unwrap();
/// let server: Build = "1.4.0 122daab96637 2026-10-15T23:08:34Z".parse().unwrap();
/// assert!(!riff_client::sessions::out_of_date(&s, Some(&server)));
/// s.binary = Some(RiffBinary { build: "1.3.1 aaaa 2026-10-14T10:00:00Z".into(), replaced: false });
/// assert!(riff_client::sessions::out_of_date(&s, Some(&server)));
/// s.binary = Some(RiffBinary { build: server.to_string(), replaced: false });
/// assert!(!riff_client::sessions::out_of_date(&s, Some(&server)));
/// s.binary = Some(RiffBinary { build: server.to_string(), replaced: true });
/// assert!(riff_client::sessions::out_of_date(&s, Some(&server)));
/// ```
pub fn out_of_date(s: &SessionInfo, server: Option<&Build>) -> bool {
    s.binary.as_ref().is_some_and(|b| b.out_of_date(server))
}

/// The top level of a tree: of `riff top` (01M42KHNCBMBCT3TFBYWE339H5,
/// 01M77F8JJ8WC0TSCYP3ARJNQGS) and of the Sessions tab of the TUI. Each
/// has its own shape of the levels: [`By::top_levels`] and
/// [`By::tab_levels`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum By {
    /// The person first.
    #[default]
    Person,
    /// The repository first: a look across people.
    Repo,
    /// The project first, and last the sessions in no project
    /// ([`Sessions::tree`]): the default of `riff top`.
    Project,
}

impl By {
    /// The levels of the tree of `riff top` above the sessions: person,
    /// host, repository, or repository, person, host. The row of a
    /// project is a row of its repository.
    ///
    /// ```
    /// use riff_client::sessions::{By, Level};
    ///
    /// assert_eq!(By::Person.top_levels(), [Level::Person, Level::Host, Level::Repo]);
    /// assert_eq!(By::Repo.top_levels(), [Level::Repo, Level::Person, Level::Host]);
    /// assert_eq!(By::Project.top_levels(), By::Repo.top_levels());
    /// ```
    pub fn top_levels(self) -> [Level; 3] {
        match self {
            By::Person => [Level::Person, Level::Host, Level::Repo],
            By::Repo | By::Project => [Level::Repo, Level::Person, Level::Host],
        }
    }

    /// The levels of the tree of the Sessions tab above the sessions:
    /// repository, person, or person, repository. The host is a column
    /// of each session row, and no level. The tab has no row of a
    /// project: [`By::Project`] puts the repository first there.
    ///
    /// ```
    /// use riff_client::sessions::{By, Level};
    ///
    /// assert_eq!(By::Repo.tab_levels(), [Level::Repo, Level::Person]);
    /// assert_eq!(By::Person.tab_levels(), [Level::Person, Level::Repo]);
    /// ```
    pub fn tab_levels(self) -> [Level; 2] {
        match self {
            By::Person => [Level::Person, Level::Repo],
            By::Repo | By::Project => [Level::Repo, Level::Person],
        }
    }
}

/// The part of the riff that a view shows: `--user`, `--host`, `--repo`
/// and `--by` of `riff top` (01M42KHNCBMBCT3TFBYWE339H5). Each filter
/// that is set must match.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Show {
    pub user: Option<String>,
    pub host: Option<String>,
    /// `OWNER/REPO`.
    pub repo: Option<String>,
    pub by: By,
}

impl Show {
    /// True when `s` matches each filter that is set.
    pub fn shows(&self, s: &SessionInfo) -> bool {
        let place = s.uri.place();
        self.user.as_ref().is_none_or(|u| s.uri.who().user() == u)
            && self.host.as_ref().is_none_or(|h| place.host() == h)
            && self.repo.as_ref().is_none_or(|r| &place.repo_text() == r)
    }

    /// True when a person with no session can show: no filter of a
    /// host or a repository is set, and the user matches.
    pub fn shows_person(&self, user: &str) -> bool {
        self.host.is_none() && self.repo.is_none() && self.user.as_ref().is_none_or(|u| u == user)
    }
}

/// One person of the tree: the role, and live or the time since the
/// last call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonRow {
    pub role: PersonRole,
    pub live: bool,
    pub seen_secs: Option<u64>,
}

/// A level of the tree above the sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Person,
    Host,
    Repo,
}

impl Level {
    /// The key of `s` at this level: the user, the host, or
    /// `OWNER/REPO`.
    pub fn key(self, s: &SessionInfo) -> String {
        match self {
            Level::Person => s.uri.who().user().to_owned(),
            Level::Host => s.uri.place().host().to_owned(),
            Level::Repo => s.uri.place().repo_text(),
        }
    }
}

/// One row of the tree above the sessions: a person, a host or a
/// repository, each session under it, and the rows of the next level.
/// The rows of the last level have no rows under them, only sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group<'a> {
    pub level: Level,
    pub key: String,
    pub sessions: Vec<&'a SessionInfo>,
    pub kids: Vec<Group<'a>>,
}

/// The rows of `sessions` at the first of `levels`, by key, each with
/// the rows of the next levels. In each row, blocked sessions come
/// first, then each session by its ID.
pub fn groups<'a>(sessions: &[&'a SessionInfo], levels: &[Level]) -> Vec<Group<'a>> {
    let Some((&level, rest)) = levels.split_first() else {
        return Vec::new();
    };
    let mut by: BTreeMap<String, Vec<&SessionInfo>> = BTreeMap::new();
    for s in sessions {
        by.entry(level.key(s)).or_default().push(s);
    }
    by.into_iter()
        .map(|(key, mut sessions)| {
            in_order(&mut sessions);
            Group {
                level,
                key,
                kids: groups(&sessions, rest),
                sessions,
            }
        })
        .collect()
}

/// Puts the sessions of a row in order: the blocked sessions first, then
/// each session by its ID.
fn in_order(sessions: &mut [&SessionInfo]) {
    sessions.sort_by_key(|s| (!blocked(s), s.uri.who().session().map(str::to_owned)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(uri: &str) -> SessionInfo {
        serde_json::from_value(serde_json::json!({ "uri": uri, "live": true })).unwrap()
    }

    fn live(name: &str, session: &str, phase: Phase) -> Live {
        Live {
            digest: String::new(),
            name: name.into(),
            session: session.into(),
            item: "issue-12".into(),
            image: "riff-session".into(),
            since_ms: 0,
            phase,
            lead: false,
            cause: None,
            last: String::new(),
        }
    }

    /// The sample reply of `who` of riff-server, as JSON.
    const WHO: &str = r#"[
        {"uri": "riff://mike@thelio/o/r?session=aaaa1111&lead=true", "live": true, "state": "idle"},
        {"uri": "riff://mike@thelio/o/r?session=bbbb2222&claim=issue-12", "live": true,
         "container": true, "state": "busy"}
    ]"#;

    #[test]
    fn the_model_comes_from_a_reply_of_who() {
        let who: Vec<SessionInfo> = serde_json::from_str(WHO).unwrap();
        let sessions = Sessions::new(who.clone(), vec![]);
        assert_eq!(sessions.on_server().cloned().collect::<Vec<_>>(), who);
        assert_eq!(sessions.containers().count(), 0);
        assert_eq!(sessions.entries[0].id(), Some("aaaa1111"));
    }

    #[test]
    fn the_model_comes_from_the_container_sessions_of_the_daemon() {
        let containers = vec![
            live("riff-lead-o-r", "1a2b", Phase::Terminal),
            live("riff-issue-12-2b3c", "2b3c", Phase::Works),
        ];
        let sessions = Sessions::new(vec![], containers.clone());
        assert_eq!(
            sessions.containers().cloned().collect::<Vec<_>>(),
            containers
        );
        assert_eq!(sessions.on_server().count(), 0);
        assert_eq!(sessions.entries[1].id(), Some("2b3c"));
    }

    #[test]
    fn a_container_session_on_the_server_is_one_entry_with_the_state_of_its_container() {
        let who: Vec<SessionInfo> = serde_json::from_str(WHO).unwrap();
        let sessions = Sessions::new(
            who,
            vec![
                live("riff-issue-12-bbbb2222", "bbbb2222", Phase::Holds),
                live("riff-issue-13-cccc3333", "cccc3333", Phase::Works),
            ],
        );
        let ids: Vec<Option<&str>> = sessions.entries.iter().map(Entry::id).collect();
        assert_eq!(ids, [Some("aaaa1111"), Some("bbbb2222"), Some("cccc3333")]);
        let both = &sessions.entries[1];
        assert!(both.server.as_ref().is_some_and(|s| s.container));
        assert_eq!(both.phase(), Some(Phase::Holds));
        assert_eq!(sessions.entries[0].phase(), None);
        assert!(sessions.entries[2].server.is_none());
    }

    #[test]
    fn the_model_is_plain_data_with_serde() {
        let sessions = Sessions::new(
            vec![info("riff://mike@thelio/o/r?session=a1")],
            vec![live("riff-issue-12-a1", "a1", Phase::Ends)],
        );
        let json = serde_json::to_string(&sessions).unwrap();
        assert!(json.contains(r#""phase":"ends""#), "{json}");
        let back: Sessions = serde_json::from_str(&json).unwrap();
        assert_eq!(back, sessions);
    }

    #[test]
    fn blocked_sessions_come_first_in_a_row() {
        let mut blocked = info("riff://mike@thelio/o/r?session=zz");
        blocked.state = Some(SessionState::Blocked);
        let sessions = Sessions::new(
            vec![info("riff://mike@thelio/o/r?session=aa"), blocked],
            vec![],
        );
        let people = sessions.people(&[]);
        let tree = sessions.tree(&Show::default(), &people, &[]);
        let ids: Vec<&str> = tree[0]
            .sessions
            .iter()
            .filter_map(|s| s.uri.who().session())
            .collect();
        assert_eq!(ids, ["zz", "aa"]);
    }

    /// 01M77F8JJ8WC0TSCYP3ARJNQGS: by project, each project with a
    /// session in the order of `OWNER/REPO`, then the sessions in no
    /// project, also outside git, then each member with no session.
    #[test]
    fn the_tree_by_project_puts_the_sessions_in_no_project_last() {
        let mut blocked = info("riff://mike@thelio/local/theme?session=zz");
        blocked.state = Some(SessionState::Blocked);
        let sessions = Sessions::new(
            vec![
                info("riff://mike@thelio/o/s?session=s1"),
                info("riff://ann@heron/o/r?session=r1"),
                info("riff://mike@thelio?session=aa"),
                blocked,
            ],
            vec![],
        );
        let bob = Person {
            user: "bob".into(),
            role: PersonRole::Member,
            live: false,
            seen_secs: None,
        };
        let people = sessions.people(&[bob]);
        let projects = ["o/s".parse().unwrap(), "o/r".parse().unwrap()];
        let by = Show {
            by: By::Project,
            ..Show::default()
        };
        let tree = sessions.tree(&by, &people, &projects);
        let tops: Vec<(Level, &str)> = tree.iter().map(|g| (g.level, g.key.as_str())).collect();
        assert_eq!(
            tops,
            [
                (Level::Repo, "o/r"),
                (Level::Repo, "o/s"),
                (Level::Repo, NO_PROJECT),
                (Level::Person, "bob"),
            ]
        );
        let none = &tree[2];
        let ids: Vec<&str> = none
            .sessions
            .iter()
            .filter_map(|s| s.uri.who().session())
            .collect();
        assert_eq!(ids, ["zz", "aa"], "blocked first");
        let repos: Vec<&str> = none.kids[0].kids[0]
            .kids
            .iter()
            .map(|g| g.key.as_str())
            .collect();
        assert_eq!(repos, ["-", "local/theme"]);
        // A filter of a host shows no member with no session.
        let thelio = Show {
            host: Some("thelio".into()),
            ..by
        };
        let keys: Vec<String> = sessions
            .tree(&thelio, &people, &projects)
            .into_iter()
            .map(|g| g.key)
            .collect();
        assert_eq!(keys, ["o/s", NO_PROJECT]);
    }
}
