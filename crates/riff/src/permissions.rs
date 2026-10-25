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
//! | session | the riff daemon, for each session (01M5084K4YQCZF20XA0P2485SG) | the `settings.json` of the own Claude folder of the session ([`crate::run::settings`]) |
//!
//! - riff adds only the rules that are missing. It keeps each other rule
//!   and each other key, in its order ([`with_rules`]).
//! - For `riff setup`, a rule counts as there when the user settings,
//!   the project settings or the local settings
//!   (`.claude/settings.local.json`) have it ([`missing`]).
//! - `riff setup --check` changes nothing. It names each missing rule.
//! - The start hook tells the lead when project rules are missing
//!   (01M3Q53RQGXMYVYGCQQMWA9380).
//!
//! ```mermaid
//! flowchart LR
//!     T["TABLE"] --> P["project rules"]
//!     T --> S["session rules"]
//!     P -- "riff setup" --> PF[".claude/settings.json<br/>of the repository"]
//!     S -- "the daemon" --> SF["settings.json of<br/>each session"]
//! ```
//!
//! ```
//! use riff::permissions::{rules, with_rules};
//!
//! let rules = rules("main");
//! let text = with_rules("{}", &rules)?.unwrap();
//! assert!(text.contains("\"Bash(riff *)\""));
//! assert!(text.contains("\"Bash(git push * main)\""));
//! assert_eq!(with_rules(&text, &rules)?, None);
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! # The table
//!
//! Each step of the start text of a session of the daemon matches
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
//! path of the API of the forge. The lead in a container calls its riff
//! by the path `/opt/riff/riff`, so a session rule of the lead covers its
//! `wave` commands (01M51K1VJ67KYHRD7DQ86DSX1X). The tests check each
//! allow rule, and a list of commands that no allow rule may match.
//!
//! Claude Code matches a rule to each part of a Bash call. A `cd` out of
//! the working folders matches no rule, and then the auto mode check
//! judges the whole call. So a session runs each step alone, with each
//! file by its absolute path, and the skill says so. A test checks that
//! no code line of the skill joins a step to another command
//! (01M51V0Q5E7MNPWP9T7TSJFT5W).
//!
//! | Rule | Kind | Roles | Places | The step |
//! |---|---|---|---|---|
//! | `mcp__riff` | allow | lead, worker, verifier | project, session | each riff tool of a worker, and of a session of the daemon |
//! | `Bash(riff)` | allow | lead, worker, verifier | project | `riff` |
//! | `Bash(riff *)` | allow | lead, worker, verifier | project | each `riff` command of the skill |
//! | `Bash(/opt/riff/riff relay watch)` | allow | lead | session | the wakes of the lead in a container |
//! | `Bash(/opt/riff/riff relay watch --follow)` | allow | lead | session | the wakes of the lead, one watch that never ends |
//! | `Bash(/opt/riff/riff wave *)` | allow | lead | session | the steps of the waves of the lead in a container |
//! | `Bash(/opt/riff/riff wave move *)` | allow | lead | session | the same, in a container |
//! | `Bash(/opt/riff/riff wave end *)` | allow | lead | session | the same, in a container |
//! | `Bash(/opt/riff/riff release status)` | allow | lead | session | the state of the release cycle, in a container |
//! | `Bash(/opt/riff/riff release cut *)` | allow | lead | session | the release pull request, in a container |
//! | `Bash(gh pr create *)` | allow | worker | project, session | open the pull request |
//! | `Bash(gh pr merge --auto --squash *)` | allow | worker | project, session | turn on auto-merge |
//! | `Bash(gh pr comment *)` | allow | worker, verifier | project | write a comment on a pull request |
//! | `Bash(gh pr view *)` | allow | lead, worker, verifier | project, session | read a pull request |
//! | `Bash(gh pr checks *)` | allow | worker, verifier | session | see the checks of a pull request |
//! | `Bash(gh run view *)` | allow | worker | session | read the log of a failed run of a check, to fix it (01M5QWJJBY5VW53F0XAWWDQ79K) |
//! | `Bash(gh run rerun *)` | allow | worker | session | run a job again that failed for a known flake (01M5QWJJBY5VW53F0XAWWDQ79K) |
//! | `Bash(gh issue view *)` | allow | lead, worker, verifier | session | read an issue |
//! | `Bash(gh issue list *)` | allow | lead, worker, verifier | session | find the items of a wave |
//! | `Bash(gh issue edit *)` | allow | lead, worker, verifier | session | write the `Done when:` line of an item |
//! | `Bash(git push * {BRANCH})` | deny | lead, worker, verifier | project | never push to the default branch |
//! | `Bash(git push * {BRANCH} *)` | deny | lead, worker, verifier | project | the same |
//! | `Bash(git push *:{BRANCH})` | deny | lead, worker, verifier | project | the same |
//! | `Bash(git push *:{BRANCH} *)` | deny | lead, worker, verifier | project | the same |
//! | `Bash(git push *:refs/heads/{BRANCH})` | deny | lead, worker, verifier | project | the same |
//! | `Bash(git push *:refs/heads/{BRANCH} *)` | deny | lead, worker, verifier | project | the same |
//! | `Bash(gh pr merge *--admin*)` | deny | lead, worker, verifier | project | never merge as an admin |
//! | `Bash(gh workflow run *)` | person | - | - | only by hand: `riff release rollback` deploys an older release |
//! | `Bash(riff release approve *)` | person | - | - | the approval of a deploy: no rule, the guard of riff refuses each session |
//!
//! When the auto mode check refuses a step, the session writes the step
//! on the issue of its item, and tells the lead. The fix is a new row of
//! the table (the step is in bounds), or a change of the skill (it is
//! not). It is never a step that a person types by hand
//! (01M5084K9J566981EXZ7YJRGRR).
//!
//! # Wide rules
//!
//! riff adds rules, and never removes one with no yes. A person can also
//! add an allow rule by hand, and that rule can be much wider than its
//! step. So `riff init` reads the allow rules of the user settings, the
//! project settings and the local settings
//! ([`crate::first_run::Wide`]). A rule is [`wide`] when it is a rule of
//! the Bash tool, and (01M51JCW04FAHZDQ144KDNCCPX):
//!
//! - it has no pattern: `Bash` allows each command;
//! - or it has a `*` that is not at its end after a space;
//! - or it matches a command of [`OUT_OF_BOUNDS`], for example
//!   `Bash(gh api *)` or `Bash(git push *)`.
//!
//! The old end `:*` of a pattern is ` *`. A rule of another tool is
//! never wide. riff removes the wide rules only after a yes to the
//! question of `riff init` ([`remove`], 01M51JCW2EKF8H8QFQYSVRSPET).
//!
//! ```
//! use riff::permissions::{Facts, remove, wide_rules};
//!
//! let dir = tempfile::tempdir()?;
//! let path = dir.path().join("settings.json");
//! let text = r#"{"permissions": {"allow": ["Bash(riff *)", "Bash(gh pr merge * --auto --squash)"]}}"#;
//! std::fs::write(&path, text)?;
//! let wide = wide_rules(text, &Facts::default());
//! assert_eq!(wide.allow, ["Bash(gh pr merge * --auto --squash)"]);
//! assert_eq!(remove(&path, &wide)?, wide);
//! let after = std::fs::read_to_string(&path)?;
//! assert!(wide_rules(&after, &Facts::default()).is_empty());
//! assert!(after.contains("\"Bash(riff *)\""));
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! # Auto mode
//!
//! When auto mode blocks several steps and the settings have no
//! `autoMode.environment` entries, Claude Code shows the dialog "Teach
//! auto mode about your environment?" at the end of a turn. No person
//! answers it in a worker, and a wake that riff types into the pane can
//! pick a choice. So riff gives each session that it starts the settings
//! of [`auto_mode`] (01M516HETNZCBFBJNHT8BKFB1Y):
//!
//! - `skillOverrides` turns off `/auto-mode-setup`, and with it the
//!   dialog.
//! - `autoMode.environment` keeps the entries of Claude Code
//!   (`"$defaults"`), and adds the places of riff work ([`environment`]).
//!   So auto mode blocks fewer normal steps.
//!
//! Claude Code reads `autoMode` only from the user settings, the managed
//! settings and `--settings`, not from the project settings. So riff
//! gives the settings in these places (01M516HEX3GVTN9K2QS12A4PG3):
//!
//! ```mermaid
//! flowchart LR
//!     A["auto_mode<br/>owner, server"] -- "riff workers start" --> W["--settings of<br/>each worker"]
//!     A -- "the daemon" --> S["settings.json of<br/>each session"]
//!     A -- "riff lead start" --> L["settings.json of<br/>the lead: missing entries"]
//! ```
//!
//! ```
//! use riff::permissions::{Trust, auto_mode};
//!
//! let trust = Trust { owner: Some("acme"), server: Some("https://riff.acme.dev") };
//! let settings = auto_mode(&trust);
//! assert_eq!(settings["skillOverrides"]["auto-mode-setup"], "off");
//! let environment = settings["autoMode"]["environment"].as_array().unwrap();
//! assert_eq!(environment[0], "$defaults");
//! assert!(environment.iter().any(|e| e.as_str().unwrap().contains("github.com/acme ")));
//! ```

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use riff_core::name::Repo;
use serde_json::{Map, Value, json};

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
    /// The settings of each session of the daemon
    /// (01M5084K4YQCZF20XA0P2485SG).
    Session,
}

impl Place {
    /// The name of the place in the table.
    pub fn as_str(self) -> &'static str {
        match self {
            Place::Project => "project",
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
    use Place::{Project, Session};
    use Role::{Lead, Verifier, Worker};
    &[
        allow("mcp__riff", ALL, &[Project, Session]),
        allow("Bash(riff)", ALL, &[Project]),
        allow("Bash(riff *)", ALL, &[Project]),
        allow("Bash(/opt/riff/riff relay watch)", &[Lead], &[Session]),
        allow(
            "Bash(/opt/riff/riff relay watch --follow)",
            &[Lead],
            &[Session],
        ),
        allow("Bash(/opt/riff/riff wave *)", &[Lead], &[Session]),
        allow("Bash(/opt/riff/riff wave move *)", &[Lead], &[Session]),
        allow("Bash(/opt/riff/riff wave end *)", &[Lead], &[Session]),
        allow("Bash(/opt/riff/riff release status)", &[Lead], &[Session]),
        allow("Bash(/opt/riff/riff release cut *)", &[Lead], &[Session]),
        allow("Bash(gh pr create *)", &[Worker], &[Project, Session]),
        allow(
            "Bash(gh pr merge --auto --squash *)",
            &[Worker],
            &[Project, Session],
        ),
        allow("Bash(gh pr comment *)", &[Worker, Verifier], &[Project]),
        allow("Bash(gh pr view *)", ALL, &[Project, Session]),
        allow("Bash(gh pr checks *)", &[Worker, Verifier], &[Session]),
        allow("Bash(gh run view *)", &[Worker], &[Session]),
        allow("Bash(gh run rerun *)", &[Worker], &[Session]),
        allow("Bash(gh issue view *)", ALL, &[Session]),
        allow("Bash(gh issue list *)", ALL, &[Session]),
        allow("Bash(gh issue edit *)", ALL, &[Session]),
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
        Row {
            rule: "Bash(riff release approve *)",
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

/// The text `text` with the facts `facts` in place of `{HOME}` and
/// `{BRANCH}`, or `None` when it needs a fact that `facts` does not have.
fn fill(text: &str, facts: &Facts) -> Option<String> {
    let home = facts.home.map(|h| h.display().to_string());
    let values = [("{HOME}", home.as_deref()), ("{BRANCH}", facts.branch)];
    let mut text = text.to_owned();
    for (key, value) in values {
        if text.contains(key) {
            text = text.replace(key, value?);
        }
    }
    Some(text)
}

impl Row {
    /// The rule with the facts `facts`, or `None` when it needs a fact
    /// that `facts` does not have.
    ///
    /// ```
    /// use riff::permissions::{Facts, TABLE};
    ///
    /// let row = TABLE.iter().find(|r| r.rule == "Bash(git push * {BRANCH})").unwrap();
    /// assert_eq!(row.rule_of(&Facts::default()), None);
    /// let facts = Facts { branch: Some("main"), ..Facts::default() };
    /// assert_eq!(row.rule_of(&facts).unwrap(), "Bash(git push * main)");
    /// ```
    pub fn rule_of(&self, facts: &Facts) -> Option<String> {
        fill(self.rule, facts)
    }
}

/// The commands that no allow rule may match: no step of a role needs
/// them (01M509TJNREVNYR9MNN4P9P4RA). `{HOME}` takes the home of the
/// person ([`Facts`]). An allow rule that matches one of them is
/// [`wide`].
pub const OUT_OF_BOUNDS: &[&str] = &[
    "~/.cache/riff/check/other x/riff y",
    "~/.cache/riff/checks/x/target/static/release/riff daemon install",
    "{HOME}/.cache/riff/check/target/static/release/riffx daemon install",
    "{HOME}/.cache/riff/check/../../../../bin/sh -c x riff y",
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
    "/opt/riff/riff waves x",
    "/opt/riff/riff daemon install",
    "sh -c 'riff who'",
];

/// The commands of [`OUT_OF_BOUNDS`] with the facts `facts`. It leaves
/// out a command that needs a fact that `facts` does not have.
fn out_of_bounds(facts: &Facts) -> Vec<String> {
    OUT_OF_BOUNDS
        .iter()
        .filter_map(|c| fill(c, facts))
        .collect()
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
/// - Allow: each riff tool
///   (`mcp__riff` in a session), each `riff` command, and the steps of the
///   pull request flow.
/// - Deny: a push to `branch`, and `gh pr merge --admin`.
///
/// ```
/// let rules = riff::permissions::rules("trunk");
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

/// The skill of Claude Code that offers the dialog "Teach auto mode
/// about your environment?".
pub const AUTO_MODE_SETUP: &str = "auto-mode-setup";

/// The facts that the auto mode settings of a session take.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Trust<'a> {
    /// The owner of the repository on GitHub, for example
    /// `como-technologies`.
    pub owner: Option<&'a str>,
    /// The riff server of the session.
    pub server: Option<&'a str>,
}

impl<'a> Trust<'a> {
    /// The facts of a session in `repo` that talks to the riff server
    /// `server`. A repository with no `origin` remote has the owner
    /// `local` ([`crate::identity::place`]): it has no owner on GitHub.
    /// An empty `server` names no riff server.
    ///
    /// ```
    /// use riff::permissions::Trust;
    /// use riff_core::name::Repo;
    ///
    /// let repo = Repo::Git { owner: "acme".into(), name: "app".into() };
    /// let trust = Trust::of(&repo, "http://h:7878");
    /// assert_eq!(trust, Trust { owner: Some("acme"), server: Some("http://h:7878") });
    /// assert_eq!(Trust::of(&Repo::None, "http://h:7878").owner, None);
    /// let local = Repo::Git { owner: "local".into(), name: "app".into() };
    /// assert_eq!(Trust::of(&local, "http://h:7878").owner, None);
    /// assert_eq!(Trust::of(&repo, "").server, None);
    /// ```
    pub fn of(repo: &'a Repo, server: &'a str) -> Self {
        let owner = match repo {
            Repo::Git { owner, .. } if owner != "local" => Some(owner.as_str()),
            _ => None,
        };
        Trust {
            owner,
            server: (!server.is_empty()).then_some(server),
        }
    }
}

/// The entries of `autoMode.environment` of a session
/// (01M516HETNZCBFBJNHT8BKFB1Y): first `"$defaults"`, so the entries of
/// Claude Code stay; then the places of riff work. The owner of the
/// repository on GitHub, the releases of riff, the crates of Rust, the
/// session images and the riff server. An entry that needs a fact that
/// `trust` does not have is not there.
///
/// ```
/// use riff::permissions::{Trust, environment};
///
/// let all = environment(&Trust { owner: Some("acme"), server: Some("http://h:7878") });
/// assert_eq!(all[0], "$defaults");
/// assert!(all.iter().any(|e| e.starts_with("Source control: github.com/acme ")));
/// assert!(all.iter().any(|e| e.contains("github.com/como-technologies/riff")));
/// assert!(all.iter().any(|e| e.contains("crates.io")));
/// assert!(all.iter().any(|e| e.contains("ghcr.io")));
/// assert!(all.iter().any(|e| e.contains("http://h:7878")));
///
/// let few = environment(&Trust::default());
/// assert_eq!(few.len(), all.len() - 2);
/// assert!(!few.iter().any(|e| e.contains("github.com/acme")));
/// ```
pub fn environment(trust: &Trust) -> Vec<String> {
    let riff = env!("CARGO_PKG_REPOSITORY").trim_start_matches("https://");
    let mut out = vec!["$defaults".to_owned()];
    if let Some(owner) = trust.owner {
        out.push(format!(
            "Source control: github.com/{owner} and each repository under it, with \
             its issues, pull requests, checks and releases. The sessions of riff \
             read and write them with git and gh."
        ));
    }
    out.push(format!(
        "Source control: {riff}, the repository of riff. riff update downloads \
         the releases of riff from it."
    ));
    out.push(
        "Trusted internal domains: crates.io, index.crates.io and static.crates.io. \
         cargo gets the crates of Rust from them."
            .to_owned(),
    );
    out.push(
        "Trusted internal domains: ghcr.io. podman pulls the session image of riff \
         from it."
            .to_owned(),
    );
    if let Some(server) = trust.server {
        out.push(format!(
            "Key internal services: the riff server at {server}. Each riff command \
             and each riff tool talks to it."
        ));
    }
    out
}

/// The auto mode settings that riff gives each session that it starts
/// (01M516HETNZCBFBJNHT8BKFB1Y): `/auto-mode-setup` off, so no session
/// shows the dialog "Teach auto mode about your environment?", and
/// `autoMode.environment` with the entries of [`environment`].
///
/// ```
/// use riff::permissions::{Trust, auto_mode, environment};
///
/// let trust = Trust { owner: Some("acme"), server: None };
/// let settings = auto_mode(&trust);
/// assert_eq!(settings["skillOverrides"], serde_json::json!({"auto-mode-setup": "off"}));
/// assert_eq!(settings["autoMode"]["environment"], serde_json::json!(environment(&trust)));
/// assert_eq!(settings.as_object().unwrap().len(), 2);
/// ```
pub fn auto_mode(trust: &Trust) -> Value {
    json!({
        "skillOverrides": { AUTO_MODE_SETUP: "off" },
        "autoMode": { "environment": environment(trust) },
    })
}

/// The settings text with the auto mode settings of `trust`, or `None`
/// when it has them all: `/auto-mode-setup` off, and each entry of
/// [`environment`] that `autoMode.environment` lacks, at its end. It
/// keeps each other key and each other entry in its order. It fails when
/// the text is not a JSON object, or when a key has the wrong type.
///
/// ```
/// use riff::permissions::{Trust, auto_mode, with_auto_mode};
///
/// let trust = Trust { owner: Some("acme"), server: None };
/// let text = r#"{"model": "opus", "autoMode": {"environment": ["Mine"]}}"#;
/// let out = with_auto_mode(text, &trust)?.unwrap();
/// let value: serde_json::Value = serde_json::from_str(&out)?;
/// assert_eq!(value["model"], "opus");
/// assert_eq!(value["skillOverrides"]["auto-mode-setup"], "off");
/// let environment = value["autoMode"]["environment"].as_array().unwrap();
/// assert_eq!(environment[0], "Mine");
/// assert_eq!(environment.len(), 1 + auto_mode(&trust)["autoMode"]["environment"].as_array().unwrap().len());
/// assert_eq!(with_auto_mode(&out, &trust)?, None);
/// assert!(with_auto_mode(r#"{"autoMode": []}"#, &trust).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn with_auto_mode(text: &str, trust: &Trust) -> Result<Option<String>> {
    let mut value: Value = serde_json::from_str(text).context("the settings are not valid JSON")?;
    let object = value
        .as_object_mut()
        .context("the settings are not a JSON object")?;
    let mut changed = false;
    let skills = object
        .entry("skillOverrides")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("`skillOverrides` is not a JSON object")?;
    if skills.get(AUTO_MODE_SETUP) != Some(&json!("off")) {
        skills.insert(AUTO_MODE_SETUP.to_owned(), json!("off"));
        changed = true;
    }
    let have = object
        .entry("autoMode")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("`autoMode` is not a JSON object")?
        .entry("environment")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .context("`autoMode.environment` is not a JSON array")?;
    for entry in environment(trust) {
        if !have.iter().any(|e| e == entry.as_str()) {
            have.push(Value::String(entry));
            changed = true;
        }
    }
    if !changed {
        return Ok(None);
    }
    let mut out = serde_json::to_string_pretty(&value)?;
    out.push('\n');
    Ok(Some(out))
}

/// The pattern of the rule `rule` of the Bash tool: `*` for `Bash`, else
/// the text in `Bash(PATTERN)`, with the old end `:*` read as ` *`.
/// `None` for a rule of another tool.
fn bash_pattern(rule: &str) -> Option<String> {
    if rule == "Bash" {
        return Some("*".to_owned());
    }
    let pattern = rule.strip_prefix("Bash(")?.strip_suffix(')')?;
    Some(match pattern.strip_suffix(":*") {
        Some(prefix) => format!("{prefix} *"),
        None => pattern.to_owned(),
    })
}

/// True when the rule `rule` of Claude Code matches the Bash command
/// `command`. It is the model of the match of Claude Code that the tests
/// use: a rule `Bash(PATTERN)` matches the whole command, and each `*` in
/// PATTERN matches each text, also no text. The rule `Bash` matches each
/// command. The old end `:*` of a pattern is ` *`. Each other rule
/// matches no command.
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
/// assert!(matches("Bash", "rm -rf build"));
/// assert!(matches("Bash(cargo test:*)", "cargo test -p riff"));
/// assert!(!matches("mcp__riff", "mcp__riff"));
/// ```
pub fn matches(rule: &str, command: &str) -> bool {
    let Some(pattern) = bash_pattern(rule) else {
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

/// True when the pattern `pattern` of a Bash rule names its program and
/// its words, with its only `*` at its end, after a space.
fn star_only_at_the_end(pattern: &str) -> bool {
    match pattern.matches('*').count() {
        0 => true,
        1 => pattern.ends_with(" *") && pattern.len() > 2,
        _ => false,
    }
}

/// True when the allow rule `rule` is wider than the steps of riff
/// (01M51JCW04FAHZDQ144KDNCCPX): a rule of the Bash tool with no
/// pattern, with a `*` that is not at its end after a space, or that
/// matches a command of [`OUT_OF_BOUNDS`]. The old end `:*` of a pattern
/// is ` *`. A rule of another tool is never wide.
///
/// ```
/// use riff::permissions::{Facts, wide};
///
/// let facts = Facts::default();
/// assert!(!wide("Bash(gh pr view *)", &facts));
/// assert!(!wide("Bash(cargo test:*)", &facts));
/// assert!(!wide("mcp__riff", &facts));
/// assert!(wide("Bash", &facts));
/// assert!(wide("Bash(gh pr merge * --auto --squash)", &facts));
/// assert!(wide("Bash(gh api repos/o/r/statuses/*)", &facts));
/// assert!(wide("Bash(gh api *)", &facts));
/// assert!(wide("Bash(git:*)", &facts));
/// ```
pub fn wide(rule: &str, facts: &Facts) -> bool {
    let Some(pattern) = bash_pattern(rule) else {
        return false;
    };
    !star_only_at_the_end(&pattern) || out_of_bounds(facts).iter().any(|c| matches(rule, c))
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

/// The allow rules of the settings text `text` that are [`wide`], in
/// their order. A text that is not JSON has none.
///
/// ```
/// use riff::permissions::{Facts, wide_rules};
///
/// let text = r#"{"permissions": {"allow": ["Bash(riff *)", "Bash(gh api *)"], "deny": ["Bash"]}}"#;
/// assert_eq!(wide_rules(text, &Facts::default()).allow, ["Bash(gh api *)"]);
/// assert!(wide_rules(text, &Facts::default()).deny.is_empty());
/// assert!(wide_rules("not json", &Facts::default()).is_empty());
/// ```
pub fn wide_rules(text: &str, facts: &Facts) -> Rules {
    let value: Value = serde_json::from_str(text).unwrap_or_default();
    let mut allow: Vec<String> = Vec::new();
    for rule in value["permissions"]["allow"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(rule) = rule.as_str()
            && wide(rule, facts)
            && !allow.iter().any(|r| r == rule)
        {
            allow.push(rule.to_owned());
        }
    }
    Rules {
        allow,
        deny: Vec::new(),
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

/// The settings text with no rule of `rules`, or `None` when it has none
/// of them (01M51JCW2EKF8H8QFQYSVRSPET). It keeps each other key and
/// each other rule in its order. It fails when the text is not a JSON
/// object, or when `permissions`, `allow` or `deny` has the wrong type.
///
/// ```
/// use riff::permissions::{Rules, without_rules};
///
/// let rules = Rules { allow: vec!["Bash(gh api *)".into()], deny: vec![] };
/// let text = r#"{"model": "opus", "permissions": {"allow": ["X", "Bash(gh api *)", "Y"]}}"#;
/// let out = without_rules(text, &rules)?.unwrap();
/// let value: serde_json::Value = serde_json::from_str(&out)?;
/// assert_eq!(value["permissions"]["allow"], serde_json::json!(["X", "Y"]));
/// assert_eq!(value["model"], "opus");
/// assert_eq!(without_rules(&out, &rules)?, None);
/// assert_eq!(without_rules("{}", &rules)?, None);
/// assert!(without_rules("[1]", &rules).is_err());
/// assert!(without_rules(r#"{"permissions": {"allow": "X"}}"#, &rules).is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn without_rules(text: &str, rules: &Rules) -> Result<Option<String>> {
    let mut value: Value = serde_json::from_str(text).context("the settings are not valid JSON")?;
    let object = value
        .as_object_mut()
        .context("the settings are not a JSON object")?;
    let Some(permissions) = object.get_mut("permissions") else {
        return Ok(None);
    };
    let permissions = permissions
        .as_object_mut()
        .context("`permissions` is not a JSON object")?;
    let mut changed = false;
    for (list, drop) in [("allow", &rules.allow), ("deny", &rules.deny)] {
        let Some(have) = permissions.get_mut(list) else {
            continue;
        };
        let have = have
            .as_array_mut()
            .with_context(|| format!("`permissions.{list}` is not a JSON array"))?;
        let before = have.len();
        have.retain(|r| !drop.iter().any(|d| r == d.as_str()));
        changed |= have.len() != before;
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

/// Removes each rule of `rules` from the settings file at `path`
/// ([`without_rules`], 01M51JCW2EKF8H8QFQYSVRSPET). It writes the file
/// only when it changes. It returns the rules that it removed. A file
/// that is not there has no rule.
///
/// ```
/// use riff::permissions::{Rules, add, remove};
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("settings.json");
/// let rules = Rules { allow: vec!["Bash(gh api *)".into()], deny: vec![] };
/// assert!(remove(&path, &rules)?.is_empty());
/// assert!(!path.exists());
/// add(&path, &rules)?;
/// assert_eq!(remove(&path, &rules)?, rules);
/// assert!(remove(&path, &rules)?.is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn remove(path: &Path, rules: &Rules) -> Result<Rules> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Rules::default()),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    let left = missing(rules, &[&text]);
    let had = |want: &[String], left: &[String]| -> Vec<String> {
        want.iter().filter(|r| !left.contains(r)).cloned().collect()
    };
    let removed = Rules {
        allow: had(&rules.allow, &left.allow),
        deny: had(&rules.deny, &left.deny),
    };
    let Some(new) = without_rules(&text, rules).with_context(|| path.display().to_string())? else {
        return Ok(Rules::default());
    };
    write(path, &new)?;
    Ok(removed)
}

/// Adds the auto mode settings of `trust` that the settings file at
/// `path` lacks ([`with_auto_mode`]). It makes the file when it is not
/// there, and writes it only when it changes. True when it changed.
///
/// ```
/// use riff::permissions::{Trust, add_auto_mode};
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("settings.json");
/// assert!(add_auto_mode(&path, &Trust::default())?);
/// assert!(!add_auto_mode(&path, &Trust::default())?);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn add_auto_mode(path: &Path, trust: &Trust) -> Result<bool> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => "{}".to_owned(),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    let Some(new) = with_auto_mode(&text, trust).with_context(|| path.display().to_string())?
    else {
        return Ok(false);
    };
    write(path, &new)?;
    Ok(true)
}

/// Writes `text` into the settings file at `path`. It writes a new file
/// in the same folder and renames it, so a process that reads the file
/// at the same time sees the old text or the new text, never a part. The
/// file keeps its mode. A settings file that is a symbolic link stays a
/// link: riff writes the file that it names (01M509TK01PW3RN2YPRJD38RSX).
pub(crate) fn write(path: &Path, text: &str) -> Result<()> {
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
    fn the_auto_mode_settings_keep_each_other_skill_and_entry() {
        let trust = Trust {
            owner: Some("acme"),
            server: None,
        };
        let text = r#"{
            "skillOverrides": {"other": "off", "auto-mode-setup": "on"},
            "autoMode": {"environment": ["Mine"], "allow": ["$defaults"]}
        }"#;
        let out: Value =
            serde_json::from_str(&with_auto_mode(text, &trust).unwrap().unwrap()).unwrap();
        assert_eq!(
            out["skillOverrides"],
            json!({"other": "off", "auto-mode-setup": "off"})
        );
        assert_eq!(out["autoMode"]["allow"], json!(["$defaults"]));
        let mut want = vec![json!("Mine")];
        want.extend(environment(&trust).into_iter().map(Value::from));
        assert_eq!(out["autoMode"]["environment"], Value::Array(want));
    }

    #[test]
    fn the_auto_mode_settings_fail_on_a_key_of_the_wrong_type() {
        for text in [
            r#"{"skillOverrides": []}"#,
            r#"{"autoMode": []}"#,
            r#"{"autoMode": {"environment": "x"}}"#,
            "[1]",
            "no json",
        ] {
            assert!(with_auto_mode(text, &Trust::default()).is_err(), "{text}");
        }
    }

    #[test]
    fn each_environment_entry_is_one_line_of_prose_with_no_quote() {
        // A worker gets the entries in a quoted argument of a shell
        // command, and the classifier reads each one as a sentence.
        let trust = Trust {
            owner: Some("acme"),
            server: Some("http://h:7878"),
        };
        for entry in environment(&trust).iter().skip(1) {
            assert!(
                !entry.contains(['\n', '\'']) && !entry.contains("  "),
                "{entry:?}"
            );
            assert!(entry.ends_with('.'), "{entry:?}");
        }
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
        let value: Value =
            serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(value["model"], "opus");
        assert_eq!(missing(&rules, &[&value.to_string()]), Rules::default());
        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
        // No temp file stays in the folder.
        assert_eq!(
            std::fs::read_dir(dir.path().join("dotfiles"))
                .unwrap()
                .count(),
            1
        );
    }

    /// The facts of the tests: the home of mike, and the branch `main`.
    fn mike() -> Facts<'static> {
        Facts {
            home: Some(Path::new("/home/mike")),
            branch: Some("main"),
        }
    }

    /// 01M509TJNREVNYR9MNN4P9P4RA, 01M51JCW04FAHZDQ144KDNCCPX.
    #[test]
    fn each_allow_rule_has_its_only_star_at_its_end_after_a_space() {
        let facts = mike();
        for row in TABLE.iter().filter(|r| r.kind == Kind::Allow) {
            let rule = row.rule_of(&facts).unwrap();
            assert!(!wide(&rule, &facts), "{row:?}");
            // A rule of another tool names the tool: it has no `*`.
            assert!(rule.starts_with("Bash(") || !rule.contains('*'), "{row:?}");
        }
        // The four rules of the failed verify of #776 are too wide, and
        // so is a rule of the Bash tool with no pattern.
        for rule in [
            "Bash(~/.cache/riff/checks/*/riff *)",
            "Bash(gh api repos/*/milestones *)",
            "Bash(gh api -X PATCH repos/*/milestones/*)",
            "Bash(gh pr merge * --auto --squash)",
            "Bash(gh api repos/o/r/statuses/*)",
            "Bash(gh pr view*)",
            "Bash(*)",
            "Bash",
            "Bash( *)",
            "Bash(gh pr view:*:*)",
        ] {
            assert!(wide(rule, &facts), "{rule}");
        }
        // A rule that names its program and its words, in the new form or
        // the old form, and a rule of another tool.
        for rule in [
            "Bash(cargo test *)",
            "Bash(cargo test:*)",
            "Bash(npm run build)",
            "mcp__github",
            "Read(~/**)",
            "WebFetch(domain:docs.rs)",
        ] {
            assert!(!wide(rule, &facts), "{rule}");
        }
    }

    /// 01M509TJNREVNYR9MNN4P9P4RA: no allow rule of a place matches a
    /// command that no step of a role needs.
    #[test]
    fn no_allow_rule_matches_a_command_out_of_bounds() {
        let facts = mike();
        let commands = out_of_bounds(&facts);
        assert_eq!(commands.len(), OUT_OF_BOUNDS.len());
        assert!(commands.contains(
            &"/home/mike/.cache/riff/check/target/static/release/riffx daemon install".into()
        ));
        for place in [Place::Project, Place::Session] {
            let allow = rules_of(place, &facts).allow;
            for command in &commands {
                let rule = allow.iter().find(|r| matches(r, command));
                assert_eq!(rule, None, "{} allows `{command}`", place.as_str());
            }
        }
        // With no home, the commands that need it are not there.
        let few = out_of_bounds(&Facts::default());
        assert_eq!(few.len(), OUT_OF_BOUNDS.len() - 2);
        assert!(!few.iter().any(|c| c.contains("{HOME}")));
    }

    /// 01M51JCW04FAHZDQ144KDNCCPX: a rule that matches a command out of
    /// bounds is wide, also with its only `*` at its end.
    #[test]
    fn a_rule_of_a_whole_command_group_is_wide() {
        let facts = mike();
        for rule in [
            "Bash(gh api *)",
            "Bash(gh pr *)",
            "Bash(gh pr merge *)",
            "Bash(git push *)",
            "Bash(git *)",
            "Bash(gh:*)",
            "Bash(sh -c *)",
            "Bash(/home/mike/.cache/riff/check/target/static/release/riffx *)",
        ] {
            assert!(wide(rule, &facts), "{rule}");
        }
    }

    /// The project settings of this repository have each project rule,
    /// and none of their allow rules is wide (01M3Q53RNDJBDHVDFHJ9HCX9S1,
    /// 01M509TJNREVNYR9MNN4P9P4RA).
    #[test]
    fn the_project_settings_of_riff_have_the_rules_and_no_wide_rule() {
        let text = include_str!("../../../.claude/settings.json");
        assert_eq!(missing(&rules("main"), &[text]), Rules::default());
        assert_eq!(wide_rules(text, &mike()), Rules::default());
        let value: Value = serde_json::from_str(text).unwrap();
        assert!(value["permissions"]["allow"].as_array().unwrap().len() > 5);
    }

    /// 01M51JCW2EKF8H8QFQYSVRSPET: a remove of the wide rules keeps each
    /// other rule and key in its order, the deny rules, a link and the
    /// mode of the file.
    #[test]
    fn a_remove_keeps_each_other_rule_and_key() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dotfiles/settings.json");
        std::fs::create_dir(dir.path().join("dotfiles")).unwrap();
        std::fs::write(
            &target,
            r#"{"model": "opus", "permissions": {"deny": ["Bash(gh api *)"],
                "allow": ["Bash(riff *)", "Bash(gh api *)", "Bash(git push *)", "X"]},
                "statusLine": {"type": "command"}}"#,
        )
        .unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        let link = dir.path().join("settings.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let wide = wide_rules(&std::fs::read_to_string(&link).unwrap(), &mike());
        assert_eq!(wide.allow, ["Bash(gh api *)", "Bash(git push *)"]);
        assert_eq!(remove(&link, &wide).unwrap(), wide);

        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        let text = std::fs::read_to_string(&target).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["permissions"]["allow"], json!(["Bash(riff *)", "X"]));
        assert_eq!(value["permissions"]["deny"], json!(["Bash(gh api *)"]));
        let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["model", "permissions", "statusLine"]);
        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
        assert!(remove(&link, &wide).unwrap().is_empty());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), text);
    }

    #[test]
    fn a_remove_leaves_settings_that_are_not_an_object() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "[1]").unwrap();
        let rules = Rules {
            allow: vec!["Bash".into()],
            deny: vec![],
        };
        assert!(remove(&path, &rules).is_err());
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

    /// 01M592406KTHSXHTWQWA3RN6Q2: the session place has a rule for
    /// `riff wave move` and `riff wave end` of the lead.
    #[test]
    fn the_session_place_has_the_wave_move_and_end_rules_of_the_lead() {
        let session = session_rules();
        for (rule, step) in [
            (
                "Bash(/opt/riff/riff wave move *)",
                "/opt/riff/riff wave move 71 --to 7",
            ),
            (
                "Bash(/opt/riff/riff wave end *)",
                "/opt/riff/riff wave end 6 --move-open-to-backlog",
            ),
        ] {
            assert!(session.allow.iter().any(|r| r == rule), "{rule}");
            assert!(session.allow.iter().any(|r| matches(r, step)), "{step}");
        }
    }

    /// 01M58KMMF9WMH39ZMD0MKGRVZB: the session place has a rule for the
    /// release steps of the lead, and no rule that names `approve`. The
    /// guard of riff refuses `approve` in each session.
    #[test]
    fn the_session_place_has_the_release_steps_of_the_lead_and_none_for_approve() {
        let session = session_rules();
        for step in [
            "/opt/riff/riff release status",
            "/opt/riff/riff release cut 3.3.0",
        ] {
            assert!(session.allow.iter().any(|r| matches(r, step)), "{step}");
        }
        assert!(
            !session.allow.iter().any(|r| r.contains("approve")),
            "{session:?}"
        );
        assert!(
            !session
                .allow
                .iter()
                .any(|r| matches(r, "/opt/riff/riff release approve v3.3.0"))
        );
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

    /// Each code span and each line of a code block of the markdown
    /// `text`.
    fn code_lines(text: &str) -> Vec<String> {
        let mut lines = Vec::new();
        let mut prose = String::new();
        let mut block = false;
        for line in text.lines() {
            if line.trim_start().starts_with("```") {
                block = !block;
            } else if block {
                lines.push(line.trim().to_owned());
            } else {
                prose.push_str(line);
                prose.push('\n');
            }
        }
        let spans = prose.split('`').skip(1).step_by(2);
        lines.extend(spans.map(|s| s.split_whitespace().collect::<Vec<_>>().join(" ")));
        lines
    }

    /// Each `riff` and `gh` step of the markdown `text`: each code span,
    /// and each line of a code block.
    fn skill_steps(text: &str) -> Vec<String> {
        let mut steps = code_lines(text);
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

    /// True when the shell line `line` joins a `riff` or `gh` step to
    /// another command with `&&`, `||` or `;`, or names the repository
    /// with `gh -R` before the words of the step. Claude Code matches a
    /// rule to each part of a call (01M51V0Q5E7MNPWP9T7TSJFT5W).
    fn joins_a_step(line: &str) -> bool {
        let parts: Vec<&str> = line
            .split("&&")
            .flat_map(|p| p.split("||"))
            .flat_map(|p| p.split(';'))
            .map(str::trim)
            .collect();
        let joined = parts.len() > 1 && parts.iter().any(|p| is_step(p));
        joined || line.contains("gh -R ") || line.contains("gh --repo ")
    }

    /// 01M51V0Q5E7MNPWP9T7TSJFT5W: the refused calls of #897 joined their
    /// steps to a `cd` out of the worktree. Alone, each step matches a
    /// rule of the session place; the `cd` matches none.
    #[test]
    fn a_step_alone_matches_a_session_rule_and_a_cd_to_the_scratchpad_matches_none() {
        let session = session_rules();
        let allowed = |c: &str| session.allow.iter().any(|r| matches(r, c));
        let scratchpad = "/home/mike/.cache/riff/tmp/e750ad62/scratchpad";
        let refused = format!(
            "cd {scratchpad} && gh issue edit 840 --body-file body-840.md && gh issue edit \
             841 --body-file body-841.md"
        );
        assert!(joins_a_step(&refused));
        assert!(!allowed(&format!("cd {scratchpad}")));
        for step in [
            format!("gh issue edit 840 --body-file {scratchpad}/body-840.md"),
            format!("gh issue edit 884 --repo o/r --body-file {scratchpad}/new-884.md"),
        ] {
            assert!(!joins_a_step(&step), "{step}");
            assert!(allowed(&step), "{step}");
        }
        let early = format!("gh -R o/r issue edit 884 --body-file {scratchpad}/new-884.md");
        assert!(joins_a_step(&early));
        assert!(!allowed(&early));
        assert!(!joins_a_step(
            "git diff --cached --quiet || git commit -q -m \"WIP: STEP\""
        ));
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
                    "main",
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
                Role::Worker,
                text::wave_rebase_start(
                    "o/r",
                    "main",
                    12,
                    40,
                    "1a2b3c4d",
                    "worktree-issue-12",
                    "1a2b",
                ),
            ),
            // #279: the fix of a failed check reads the failed run, and
            // may run the job again.
            (
                Role::Worker,
                text::wave_checks_start(
                    "o/r",
                    "main",
                    12,
                    40,
                    "1a2b3c4d",
                    &["Gate".to_owned()],
                    "worktree-issue-12",
                    "1a2b",
                ),
            ),
            (
                Role::Verifier,
                text::wave_verify_start("o/r", "main", 12, 40, "1a2b3c4d", &["CI".to_owned()]),
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
            // The lead in a container plans the waves (01M51K1VJ67KYHRD7DQ86DSX1X).
            if role == Role::Lead {
                for wave in ["list", "make", "rename", "end"] {
                    let step = format!("/opt/riff/riff wave {wave}");
                    assert!(steps.contains(&step), "{steps:#?}");
                }
                for step in [
                    "/opt/riff/riff wave move ITEM --to WAVE",
                    "/opt/riff/riff wave end NUMBER --move-open-to-backlog",
                ] {
                    assert!(steps.contains(&step.to_owned()), "{steps:#?}");
                }
            }
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
