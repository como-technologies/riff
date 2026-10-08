//! The Claude Code permission rules of riff work.
//!
//! # Design
//!
//! Claude Code auto mode can block normal riff work: a riff tool, `riff
//! workers start`, or a step of the pull request flow. A session cannot
//! add allow rules itself: auto mode blocks a session that edits its own
//! settings. So riff gives the rules of [`rules()`] in the flag settings
//! of each `claude` that it starts, with the rules of the profile of the
//! role (01M4BYH874WQ16Q0337WQA8AMV, see [`crate::launch`]). They are in
//! no settings file of the person or of the project.
//!
//! ```mermaid
//! flowchart LR
//!     G["origin: its URL, and its HEAD on the remote"] --> R["rules()"]
//!     R --> S["--settings of claude"]
//! ```
//!
//! ```
//! use riff::permissions::rules;
//!
//! let rules = rules(Some(("acme", "app")), "main");
//! assert!(rules.allow.contains(&"Bash(riff *)".to_owned()));
//! assert!(rules.deny.contains(&"Bash(git push * main)".to_owned()));
//! ```

use std::path::{Path, PathBuf};

use crate::identity::parse_remote;

/// A set of permission rules of Claude Code.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Rules {
    /// The rules for `permissions.allow`.
    pub allow: Vec<String>,
    /// The rules for `permissions.deny`.
    pub deny: Vec<String>,
}

impl Rules {
    /// True when the set has no rule.
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.deny.is_empty()
    }

    /// The number of rules.
    pub fn len(&self) -> usize {
        self.allow.len() + self.deny.len()
    }
}

/// The rules of riff work for the repository `repo` (OWNER and REPO,
/// when it has a GitHub `origin`) with the default branch `branch`
/// (01M4BYH874WQ16Q0337WQA8AMV).
///
/// - Allow: each riff tool (`mcp__riff`), each `riff` command, and the
///   steps of the pull request flow.
/// - Deny: a push to `branch`, and `gh pr merge --admin`.
///
/// ```
/// let rules = riff::permissions::rules(Some(("acme", "app")), "trunk");
/// assert!(rules.allow.contains(&"mcp__riff".to_owned()));
/// assert!(rules.allow.contains(&"Bash(riff)".to_owned()));
/// assert!(rules.allow.contains(&"Bash(gh api repos/acme/app/statuses/*)".to_owned()));
/// assert!(rules.deny.contains(&"Bash(git push * trunk)".to_owned()));
/// assert!(rules.deny.contains(&"Bash(gh pr merge *--admin*)".to_owned()));
/// let no_repo = riff::permissions::rules(None, "main");
/// assert!(!no_repo.allow.iter().any(|r| r.contains("statuses")));
/// ```
pub fn rules(repo: Option<(&str, &str)>, branch: &str) -> Rules {
    let mut allow: Vec<String> = [
        "mcp__riff",
        "Bash(riff)",
        "Bash(riff *)",
        "Bash(gh pr create *)",
        "Bash(gh pr merge * --auto --squash)",
        "Bash(gh pr comment *)",
        "Bash(gh pr view *)",
    ]
    .map(String::from)
    .into();
    if let Some((owner, name)) = repo {
        allow.push(format!("Bash(gh api repos/{owner}/{name}/statuses/*)"));
    }
    let mut deny: Vec<String> = [
        format!("Bash(git push * {branch})"),
        format!("Bash(git push * {branch} *)"),
        format!("Bash(git push *:{branch})"),
        format!("Bash(git push *:{branch} *)"),
        format!("Bash(git push *:refs/heads/{branch})"),
        format!("Bash(git push *:refs/heads/{branch} *)"),
    ]
    .into();
    deny.push("Bash(gh pr merge *--admin*)".into());
    Rules { allow, deny }
}

/// The facts of a project that its rules need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The top of the repository, or the directory outside git.
    pub top: PathBuf,
    /// OWNER and REPO of a GitHub `origin`.
    pub repo: Option<(String, String)>,
    /// The default branch, from `origin` itself, else `main`
    /// ([`crate::hygiene::default_branch`], 01M4DVXP20SHYTFE1D4NVF0FSF).
    pub branch: String,
}

impl Project {
    /// The project of `dir`.
    pub fn of(dir: &Path) -> Project {
        let top =
            git(dir, &["rev-parse", "--show-toplevel"]).map_or_else(|| dir.into(), Into::into);
        let repo = git(dir, &["remote", "get-url", "--", "origin"])
            .filter(|url| url.contains("github.com"))
            .and_then(|url| parse_remote(&url));
        let branch = crate::hygiene::default_branch(dir, crate::hygiene::REMOTE_WAIT)
            .unwrap_or_else(|_| "main".into());
        Project { top, repo, branch }
    }

    /// The rules of this project ([`rules()`]).
    pub fn rules(&self) -> Rules {
        let repo = self.repo.as_ref().map(|(o, n)| (o.as_str(), n.as_str()));
        rules(repo, &self.branch)
    }
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = crate::confine::git_in(dir).ok()?.args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_in(dir: &Path, args: &[&str]) {
        let ok = crate::confine::git()
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn a_github_clone_gets_its_repo() {
        let dir = tempfile::tempdir().unwrap();
        git_in(dir.path(), &["init", "-q", "-b", "trunk"]);
        git_in(
            dir.path(),
            &["remote", "add", "origin", "git@github.com:acme/app.git"],
        );
        let sub = dir.path().join("src");
        std::fs::create_dir(&sub).unwrap();
        let project = Project::of(&sub);
        assert_eq!(project.top, dir.path().canonicalize().unwrap());
        assert_eq!(project.repo, Some(("acme".into(), "app".into())));
    }

    /// 01M4DVXP20SHYTFE1D4NVF0FSF: the default branch comes from
    /// `origin`, not from a `refs/remotes/origin/HEAD` that a session
    /// wrote.
    #[test]
    fn the_default_branch_comes_from_origin_not_from_a_planted_ref() {
        let dir = tempfile::tempdir().unwrap();
        let (origin, clone) = (dir.path().join("origin.git"), dir.path().join("clone"));
        git_in(
            dir.path(),
            &["init", "-q", "--bare", "-b", "trunk", "origin.git"],
        );
        git_in(dir.path(), &["init", "-q", "-b", "trunk", "clone"]);
        git_in(
            &clone,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "one",
            ],
        );
        git_in(
            &clone,
            &["remote", "add", "origin", &origin.to_string_lossy()],
        );
        git_in(&clone, &["push", "-q", "origin", "trunk", "trunk:evil"]);
        git_in(&clone, &["fetch", "-q", "origin"]);
        git_in(
            &clone,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/evil",
            ],
        );
        let project = Project::of(&clone);
        assert_eq!(project.branch, "trunk");
        assert!(
            project
                .rules()
                .deny
                .contains(&"Bash(git push * trunk)".into())
        );
    }

    #[test]
    fn a_directory_outside_git_uses_main_and_no_repo() {
        let dir = isolated::outside_git();
        let project = Project::of(dir.path());
        assert_eq!(project.top, dir.path());
        assert_eq!(project.repo, None);
        assert_eq!(project.branch, "main");
    }
}
