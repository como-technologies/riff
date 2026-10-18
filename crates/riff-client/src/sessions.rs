//! The sessions model: the tree of `riff top` and the rows of
//! `riff view`, in one model (01M56Y8KAPHJMSK89ZW1PA8B0E,
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
//!     M -- "Sessions::containers" --> V["riff view: the rows"]
//! ```
//!
//! `riff top` gives the sessions of riff-server and no container
//! session: it asks no riff daemon (01M53NGW6F5HP1DZ9QRGP50MGT).
//! `riff view` gives the container sessions of its daemon and no
//! session of riff-server. Each draws its part of the model: `riff top`
//! the sessions on the server, `riff view` the containers.
//!
//! The tree of `riff top` has four levels: person, host, repository,
//! session (01M3NT4M5D36KTZ5XZMDP6QFQT, 01M42KHN33M4K13GKTX2WM6CMM).
//! [`By::Repo`] puts the repository first. [`Show`] picks a part of the
//! riff (01M42KHNCBMBCT3TFBYWE339H5). [`Sessions::tree`] makes the rows,
//! and [`Sessions::people`] the person of each row.
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
//!     phase: Phase::Works,
//!     last: String::new(),
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
use riff_core::wire::{Person, PersonRole, RiffOwner, SessionInfo, SessionState};
use serde::{Deserialize, Serialize};

/// What a container session does now, as `riff view` shows it
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
    /// The start, in milliseconds since the epoch.
    pub since_ms: i64,
    /// What the session does now.
    #[serde(default)]
    pub phase: Phase,
    /// The last step of the session in a few words: the last line that
    /// `riff run` shows of its events.
    #[serde(default)]
    pub last: String,
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
    /// time from its sessions, and the role `owner` when `owner` names
    /// it.
    ///
    /// ```
    /// use riff_client::sessions::Sessions;
    /// use riff_core::wire::{Person, PersonRole, RiffOwner, SessionInfo};
    ///
    /// let s: SessionInfo = serde_json::from_value(serde_json::json!({
    ///     "uri": "riff://mike@thelio/o/r?session=a1", "live": false, "idle_secs": 300,
    /// }))?;
    /// let sessions = Sessions::new(vec![s], vec![]);
    /// let ann = Person { user: "ann".into(), role: PersonRole::Admin, live: true, seen_secs: Some(0) };
    /// let owner = RiffOwner::Owner { user: "mike".into(), email: "m@x.io".into() };
    /// let people = sessions.people(&[ann], &owner);
    /// assert_eq!(people["ann"].role, PersonRole::Admin);
    /// assert_eq!((people["mike"].role, people["mike"].seen_secs), (PersonRole::Owner, Some(300)));
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    pub fn people(&self, members: &[Person], owner: &RiffOwner) -> BTreeMap<String, PersonRow> {
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
                role: if owner.is(user) {
                    PersonRole::Owner
                } else {
                    PersonRole::Member
                },
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
    /// ```
    /// use riff_client::sessions::{By, Level, Sessions, Show};
    /// use riff_core::wire::{RiffOwner, SessionInfo};
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
    /// let people = sessions.people(&[], &RiffOwner::NoSignIn);
    /// let tree = sessions.tree(&Show::default(), &people);
    /// let keys: Vec<&str> = tree.iter().map(|g| g.key.as_str()).collect();
    /// assert_eq!(keys, ["ann", "mike"]);
    /// let hosts: Vec<&str> = tree[1].kids.iter().map(|g| g.key.as_str()).collect();
    /// assert_eq!(hosts, ["pangolin", "thelio"]);
    /// assert_eq!(tree[1].kids[0].kids[0].level, Level::Repo);
    ///
    /// let by_repo = Show { by: By::Repo, ..Show::default() };
    /// let tree = sessions.tree(&by_repo, &people);
    /// assert_eq!(tree.iter().map(|g| g.key.as_str()).collect::<Vec<_>>(), ["o/r", "o/s"]);
    /// ```
    pub fn tree(&self, show: &Show, people: &BTreeMap<String, PersonRow>) -> Vec<Group<'_>> {
        // A person on the command line has no session, and no row.
        let agents: Vec<&SessionInfo> = self
            .on_server()
            .filter(|s| show.shows(s))
            .filter(|s| s.uri.who().session().is_some())
            .collect();
        match show.by {
            By::Person => {
                let mut tops = groups(&agents, &[Level::Person, Level::Host, Level::Repo]);
                for user in people.keys().filter(|u| show.shows_person(u)) {
                    if !tops.iter().any(|g| &g.key == user) {
                        tops.push(Group {
                            level: Level::Person,
                            key: user.clone(),
                            sessions: Vec::new(),
                            kids: Vec::new(),
                        });
                    }
                }
                tops.sort_by(|a, b| a.key.cmp(&b.key));
                tops
            }
            By::Repo => groups(&agents, &[Level::Repo, Level::Person, Level::Host]),
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

/// The top level of the tree of `riff top` (01M42KHNCBMBCT3TFBYWE339H5).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum By {
    /// Person, host, repository, session.
    #[default]
    Person,
    /// Repository, person, host, session: a look across people.
    Repo,
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
            sessions.sort_by_key(|s| (!blocked(s), s.uri.who().session().map(str::to_owned)));
            Group {
                level,
                key,
                kids: groups(&sessions, rest),
                sessions,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(uri: &str) -> SessionInfo {
        serde_json::from_value(serde_json::json!({ "uri": uri, "live": true })).unwrap()
    }

    fn live(name: &str, session: &str, phase: Phase) -> Live {
        Live {
            name: name.into(),
            session: session.into(),
            item: "issue-12".into(),
            image: "riff-session".into(),
            since_ms: 0,
            phase,
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
        let people = sessions.people(&[], &RiffOwner::NoSignIn);
        let tree = sessions.tree(&Show::default(), &people);
        let ids: Vec<&str> = tree[0]
            .sessions
            .iter()
            .filter_map(|s| s.uri.who().session())
            .collect();
        assert_eq!(ids, ["zz", "aa"]);
    }
}
