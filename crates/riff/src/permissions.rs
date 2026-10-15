//! The Claude Code permission rules of riff work.
//!
//! # Design
//!
//! Claude Code auto mode can block normal riff work: a riff tool, `riff
//! workers start`, or a step of the pull request flow. A session cannot
//! add allow rules itself: auto mode blocks a session that edits its own
//! settings. So a person runs `riff setup` once in the project
//! (01M3Q53RNDJBDHVDFHJ9HCX9S1). It adds the rules of [`rules()`] to the
//! project settings of Claude Code, `.claude/settings.json` at the top
//! of the repository ([`Project::settings`]).
//!
//! - It adds only the rules that are missing. It keeps each other rule
//!   and each other key, in its order ([`with_rules`]).
//! - A rule counts as there when the user settings, the project
//!   settings or the local settings (`.claude/settings.local.json`) have
//!   it ([`missing`]).
//! - `riff setup --check` changes nothing. It names each missing rule.
//! - The start hook tells the lead when rules are missing
//!   (01M3Q53RQGXMYVYGCQQMWA9380).
//!
//! ```mermaid
//! flowchart LR
//!     G["git: origin, origin/HEAD"] --> R["rules()"]
//!     R --> M{"missing in user,<br/>project or local?"}
//!     M -- "some" --> W["add to<br/>.claude/settings.json"]
//!     M -- "none" --> N["change nothing"]
//! ```
//!
//! ```
//! use riff::permissions::{rules, with_rules};
//!
//! let rules = rules(Some(("acme", "app")), "main");
//! let text = with_rules("{}", &rules)?.unwrap();
//! assert!(text.contains("\"Bash(riff *)\""));
//! assert!(text.contains("\"Bash(git push * main)\""));
//! assert_eq!(with_rules(&text, &rules)?, None);
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde_json::{Map, Value};

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
/// (01M3Q53RNDJBDHVDFHJ9HCX9S1).
///
/// - Allow: each riff tool (`mcp__plugin_riff_riff` from the plugin,
///   `mcp__riff` in a worker), each `riff` command, and the steps of the
///   pull request flow.
/// - Deny: a push to `branch`, and `gh pr merge --admin`.
///
/// ```
/// let rules = riff::permissions::rules(Some(("acme", "app")), "trunk");
/// assert!(rules.allow.contains(&"mcp__plugin_riff_riff".to_owned()));
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
        "mcp__plugin_riff_riff",
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

/// The rules of `rules` that none of the settings texts in `texts` has.
/// A text that is not JSON has no rule.
///
/// ```
/// use riff::permissions::{Rules, missing};
///
/// let want = Rules { allow: vec!["A".into(), "B".into()], deny: vec!["C".into()] };
/// let user = r#"{"permissions": {"allow": ["A"]}}"#;
/// let local = r#"{"permissions": {"deny": ["C"]}}"#;
/// let left = missing(&want, &[user, "not json", local]);
/// assert_eq!(left, Rules { allow: vec!["B".into()], deny: vec![] });
/// ```
pub fn missing(rules: &Rules, texts: &[&str]) -> Rules {
    let values: Vec<Value> = texts
        .iter()
        .filter_map(|t| serde_json::from_str(t).ok())
        .collect();
    let has = |list: &str, rule: &str| {
        values.iter().any(|v| {
            v["permissions"][list]
                .as_array()
                .is_some_and(|a| a.iter().any(|r| r == rule))
        })
    };
    let left = |list: &str, want: &[String]| {
        want.iter()
            .filter(|r| !has(list, r))
            .cloned()
            .collect::<Vec<_>>()
    };
    Rules {
        allow: left("allow", &rules.allow),
        deny: left("deny", &rules.deny),
    }
}

/// The settings text with each rule of `rules` that it lacks, or `None`
/// when it has them all. It keeps each other key and each other rule in
/// its order. It fails when the text is not a JSON object, or when
/// `permissions`, `allow` or `deny` has the wrong type.
///
/// ```
/// use riff::permissions::{Rules, with_rules};
///
/// let rules = Rules { allow: vec!["A".into()], deny: vec!["D".into()] };
/// let text = r#"{"model": "opus", "permissions": {"allow": ["X"]}}"#;
/// let out = with_rules(text, &rules)?.unwrap();
/// let value: serde_json::Value = serde_json::from_str(&out)?;
/// assert_eq!(value["permissions"]["allow"], serde_json::json!(["X", "A"]));
/// assert_eq!(value["permissions"]["deny"], serde_json::json!(["D"]));
/// assert!(out.find("model").unwrap() < out.find("permissions").unwrap());
/// assert!(with_rules("[1]", &rules).is_err());
/// assert!(with_rules(r#"{"permissions": []}"#, &rules).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn with_rules(text: &str, rules: &Rules) -> Result<Option<String>> {
    let mut value: Value = serde_json::from_str(text).context("the settings are not valid JSON")?;
    let object = value
        .as_object_mut()
        .context("the settings are not a JSON object")?;
    let permissions = object
        .entry("permissions")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("`permissions` is not a JSON object")?;
    let mut changed = false;
    for (list, want) in [("allow", &rules.allow), ("deny", &rules.deny)] {
        if want.is_empty() {
            continue;
        }
        let have = permissions
            .entry(list)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .with_context(|| format!("`permissions.{list}` is not a JSON array"))?;
        for rule in want {
            if !have.iter().any(|r| r == rule.as_str()) {
                have.push(Value::String(rule.clone()));
                changed = true;
            }
        }
    }
    if !changed {
        return Ok(None);
    }
    let mut out = serde_json::to_string_pretty(&value)?;
    out.push('\n');
    Ok(Some(out))
}

/// The facts of a project that its rules need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The top of the repository, or the directory outside git.
    pub top: PathBuf,
    /// OWNER and REPO of a GitHub `origin`.
    pub repo: Option<(String, String)>,
    /// The default branch: the branch of `origin/HEAD`, else `main`.
    pub branch: String,
}

impl Project {
    /// The project of `dir`.
    pub fn of(dir: &Path) -> Project {
        let top =
            git(dir, &["rev-parse", "--show-toplevel"]).map_or_else(|| dir.into(), Into::into);
        let repo = git(dir, &["remote", "get-url", "origin"])
            .filter(|url| url.contains("github.com"))
            .and_then(|url| parse_remote(&url));
        let branch = git(
            dir,
            &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        )
        .and_then(|b| b.strip_prefix("origin/").map(str::to_owned))
        .unwrap_or_else(|| "main".into());
        Project { top, repo, branch }
    }

    /// The rules of this project ([`rules()`]).
    pub fn rules(&self) -> Rules {
        let repo = self.repo.as_ref().map(|(o, n)| (o.as_str(), n.as_str()));
        rules(repo, &self.branch)
    }

    /// The project settings: `.claude/settings.json` at the top.
    pub fn settings(&self) -> PathBuf {
        self.top.join(".claude/settings.json")
    }

    /// The local settings: `.claude/settings.local.json` at the top.
    pub fn local_settings(&self) -> PathBuf {
        self.top.join(".claude/settings.local.json")
    }

    /// The rules of this project that the user settings at `user`, the
    /// project settings and the local settings do not have.
    ///
    /// ```
    /// let dir = isolated::outside_git();
    /// let project = riff::permissions::Project::of(dir.path());
    /// let user = dir.path().join("user.json");
    /// assert_eq!(project.missing(Some(&user)), project.rules());
    /// riff::permissions::add(&project.settings(), &project.rules())?;
    /// assert!(project.missing(Some(&user)).is_empty());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn missing(&self, user: Option<&Path>) -> Rules {
        let paths = [Some(self.settings()), Some(self.local_settings())];
        let texts: Vec<String> = paths
            .into_iter()
            .chain([user.map(Path::to_owned)])
            .flatten()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .collect();
        let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
        missing(&self.rules(), &texts)
    }
}

/// Adds each rule of `rules` that the settings file at `path` lacks. It
/// makes the file when it is not there, and writes it only when it
/// changes. It returns the rules that it added.
///
/// ```
/// use riff::permissions::{Rules, add};
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join(".claude/settings.json");
/// let rules = Rules { allow: vec!["A".into()], deny: vec![] };
/// assert_eq!(add(&path, &rules)?, rules);
/// assert!(add(&path, &rules)?.is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn add(path: &Path, rules: &Rules) -> Result<Rules> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => "{}".to_owned(),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    let added = missing(rules, &[&text]);
    let Some(new) = with_rules(&text, &added).with_context(|| path.display().to_string())? else {
        return Ok(Rules::default());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("make {}", parent.display()))?;
    }
    std::fs::write(path, new).with_context(|| format!("write {}", path.display()))?;
    Ok(added)
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

    fn git_in(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
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
    fn a_github_clone_gets_its_repo_and_its_default_branch() {
        let dir = tempfile::tempdir().unwrap();
        git_in(dir.path(), &["init", "-q", "-b", "trunk"]);
        git_in(
            dir.path(),
            &["remote", "add", "origin", "git@github.com:acme/app.git"],
        );
        git_in(
            dir.path(),
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/trunk",
            ],
        );
        let sub = dir.path().join("src");
        std::fs::create_dir(&sub).unwrap();
        let project = Project::of(&sub);
        assert_eq!(project.top, dir.path().canonicalize().unwrap());
        assert_eq!(project.repo, Some(("acme".into(), "app".into())));
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

    #[test]
    fn add_keeps_the_other_keys_and_rules_in_their_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"statusLine": {"type": "command"}, "model": "opus",
                "permissions": {"deny": ["Z"], "allow": ["Bash(riff *)", "X"]}}"#,
        )
        .unwrap();
        let rules = rules(None, "main");
        let added = add(&path, &rules).unwrap();
        assert!(!added.allow.contains(&"Bash(riff *)".into()));
        assert_eq!(added.len(), rules.len() - 1);
        let text = std::fs::read_to_string(&path).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["permissions"]["allow"][0], "Bash(riff *)");
        assert_eq!(value["permissions"]["allow"][1], "X");
        assert_eq!(value["permissions"]["deny"][0], "Z");
        let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["statusLine", "model", "permissions"]);
        assert!(text.find("\"deny\"").unwrap() < text.find("\"allow\"").unwrap());
        assert!(add(&path, &rules).unwrap().is_empty());
    }

    #[test]
    fn add_leaves_settings_that_are_not_an_object() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "[1]").unwrap();
        assert!(add(&path, &rules(None, "main")).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[1]");
    }

    #[test]
    fn a_rule_in_the_local_settings_is_not_missing() {
        let dir = isolated::outside_git();
        let project = Project::of(dir.path());
        std::fs::create_dir(dir.path().join(".claude")).unwrap();
        std::fs::write(
            project.local_settings(),
            r#"{"permissions": {"allow": ["Bash(riff *)"]}}"#,
        )
        .unwrap();
        let left = project.missing(None);
        assert!(!left.allow.contains(&"Bash(riff *)".into()));
        assert_eq!(left.len(), project.rules().len() - 1);
    }
}
