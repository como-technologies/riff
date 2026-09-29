//! Session URIs and thread names.
//!
//! # Session URIs
//!
//! A session URI shows who a session is, where it works, and what it
//! works on:
//!
//! ```text
//! riff://mike@pangolin/como-technologies/riff?session=a6cf&claim=issue-6#pr-23
//!        └─┬┘ └──┬───┘ └─────────┬────────┘ └────┬─────┘ └────┬────┘ └─┬─┘
//!        user   host        owner/repo        session ID    claim    worktree
//! ```
//!
//! | Part | Kind | Source |
//! |---|---|---|
//! | user | who | The sign-in. |
//! | session | who | The session ID of the agent tool. A person has none. |
//! | host | where | The machine name, without its domain. |
//! | owner/repo | where | The `origin` remote of the git repository. |
//! | worktree | where | The directory name of a linked worktree. The main worktree has none. |
//! | lead | what | `lead=true` when the session is the lead of its user in its repository. |
//! | claim | what | One part for each claim that the session holds. |
//!
//! *Who* ([`Who`]) never changes. *Where* ([`Place`]) changes when the
//! session moves. *What* changes with each claim, and when the session
//! becomes the lead or stops being the lead. The server keys each
//! session by its [`Who`].
//!
//! Outside git, the repository part is `-` and the worktree part is the
//! directory name: `riff://mike@pangolin/-?session=a6cf#notes`. A person
//! who posts from the command line has no session and no place:
//! `riff://mike@pangolin`.
//!
//! Each part holds only ASCII letters, digits, `-`, `_`, `.` and `~`.
//! [`sanitize`] makes any text fit.
//!
//! The *short form* is for people: `mike@pangolin:riff#pr-23`. It drops
//! the owner, the session and the claims, so it is not unique.
//!
//! # Thread names
//!
//! A thread name is free text without spaces, for example
//! `como-technologies/riff`. The names that start with `dm:` belong to
//! direct-message threads. Only [`ThreadName::direct`] makes them.

use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

const SCHEME: &str = "riff://";

/// Why a name did not parse or validate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameError(String);

impl NameError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NameError {}

/// Who a session is: a user, and the session ID of the agent tool.
/// A person on the command line has no session ID.
///
/// ```
/// use riff_core::name::Who;
///
/// let agent = Who::new("mike", Some("a6cf"))?;
/// assert_eq!(agent.to_string(), "mike/a6cf");
/// assert_eq!(Who::new("mike", None)?.to_string(), "mike");
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(
    Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct Who {
    user: String,
    session: Option<String>,
}

impl Who {
    pub fn new(user: &str, session: Option<&str>) -> Result<Self, NameError> {
        check("user", user)?;
        if let Some(session) = session {
            check("session", session)?;
        }
        Ok(Self {
            user: user.into(),
            session: session.map(Into::into),
        })
    }

    pub fn user(&self) -> &str {
        &self.user
    }

    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }
}

impl fmt::Display for Who {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.user)?;
        if let Some(session) = &self.session {
            write!(f, "/{session}")?;
        }
        Ok(())
    }
}

/// The repository part of a place.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Repo {
    /// A git repository, named by its `origin` remote.
    Git { owner: String, name: String },
    /// No git repository (`-`).
    None,
}

/// Where a session works: a host, a repository and a worktree.
///
/// ```
/// use riff_core::name::{Place, Repo};
///
/// let repo = Repo::Git { owner: "como-technologies".into(), name: "riff".into() };
/// let place = Place::new("pangolin", repo, Some("pr-23"))?;
/// assert_eq!(place.repo_text(), "como-technologies/riff");
/// assert_eq!(place.default_thread().unwrap().to_string(), "como-technologies/riff");
///
/// // A person has a host and nothing else.
/// assert!(Place::host_only("pangolin")?.default_thread().is_none());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Place {
    host: String,
    repo: Repo,
    worktree: Option<String>,
}

impl Place {
    pub fn new(host: &str, repo: Repo, worktree: Option<&str>) -> Result<Self, NameError> {
        check("host", host)?;
        if let Repo::Git { owner, name } = &repo {
            check("owner", owner)?;
            check("repo", name)?;
        }
        if let Some(worktree) = worktree {
            check("worktree", worktree)?;
        }
        Ok(Self {
            host: host.into(),
            repo,
            worktree: worktree.map(Into::into),
        })
    }

    /// The place of a person on the command line: a host only.
    pub fn host_only(host: &str) -> Result<Self, NameError> {
        Self::new(host, Repo::None, None)
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn repo(&self) -> &Repo {
        &self.repo
    }

    pub fn worktree(&self) -> Option<&str> {
        self.worktree.as_deref()
    }

    /// `OWNER/REPO`, or `-` outside git.
    pub fn repo_text(&self) -> String {
        match &self.repo {
            Repo::Git { owner, name } => format!("{owner}/{name}"),
            Repo::None => "-".into(),
        }
    }

    /// The thread that a session joins in this place: OWNER/REPO.
    /// Outside git there is none.
    pub fn default_thread(&self) -> Option<ThreadName> {
        match &self.repo {
            Repo::Git { .. } => Some(ThreadName(self.repo_text())),
            Repo::None => None,
        }
    }

    fn has_path(&self) -> bool {
        self.repo != Repo::None || self.worktree.is_some()
    }
}

/// The URI of one session: who, where and what.
///
/// It serializes as its full URI. Two URIs of one session differ when
/// the session moves or claims.
///
/// ```
/// use riff_core::name::SessionUri;
///
/// let text = "riff://mike@pangolin/como-technologies/riff?session=a6cf&claim=issue-6#pr-23";
/// let uri: SessionUri = text.parse()?;
/// assert_eq!(uri.to_string(), text);
/// assert_eq!(uri.short(), "mike@pangolin:riff#pr-23");
/// assert_eq!(uri.who().session(), Some("a6cf"));
/// assert_eq!(uri.claims(), ["issue-6"]);
/// assert!(!uri.lead());
///
/// let lead: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf&lead=true".parse()?;
/// assert!(lead.lead());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SessionUri {
    who: Who,
    place: Place,
    lead: bool,
    claims: Vec<String>,
}

impl SessionUri {
    /// A URI with no claims. It is not the lead.
    pub fn new(who: Who, place: Place) -> Self {
        Self {
            who,
            place,
            lead: false,
            claims: Vec::new(),
        }
    }

    /// The same URI, as the lead or not.
    pub fn with_lead(mut self, lead: bool) -> Self {
        self.lead = lead;
        self
    }

    /// True when the session is the lead of its user in its repository.
    pub fn lead(&self) -> bool {
        self.lead
    }

    /// The same URI with these claims, in sorted order.
    pub fn with_claims(mut self, mut claims: Vec<String>) -> Self {
        claims.sort();
        claims.dedup();
        self.claims = claims;
        self
    }

    pub fn who(&self) -> &Who {
        &self.who
    }

    pub fn place(&self) -> &Place {
        &self.place
    }

    pub fn claims(&self) -> &[String] {
        &self.claims
    }

    /// The same URI in a new place.
    pub fn moved(mut self, place: Place) -> Self {
        self.place = place;
        self
    }

    /// The short form for people: `USER@HOST:REPO#WORKTREE`.
    ///
    /// ```
    /// # use riff_core::name::SessionUri;
    /// let main: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// assert_eq!(main.short(), "mike@pangolin:riff");
    /// let notes: SessionUri = "riff://mike@pangolin/-?session=a6cf#notes".parse()?;
    /// assert_eq!(notes.short(), "mike@pangolin:-#notes");
    /// let person: SessionUri = "riff://mike@pangolin".parse()?;
    /// assert_eq!(person.short(), "mike@pangolin");
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn short(&self) -> String {
        let mut out = format!("{}@{}", self.who.user, self.place.host);
        if !self.place.has_path() {
            return out;
        }
        out.push(':');
        match &self.place.repo {
            Repo::Git { name, .. } => out.push_str(name),
            Repo::None => out.push('-'),
        }
        if let Some(worktree) = &self.place.worktree {
            out.push('#');
            out.push_str(worktree);
        }
        out
    }

    /// The thread that this session joins by default: OWNER/REPO.
    pub fn default_thread(&self) -> Option<ThreadName> {
        self.place.default_thread()
    }
}

impl fmt::Display for SessionUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{SCHEME}{}@{}", self.who.user, self.place.host)?;
        let query: Vec<String> = self
            .who
            .session
            .iter()
            .map(|s| format!("session={s}"))
            .chain(self.lead.then(|| "lead=true".to_owned()))
            .chain(self.claims.iter().map(|c| format!("claim={c}")))
            .collect();
        if self.place.has_path() || !query.is_empty() {
            write!(f, "/{}", self.place.repo_text())?;
        }
        if !query.is_empty() {
            write!(f, "?{}", query.join("&"))?;
        }
        if let Some(worktree) = &self.place.worktree {
            write!(f, "#{worktree}")?;
        }
        Ok(())
    }
}

impl FromStr for SessionUri {
    type Err = NameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || NameError(format!("not a session URI: {s}"));
        let rest = s.strip_prefix(SCHEME).ok_or_else(bad)?;
        let (rest, worktree) = match rest.split_once('#') {
            Some((rest, worktree)) => (rest, Some(worktree)),
            None => (rest, None),
        };
        let (rest, query) = match rest.split_once('?') {
            Some((rest, query)) => (rest, Some(query)),
            None => (rest, None),
        };
        let (authority, path) = match rest.split_once('/') {
            Some((authority, path)) => (authority, Some(path)),
            None => (rest, None),
        };
        let (user, host) = authority.split_once('@').ok_or_else(bad)?;
        let repo = match path {
            None | Some("-") => Repo::None,
            Some(path) => {
                let (owner, name) = path.split_once('/').ok_or_else(bad)?;
                Repo::Git {
                    owner: owner.into(),
                    name: name.into(),
                }
            }
        };
        let mut session = None;
        let mut lead = false;
        let mut claims = Vec::new();
        for pair in query.into_iter().flat_map(|q| q.split('&')) {
            match pair.split_once('=') {
                Some(("session", value)) if session.is_none() => session = Some(value),
                Some(("lead", "true")) => lead = true,
                Some(("claim", value)) => {
                    check("claim", value)?;
                    claims.push(value.to_owned());
                }
                _ => return Err(bad()),
            }
        }
        let who = Who::new(user, session)?;
        let place = Place::new(host, repo, worktree)?;
        Ok(Self::new(who, place).with_lead(lead).with_claims(claims))
    }
}

impl TryFrom<String> for SessionUri {
    type Error = NameError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<SessionUri> for String {
    fn from(uri: SessionUri) -> Self {
        uri.to_string()
    }
}

/// Replaces each character that a URI part cannot hold with `-`.
///
/// ```
/// assert_eq!(riff_core::name::sanitize("feat/login page"), "feat-login-page");
/// ```
pub fn sanitize(part: &str) -> String {
    part.chars()
        .map(|c| if allowed(c) { c } else { '-' })
        .collect()
}

fn allowed(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~')
}

/// Checks that one URI part is not empty and holds only allowed
/// characters. `what` names the part in the error.
pub fn check(what: &str, part: &str) -> Result<(), NameError> {
    if part.is_empty() {
        return Err(NameError(format!("the {what} part is empty")));
    }
    if !part.chars().all(allowed) {
        return Err(NameError(format!(
            "the {what} part has a character that is not allowed: {part}"
        )));
    }
    Ok(())
}

/// The name of a thread.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ThreadName(String);

const DIRECT_PREFIX: &str = "dm:";

/// The name of the chat thread of `riff chat`.
pub const CHAT: &str = "chat";

impl ThreadName {
    /// The chat thread of `riff chat`.
    ///
    /// ```
    /// assert_eq!(riff_core::name::ThreadName::chat().to_string(), "chat");
    /// ```
    pub fn chat() -> Self {
        Self(CHAT.to_owned())
    }

    /// The thread that holds the direct messages between two sessions.
    /// The order of the two sessions does not matter.
    ///
    /// ```
    /// use riff_core::name::{ThreadName, Who};
    ///
    /// let a = Who::new("mike", Some("a6cf"))?;
    /// let b = Who::new("brett", Some("77e0"))?;
    /// assert_eq!(ThreadName::direct(&a, &b), ThreadName::direct(&b, &a));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn direct(a: &Who, b: &Who) -> Self {
        let (first, second) = if a <= b { (a, b) } else { (b, a) };
        Self(format!("{DIRECT_PREFIX}{first}|{second}"))
    }

    /// True for a thread of direct messages.
    pub fn is_direct(&self) -> bool {
        self.0.starts_with(DIRECT_PREFIX)
    }

    /// The other session of a direct thread of `who`. `None` when this
    /// is not a direct thread of `who`.
    ///
    /// ```
    /// use riff_core::name::{ThreadName, Who};
    ///
    /// let a = Who::new("mike", Some("a6cf"))?;
    /// let b = Who::new("brett", Some("77e0"))?;
    /// let c = Who::new("mike", None)?;
    /// let thread = ThreadName::direct(&a, &b);
    /// assert_eq!(thread.peer(&a), Some(b.clone()));
    /// assert_eq!(thread.peer(&b), Some(a.clone()));
    /// assert_eq!(thread.peer(&c), None);
    /// assert_eq!(ThreadName::direct(&c, &a).peer(&a), Some(c));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn peer(&self, who: &Who) -> Option<Who> {
        let (first, second) = self.0.strip_prefix(DIRECT_PREFIX)?.split_once('|')?;
        let parse = |text: &str| {
            let (user, session) = match text.split_once('/') {
                Some((user, session)) => (user, Some(session)),
                None => (text, None),
            };
            Who::new(user, session).ok()
        };
        let (first, second) = (parse(first)?, parse(second)?);
        if &first == who {
            Some(second)
        } else if &second == who {
            Some(first)
        } else {
            None
        }
    }
}

impl fmt::Display for ThreadName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ThreadName {
    type Err = NameError;

    /// Parses a thread name that a person or an agent typed. Direct-message
    /// threads come only from [`ThreadName::direct`].
    ///
    /// ```
    /// use riff_core::name::ThreadName;
    ///
    /// assert!("como-technologies/riff".parse::<ThreadName>().is_ok());
    /// assert!("two words".parse::<ThreadName>().is_err());
    /// assert!("dm:someone".parse::<ThreadName>().is_err());
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() || s.chars().any(char::is_whitespace) {
            return Err(NameError(format!("not a thread name: {s:?}")));
        }
        if s.starts_with(DIRECT_PREFIX) {
            return Err(NameError("use tell for direct messages".into()));
        }
        Ok(Self(s.into()))
    }
}

impl TryFrom<String> for ThreadName {
    type Error = NameError;

    /// Accepts every thread name on the wire, direct ones included.
    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.starts_with(DIRECT_PREFIX) {
            return Ok(Self(s));
        }
        s.parse()
    }
}

impl From<ThreadName> for String {
    fn from(name: ThreadName) -> Self {
        name.0
    }
}

/// A session URI and a thread name go on the wire as a string.
macro_rules! string_schema {
    ($($t:ident),*) => {$(
        impl JsonSchema for $t {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($t).into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({ "type": "string" })
            }
        }
    )*};
}

string_schema!(SessionUri, ThreadName);

#[cfg(test)]
mod tests {
    use super::*;

    fn riff() -> Repo {
        Repo::Git {
            owner: "como-technologies".into(),
            name: "riff".into(),
        }
    }

    fn uri(session: Option<&str>, worktree: Option<&str>) -> SessionUri {
        SessionUri::new(
            Who::new("mike", session).unwrap(),
            Place::new("pangolin", riff(), worktree).unwrap(),
        )
    }

    #[test]
    fn a_full_uri_round_trips() {
        let text = "riff://mike@pangolin/como-technologies/riff?session=a6cf&lead=true&claim=a&claim=b#pr-23";
        let parsed: SessionUri = text.parse().unwrap();
        assert_eq!(parsed.to_string(), text);
        assert_eq!(parsed.claims(), ["a", "b"]);
        assert!(parsed.lead());
        assert_eq!(parsed.place().worktree(), Some("pr-23"));
    }

    #[test]
    fn the_main_worktree_has_no_fragment() {
        let main = uri(Some("a6cf"), None);
        assert_eq!(
            main.to_string(),
            "riff://mike@pangolin/como-technologies/riff?session=a6cf"
        );
        assert_eq!(main.short(), "mike@pangolin:riff");
    }

    #[test]
    fn two_sessions_in_one_worktree_differ() {
        assert_ne!(uri(Some("a"), None), uri(Some("b"), None));
        assert_eq!(uri(Some("a"), None).short(), uri(Some("b"), None).short());
    }

    #[test]
    fn a_uri_outside_git_uses_a_dash() {
        let text = "riff://mike@pangolin/-?session=a6cf#notes";
        let parsed: SessionUri = text.parse().unwrap();
        assert_eq!(parsed.place().repo(), &Repo::None);
        assert_eq!(parsed.to_string(), text);
        assert!(parsed.default_thread().is_none());
    }

    #[test]
    fn a_person_has_only_user_and_host() {
        let person: SessionUri = "riff://mike@pangolin".parse().unwrap();
        assert_eq!(person.who().session(), None);
        assert_eq!(person.to_string(), "riff://mike@pangolin");
    }

    #[test]
    fn claims_come_out_sorted_and_once() {
        let claimed = uri(Some("a"), None).with_claims(vec!["b".into(), "a".into(), "b".into()]);
        assert_eq!(claimed.claims(), ["a", "b"]);
    }

    #[test]
    fn bad_uris_do_not_parse() {
        for bad in [
            "",
            "mike@pangolin/o/r",
            "riff://pangolin/o/r",
            "riff://mike@pangolin/norepo",
            "riff://mike@pan golin/o/r",
            "riff://@pangolin/o/r",
            "riff://mike@pangolin/o/r?other=x",
            "riff://mike@pangolin/o/r?session=a&session=b",
            "riff://mike@pangolin/o/r?claim=a b",
            "riff://mike@pangolin/o/r?session=a&lead=false",
            "riff://mike@pangolin/o/r?session=a&lead=yes",
        ] {
            assert!(bad.parse::<SessionUri>().is_err(), "{bad}");
        }
    }

    #[test]
    fn a_direct_thread_does_not_depend_on_order() {
        let a = Who::new("mike", Some("a")).unwrap();
        let b = Who::new("mike", Some("b")).unwrap();
        let thread = ThreadName::direct(&a, &b);
        assert_eq!(thread, ThreadName::direct(&b, &a));
        assert!(thread.is_direct());
        let wire: ThreadName =
            serde_json::from_str(&serde_json::to_string(&thread).unwrap()).unwrap();
        assert_eq!(wire, thread);
    }

    #[test]
    fn a_typed_thread_name_cannot_be_direct() {
        assert!("dm:x".parse::<ThreadName>().is_err());
        assert!("two words".parse::<ThreadName>().is_err());
        assert!("como-technologies/riff".parse::<ThreadName>().is_ok());
    }
}
