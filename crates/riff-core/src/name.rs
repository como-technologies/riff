//! Session names (R35–R43) and thread names.
//!
//! A session name is a URI: `riff://USER@HOST/OWNER/REPO#WORKTREE`.
//! Outside git it is `riff://USER@HOST/-#DIRECTORY`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

const SCHEME: &str = "riff://";

/// The repository part of a session name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Repo {
    /// A git repository, named by its `origin` remote.
    Git { owner: String, name: String },
    /// No git repository (`-`).
    None,
}

/// The name of one agent session.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SessionName {
    user: String,
    host: String,
    repo: Repo,
    worktree: Option<String>,
}

/// Why a name did not parse or validate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameError(String);

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NameError {}

impl SessionName {
    /// Makes a name from its parts. A name outside git needs a worktree
    /// part (the directory name).
    pub fn new(
        user: &str,
        host: &str,
        repo: Repo,
        worktree: Option<&str>,
    ) -> Result<Self, NameError> {
        check("user", user)?;
        check("host", host)?;
        if let Repo::Git { owner, name } = &repo {
            check("owner", owner)?;
            check("repo", name)?;
        }
        if let Some(worktree) = worktree {
            check("worktree", worktree)?;
        } else if repo == Repo::None {
            return Err(NameError(
                "a name outside git needs a directory part".into(),
            ));
        }
        Ok(Self {
            user: user.into(),
            host: host.into(),
            repo,
            worktree: worktree.map(Into::into),
        })
    }

    /// The repository part.
    pub fn repo(&self) -> &Repo {
        &self.repo
    }

    /// The short form for display and mentions: `USER@HOST:REPO#WORKTREE`.
    pub fn short(&self) -> String {
        let repo = match &self.repo {
            Repo::Git { name, .. } => name.as_str(),
            Repo::None => "-",
        };
        let mut out = format!("{}@{}:{}", self.user, self.host, repo);
        if let Some(worktree) = &self.worktree {
            out.push('#');
            out.push_str(worktree);
        }
        out
    }

    /// The thread that this session joins by default (R41).
    pub fn default_thread(&self) -> Option<ThreadName> {
        match &self.repo {
            Repo::Git { owner, name } => Some(ThreadName(format!("{owner}/{name}"))),
            Repo::None => None,
        }
    }
}

impl fmt::Display for SessionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{SCHEME}{}@{}/", self.user, self.host)?;
        match &self.repo {
            Repo::Git { owner, name } => write!(f, "{owner}/{name}")?,
            Repo::None => f.write_str("-")?,
        }
        if let Some(worktree) = &self.worktree {
            write!(f, "#{worktree}")?;
        }
        Ok(())
    }
}

impl FromStr for SessionName {
    type Err = NameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || NameError(format!("not a session name: {s}"));
        let rest = s.strip_prefix(SCHEME).ok_or_else(bad)?;
        let (authority, path) = rest.split_once('/').ok_or_else(bad)?;
        let (user, host) = authority.split_once('@').ok_or_else(bad)?;
        let (path, worktree) = match path.split_once('#') {
            Some((path, worktree)) => (path, Some(worktree)),
            None => (path, None),
        };
        let repo = if path == "-" {
            Repo::None
        } else {
            let (owner, name) = path.split_once('/').ok_or_else(bad)?;
            Repo::Git {
                owner: owner.into(),
                name: name.into(),
            }
        };
        Self::new(user, host, repo, worktree)
    }
}

impl TryFrom<String> for SessionName {
    type Error = NameError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<SessionName> for String {
    fn from(name: SessionName) -> Self {
        name.to_string()
    }
}

/// Replaces each character that a name part cannot hold with `-`.
pub fn sanitize(part: &str) -> String {
    part.chars()
        .map(|c| if allowed(c) { c } else { '-' })
        .collect()
}

fn allowed(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~')
}

fn check(what: &str, part: &str) -> Result<(), NameError> {
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

impl ThreadName {
    /// The thread that holds the direct messages between two sessions.
    /// The order of the two sessions does not matter.
    pub fn direct(a: &SessionName, b: &SessionName) -> Self {
        let (first, second) = if a <= b { (a, b) } else { (b, a) };
        Self(format!("{DIRECT_PREFIX}{first}|{second}"))
    }

    /// True for a thread of direct messages.
    pub fn is_direct(&self) -> bool {
        self.0.starts_with(DIRECT_PREFIX)
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
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() || s.chars().any(char::is_whitespace) {
            return Err(NameError(format!("not a thread name: {s:?}")));
        }
        if s.starts_with(DIRECT_PREFIX) {
            return Err(NameError("use the tell command for direct messages".into()));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn git(worktree: Option<&str>) -> SessionName {
        let repo = Repo::Git {
            owner: "como-technologies".into(),
            name: "riff".into(),
        };
        SessionName::new("mike", "pangolin", repo, worktree).unwrap()
    }

    #[test]
    fn a_worktree_name_round_trips() {
        let name = git(Some("pr-23"));
        let text = "riff://mike@pangolin/como-technologies/riff#pr-23";
        assert_eq!(name.to_string(), text);
        assert_eq!(text.parse::<SessionName>().unwrap(), name);
        assert_eq!(name.short(), "mike@pangolin:riff#pr-23");
    }

    #[test]
    fn the_main_worktree_has_no_fragment() {
        let name = git(None);
        assert_eq!(
            name.to_string(),
            "riff://mike@pangolin/como-technologies/riff"
        );
        assert_eq!(name.short(), "mike@pangolin:riff");
        assert_eq!(
            name.default_thread().unwrap().to_string(),
            "como-technologies/riff"
        );
    }

    #[test]
    fn a_name_outside_git_uses_a_dash() {
        let name: SessionName = "riff://mike@pangolin/-#notes".parse().unwrap();
        assert_eq!(name.repo(), &Repo::None);
        assert_eq!(name.short(), "mike@pangolin:-#notes");
        assert!(name.default_thread().is_none());
        assert!("riff://mike@pangolin/-".parse::<SessionName>().is_err());
    }

    #[test]
    fn bad_names_do_not_parse() {
        for bad in [
            "",
            "mike@pangolin/o/r",
            "riff://pangolin/o/r",
            "riff://mike@pangolin/norepo",
            "riff://mike@pan golin/o/r",
            "riff://@pangolin/o/r",
        ] {
            assert!(bad.parse::<SessionName>().is_err(), "{bad}");
        }
    }

    #[test]
    fn sanitize_replaces_characters_that_are_not_allowed() {
        assert_eq!(sanitize("feat/login page"), "feat-login-page");
    }

    #[test]
    fn a_direct_thread_does_not_depend_on_order() {
        let a = git(Some("a"));
        let b = git(Some("b"));
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
