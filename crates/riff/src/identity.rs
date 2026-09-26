//! Works out the session name from the sign-in, the machine and git
//! (R35–R37, R43).

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use riff_core::name::{Repo, SessionName, sanitize};

/// The session name for a session that runs in `dir`.
///
/// Slice 1 has no sign-in, so the user part comes from `RIFF_USER`, then
/// `USER`. `RIFF_HOST` overrides the host name.
pub fn session_name(dir: &Path) -> Result<SessionName> {
    let user = std::env::var("RIFF_USER")
        .or_else(|_| std::env::var("USER"))
        .context("set RIFF_USER or USER")?;
    let host = match std::env::var("RIFF_HOST") {
        Ok(host) => host,
        Err(_) => short_host(&gethostname::gethostname().to_string_lossy()),
    };
    let (repo, worktree) = match git(dir, &["rev-parse", "--show-toplevel"]) {
        Some(top) => (
            repo_of(dir, &top),
            linked_worktree(dir).then(|| base_name(&top)),
        ),
        None => (Repo::None, Some(base_name(&dir.to_string_lossy()))),
    };
    Ok(SessionName::new(
        &sanitize(&user.to_lowercase()),
        &sanitize(&host),
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
}
