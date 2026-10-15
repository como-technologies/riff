//! The Claude Code permission rules of riff work.
//!
//! # Design
//!
//! Claude Code auto mode can block normal riff work: a riff tool, `riff
//! workers start`, a step of the pull request flow, or the riff of a new
//! build. A session cannot add allow rules itself: auto mode blocks a
//! session that edits its own settings. So riff owns the rules of each
//! role in one table, [`TABLE`] (01M5084JXTWZQEG7DJHS2E96NQ). riff writes
//! each rule to its places:
//!
//! | Place | riff writes it with | Into |
//! |---|---|---|
//! | project | `riff setup` (01M3Q53RNDJBDHVDFHJ9HCX9S1) | `.claude/settings.json` at the top of the repository ([`Project::settings`]) |
//! | user | `riff init`, after one question (01M5084K09Z2S7N57315CX2R0F) | the user settings of Claude Code: `$CLAUDE_CONFIG_DIR/settings.json`, else `~/.claude/settings.json` |
//! | session | the riff daemon, for each session (01M5084K4YQCZF20XA0P2485SG) | the `settings.json` of the own Claude folder of the session ([`crate::run::settings`]) |
//!
//! - riff adds only the rules that are missing. It keeps each other rule
//!   and each other key, in its order ([`with_rules`]).
//! - For `riff setup`, a rule counts as there when the user settings,
//!   the project settings or the local settings
//!   (`.claude/settings.local.json`) have it ([`missing`]).
//! - `riff setup --check` and `riff init --check` change nothing. Each
//!   names each missing rule.
//! - The start hook tells the lead when project rules are missing
//!   (01M3Q53RQGXMYVYGCQQMWA9380).
//!
//! ```mermaid
//! flowchart LR
//!     T["TABLE"] --> P["project rules"]
//!     T --> U["user rules"]
//!     T --> S["session rules"]
//!     P -- "riff setup" --> PF[".claude/settings.json<br/>of the repository"]
//!     U -- "riff init" --> UF["~/.claude/settings.json"]
//!     S -- "the daemon" --> SF["settings.json of<br/>each session"]
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
//!
//! # The table
//!
//! Each `riff` and `gh` step of the skill matches a rule of the user
//! place. Each step of the start text of a session of the daemon matches
//! a rule of the session place for its role. A step that the skill
//! forbids matches a deny rule. A step of a person has no rule: the auto
//! mode check judges it. The tests check each step with [`matches()`].
//! `{HOME}` and `{BRANCH}` take the [`Facts`] of the person and of the
//! project.
//!
//! An allow rule names its program and its words. Its only `*` is at its
//! end, after a space (01M509TJNREVNYR9MNN4P9P4RA). A `*` matches each
//! text, also `/` and spaces. So a `*` in the middle of a rule matches
//! each command that has the fixed text after it, in each argument. The
//! steps of the waves of the lead are `riff wave` commands
//! ([`crate::waves`]), so `Bash(riff *)` covers them, and no rule names a
//! path of the API of the forge. The tests check each allow rule, and a
//! list of commands that no allow rule may match.
//!
//! | Rule | Kind | Roles | Places | The step |
//! |---|---|---|---|---|
//! | `mcp__plugin_riff_riff` | allow | lead, worker, verifier | project, user | each riff tool of the plugin |
//! | `mcp__riff` | allow | lead, worker, verifier | project, user, session | each riff tool of a worker, and of a session of the daemon |
//! | `Bash(riff)` | allow | lead, worker, verifier | project, user | `riff` |
//! | `Bash(riff *)` | allow | lead, worker, verifier | project, user | each `riff` command of the skill |
//! | `Bash(~/.cache/riff/check/target/static/release/riff *)` | allow | worker | user | the riff of a new build in a check (01M5084K79M5XJT3MP8FFVQKDX) |
//! | `Bash({HOME}/.cache/riff/check/target/static/release/riff *)` | allow | worker | user | the same riff, by its full path |
//! | `Bash(/opt/riff/riff relay watch)` | allow | lead | session | the wakes of the lead in a container |
//! | `Bash(gh pr create *)` | allow | worker | project, user, session | open the pull request |
//! | `Bash(gh pr merge --auto --squash *)` | allow | worker | project, user, session | turn on auto-merge |
//! | `Bash(gh pr comment *)` | allow | worker, verifier | project, user | write a comment on a pull request |
//! | `Bash(gh pr view *)` | allow | lead, worker, verifier | project, user, session | read a pull request |
//! | `Bash(gh pr checks *)` | allow | verifier | user, session | see the checks of a pull request |
//! | `Bash(gh issue view *)` | allow | lead, worker, verifier | user, session | read an issue |
//! | `Bash(gh issue list *)` | allow | lead, worker, verifier | user, session | find the items of a wave |
//! | `Bash(gh issue create *)` | allow | lead, worker, verifier | user | file an issue |
//! | `Bash(gh issue comment *)` | allow | worker, verifier | user | write the state on an issue |
//! | `Bash(gh issue edit *)` | allow | lead, worker, verifier | user, session | put an item in a wave, write its `Done when:` line |
//! | `Bash(git push * {BRANCH})` | deny | lead, worker, verifier | project | never push to the default branch |
//! | `Bash(git push * {BRANCH} *)` | deny | lead, worker, verifier | project | the same |
//! | `Bash(git push *:{BRANCH})` | deny | lead, worker, verifier | project | the same |
//! | `Bash(git push *:{BRANCH} *)` | deny | lead, worker, verifier | project | the same |
//! | `Bash(git push *:refs/heads/{BRANCH})` | deny | lead, worker, verifier | project | the same |
//! | `Bash(git push *:refs/heads/{BRANCH} *)` | deny | lead, worker, verifier | project | the same |
//! | `Bash(gh pr merge *--admin*)` | deny | lead, worker, verifier | project | never merge as an admin |
//! | `Bash(gh workflow run *)` | person | - | - | an admin makes a release, deploys it again or rolls it back |
//!
//! When the auto mode check refuses a step, the session writes the step
//! on the issue of its item, and tells the lead. The fix is a new row of
//! the table (the step is in bounds), or a change of the skill (it is
//! not). It is never a step that a person types by hand
//! (01M5084K9J566981EXZ7YJRGRR).

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde_json::{Map, Value};

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

/// A role of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The lead of a person in a repository.
    Lead,
    /// A session that does a work item.
    Worker,
    /// A session that verifies the work of another session.
    Verifier,
}

impl Role {
    /// The name of the role in the table.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Lead => "lead",
            Role::Worker => "worker",
            Role::Verifier => "verifier",
        }
    }
}

/// A place where riff writes a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// The project settings: `riff setup` (01M3Q53RNDJBDHVDFHJ9HCX9S1).
    Project,
    /// The user settings of the person: `riff init`
    /// (01M5084K09Z2S7N57315CX2R0F).
    User,
    /// The settings of each session of the daemon
    /// (01M5084K4YQCZF20XA0P2485SG).
    Session,
}

impl Place {
    /// The name of the place in the table.
    pub fn as_str(self) -> &'static str {
        match self {
            Place::Project => "project",
            Place::User => "user",
            Place::Session => "session",
        }
    }
}

/// The kind of a row of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A rule of `permissions.allow`.
    Allow,
    /// A rule of `permissions.deny`.
    Deny,
    /// The step of a person. riff writes no rule, so the auto mode check
    /// judges the step.
    Person,
}

impl Kind {
    /// The name of the kind in the table.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Allow => "allow",
            Kind::Deny => "deny",
            Kind::Person => "person",
        }
    }
}

/// One row of [`TABLE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// The rule, with `{HOME}` or `{BRANCH}` for a fact ([`Facts`]).
    pub rule: &'static str,
    /// Allow, deny, or the step of a person.
    pub kind: Kind,
    /// The roles whose steps need the rule.
    pub roles: &'static [Role],
    /// Where riff writes the rule.
    pub places: &'static [Place],
}

const ALL: &[Role] = &[Role::Lead, Role::Worker, Role::Verifier];

const fn allow(rule: &'static str, roles: &'static [Role], places: &'static [Place]) -> Row {
    Row {
        rule,
        kind: Kind::Allow,
        roles,
        places,
    }
}

const fn deny(rule: &'static str) -> Row {
    Row {
        rule,
        kind: Kind::Deny,
        roles: ALL,
        places: &[Place::Project],
    }
}

/// The permission rules of each role (01M5084JXTWZQEG7DJHS2E96NQ). The
/// table in the design of this module shows each row, with the step that
/// needs it.
pub const TABLE: &[Row] = {
    use Place::{Project, Session, User};
    use Role::{Lead, Verifier, Worker};
    &[
        allow("mcp__plugin_riff_riff", ALL, &[Project, User]),
        allow("mcp__riff", ALL, &[Project, User, Session]),
        allow("Bash(riff)", ALL, &[Project, User]),
        allow("Bash(riff *)", ALL, &[Project, User]),
        allow(
            "Bash(~/.cache/riff/check/target/static/release/riff *)",
            &[Worker],
            &[User],
        ),
        allow(
            "Bash({HOME}/.cache/riff/check/target/static/release/riff *)",
            &[Worker],
            &[User],
        ),
        allow("Bash(/opt/riff/riff relay watch)", &[Lead], &[Session]),
        allow("Bash(gh pr create *)", &[Worker], &[Project, User, Session]),
        allow(
            "Bash(gh pr merge --auto --squash *)",
            &[Worker],
            &[Project, User, Session],
        ),
        allow(
            "Bash(gh pr comment *)",
            &[Worker, Verifier],
            &[Project, User],
        ),
        allow("Bash(gh pr view *)", ALL, &[Project, User, Session]),
        allow("Bash(gh pr checks *)", &[Verifier], &[User, Session]),
        allow("Bash(gh issue view *)", ALL, &[User, Session]),
        allow("Bash(gh issue list *)", ALL, &[User, Session]),
        allow("Bash(gh issue create *)", ALL, &[User]),
        allow("Bash(gh issue comment *)", &[Worker, Verifier], &[User]),
        allow("Bash(gh issue edit *)", ALL, &[User, Session]),
        deny("Bash(git push * {BRANCH})"),
        deny("Bash(git push * {BRANCH} *)"),
        deny("Bash(git push *:{BRANCH})"),
        deny("Bash(git push *:{BRANCH} *)"),
        deny("Bash(git push *:refs/heads/{BRANCH})"),
        deny("Bash(git push *:refs/heads/{BRANCH} *)"),
        deny("Bash(gh pr merge *--admin*)"),
        Row {
            rule: "Bash(gh workflow run *)",
            kind: Kind::Person,
            roles: &[],
            places: &[],
        },
    ]
};

/// The facts that the rules of [`TABLE`] take.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Facts<'a> {
    /// The home of the person: `{HOME}`.
    pub home: Option<&'a Path>,
    /// The default branch: `{BRANCH}`.
    pub branch: Option<&'a str>,
}

impl Row {
    /// The rule with the facts `facts`, or `None` when it needs a fact
    /// that `facts` does not have.
    ///
    /// ```
    /// use riff::permissions::{Facts, TABLE};
    ///
    /// let row = TABLE.iter().find(|r| r.rule.contains("{HOME}")).unwrap();
    /// assert_eq!(row.rule_of(&Facts::default()), None);
    /// let facts = Facts { home: Some("/home/mike".as_ref()), ..Facts::default() };
    /// assert_eq!(
    ///     row.rule_of(&facts).unwrap(),
    ///     "Bash(/home/mike/.cache/riff/check/target/static/release/riff *)"
    /// );
    /// ```
    pub fn rule_of(&self, facts: &Facts) -> Option<String> {
        let home = facts.home.map(|h| h.display().to_string());
        let values = [("{HOME}", home.as_deref()), ("{BRANCH}", facts.branch)];
        let mut rule = self.rule.to_owned();
        for (key, value) in values {
            if rule.contains(key) {
                rule = rule.replace(key, value?);
            }
        }
        Some(rule)
    }
}

/// The rules of [`TABLE`] for the place `place`, with the facts `facts`.
/// It leaves out a rule that needs a fact that `facts` does not have.
///
/// ```
/// use riff::permissions::{Facts, Place, rules_of};
///
/// let session = rules_of(Place::Session, &Facts::default());
/// assert!(session.allow.contains(&"Bash(gh issue view *)".to_owned()));
/// assert!(!session.allow.contains(&"Bash(riff *)".to_owned()));
/// assert!(session.deny.is_empty());
/// ```
pub fn rules_of(place: Place, facts: &Facts) -> Rules {
    let mut rules = Rules::default();
    for row in TABLE.iter().filter(|r| r.places.contains(&place)) {
        let Some(rule) = row.rule_of(facts) else {
            continue;
        };
        match row.kind {
            Kind::Allow => rules.allow.push(rule),
            Kind::Deny => rules.deny.push(rule),
            Kind::Person => {}
        }
    }
    rules
}

/// The rules of riff work for a repository with the default branch
/// `branch`: the project place of [`TABLE`] (01M3Q53RNDJBDHVDFHJ9HCX9S1).
///
/// - Allow: each riff tool (`mcp__plugin_riff_riff` from the plugin,
///   `mcp__riff` in a worker), each `riff` command, and the steps of the
///   pull request flow.
/// - Deny: a push to `branch`, and `gh pr merge --admin`.
///
/// ```
/// let rules = riff::permissions::rules("trunk");
/// assert!(rules.allow.contains(&"mcp__plugin_riff_riff".to_owned()));
/// assert!(rules.allow.contains(&"mcp__riff".to_owned()));
/// assert!(rules.allow.contains(&"Bash(riff)".to_owned()));
/// assert!(rules.allow.contains(&"Bash(gh pr merge --auto --squash *)".to_owned()));
/// assert!(rules.deny.contains(&"Bash(git push * trunk)".to_owned()));
/// assert!(rules.deny.contains(&"Bash(gh pr merge *--admin*)".to_owned()));
/// assert!(!rules.allow.iter().any(|r| r.contains("gh api")));
/// ```
pub fn rules(branch: &str) -> Rules {
    let facts = Facts {
        branch: Some(branch),
        ..Facts::default()
    };
    rules_of(Place::Project, &facts)
}

/// The rules that `riff init` writes into the user settings of Claude
/// Code, for the person with the home `home` (01M5084K09Z2S7N57315CX2R0F).
/// They name no repository and deny nothing, so they hold in each
/// project of the person.
///
/// ```
/// let rules = riff::permissions::user_rules(Some("/home/mike".as_ref()));
/// assert!(rules.allow.contains(&"Bash(gh issue list *)".to_owned()));
/// let check = "Bash(~/.cache/riff/check/target/static/release/riff *)";
/// assert!(rules.allow.contains(&check.to_owned()));
/// let full = "Bash(/home/mike/.cache/riff/check/target/static/release/riff *)";
/// assert!(rules.allow.contains(&full.to_owned()));
/// assert!(rules.deny.is_empty());
/// assert!(!riff::permissions::user_rules(None).allow.iter().any(|r| r.contains("/home/")));
/// ```
pub fn user_rules(home: Option<&Path>) -> Rules {
    let facts = Facts {
        home,
        ..Facts::default()
    };
    rules_of(Place::User, &facts)
}

/// The rules that the daemon writes into the settings of each session
/// (01M5084K4YQCZF20XA0P2485SG).
///
/// ```
/// let rules = riff::permissions::session_rules();
/// assert!(rules.allow.contains(&"mcp__riff".to_owned()));
/// assert!(rules.allow.contains(&"Bash(gh pr checks *)".to_owned()));
/// ```
pub fn session_rules() -> Rules {
    rules_of(Place::Session, &Facts::default())
}

/// True when the rule `rule` of Claude Code matches the Bash command
/// `command`. It is the model of the match of Claude Code that the tests
/// use: a rule `Bash(PATTERN)` matches the whole command, and each `*` in
/// PATTERN matches each text, also no text. Each other rule matches no
/// command.
///
/// ```
/// use riff::permissions::matches;
///
/// assert!(matches("Bash(riff *)", "riff workers start 2"));
/// assert!(!matches("Bash(riff *)", "riff"));
/// assert!(matches("Bash(riff)", "riff"));
/// assert!(matches("Bash(gh pr view *)", "gh pr view 40 --json state"));
/// assert!(matches("Bash(git push * main)", "git push origin main"));
/// assert!(matches("Bash(gh pr merge *--admin*)", "gh pr merge --admin"));
/// assert!(!matches("Bash(gh issue view *)", "gh issue list"));
/// assert!(!matches("mcp__riff", "mcp__riff"));
/// ```
pub fn matches(rule: &str, command: &str) -> bool {
    let Some(pattern) = rule.strip_prefix("Bash(").and_then(|r| r.strip_suffix(')')) else {
        return false;
    };
    let (pattern, text) = (pattern.as_bytes(), command.as_bytes());
    let (mut p, mut t) = (0, 0);
    // The last `*` and the place in the text where it started.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if pattern.get(p) == Some(&b'*') {
            star = Some((p, t));
            p += 1;
        } else if pattern.get(p) == Some(&text[t]) {
            p += 1;
            t += 1;
        } else if let Some((s, from)) = star {
            star = Some((s, from + 1));
            p = s + 1;
            t = from + 1;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
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
    /// The default branch: the branch of `origin/HEAD`, else `main`.
    pub branch: String,
}

impl Project {
    /// The project of `dir`.
    pub fn of(dir: &Path) -> Project {
        let top =
            git(dir, &["rev-parse", "--show-toplevel"]).map_or_else(|| dir.into(), Into::into);
        let branch = git(
            dir,
            &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        )
        .and_then(|b| b.strip_prefix("origin/").map(str::to_owned))
        .unwrap_or_else(|| "main".into());
        Project { top, branch }
    }

    /// The rules of this project ([`rules()`]).
    pub fn rules(&self) -> Rules {
        rules(&self.branch)
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
    write(path, &new)?;
    Ok(added)
}

/// Writes `text` into the settings file at `path`. It writes a new file
/// in the same folder and renames it, so a process that reads the file
/// at the same time sees the old text or the new text, never a part. The
/// file keeps its mode. A settings file that is a symbolic link stays a
/// link: riff writes the file that it names (01M509TK01PW3RN2YPRJD38RSX).
fn write(path: &Path, text: &str) -> Result<()> {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).with_context(|| format!("make {}", dir.display()))?;
    let fail = || format!("write {}", path.display());
    let mut new = tempfile::NamedTempFile::new_in(dir).with_context(fail)?;
    new.write_all(text.as_bytes()).with_context(fail)?;
    if let Ok(old) = std::fs::metadata(&path) {
        new.as_file()
            .set_permissions(old.permissions())
            .with_context(fail)?;
    }
    new.persist(&path).map_err(|e| e.error).with_context(fail)?;
    Ok(())
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
    fn a_clone_gets_its_default_branch() {
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
        assert_eq!(project.branch, "trunk");
        assert!(
            project
                .rules()
                .deny
                .contains(&"Bash(git push * trunk)".into())
        );
    }

    #[test]
    fn a_directory_outside_git_uses_main() {
        let dir = isolated::outside_git();
        let project = Project::of(dir.path());
        assert_eq!(project.top, dir.path());
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
        let rules = rules("main");
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
        assert!(add(&path, &rules("main")).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[1]");
    }

    /// 01M509TK01PW3RN2YPRJD38RSX.
    #[test]
    fn add_keeps_a_link_and_the_mode_of_the_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dotfiles/settings.json");
        std::fs::create_dir(dir.path().join("dotfiles")).unwrap();
        std::fs::write(&target, r#"{"model": "opus"}"#).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        let link = dir.path().join("settings.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let rules = rules("main");
        assert_eq!(add(&link, &rules).unwrap(), rules);
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(value["model"], "opus");
        assert_eq!(missing(&rules, &[&value.to_string()]), Rules::default());
        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
        // No temp file stays in the folder.
        assert_eq!(std::fs::read_dir(dir.path().join("dotfiles")).unwrap().count(), 1);
    }

    /// True when the allow rule `rule` names its program and its words,
    /// with its only `*` at its end, after a space. A rule that is not a
    /// `Bash` rule names a tool: it has no `*`.
    fn star_only_at_the_end(rule: &str) -> bool {
        let pattern = rule
            .strip_prefix("Bash(")
            .and_then(|r| r.strip_suffix(')'))
            .unwrap_or(rule);
        match pattern.matches('*').count() {
            0 => true,
            1 => pattern.ends_with(" *") && pattern.len() > 2,
            _ => false,
        }
    }

    /// 01M509TJNREVNYR9MNN4P9P4RA.
    #[test]
    fn each_allow_rule_has_its_only_star_at_its_end_after_a_space() {
        for row in TABLE.iter().filter(|r| r.kind == Kind::Allow) {
            assert!(star_only_at_the_end(row.rule), "{row:?}");
        }
        // The four rules of the failed verify of #776 are too wide.
        for rule in [
            "Bash(~/.cache/riff/checks/*/riff *)",
            "Bash(gh api repos/*/milestones *)",
            "Bash(gh api -X PATCH repos/*/milestones/*)",
            "Bash(gh pr merge * --auto --squash)",
            "Bash(gh api repos/o/r/statuses/*)",
            "Bash(gh pr view*)",
            "Bash(*)",
        ] {
            assert!(!star_only_at_the_end(rule), "{rule}");
        }
    }

    /// 01M509TJNREVNYR9MNN4P9P4RA: no allow rule of a place matches a
    /// command that no step of a role needs.
    #[test]
    fn no_allow_rule_matches_a_command_out_of_bounds() {
        let facts = Facts {
            home: Some(Path::new("/home/mike")),
            branch: Some("main"),
        };
        let out_of_bounds = [
            "~/.cache/riff/check/other x/riff y",
            "~/.cache/riff/checks/x/target/static/release/riff daemon install",
            "/home/mike/.cache/riff/check/target/static/release/riffx daemon install",
            "/home/mike/.cache/riff/check/../../../../bin/sh -c x riff y",
            "gh api repos/o/r/milestones --jq .",
            "gh api repos/o/r/issues -f title=x/milestones y",
            "gh api -X PATCH repos/o/r -f description=x/milestones/y",
            "gh api -X PATCH repos/o/r/milestones/3 -f state=closed",
            "gh api -X POST repos/o/r/statuses/abc -f state=success",
            "gh api -X PUT repos/o/r/rulesets/1 -f enforcement=disabled",
            "gh pr merge 40 --squash",
            "gh pr merge 40 --admin --auto --squash",
            "gh pr merge --admin 40",
            "gh workflow run CI --ref main -f tag=v1.2.3",
            "gh repo delete o/r --yes",
            "gh issue delete 12 --yes",
            "git push origin main",
            "riffx who",
            "/opt/riff/riff relay mcp",
            "sh -c 'riff who'",
        ];
        for place in [Place::Project, Place::User, Place::Session] {
            let allow = rules_of(place, &facts).allow;
            for command in out_of_bounds {
                let rule = allow.iter().find(|r| matches(r, command));
                assert_eq!(rule, None, "{} allows `{command}`", place.as_str());
            }
        }
    }

    /// The project settings of this repository have each project rule,
    /// and each of their allow rules has its only `*` at its end
    /// (01M3Q53RNDJBDHVDFHJ9HCX9S1, 01M509TJNREVNYR9MNN4P9P4RA).
    #[test]
    fn the_project_settings_of_riff_have_the_rules_and_no_wide_rule() {
        let text = include_str!("../../../.claude/settings.json");
        assert_eq!(missing(&rules("main"), &[text]), Rules::default());
        let value: Value = serde_json::from_str(text).unwrap();
        for rule in value["permissions"]["allow"].as_array().unwrap() {
            let rule = rule.as_str().unwrap();
            assert!(star_only_at_the_end(rule), "{rule}");
        }
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

    /// The names of `names`, or `-` when there is none.
    fn names<T: Copy>(names: &[T], name: fn(T) -> &'static str) -> String {
        if names.is_empty() {
            return "-".into();
        }
        names
            .iter()
            .map(|n| name(*n))
            .collect::<Vec<_>>()
            .join(", ")
    }

    #[test]
    fn the_table_of_the_design_shows_each_row_in_order() {
        let source = include_str!("permissions.rs");
        let lines: Vec<&str> = source
            .lines()
            .filter_map(|l| l.strip_prefix("//! | `"))
            .collect();
        assert_eq!(lines.len(), TABLE.len(), "{lines:#?}");
        for (row, line) in TABLE.iter().zip(lines) {
            let want = format!(
                "{}` | {} | {} | {} | ",
                row.rule,
                row.kind.as_str(),
                names(row.roles, Role::as_str),
                names(row.places, Place::as_str),
            );
            assert!(line.starts_with(&want), "{line}\nis not\n{want}");
        }
    }

    #[test]
    fn a_row_of_a_person_has_no_place_and_each_other_row_has_one() {
        for row in TABLE {
            assert_eq!(row.kind == Kind::Person, row.places.is_empty(), "{row:?}");
            assert_eq!(row.kind == Kind::Person, row.roles.is_empty(), "{row:?}");
        }
    }

    /// True when `text` is a command with one word or more after its
    /// first word: `riff`, `gh`, or the path of a riff. A code span with
    /// only `gh` or `riff` names the tool: it is no step.
    fn is_step(text: &str) -> bool {
        let mut words = text.split_whitespace();
        let first = words.next().unwrap_or_default();
        let command = first == "riff" || first == "gh" || first.ends_with("/riff");
        command && words.next().is_some()
    }

    /// [`is_step`] for plain text. There `riff` at the start of a
    /// sentence is a word of the prose: a session in a container calls
    /// its riff by the path.
    fn is_plain_step(text: &str) -> bool {
        is_step(text) && !text.starts_with("riff ")
    }

    /// Each `riff` and `gh` step of the markdown `text`: each code span,
    /// and each line of a code block.
    fn skill_steps(text: &str) -> Vec<String> {
        let mut steps = Vec::new();
        let mut prose = String::new();
        let mut block = false;
        for line in text.lines() {
            if line.trim_start().starts_with("```") {
                block = !block;
            } else if block {
                steps.push(line.trim().to_owned());
            } else {
                prose.push_str(line);
                prose.push('\n');
            }
        }
        let spans = prose.split('`').skip(1).step_by(2);
        steps.extend(spans.map(|s| s.split_whitespace().collect::<Vec<_>>().join(" ")));
        steps.retain(|s| is_step(s));
        steps.sort();
        steps.dedup();
        steps
    }

    /// Each `riff` and `gh` step of the plain text `text`: each code span,
    /// each line that starts with a step, and each step after `: ` up to
    /// the next comma.
    fn text_steps(text: &str) -> Vec<String> {
        let mut steps: Vec<String> = text
            .split('`')
            .skip(1)
            .step_by(2)
            .filter(|s| is_step(s))
            .map(Into::into)
            .collect();
        for line in text.lines().map(str::trim) {
            let after = line.split_once(": ").map(|(_, rest)| rest);
            for part in [Some(line), after].into_iter().flatten() {
                let part = part.split(',').next().unwrap_or_default().trim();
                if is_plain_step(part) {
                    steps.push(part.to_owned());
                }
            }
        }
        steps.sort();
        steps.dedup();
        steps
    }

    #[test]
    fn the_steps_of_a_text_are_its_commands() {
        assert_eq!(
            skill_steps(
                "Run `riff who`, or\n`riff\n  workers`, a wrapper of `gh`.\n```sh\ngh issue \
                 list\n```\n`git log`"
            ),
            ["gh issue list", "riff who", "riff workers"]
        );
        assert_eq!(
            text_steps(
                "1. Read it: gh pr view 4 --comments, and git diff\n   gh pr checks 4\nriff does \
                 it, `/opt/riff/riff relay watch` or `riff who`"
            ),
            [
                "/opt/riff/riff relay watch",
                "gh pr checks 4",
                "gh pr view 4 --comments",
                "riff who"
            ]
        );
    }

    /// 01M5084JXTWZQEG7DJHS2E96NQ: each `riff` and `gh` step of the skill
    /// matches a rule of the user place, a deny rule (the skill forbids
    /// the step), or the row of a person.
    #[test]
    fn each_step_of_the_skill_matches_a_user_rule() {
        let (_, skill) = crate::plugin::FILES
            .iter()
            .find(|(path, _)| path.ends_with("SKILL.md"))
            .unwrap();
        let home = Path::new("/home/mike");
        let user = user_rules(Some(home));
        let deny = rules("main").deny;
        let person: Vec<&str> = TABLE
            .iter()
            .filter(|r| r.kind == Kind::Person)
            .map(|r| r.rule)
            .collect();
        let steps = skill_steps(skill);
        assert!(steps.len() > 40, "{steps:#?}");
        // The check of a new build is in the skill (01M5084K79M5XJT3MP8FFVQKDX).
        assert!(
            steps
                .iter()
                .any(|s| s.starts_with("~/.cache/riff/check/target/static/release/riff ")),
            "{steps:#?}"
        );

        for step in &steps {
            let found = user.allow.iter().any(|r| matches(r, step))
                || deny.iter().any(|r| matches(r, step))
                || person.iter().any(|r| matches(r, step));
            assert!(found, "no rule of the table matches the step `{step}`");
        }
    }

    /// 01M5084JXTWZQEG7DJHS2E96NQ: each `riff` and `gh` step of the start
    /// text of a session of the daemon matches a rule of the session
    /// place for the role of the session.
    #[test]
    fn each_step_of_a_session_of_the_daemon_matches_a_session_rule_of_its_role() {
        use crate::text;
        let frame = "Closes #12\n\nSUMMARY\n\nIssue: #12\nMilestone: Wave 3";
        let texts = [
            (
                Role::Worker,
                text::run_start(
                    "o/r",
                    12,
                    "worktree-issue-12",
                    "1a2b",
                    frame,
                    "Wave 3",
                    true,
                ),
            ),
            (
                Role::Worker,
                text::wave_failed_verify(40, "1a2b3c4d", "https://c"),
            ),
            (
                Role::Verifier,
                text::wave_verify_start("o/r", 12, 40, "1a2b3c4d"),
            ),
            (Role::Lead, text::MCP_LEAD.to_owned()),
        ];
        for (role, text) in texts {
            let rules: Vec<String> = TABLE
                .iter()
                .filter(|r| r.places.contains(&Place::Session) && r.roles.contains(&role))
                .filter_map(|r| r.rule_of(&Facts::default()))
                .collect();
            let steps = text_steps(&text);
            assert!(!steps.is_empty(), "{text}");
            for step in &steps {
                assert!(
                    rules.iter().any(|r| matches(r, step)),
                    "no session rule of the {} matches the step `{step}`",
                    role.as_str()
                );
            }
        }
    }
}
