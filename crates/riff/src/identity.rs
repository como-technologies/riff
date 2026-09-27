//! Works out the session URI from the agent tool, the user, the machine
//! and git.
//!
//! # Rules
//!
//! | Part | Source, in order |
//! |---|---|
//! | user | `RIFF_USER`, then the sign-in at the server (see [`crate::login`]), then `USER`. A keyring error stops the command (R157, R158). |
//! | session | `RIFF_SESSION`, then `CLAUDE_CODE_SESSION_ID`. A person has none. |
//! | host | `RIFF_HOST`, then `cloud` in a cloud session, then the machine name without its domain. |
//! | owner/repo | The `origin` remote. Without a remote: `local/<main worktree directory>`. |
//! | worktree | The directory name of a linked worktree. The main worktree has none. |
//!
//! Outside git, the repository part is `-` and the worktree part is the
//! directory name. Each part goes through
//! [`riff_core::name::sanitize`].
//!
//! Claude Code gives the session ID to each process that it starts for
//! a session: `riff mcp` and a `riff watch` under the Monitor tool get it
//! in the environment, and the hooks get it on stdin (see [`agent`]).
//! So they all find the same session, in any directory (R57).
//!
//! A person on the command line has no session ID. The URI of a person
//! is `riff://USER@HOST` (R65). The directory still gives the default
//! thread.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use riff_core::name::{Place, Repo, SessionUri, Who, sanitize};

use crate::login;

/// The environment variables that hold the session ID, in order.
pub const SESSION_VARS: [&str; 2] = ["RIFF_SESSION", "CLAUDE_CODE_SESSION_ID"];

/// The session ID from the environment, if there is one.
pub fn session_id() -> Option<String> {
    SESSION_VARS
        .iter()
        .find_map(|var| std::env::var(var).ok())
        .filter(|id| !id.is_empty())
}

/// The URI of the caller: an agent session when there is a session ID,
/// otherwise a person. `place` is where the caller works. `server` is
/// the riff-server, for the user of its sign-in.
pub fn me(place: &Place, server: &str) -> Result<SessionUri> {
    match session_id() {
        Some(id) => agent(place, &id, server),
        None => Ok(SessionUri::new(
            Who::new(&user(server)?, None)?,
            Place::host_only(place.host())?,
        )),
    }
}

/// The URI of the agent session `id` at `place`. A hook uses it: Claude
/// Code gives a hook the session ID on stdin.
pub fn agent(place: &Place, id: &str, server: &str) -> Result<SessionUri> {
    Ok(SessionUri::new(
        Who::new(&user(server)?, Some(&sanitize(id)))?,
        place.clone(),
    ))
}

/// The user for `server`: from `RIFF_USER`, the sign-in, or `USER`.
/// Only a missing `RIFF_USER` makes it read the keyring. A keyring error
/// stops it, also when riff cannot open the keyring: `USER` stands in
/// only when there is no sign-in (R157, R158).
fn user(server: &str) -> Result<String> {
    let riff_user = std::env::var("RIFF_USER").ok();
    let signed_in = match riff_user {
        Some(_) => None,
        None => login::user(server)
            .context("riff cannot find your user. Unlock the keyring, or set RIFF_USER")?,
    };
    pick_user(
        riff_user.as_deref(),
        signed_in.as_deref(),
        std::env::var("USER").ok().as_deref(),
    )
}

/// The user from `RIFF_USER`, the user of the sign-in, and `USER`, in
/// that order (R36). Without a sign-in, `USER` stands in until the
/// server checks tokens. The caller passes no sign-in only when there
/// is none, never when the keyring failed (R157).
///
/// ```
/// use riff::identity::pick_user;
///
/// assert_eq!(pick_user(None, Some("mike"), Some("lovelace")).unwrap(), "mike");
/// assert_eq!(pick_user(Some("brett"), Some("mike"), None).unwrap(), "brett");
/// assert_eq!(pick_user(None, None, Some("Lovelace")).unwrap(), "lovelace");
/// assert!(pick_user(None, None, None).is_err());
/// ```
pub fn pick_user(
    riff_user: Option<&str>,
    signed_in: Option<&str>,
    os_user: Option<&str>,
) -> Result<String> {
    let user = riff_user
        .or(signed_in)
        .or(os_user)
        .context("run riff login, or set RIFF_USER")?;
    Ok(sanitize(&user.to_lowercase()))
}

/// The URI of an agent session. It fails when there is no session ID.
pub fn session(place: &Place, server: &str) -> Result<SessionUri> {
    let me = me(place, server)?;
    if me.who().session().is_none() {
        bail!(
            "no session ID: run this inside Claude Code, or set {}",
            SESSION_VARS[0]
        );
    }
    Ok(me)
}

/// The place for `dir`, with the host from the environment.
pub fn place(dir: &Path) -> Result<Place> {
    let host = host(
        std::env::var("RIFF_HOST").ok().as_deref(),
        std::env::var(REMOTE_VAR).ok().as_deref(),
        &gethostname::gethostname().to_string_lossy(),
    );
    place_in(dir, &host)
}

/// Claude Code sets this variable to `true` in a cloud session (R100).
pub const REMOTE_VAR: &str = "CLAUDE_CODE_REMOTE";

/// The host from `RIFF_HOST`, the value of [`REMOTE_VAR`] and the machine
/// name. A cloud session has a random container name, so its host is
/// `cloud` (R42).
///
/// ```
/// use riff::identity::host;
///
/// assert_eq!(host(None, None, "Pangolin.local"), "pangolin");
/// assert_eq!(host(None, Some("true"), "a1b2c3d4e5"), "cloud");
/// assert_eq!(host(Some("brett"), Some("true"), "a1b2c3d4e5"), "brett");
/// ```
pub fn host(riff_host: Option<&str>, remote: Option<&str>, machine: &str) -> String {
    match (riff_host, remote) {
        (Some(host), _) => host.to_owned(),
        (None, Some("true")) => "cloud".to_owned(),
        _ => short_host(machine),
    }
}

/// The place for `dir` on a known host.
pub fn place_in(dir: &Path, host: &str) -> Result<Place> {
    let (repo, worktree) = match git(dir, &["rev-parse", "--show-toplevel"]) {
        Some(top) => (
            repo_of(dir, &top),
            linked_worktree(dir).then(|| base_name(&top)),
        ),
        None => (Repo::None, Some(base_name(&dir.to_string_lossy()))),
    };
    Ok(Place::new(
        &sanitize(host),
        repo,
        worktree.as_deref().map(sanitize).as_deref(),
    )?)
}

/// `pangolin.local` becomes `pangolin`.
fn short_host(host: &str) -> String {
    host.split('.').next().unwrap_or(host).to_lowercase()
}

/// OWNER/REPO from the `origin` remote. Without a remote, the owner is
/// `local` and the repo is the directory name of the main worktree.
fn repo_of(dir: &Path, top: &str) -> Repo {
    if let Some((owner, name)) =
        git(dir, &["remote", "get-url", "origin"]).and_then(|url| parse_remote(&url))
    {
        return Repo::Git {
            owner: sanitize(&owner),
            name: sanitize(&name),
        };
    }
    let main = git(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .and_then(|common| {
        Path::new(&common)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
    })
    .unwrap_or_else(|| top.to_owned());
    Repo::Git {
        owner: "local".into(),
        name: sanitize(&base_name(&main)),
    }
}

/// True when `dir` is in a linked worktree, not the main one.
fn linked_worktree(dir: &Path) -> bool {
    let git_dir = git(dir, &["rev-parse", "--path-format=absolute", "--git-dir"]);
    let common = git(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    );
    matches!((git_dir, common), (Some(a), Some(b)) if a != b)
}

/// OWNER and REPO from a remote URL, for example
/// `https://github.com/como-technologies/riff.git` or
/// `git@github.com:como-technologies/riff.git`.
///
/// ```
/// use riff::identity::parse_remote;
///
/// let expected = Some(("como-technologies".to_owned(), "riff".to_owned()));
/// assert_eq!(parse_remote("https://github.com/como-technologies/riff.git"), expected);
/// assert_eq!(parse_remote("git@github.com:como-technologies/riff.git"), expected);
/// assert_eq!(parse_remote("riff"), None);
/// ```
pub fn parse_remote(url: &str) -> Option<(String, String)> {
    let path = url.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = path.rsplit(['/', ':']);
    let name = parts.next().filter(|s| !s.is_empty())?;
    let owner = parts.next().filter(|s| !s.is_empty())?;
    Some((owner.to_owned(), name.to_owned()))
}

fn base_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| "root".into(), |n| n.to_string_lossy().into_owned())
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes_parse_in_both_forms() {
        for url in [
            "https://github.com/como-technologies/riff.git",
            "https://github.com/como-technologies/riff",
            "git@github.com:como-technologies/riff.git",
            "ssh://git@github.com/como-technologies/riff/",
        ] {
            assert_eq!(
                parse_remote(url),
                Some(("como-technologies".into(), "riff".into())),
                "{url}"
            );
        }
        assert_eq!(parse_remote("riff"), None);
    }

    #[test]
    fn host_names_lose_their_domain() {
        assert_eq!(short_host("Pangolin.local"), "pangolin");
    }

    #[test]
    fn only_a_true_remote_flag_gives_the_cloud_host() {
        assert_eq!(host(None, Some("false"), "pangolin"), "pangolin");
        assert_eq!(host(None, Some(""), "pangolin"), "pangolin");
        assert_eq!(host(None, Some("true"), "pangolin"), "cloud");
    }
}
