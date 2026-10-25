//! The setup of a repository on GitHub that riff needs: the checks of
//! `riff init` and the steps that do them (01M5QVF5535YRN1EF3DZAB4W8G,
//! 01M5R1672NF56P15VD0JEX51K4).
//!
//! # Design
//!
//! `riff init` checks six facts of the repository of the current
//! folder. Each fact that fails carries its fix for a person, and for
//! five of them its [`Step`]: the code that does the fix. A person does
//! the fix, for example from the how-to "Add another repository to the
//! riff" in the book. Or `riff init` does the steps after one question,
//! or with `--yes` ([`setup`]). The step and the how-to make the same
//! change: the ruleset of the how-to is [`RULESET`]
//! (01M5QWA0E9A4ZJ8X49EM2MJM5Q).
//!
//! ```mermaid
//! flowchart LR
//!     O["remote origin<br/>github.com/OWNER/REPO"] --> R["gh api repos/OWNER/REPO"]
//!     O --> B["gh api repos/OWNER/REPO/rules/branches/BRANCH"]
//!     O --> M["gh api repos/OWNER/REPO/milestones"]
//!     O --> F["gh api repos/OWNER/REPO/contents"]
//!     O --> P["gh api repos/OWNER/REPO/pulls<br/>and the check runs of their heads"]
//!     R --> C["the six checks"]
//!     B --> C
//!     M --> C
//!     F --> C
//!     P --> C
//!     C --> L["pass or fail lines of riff init"]
//!     C -- "a step, one question or --yes" --> S["the steps, in order"]
//! ```
//!
//! | Check | riff reads | Pass | Step |
//! |---|---|---|---|
//! | `riff.toml` on the default branch | the files at the root of the default branch | `riff.toml` is there: without it `riff lead start` stops | [`Step::Config`] |
//! | auto-merge and squash merge | `allow_auto_merge` and `allow_squash_merge` of the repository | both are on: the work sessions run `gh pr merge --auto --squash` | [`Step::Merge`] |
//! | the ruleset of the default branch | the rules that apply to the `default_branch` of the repository ([`rules_path`]) | a `required_status_checks` rule has the check [`VERIFY_CONTEXT`]: without it, nothing on GitHub makes a merge wait for the verify | [`Step::Ruleset`] |
//! | the milestone `Backlog` | the open milestones ([`crate::waves::open`]) | one has the title [`BACKLOG`]: `riff wave move --to Backlog` and `riff wave end --move-open-to-backlog` need it | [`Step::Backlog`] |
//! | the first wave | the open milestones | one is a wave: the lead has a wave to plan | [`Step::Wave`] |
//! | the checks of a pull request | the required checks, and the check runs of the latest pull requests | a rule requires a check of the CI, or the ruleset that the step makes will require the checks that the pull requests ran, or the repository has no CI and [`VERIFY_CONTEXT`] is the only check (a note says so) | [`Step::Ruleset`] when no pull request ran a check yet and the default branch has no `riff.toml`: the pull request of [`Step::Config`] may run the first check. Else none |
//!
//! The fix of the ruleset names the check [`VERIFY_CONTEXT`] and the checks
//! of the CI.
//!
//! `riff init` reads no file of rules for agents, such as `CLAUDE.md` or
//! `AGENTS.md`: the CI of the repository is the gate of a change, and a
//! `CLAUDE.md` is the choice of the repository (01M5RN89Z749VYS1CRYJQTKAWY).
//!
//! # The steps
//!
//! The steps run in the order of [`Step`]. Each uses `gh` and the API of
//! GitHub, never the working tree of the clone: the clone may have
//! changes, and the person may not be on the default branch. A step that
//! fails does not stop the next one.
//!
//! ```mermaid
//! flowchart LR
//!     A["allow auto-merge<br/>and squash merge"] --> B["riff.toml: branch riff-setup,<br/>pull request Set up riff,<br/>squash merge"]
//!     B --> K["the milestone Backlog"]
//!     K --> W["the milestone Wave 1"]
//!     W --> R["the ruleset riff"]
//! ```
//!
//! - The merge settings come first: the pull request of `riff.toml` has
//!   a squash merge.
//! - `riff.toml` pins the session image of the release of riff that runs
//!   ([`crate::session_image::of_release`]). The step reuses an open pull
//!   request from the branch [`SETUP_BRANCH`], and it resets that branch
//!   to the default branch, so a step that failed half way can run again.
//!   GitHub may need a moment to say that the pull request can merge, so
//!   riff tries again for [`Waits::merge`].
//! - The ruleset comes last. It needs a pull request, so it comes after
//!   the pull request of `riff.toml`. It requires [`VERIFY_CONTEXT`] and
//!   the checks of the CI: the required checks that the rules have
//!   already, else the check runs of the head of the latest pull request
//!   that has some ([`ci_checks`]). The pull request of `riff.toml` is
//!   often the first one that has check runs, so riff waits up to
//!   [`Waits::ci`] for them. A repository with no check run has no CI: the
//!   ruleset requires [`VERIFY_CONTEXT`] only, and riff invents no check
//!   for it (01M5SMNKDWBD1J6Y18ENQG87CK).
//! - A person who is not an admin gets no step: riff names the fix of
//!   each failed check and changes nothing ([`Github::admin`]).
//!
//! The same rules tell riff which check is the full check of the
//! repository ([`full_check`], 01M5QYDE41FVFX26EC8K5QGYH4): each check
//! that the rules of the default branch require, but [`VERIFY_CONTEXT`].
//! The wave starts a verify only when each of them is done on the head of
//! the pull request, and `riff verify pass` needs a success of each
//! ([`crate::pr::gate_of`]). A repository whose rules require no other
//! check has no CI: its full check is empty, and [`VERIFY_CONTEXT`] is the
//! only check that gates a merge (01M5SMNKDWBD1J6Y18ENQG87CK). The wave
//! starts its verify at once, and `riff verify pass` needs no check run.
//!
//! ```mermaid
//! flowchart LR
//!     B["gh api repos/OWNER/REPO<br/>default_branch"] --> R["gh api .../rules/branches/BRANCH<br/>required_status_checks"]
//!     R --> F["the full check:<br/>the required checks but riff/verify,<br/>none when there are none"]
//!     F --> C["gh api .../commits/COMMIT/check-runs?check_name=NAME<br/>for each NAME"]
//!     C --> S["pass, running, ended or missing"]
//! ```
//!
//! ```
//! use riff::repo_setup::full_check_of;
//!
//! let required = ["checks / lint and test".to_owned(), "riff/verify".to_owned()];
//! assert_eq!(full_check_of(&required), ["checks / lint and test"]);
//! // No CI: the verify is the only check, and the full check has none.
//! assert!(full_check_of(&["riff/verify".to_owned()]).is_empty());
//! ```
//!
//! A folder with no remote `origin` on GitHub has no such check
//! ([`crate::identity::github_origin`]). When `gh` cannot read the
//! repository, one check fails ([`crate::text::INIT_GITHUB`]) and the
//! others do not run.
//!
//! ```
//! use riff::repo_setup::{Github, Step, checks};
//!
//! let mut github = Github {
//!     repo: "acme/app".into(),
//!     default_branch: "main".into(),
//!     auto_merge: true,
//!     squash_merge: true,
//!     required: vec!["Gate".into(), "riff/verify".into()],
//!     milestones: vec!["Backlog".into(), "Wave 1".into()],
//!     admin: true,
//!     files: vec!["riff.toml".into()],
//!     ci: vec![],
//! };
//! assert!(checks(&github).iter().all(|c| c.passed()));
//!
//! github.auto_merge = false;
//! github.required = vec!["Gate".into()];
//! github.milestones = vec![];
//! github.files = vec![];
//! let failed: Vec<String> = checks(&github).iter().filter(|c| !c.passed()).map(|c| c.name.clone()).collect();
//! assert_eq!(
//!     failed,
//!     [
//!         "riff.toml on main",
//!         "auto-merge and squash merge",
//!         "the ruleset of main",
//!         "the milestone Backlog",
//!         "the first wave",
//!     ]
//! );
//! let steps: Vec<Step> = checks(&github).iter().filter_map(|c| c.step).collect();
//! assert_eq!(steps, [Step::Config, Step::Merge, Step::Ruleset, Step::Backlog, Step::Wave]);
//! ```

use std::time::{Duration, Instant};

use anyhow::Context;
use base64::Engine;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::first_run::Check;
use crate::pr::{Gh, VERIFY_CONTEXT, percent};
use crate::rollout::Issue;
use crate::session_image;
use crate::text;
use crate::waves::{self, BACKLOG, FIRST_WAVE};

/// The time that riff waits for each call of `gh`.
pub const LIMIT: Duration = Duration::from_secs(30);

/// The branch of the pull request that [`Step::Config`] opens.
pub const SETUP_BRANCH: &str = "riff-setup";

/// The title of the pull request, and of the commit, of [`Step::Config`].
pub const SETUP_TITLE: &str = "Set up riff";

/// The body of the pull request of [`Step::Config`].
const SETUP_BODY: &str = "riff init made this pull request. It adds riff.toml, which pins the \
     session image of each session of this repository.";

/// The file that [`Step::Config`] makes.
const CONFIG: &str = session_image::FILE;

/// The pull requests that [`ci_checks`] reads, the latest first.
pub const PULLS: usize = 5;

/// How long riff waits for GitHub in the steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Waits {
    /// For the check runs of the pull request of `riff.toml`, before the
    /// ruleset names the checks of the CI.
    pub ci: Duration,
    /// For GitHub to merge a pull request that it just opened.
    pub merge: Duration,
    /// Between two tries.
    pub poll: Duration,
}

impl Waits {
    /// No wait: a test with a fake forge.
    pub const NONE: Waits = Waits {
        ci: Duration::ZERO,
        merge: Duration::ZERO,
        poll: Duration::ZERO,
    };
}

impl Default for Waits {
    fn default() -> Self {
        Waits {
            ci: Duration::from_secs(30),
            merge: Duration::from_secs(20),
            poll: Duration::from_secs(2),
        }
    }
}

/// The forge that the steps change: the `gh` of the repository, and
/// how long riff waits for it.
#[derive(Debug, Clone, Default)]
pub struct Forge {
    /// The `gh` of the repository.
    pub gh: Gh,
    /// What riff waits for.
    pub waits: Waits,
}

impl Forge {
    /// The forge of the repository `repo` (`OWNER/REPO`), with the `gh`
    /// of the machine.
    pub fn of(repo: &str) -> Forge {
        Forge {
            gh: Gh::for_repo(repo).with_limit(LIMIT),
            waits: Waits::default(),
        }
    }
}

/// The ruleset that [`Step::Ruleset`] makes when the repository has no CI:
/// a pull request with squash merges only, no bypass, and the check
/// [`VERIFY_CONTEXT`] on the default branch. The how-to "Add another
/// repository to the riff" shows the same text, and a test checks that
/// they agree. [`ruleset_for`] puts the checks of a CI in it.
pub const RULESET: &str = r#"{
  "name": "riff",
  "target": "branch",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
  "bypass_actors": [],
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": false,
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": false,
        "allowed_merge_methods": ["squash"]
      }
    },
    {
      "type": "required_status_checks",
      "parameters": {
        "strict_required_status_checks_policy": false,
        "required_status_checks": [
          { "context": "riff/verify" }
        ]
      }
    }
  ]
}"#;

/// [`RULESET`] with the checks `checks` of the CI before the check
/// [`VERIFY_CONTEXT`], which comes last, once.
///
/// ```
/// use riff::repo_setup::{RULESET, ruleset_for};
///
/// let same = ruleset_for(&[]);
/// assert_eq!(same, serde_json::from_str::<serde_json::Value>(RULESET)?);
///
/// let ruleset = ruleset_for(&["checks / docs".into(), "riff/verify".into()]);
/// let contexts: Vec<&str> = ruleset["rules"][3]["parameters"]["required_status_checks"]
///     .as_array().unwrap().iter().map(|c| c["context"].as_str().unwrap()).collect();
/// assert_eq!(contexts, ["checks / docs", "riff/verify"]);
/// # Ok::<(), serde_json::Error>(())
/// ```
pub fn ruleset_for(checks: &[String]) -> Value {
    let mut ruleset: Value = serde_json::from_str(RULESET).expect("RULESET is JSON");
    let mut contexts: Vec<&str> = Vec::new();
    for check in checks {
        if check != VERIFY_CONTEXT && !contexts.contains(&check.as_str()) {
            contexts.push(check);
        }
    }
    contexts.push(VERIFY_CONTEXT);
    let required: Vec<Value> = contexts.iter().map(|c| json!({ "context": c })).collect();
    if let Some(rules) = ruleset["rules"].as_array_mut() {
        for rule in rules
            .iter_mut()
            .filter(|r| r["type"] == "required_status_checks")
        {
            rule["parameters"]["required_status_checks"] = Value::Array(required.clone());
        }
    }
    ruleset
}

/// The step that fixes a failed check of the repository on GitHub. The
/// order of the variants is the order of the steps. Each step needs the
/// rights of an admin of the repository, and it is safe to do again.
///
/// ```
/// use riff::repo_setup::{Github, Step, checks};
///
/// let github = Github {
///     repo: "acme/app".into(),
///     default_branch: "main".into(),
///     auto_merge: false,
///     squash_merge: true,
///     required: vec![],
///     milestones: vec!["Wave 1".into()],
///     admin: true,
///     files: vec!["riff.toml".into()],
///     ci: vec!["checks / lint".into()],
/// };
/// let steps: Vec<Step> = checks(&github).iter().filter_map(|c| c.step).collect();
/// assert_eq!(steps, [Step::Merge, Step::Ruleset, Step::Backlog]);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// Allow auto-merge and squash merge: `gh api -X PATCH`.
    Merge,
    /// Make `riff.toml` on the default branch with the session image of
    /// this release of riff: the branch [`SETUP_BRANCH`], the pull request
    /// [`SETUP_TITLE`] and its squash merge.
    Config,
    /// Make the open milestone [`BACKLOG`]
    /// ([`crate::waves::make_backlog`]).
    Backlog,
    /// Make the open wave [`FIRST_WAVE`] ([`crate::waves::make_first`]).
    Wave,
    /// Make the ruleset [`RULESET`] with the checks of the CI:
    /// `gh api -X POST .../rulesets`, or `PUT` when a ruleset named `riff`
    /// is there.
    Ruleset,
}

/// The pull request of [`Step::Config`].
#[derive(Debug, Clone, Deserialize)]
struct Pull {
    number: u64,
    #[serde(deserialize_with = "crate::text::forge_de")]
    html_url: String,
    head: Ref,
    base: Ref,
}

/// A side of a pull request.
#[derive(Debug, Clone, Deserialize)]
struct Ref {
    #[serde(rename = "ref", deserialize_with = "crate::text::forge_de")]
    name: String,
    #[serde(default)]
    sha: String,
}

/// True when `error` is an answer 404 of GitHub.
fn not_found(error: &anyhow::Error) -> bool {
    format!("{error:#}").contains("HTTP 404")
}

/// The body of a reply that `gh api --include` gave ([`Gh::send`]): the
/// text after the headers.
fn body_of(reply: &str) -> &str {
    reply
        .split_once("\r\n\r\n")
        .or_else(|| reply.split_once("\n\n"))
        .map_or(reply, |(_, body)| body)
}

/// A branch name in the path of a ref: the `/` stays.
fn ref_of(branch: &str) -> String {
    percent(branch).replace("%2F", "/")
}

impl Step {
    /// What the step does in `github`, in one line.
    ///
    /// ```
    /// use riff::repo_setup::{Github, Step};
    ///
    /// let github = Github {
    ///     repo: "acme/app".into(),
    ///     default_branch: "main".into(),
    ///     auto_merge: false,
    ///     squash_merge: false,
    ///     required: vec![],
    ///     milestones: vec![],
    ///     admin: true,
    ///     files: vec![],
    ///     ci: vec![],
    /// };
    /// assert_eq!(Step::Config.title(&github), "make riff.toml on main with a pull request");
    /// assert_eq!(Step::Wave.title(&github), "make the milestone Wave 1");
    /// ```
    pub fn title(self, github: &Github) -> String {
        match self {
            Step::Merge => "allow auto-merge and squash merge".to_owned(),
            Step::Config => format!(
                "make {CONFIG} on {} with a pull request",
                github.default_branch
            ),
            Step::Backlog => format!("make the milestone {BACKLOG}"),
            Step::Wave => format!("make the milestone {FIRST_WAVE}"),
            Step::Ruleset => format!("make the ruleset riff on {}", github.default_branch),
        }
    }

    /// Does the step in the repository of `forge`, and records its effect
    /// in `github`. It returns what it did. `pull` is the pull request
    /// of [`Step::Config`], when a step of this run opened it.
    fn apply(
        self,
        forge: &Forge,
        github: &mut Github,
        pull: &mut Option<Pull>,
    ) -> anyhow::Result<String> {
        let gh = &forge.gh;
        match self {
            Step::Merge => {
                gh.run(
                    &[
                        "api",
                        "-X",
                        "PATCH",
                        "repos/{owner}/{repo}",
                        "-F",
                        "allow_auto_merge=true",
                        "-F",
                        "allow_squash_merge=true",
                    ],
                    None,
                )?;
                github.auto_merge = true;
                github.squash_merge = true;
                Ok("allowed both".to_owned())
            }
            Step::Config => {
                let made = config(forge, &github.default_branch)?;
                github.files.push(CONFIG.to_owned());
                let said = format!(
                    "{} {} and merged it",
                    if made.opened { "opened" } else { "found" },
                    made.pull.html_url
                );
                *pull = Some(made.pull);
                Ok(said)
            }
            Step::Backlog => {
                waves::make_backlog(gh)?;
                github.milestones.push(BACKLOG.to_owned());
                Ok(format!("made {BACKLOG}"))
            }
            Step::Wave => {
                waves::make_first(gh)?;
                github.milestones.push(FIRST_WAVE.to_owned());
                Ok(format!("made {FIRST_WAVE}"))
            }
            Step::Ruleset => {
                let names = wanted(forge, github, pull.as_ref())?;
                let body = ruleset_for(&names);
                let rulesets: Vec<Value> =
                    gh.json(&["api", "repos/{owner}/{repo}/rulesets?per_page=100"])?;
                let there = rulesets.iter().find(|r| r["name"] == body["name"]);
                match there.and_then(|r| r["id"].as_u64()) {
                    Some(id) => gh.send(
                        "PUT",
                        &format!("repos/{{owner}}/{{repo}}/rulesets/{id}"),
                        &body,
                    ),
                    None => gh.send("POST", "repos/{owner}/{repo}/rulesets", &body),
                }?;
                let required: Vec<&str> = names
                    .iter()
                    .map(String::as_str)
                    .chain([VERIFY_CONTEXT])
                    .collect();
                for name in &required {
                    if !github.required.iter().any(|r| r == name) {
                        github.required.push((*name).to_owned());
                    }
                }
                Ok(format!("requires {}", required.join(" and ")))
            }
        }
    }
}

/// The checks that the ruleset `riff` requires besides [`VERIFY_CONTEXT`]:
/// the required checks that the rules have; else the checks that the
/// latest pull request ran ([`ci_checks`]); else the check runs of the pull
/// request of `riff.toml`, for up to [`Waits::ci`]; else none: the
/// repository has no CI (01M5SMNKDWBD1J6Y18ENQG87CK).
fn wanted(forge: &Forge, github: &Github, pull: Option<&Pull>) -> anyhow::Result<Vec<String>> {
    let required = full_check_of(&github.required);
    if !required.is_empty() {
        return Ok(required);
    }
    let mut ci = if github.ci.is_empty() {
        ci_checks(&forge.gh, &github.default_branch)?
    } else {
        github.ci.clone()
    };
    if ci.is_empty()
        && let Some(pull) = pull
    {
        let start = Instant::now();
        loop {
            ci = check_names(&forge.gh, &pull.head.sha)?;
            if !ci.is_empty() || start.elapsed() >= forge.waits.ci {
                break;
            }
            std::thread::sleep(forge.waits.poll);
        }
    }
    Ok(ci)
}

/// The pull request that [`config`] found or opened.
struct Made {
    pull: Pull,
    opened: bool,
}

/// Makes `riff.toml` on `branch` with a pull request, and merges it with
/// a squash merge. It reuses an open pull request from [`SETUP_BRANCH`].
fn config(forge: &Forge, branch: &str) -> anyhow::Result<Made> {
    let gh = &forge.gh;
    let pulls: Vec<Pull> =
        gh.json(&["api", "repos/{owner}/{repo}/pulls?state=open&per_page=100"])?;
    let there = pulls
        .into_iter()
        .find(|p| p.head.name == SETUP_BRANCH && p.base.name == branch);
    let made = match there {
        Some(pull) => Made {
            pull,
            opened: false,
        },
        None => Made {
            pull: open_pull(gh, branch)?,
            opened: true,
        },
    };
    merge_pull(forge, made.pull.number)
        .with_context(|| text::init_pull_not_merged(&made.pull.html_url))?;
    // The branch stays for the person when GitHub keeps it: no reason to fail.
    let _ = gh.run(
        &[
            "api",
            "-X",
            "DELETE",
            &format!("repos/{{owner}}/{{repo}}/git/refs/heads/{SETUP_BRANCH}"),
        ],
        None,
    );
    Ok(made)
}

/// Makes the branch [`SETUP_BRANCH`] from `branch`, adds `riff.toml` to
/// it, and opens the pull request.
fn open_pull(gh: &Gh, branch: &str) -> anyhow::Result<Pull> {
    let base: Value = gh.json(&[
        "api",
        &format!("repos/{{owner}}/{{repo}}/git/ref/heads/{}", ref_of(branch)),
    ])?;
    let sha = base["object"]["sha"]
        .as_str()
        .with_context(|| format!("GitHub gave no commit for {branch}"))?;
    let there = gh.run(
        &[
            "api",
            &format!("repos/{{owner}}/{{repo}}/git/ref/heads/{SETUP_BRANCH}"),
        ],
        None,
    );
    match there {
        // A branch of a run that failed: start it again from the default branch.
        Ok(_) => gh.send(
            "PATCH",
            &format!("repos/{{owner}}/{{repo}}/git/refs/heads/{SETUP_BRANCH}"),
            &json!({ "sha": sha, "force": true }),
        ),
        Err(e) if not_found(&e) => gh.send(
            "POST",
            "repos/{owner}/{repo}/git/refs",
            &json!({ "ref": format!("refs/heads/{SETUP_BRANCH}"), "sha": sha }),
        ),
        Err(e) => return Err(e),
    }?;
    let config = session_image::config_of(&session_image::of_release());
    gh.send(
        "PUT",
        &format!("repos/{{owner}}/{{repo}}/contents/{CONFIG}"),
        &json!({
            "message": SETUP_TITLE,
            "content": base64::engine::general_purpose::STANDARD.encode(config),
            "branch": SETUP_BRANCH,
        }),
    )?;
    let reply = gh.send(
        "POST",
        "repos/{owner}/{repo}/pulls",
        &json!({ "title": SETUP_TITLE, "head": SETUP_BRANCH, "base": branch, "body": SETUP_BODY }),
    )?;
    serde_json::from_str(body_of(&reply)).context("GitHub gave no pull request")
}

/// Merges the pull request `number` with a squash merge. GitHub may not
/// know yet that a new pull request can merge: for up to [`Waits::merge`],
/// riff tries again on the answers 405 and 409.
fn merge_pull(forge: &Forge, number: u64) -> anyhow::Result<()> {
    let path = format!("repos/{{owner}}/{{repo}}/pulls/{number}/merge");
    let start = Instant::now();
    loop {
        match forge
            .gh
            .send("PUT", &path, &json!({ "merge_method": "squash" }))
        {
            Ok(_) => return Ok(()),
            Err(e)
                if start.elapsed() < forge.waits.merge && {
                    let said = format!("{e:#}");
                    said.contains("HTTP 405") || said.contains("HTTP 409")
                } =>
            {
                std::thread::sleep(forge.waits.poll);
            }
            Err(e) => return Err(e),
        }
    }
}

/// The names of the checks that the latest pull requests into `branch`
/// ran (01M5R1675HZ6TJPMQ6536X37T5): the check runs of the head of the
/// latest of the last [`PULLS`] pull requests that has some, but
/// [`VERIFY_CONTEXT`]. Empty when none has.
pub fn ci_checks(gh: &Gh, branch: &str) -> anyhow::Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Head {
        head: Ref,
    }
    let path = format!(
        "repos/{{owner}}/{{repo}}/pulls?state=all&base={}&per_page={PULLS}",
        percent(branch)
    );
    let pulls: Vec<Head> = gh.json(&["api", &path])?;
    for pull in pulls {
        let names = check_names(gh, &pull.head.sha)?;
        if !names.is_empty() {
            return Ok(names);
        }
    }
    Ok(vec![])
}

/// The names of the check runs of `commit`, once each, but
/// [`VERIFY_CONTEXT`].
fn check_names(gh: &Gh, commit: &str) -> anyhow::Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Runs {
        check_runs: Vec<Run>,
    }
    #[derive(Deserialize)]
    struct Run {
        #[serde(deserialize_with = "crate::text::forge_de")]
        name: String,
    }
    if commit.is_empty() || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(vec![]);
    }
    let runs: Runs = gh.json(&[
        "api",
        &format!("repos/{{owner}}/{{repo}}/commits/{commit}/check-runs?per_page=100"),
    ])?;
    let mut names: Vec<String> = Vec::new();
    for run in runs.check_runs {
        if run.name != VERIFY_CONTEXT && !names.contains(&run.name) {
            names.push(run.name);
        }
    }
    Ok(names)
}

/// What a step of [`setup`] did: the step, and what it did or why it
/// failed.
pub type Outcome = (Step, Result<String, String>);

/// Does the `steps` in the repository of `forge`, in the order of
/// [`Step`], each once (01M5R1672NF56P15VD0JEX51K4). A step that
/// succeeds changes `github`, so the checks of `github` pass for it. A
/// step that fails leaves `github` as it was, and the next step still
/// runs.
pub fn setup(forge: &Forge, steps: &[Step], github: &mut Github) -> Vec<Outcome> {
    let mut steps = steps.to_vec();
    steps.sort();
    steps.dedup();
    let mut pull = None;
    steps
        .into_iter()
        .map(|step| {
            let done = step
                .apply(forge, github, &mut pull)
                .map_err(|e| format!("{e:#}"));
            (step, done)
        })
        .collect()
}

/// The steps of the checks `checks` that failed, in the order of
/// [`Step`], each once.
pub fn steps_of(checks: &[Check]) -> Vec<Step> {
    let mut steps: Vec<Step> = checks
        .iter()
        .filter(|c| !c.passed())
        .filter_map(|c| c.step)
        .collect();
    steps.sort();
    steps.dedup();
    steps
}

/// The facts of a repository on GitHub that the checks read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Github {
    /// The repository, as `OWNER/REPO`.
    pub repo: String,
    /// The default branch.
    pub default_branch: String,
    /// The repository allows auto-merge.
    pub auto_merge: bool,
    /// The repository allows squash merges.
    pub squash_merge: bool,
    /// The checks that the active rules of the default branch require.
    pub required: Vec<String>,
    /// The titles of the open milestones.
    pub milestones: Vec<String>,
    /// The person is an admin of the repository: the steps need it.
    pub admin: bool,
    /// The names of the files at the root of the default branch.
    pub files: Vec<String>,
    /// The names of the checks that the latest pull requests ran
    /// ([`ci_checks`]). riff reads them only when no rule requires a check
    /// but [`VERIFY_CONTEXT`].
    pub ci: Vec<String>,
}

impl Github {
    /// The default branch has the file `name` at its root.
    pub fn has(&self, name: &str) -> bool {
        self.files.iter().any(|f| f == name)
    }
}

/// The repository, as `gh api repos/OWNER/REPO` gives it. GitHub gives
/// the two `allow_` keys only to a person with write access.
#[derive(Deserialize)]
struct Repository {
    #[serde(deserialize_with = "crate::text::forge_de")]
    full_name: String,
    #[serde(deserialize_with = "crate::text::forge_de")]
    default_branch: String,
    #[serde(default)]
    allow_auto_merge: Option<bool>,
    #[serde(default)]
    allow_squash_merge: Option<bool>,
    #[serde(default)]
    permissions: Option<Permissions>,
}

/// What the person may do in the repository.
#[derive(Deserialize)]
struct Permissions {
    #[serde(default)]
    admin: bool,
}

/// A rule of a branch, as `gh api .../rules/branches/BRANCH` gives it.
#[derive(Deserialize)]
struct Rule {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    parameters: serde_json::Value,
}

impl Rule {
    /// The checks that the rule requires: none for another kind of
    /// rule.
    fn required(&self) -> Vec<String> {
        if self.kind != "required_status_checks" {
            return vec![];
        }
        self.parameters["required_status_checks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["context"].as_str())
            .map(crate::text::forge)
            .collect()
    }
}

/// The path of the rules that apply to `branch` of `repo`: GitHub gives
/// the active rules of each ruleset, with no other detail. `repo` is
/// `OWNER/REPO`, or `{owner}/{repo}` for the repository of `gh`. A name
/// goes into the path with each byte but a letter, a digit and `-_.` as
/// `%XX` ([`crate::pr::percent`]).
///
/// ```
/// use riff::repo_setup::rules_path;
///
/// assert_eq!(rules_path("{owner}/{repo}", "main"), "repos/{owner}/{repo}/rules/branches/main?per_page=100");
/// assert_eq!(rules_path("acme/app", "feature/x"), "repos/acme/app/rules/branches/feature%2Fx?per_page=100");
/// ```
pub fn rules_path(repo: &str, branch: &str) -> String {
    format!(
        "repos/{repo}/rules/branches/{}?per_page=100",
        crate::pr::percent(branch)
    )
}

/// The checks that the active rules of the default branch of `repo`
/// (`OWNER/REPO`) require, with two calls of `gh`.
pub fn required(gh: &Gh, repo: &str) -> anyhow::Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Branch {
        #[serde(deserialize_with = "crate::text::forge_de")]
        default_branch: String,
    }
    let branch: Branch = gh.json(&["api", &format!("repos/{repo}")])?;
    let rules: Vec<Rule> = gh.json(&["api", &rules_path(repo, &branch.default_branch)])?;
    Ok(rules.iter().flat_map(Rule::required).collect())
}

/// The full check from the checks `required` by the default branch
/// (01M5QYDE41FVFX26EC8K5QGYH4): each of them but [`VERIFY_CONTEXT`], once,
/// in order. It is empty when there is none: the repository has no CI, and
/// the verify is the only check that gates a merge
/// (01M5SMNKDWBD1J6Y18ENQG87CK).
///
/// ```
/// use riff::repo_setup::full_check_of;
///
/// let names = |list: &[&str]| -> Vec<String> { list.iter().map(|c| (*c).into()).collect() };
/// assert_eq!(full_check_of(&names(&["Check", "riff/verify", "Check"])), ["Check"]);
/// assert!(full_check_of(&names(&["riff/verify"])).is_empty());
/// assert!(full_check_of(&[]).is_empty());
/// ```
pub fn full_check_of(required: &[String]) -> Vec<String> {
    let mut checks: Vec<String> = Vec::new();
    for name in required {
        if name != VERIFY_CONTEXT && !checks.contains(name) {
            checks.push(name.clone());
        }
    }
    checks
}

/// The full check of `repo` (`OWNER/REPO`): [`full_check_of`] the checks
/// that the rules of its default branch require ([`required`]). It fails
/// when `gh` cannot read them.
pub fn full_check(gh: &Gh, repo: &str) -> anyhow::Result<Vec<String>> {
    let names = required(gh, repo).with_context(|| {
        format!("riff cannot read the required checks of the default branch of {repo}")
    })?;
    Ok(full_check_of(&names))
}

/// The names of the files at the root of `branch`. An empty repository
/// has none, and GitHub answers 404.
fn files(gh: &Gh, branch: &str) -> anyhow::Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Entry {
        #[serde(deserialize_with = "crate::text::forge_de")]
        name: String,
    }
    let path = format!("repos/{{owner}}/{{repo}}/contents?ref={}", percent(branch));
    match gh.json::<Vec<Entry>>(&["api", &path]) {
        Ok(entries) => Ok(entries.into_iter().map(|e| e.name).collect()),
        Err(e) if not_found(&e) => Ok(vec![]),
        Err(e) => Err(e),
    }
}

/// Reads the facts of the repository of `gh` with four calls, or with
/// more when the rules require no check of the CI, or says why it
/// cannot.
pub fn read(gh: &Gh) -> Result<Github, String> {
    let why = |e: anyhow::Error| format!("{e:#}");
    let repo: Repository = gh.json(&["api", "repos/{owner}/{repo}"]).map_err(why)?;
    let rules: Vec<Rule> = gh
        .json(&["api", &rules_path("{owner}/{repo}", &repo.default_branch)])
        .map_err(why)?;
    let milestones = waves::open(gh).map_err(why)?;
    let files = files(gh, &repo.default_branch).map_err(why)?;
    let required: Vec<String> = rules.iter().flat_map(Rule::required).collect();
    let custom = required.iter().any(|c| c != VERIFY_CONTEXT);
    let ci = if custom {
        vec![]
    } else {
        ci_checks(gh, &repo.default_branch).map_err(why)?
    };
    Ok(Github {
        repo: repo.full_name,
        default_branch: repo.default_branch,
        auto_merge: repo.allow_auto_merge.unwrap_or(false),
        squash_merge: repo.allow_squash_merge.unwrap_or(false),
        required,
        milestones: milestones.into_iter().map(|m| m.title).collect(),
        admin: repo.permissions.is_some_and(|p| p.admin),
        files,
        ci,
    })
}

/// The facts of the repository of the folder `dir`, with the `gh` of
/// the machine: `None` when the folder has no remote `origin` on GitHub.
pub fn here(dir: &std::path::Path) -> Option<Result<Github, String>> {
    let repo = crate::identity::github_origin(dir)?;
    Some(read(&Forge::of(&repo).gh))
}

/// The numbers of the open issues of the repository of `gh` that have no
/// line `Done when:`, the least first (01M5SMMP7ZW2PZNGK63WWZAC8P). The wave
/// starts none of them ([`crate::rollout::Issue::has_done_when`]). A
/// tracker and a release issue are not in the list: no session does
/// them.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let gh = isolated::write_program(
///     dir.path().join("gh"),
///     r#"#!/bin/sh
/// printf '%s' '[
///   {"number":9,"title":"Late","body":"No line.","labels":[]},
///   {"number":4,"title":"Done","body":"Text.\n\nDone when: x.","labels":[]},
///   {"number":5,"title":"Tracker: part","body":"No line.","labels":[]},
///   {"number":2,"title":"Early","body":"","labels":[{"name":"bug"}]}
/// ]'
/// "#,
/// );
/// let gh = riff::pr::Gh::at(&gh);
/// assert_eq!(riff::repo_setup::no_done_when(&gh)?, [2, 9]);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn no_done_when(gh: &Gh) -> anyhow::Result<Vec<u64>> {
    let issues: Vec<Issue> = gh.json(&[
        "issue",
        "list",
        "--state",
        "open",
        "--limit",
        "1000",
        "--json",
        "number,title,body,labels",
    ])?;
    let mut numbers: Vec<u64> = issues
        .iter()
        .filter(|i| !i.release() && !i.tracker() && !i.has_done_when())
        .map(|i| i.number)
        .collect();
    numbers.sort_unstable();
    Ok(numbers)
}

/// [`no_done_when`] for the repository of the folder `dir`, with the
/// `gh` of the machine: `None` when the folder has no remote `origin` on
/// GitHub. An `Err` says why `gh` could not list the issues.
pub fn no_done_when_here(dir: &std::path::Path) -> Option<Result<Vec<u64>, String>> {
    let repo = crate::identity::github_origin(dir)?;
    Some(no_done_when(&Forge::of(&repo).gh).map_err(|e| format!("{e:#}")))
}

/// The checks on the facts `github`, in order: the facts of the
/// repository, then the steps. An `Err` is one failed check
/// ([`text::INIT_GITHUB`]).
pub fn checks_of(github: &Result<Github, String>) -> Vec<Check> {
    match github {
        Ok(github) => checks(github),
        Err(why) => vec![Check::fail(
            text::INIT_GITHUB,
            text::init_cannot_read(&format!("the repository: {why}")),
            text::INIT_FIX_GITHUB.into(),
        )],
    }
}

/// The checks on `github`, in order: `riff.toml`, the merge settings,
/// the ruleset of the default branch, the milestone `Backlog`, the first
/// wave and the checks of a pull request.
pub fn checks(github: &Github) -> Vec<Check> {
    let repo = &github.repo;
    let branch = &github.default_branch;
    let config = if github.has(CONFIG) {
        Check::pass(
            &text::init_config(branch),
            text::init_config_on(repo, branch),
        )
    } else {
        Check::fail(
            &text::init_config(branch),
            text::init_config_off(repo, branch),
            text::init_fix_config(branch),
        )
        .with_step(Step::Config)
    };
    let merge = if github.auto_merge && github.squash_merge {
        Check::pass(text::INIT_MERGE, text::init_merge_on(repo))
    } else {
        Check::fail(
            text::INIT_MERGE,
            text::init_merge_off(repo, github.auto_merge, github.squash_merge),
            text::init_fix_merge(repo),
        )
        .with_step(Step::Merge)
    };
    let ruleset = text::init_ruleset(branch);
    let ruleset = if github.required.iter().any(|c| c == VERIFY_CONTEXT) {
        Check::pass(&ruleset, text::init_ruleset_on(VERIFY_CONTEXT))
    } else {
        Check::fail(
            &ruleset,
            text::init_ruleset_off(branch, VERIFY_CONTEXT),
            text::init_fix_ruleset(branch, VERIFY_CONTEXT),
        )
        .with_step(Step::Ruleset)
    };
    let backlog = if github.milestones.iter().any(|m| m == BACKLOG) {
        Check::pass(text::INIT_BACKLOG, text::init_backlog_on(repo))
    } else {
        Check::fail(
            text::INIT_BACKLOG,
            text::init_backlog_off(repo),
            text::init_fix_backlog(repo),
        )
        .with_step(Step::Backlog)
    };
    let wave = match github
        .milestones
        .iter()
        .find(|m| waves::wave_number(m).is_some())
    {
        Some(wave) => Check::pass(text::INIT_WAVE, text::init_wave_on(repo, wave)),
        None => Check::fail(
            text::INIT_WAVE,
            text::init_wave_off(repo),
            text::INIT_FIX_WAVE.into(),
        )
        .with_step(Step::Wave),
    };
    vec![config, merge, ruleset, backlog, wave, ci_check(github)]
}

/// The check of the CI of a pull request. It passes when a rule requires a
/// check but the verify (the full check, [`full_check_of`]); or when no rule
/// requires the verify yet and the pull requests ran checks, which the
/// ruleset that [`Step::Ruleset`] makes requires; or when no pull request
/// ran a check and the repository has no CI: the verify is the only check,
/// and the line says so (01M5SMNKDWBD1J6Y18ENQG87CK). When no pull request
/// ran a check, and neither the rules nor the default branch are set up,
/// the check fails with that step: the pull request of [`Step::Config`] may
/// run the first check, and the ruleset requires it
/// (01M5RGG9KJXCKH8C94145WS8SM). If it runs none, the check passes after the
/// steps. It fails with its fix when the ruleset requires the verify and no
/// check of the CI, and the pull requests ran checks.
fn ci_check(github: &Github) -> Check {
    let full = full_check_of(&github.required);
    let verify = github.required.iter().any(|c| c == VERIFY_CONTEXT);
    if !full.is_empty() {
        Check::pass(text::INIT_CI, text::init_ci_on(&full))
    } else if github.ci.is_empty() && !verify && !github.has(CONFIG) {
        Check::fail(
            text::INIT_CI,
            text::init_ci_none(&github.repo),
            text::INIT_FIX_CI_NONE.into(),
        )
        .with_step(Step::Ruleset)
    } else if github.ci.is_empty() {
        Check::pass(
            text::INIT_CI,
            text::init_ci_no(&github.repo, VERIFY_CONTEXT),
        )
    } else if !verify {
        Check::pass(text::INIT_CI, text::init_ci_will(&github.ci))
    } else {
        Check::fail(
            text::INIT_CI,
            text::init_ci_open(&github.repo, &github.ci),
            text::init_fix_ci(&github.default_branch),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> Github {
        Github {
            repo: "acme/app".into(),
            default_branch: "main".into(),
            auto_merge: true,
            squash_merge: true,
            required: vec!["Gate".into(), "riff/verify".into()],
            milestones: vec!["Backlog".into(), "Wave 1".into()],
            admin: true,
            files: vec!["riff.toml".into()],
            ci: vec![],
        }
    }

    fn lines(github: &Github) -> Vec<String> {
        checks(github).iter().map(Check::line).collect()
    }

    #[test]
    fn a_repository_with_each_part_passes_each_check() {
        assert_eq!(
            lines(&ready()),
            [
                "pass  riff.toml on main: main of acme/app has it",
                "pass  auto-merge and squash merge: acme/app allows both",
                "pass  the ruleset of main: it requires the check riff/verify",
                "pass  the milestone Backlog: acme/app has it open",
                "pass  the first wave: acme/app has the open wave Wave 1",
                "pass  the checks of a pull request: the full check is Gate",
            ]
        );
    }

    #[test]
    fn each_missing_part_names_its_own_step() {
        let no_merge = Github {
            auto_merge: false,
            squash_merge: false,
            ..ready()
        };
        let no_rule = Github {
            required: vec!["Gate".into()],
            default_branch: "trunk".into(),
            ..ready()
        };
        let no_backlog = Github {
            milestones: vec!["Wave 1".into()],
            ..ready()
        };
        let no_wave = Github {
            milestones: vec!["Backlog".into()],
            ..ready()
        };
        let no_config = Github {
            files: vec![],
            ..ready()
        };
        let line = |github: &Github, at: usize| lines(github).swap_remove(at);
        assert_eq!(
            line(&no_config, 0),
            "fail  riff.toml on main: main of acme/app has no riff.toml, so riff lead start \
             stops. Fix: put riff.toml with the session image on main. See \"Add another \
             repository to the riff\" in the book"
        );
        assert_eq!(
            line(&no_merge, 1),
            "fail  auto-merge and squash merge: acme/app allows no auto-merge and no squash \
             merge. Fix: allow them: gh api -X PATCH repos/acme/app -F allow_auto_merge=true -F \
             allow_squash_merge=true"
        );
        let rule = line(&no_rule, 2);
        assert!(
            rule.starts_with(
                "fail  the ruleset of trunk: no ruleset of trunk requires the check riff/verify, \
                 so a merge does not wait for the verify. Fix: make a ruleset of trunk that \
                 requires a pull request, the check riff/verify and each check that your CI \
                 runs on a pull request. See "
            ),
            "{rule}"
        );
        assert_eq!(
            line(&no_backlog, 3),
            "fail  the milestone Backlog: acme/app has no open milestone Backlog. Fix: make it: \
             gh api -X POST repos/acme/app/milestones -f title=Backlog. riff wave make makes it \
             too, with the next wave"
        );
        assert_eq!(
            line(&no_wave, 4),
            "fail  the first wave: acme/app has no open wave. Fix: make it: riff wave make \
             \"Wave 1\""
        );
        // One failure leaves the other checks as they were.
        let failed = |github: &Github| checks(github).iter().filter(|c| !c.passed()).count();
        for github in [&no_merge, &no_rule, &no_backlog, &no_wave, &no_config] {
            assert_eq!(failed(github), 1);
        }
    }

    /// 01M5R1672NF56P15VD0JEX51K4: the check of the CI has no step when the
    /// pull request of `riff.toml` is not to come: only a person does its
    /// fix. A ruleset that requires no check of the CI, with pull requests
    /// that ran checks, is that case.
    #[test]
    fn the_check_of_the_ci_has_no_step() {
        let open = Github {
            required: vec!["riff/verify".into()],
            ci: vec!["checks / lint".into()],
            ..ready()
        };
        let failed: Vec<Check> = checks(&open).into_iter().filter(|c| !c.passed()).collect();
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert_eq!(failed[0].step, None);
        assert!(steps_of(&checks(&open)).is_empty());
        let line = checks(&open).swap_remove(5).line();
        assert_eq!(
            line,
            "fail  the checks of a pull request: pull requests of acme/app ran checks / lint, \
             and no rule requires them, so a merge does not wait for them. Fix: add them to \
             the ruleset of main. See \"Use the checks that you have\" in the book. Then run \
             riff init again"
        );
    }

    /// 01M5SMNKDWBD1J6Y18ENQG87CK: a repository with no CI passes the check
    /// of the CI. The line is a note: the verify is the only check.
    #[test]
    fn a_repository_with_no_ci_passes_with_a_note() {
        let no_ci = Github {
            required: vec!["riff/verify".into()],
            ci: vec![],
            ..ready()
        };
        assert!(checks(&no_ci).iter().all(Check::passed));
        assert_eq!(
            checks(&no_ci).swap_remove(5).line(),
            "pass  the checks of a pull request: acme/app has no CI: no pull request ran a \
             check, so riff/verify is the only check"
        );
    }

    /// 01M5RGG9KJXCKH8C94145WS8SM: no pull request ran a check yet, and the
    /// pull request of `riff.toml` is to come: the check has the step of the
    /// ruleset, which reads the checks of that pull request. When
    /// `riff.toml` is there, or the ruleset, no pull request is to come, and
    /// the check has no step.
    #[test]
    fn the_check_of_the_ci_has_a_step_while_the_pull_request_of_riff_toml_is_to_come() {
        let github = |files: &[&str], required: &[&str]| Github {
            files: files.iter().map(|f| (*f).into()).collect(),
            required: required.iter().map(|c| (*c).into()).collect(),
            ci: vec![],
            ..ready()
        };
        let step_of = |github: &Github| {
            let check = checks(github).swap_remove(5);
            assert_eq!(check.name, "the checks of a pull request");
            check.step
        };
        let to_come = github(&[], &[]);
        assert_eq!(step_of(&to_come), Some(Step::Ruleset));
        assert_eq!(
            checks(&to_come).swap_remove(5).planned_line(&to_come),
            "step  the checks of a pull request: no pull request of acme/app ran a check yet. \
             Step: make the ruleset riff on main"
        );
        assert!(!checks(&to_come).swap_remove(5).passed());
        assert_eq!(step_of(&github(&["riff.toml"], &[])), None);
        assert_eq!(step_of(&github(&[], &["riff/verify"])), None);
        // With riff.toml or the ruleset there, no pull request is to come: the repository
        // has no CI, and the check passes.
        assert!(checks(&github(&["riff.toml"], &[])).swap_remove(5).passed());
        assert!(
            checks(&github(&[], &["riff/verify"]))
                .swap_remove(5)
                .passed()
        );
        // The steps of the whole repository: riff.toml and the ruleset.
        let all: Vec<Step> = steps_of(&checks(&Github {
            auto_merge: false,
            milestones: vec![],
            ..to_come
        }));
        assert_eq!(
            all,
            [
                Step::Merge,
                Step::Config,
                Step::Backlog,
                Step::Wave,
                Step::Ruleset
            ]
        );
    }

    /// The check of the CI passes when a rule requires a check of the CI,
    /// when the ruleset that the step makes will require the checks that the
    /// pull requests ran, and when the repository has no CI: the verify is
    /// the only check (01M5SMNKDWBD1J6Y18ENQG87CK).
    #[test]
    fn the_check_of_the_ci_follows_the_rules_and_the_runs() {
        let line = |required: &[&str], ci: &[&str]| {
            let github = Github {
                required: required.iter().map(|c| (*c).into()).collect(),
                ci: ci.iter().map(|c| (*c).into()).collect(),
                ..ready()
            };
            checks(&github).swap_remove(5).line()
        };
        assert_eq!(
            line(&["checks / docs", "riff/verify"], &[]),
            "pass  the checks of a pull request: the full check is checks / docs"
        );
        // A check named Gate is a check like any other.
        assert_eq!(
            line(&["Gate", "riff/verify"], &[]),
            "pass  the checks of a pull request: the full check is Gate"
        );
        assert_eq!(
            line(&[], &["lint", "test"]),
            "pass  the checks of a pull request: pull requests ran lint and test, and the \
             ruleset that riff makes requires them"
        );
        // No CI: the verify is the only check, and the line says so.
        assert_eq!(
            line(&["riff/verify"], &[]),
            "pass  the checks of a pull request: acme/app has no CI: no pull request ran a \
             check, so riff/verify is the only check"
        );
        assert_eq!(
            line(&[], &[]),
            "pass  the checks of a pull request: acme/app has no CI: no pull request ran a \
             check, so riff/verify is the only check"
        );
        // The ruleset is there, the pull requests ran checks, and no rule
        // requires them: a merge does not wait for the CI.
        let open = line(&["riff/verify"], &["lint", "Gate"]);
        assert!(open.starts_with("fail  "), "{open}");
        assert!(
            open.contains("pull requests of acme/app ran lint and Gate, and no rule requires them"),
            "{open}"
        );
        assert!(
            open.contains("Fix: add them to the ruleset of main."),
            "{open}"
        );
        assert!(!open.contains("job Gate"), "{open}");
    }

    #[test]
    fn a_missing_key_of_the_repository_is_the_same_as_off() {
        // A person with no write access gets no `allow_` key.
        let json = r#"{"full_name":"acme/app","default_branch":"main"}"#;
        let repo: Repository = serde_json::from_str(json).unwrap();
        assert_eq!(
            (repo.allow_auto_merge, repo.allow_squash_merge),
            (None, None)
        );
        assert!(repo.permissions.is_none());
    }

    #[test]
    fn only_a_rule_of_required_status_checks_requires_checks() {
        let json = r#"[
            {"type":"deletion"},
            {"type":"pull_request","parameters":{"required_approving_review_count":0}},
            {"type":"required_status_checks","parameters":{"required_status_checks":[
                {"context":"Gate"},{"context":"riff/verify","integration_id":15368}]}},
            {"type":"required_status_checks"}
        ]"#;
        let rules: Vec<Rule> = serde_json::from_str(json).unwrap();
        let required: Vec<String> = rules.iter().flat_map(Rule::required).collect();
        assert_eq!(required, ["Gate", "riff/verify"]);
    }

    /// The fake forge of the tests of `riff init`: a fake `gh` for the
    /// repository `acme/app` that keeps its state in files
    /// (`tests/fake/forge`).
    const FORGE: &str = include_str!("../tests/fake/forge");

    /// A fake forge in a folder of its own.
    struct Fake {
        dir: tempfile::TempDir,
    }

    impl Fake {
        /// A forge with the `state` files, over the
        /// state of a bare repository: no `riff.toml`, no auto-merge, no
        /// ruleset, no milestone.
        fn new(state: &[(&str, &str)]) -> Fake {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path();
            std::fs::write(path.join("files"), "").unwrap();
            std::fs::write(path.join("milestones"), "").unwrap();
            for (name, text) in state {
                std::fs::write(path.join(name), text).unwrap();
            }
            isolated::write_program(path.join("gh"), FORGE);
            Fake { dir }
        }

        /// A bare repository whose latest pull request ran two checks.
        fn bare() -> Fake {
            Fake::new(&[
                (
                    "prior_pulls",
                    r#"[{"number":3,"html_url":"https://github.com/acme/app/pull/3","head":{"ref":"feature","sha":"abc1234"},"base":{"ref":"main"}}]"#,
                ),
                (
                    "checks_abc1234",
                    r#"{"check_runs":[{"name":"checks / lint"},{"name":"checks / test"},{"name":"checks / lint"}]}"#,
                ),
            ])
        }

        fn gh(&self) -> Gh {
            Gh::at(self.dir.path().join("gh"))
        }

        fn forge(&self) -> Forge {
            Forge {
                gh: self.gh(),
                waits: Waits::NONE,
            }
        }

        fn path(&self, name: &str) -> std::path::PathBuf {
            self.dir.path().join(name)
        }

        fn read_file(&self, name: &str) -> String {
            std::fs::read_to_string(self.path(name)).unwrap_or_default()
        }

        fn calls(&self) -> Vec<String> {
            self.read_file("calls").lines().map(Into::into).collect()
        }

        /// The calls that change the forge.
        fn changes(&self) -> Vec<String> {
            self.calls()
                .into_iter()
                .filter(|c| c.contains(" -X ") || c.starts_with("label "))
                .collect()
        }

        fn json(&self, name: &str) -> Value {
            serde_json::from_str(&self.read_file(name)).unwrap()
        }

        /// The facts that riff reads from the forge.
        fn github(&self) -> Github {
            read(&self.gh()).unwrap()
        }

        /// Reads, and does each step of each failed check.
        fn set_up(&self) -> Vec<Outcome> {
            let mut github = self.github();
            let steps = steps_of(&checks(&github));
            setup(&self.forge(), &steps, &mut github)
        }
    }

    #[test]
    fn read_gets_the_facts_of_a_bare_repository() {
        let fake = Fake::bare();
        let github = fake.github();
        assert_eq!(
            github,
            Github {
                repo: "acme/app".into(),
                default_branch: "main".into(),
                auto_merge: false,
                squash_merge: true,
                required: vec![],
                milestones: vec![],
                admin: true,
                files: vec![],
                ci: vec!["checks / lint".into(), "checks / test".into()],
            }
        );
        // The repository, the rules, the milestones, the files, the pull
        // requests, and the check runs of the first one.
        assert_eq!(fake.calls().len(), 6, "{:?}", fake.calls());
        assert!(fake.changes().is_empty());
    }

    #[test]
    fn read_asks_for_the_runs_of_a_pull_request_only_when_the_rules_have_no_check_of_the_ci() {
        let fake = Fake::new(&[(
            "static_rules",
            r#"[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"checks / docs"}]}}]"#,
        )]);
        let github = fake.github();
        assert_eq!(github.required, ["checks / docs"]);
        assert!(github.ci.is_empty());
        assert_eq!(fake.calls().len(), 4, "{:?}", fake.calls());
    }

    #[test]
    fn a_person_who_is_not_an_admin_is_not_an_admin() {
        let fake = Fake::new(&[("not_admin", "")]);
        assert!(!fake.github().admin);
        assert!(Fake::new(&[]).github().admin);
    }

    #[test]
    fn an_empty_repository_has_no_files_and_another_error_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let fails = |name: &str, said: &str| {
            Gh::at(isolated::write_program(
                dir.path().join(name),
                format!("#!/bin/sh\necho '{said}' >&2\nexit 1\n"),
            ))
        };
        let empty = fails("empty", "gh: This repository is empty. (HTTP 404)");
        assert_eq!(files(&empty, "main").unwrap(), Vec::<String>::new());
        // Only a 404 means no file.
        let broken = fails("broken", "gh: Server Error (HTTP 500)");
        let why = format!("{:#}", files(&broken, "main").unwrap_err());
        assert!(why.contains("HTTP 500"), "{why}");
    }

    /// 01M5R1672NF56P15VD0JEX51K4: the steps of a bare repository make
    /// `riff.toml` with a pull request that merges, allow auto-merge and
    /// squash merge, make the milestones, and make the ruleset with the
    /// checks of the CI and `riff/verify`.
    #[test]
    fn the_steps_set_up_a_bare_repository_and_a_second_run_changes_nothing() {
        let fake = Fake::bare();
        let github = fake.github();
        let failed: Vec<String> = checks(&github)
            .iter()
            .filter(|c| !c.passed())
            .map(|c| c.name.clone())
            .collect();
        assert_eq!(
            failed,
            [
                "riff.toml on main",
                "auto-merge and squash merge",
                "the ruleset of main",
                "the milestone Backlog",
                "the first wave",
            ]
        );
        let reads = fake.calls().len();
        let done = fake.set_up();
        let steps: Vec<Step> = done.iter().map(|(step, _)| *step).collect();
        assert_eq!(
            steps,
            [
                Step::Merge,
                Step::Config,
                Step::Backlog,
                Step::Wave,
                Step::Ruleset
            ]
        );
        let said: Vec<String> = done.iter().map(|(_, done)| done.clone().unwrap()).collect();
        assert_eq!(
            said,
            [
                "allowed both",
                "opened https://github.com/acme/app/pull/7 and merged it",
                "made Backlog",
                "made Wave 1",
                "requires checks / lint and checks / test and riff/verify",
            ]
        );
        // The order of the calls that change the forge.
        let changes = fake.changes();
        let starts: Vec<&str> = changes
            .iter()
            .map(|c| c.split(" --").next().unwrap())
            .collect();
        assert_eq!(
            starts,
            [
                "api -X PATCH repos/{owner}/{repo} -F allow_auto_merge=true -F allow_squash_merge=true",
                "api -X POST repos/{owner}/{repo}/git/refs",
                "api -X PUT repos/{owner}/{repo}/contents/riff.toml",
                "api -X POST repos/{owner}/{repo}/pulls",
                "api -X PUT repos/{owner}/{repo}/pulls/7/merge",
                "api -X DELETE repos/{owner}/{repo}/git/refs/heads/riff-setup",
                "api -X POST repos/{owner}/{repo}/milestones -f title=Backlog",
                "label create tracker",
                "api -X POST repos/{owner}/{repo}/milestones -f title=Wave 1",
                "api -X POST repos/{owner}/{repo}/rulesets",
            ],
            "{changes:#?}"
        );

        // The pull request: the branch, the title, and the squash merge.
        let file = fake.json("config.json");
        assert_eq!(file["branch"], SETUP_BRANCH);
        assert_eq!(file["message"], SETUP_TITLE);
        let content = base64::engine::general_purpose::STANDARD
            .decode(file["content"].as_str().unwrap())
            .unwrap();
        let content = String::from_utf8(content).unwrap();
        assert_eq!(
            content,
            format!("[session]\nimage = \"{}\"\n", session_image::of_release())
        );
        let pull = fake.json("pull.json");
        assert_eq!(pull["title"], "Set up riff");
        assert_eq!(pull["head"], "riff-setup");
        assert_eq!(pull["base"], "main");
        assert_eq!(fake.json("merge.json"), json!({ "merge_method": "squash" }));
        // The ruleset: a pull request, and the checks of the CI with the verify.
        let ruleset = fake.json("ruleset");
        assert_eq!(
            ruleset,
            ruleset_for(&["checks / lint".into(), "checks / test".into()])
        );
        assert_eq!(ruleset["name"], "riff");

        // The forge is set up: each check passes, and a second run makes no
        // call that changes the forge.
        let after = fake.github();
        assert_eq!(after.milestones, ["Wave 1", "Backlog"]);
        assert!(after.auto_merge && after.has("riff.toml"), "{after:?}");
        assert!(
            checks(&after).iter().all(Check::passed),
            "{:#?}",
            lines(&after)
        );
        let before = fake.calls().len();
        assert!(before > reads);
        assert!(fake.set_up().is_empty());
        assert_eq!(fake.changes().len(), changes.len());
    }

    #[test]
    fn the_steps_record_what_they_did_in_the_facts() {
        let fake = Fake::bare();
        let mut github = fake.github();
        let steps = steps_of(&checks(&github));
        let done = setup(&fake.forge(), &steps, &mut github);
        assert!(done.iter().all(|(_, done)| done.is_ok()), "{done:?}");
        // The facts that the steps leave are the facts that a new read gives.
        let mut read = fake.github();
        read.milestones.sort();
        github.milestones.sort();
        // riff reads the checks that the pull requests ran only when no rule requires one.
        github.ci.clear();
        assert_eq!(github, read);
        assert!(checks(&github).iter().all(Check::passed));
    }

    /// 01M5SMNKDWBD1J6Y18ENQG87CK: a repository with no CI gets a ruleset
    /// that requires only `riff/verify`, and each check passes after it.
    #[test]
    fn the_ruleset_requires_only_the_verify_when_no_pull_request_ran_a_check() {
        let fake = Fake::new(&[]);
        let mut github = fake.github();
        let done = setup(&fake.forge(), &[Step::Ruleset], &mut github);
        assert_eq!(done[0].1.as_deref(), Ok("requires riff/verify"));
        assert_eq!(fake.json("ruleset"), ruleset_for(&[]));
        assert_eq!(
            fake.json("ruleset"),
            serde_json::from_str::<Value>(RULESET).unwrap()
        );
        // No job Gate: the ruleset names no check of the CI.
        assert!(!fake.read_file("ruleset").contains("Gate"));
        // The check of the CI passes, with its note.
        let ci = checks(&github).swap_remove(5);
        assert!(ci.passed(), "{}", ci.line());
        assert!(ci.line().contains("has no CI"), "{}", ci.line());
    }

    /// 01M5SMNKDWBD1J6Y18ENQG87CK: the steps of `riff init --yes` in a
    /// repository with no CI make the ruleset with the verify alone, and
    /// each check passes after them.
    #[test]
    fn the_steps_set_up_a_repository_with_no_ci() {
        let fake = Fake::new(&[]);
        let done = fake.set_up();
        let said: Vec<String> = done.iter().map(|(_, done)| done.clone().unwrap()).collect();
        assert_eq!(
            said.last().map(String::as_str),
            Some("requires riff/verify"),
            "{said:?}"
        );
        assert_eq!(fake.json("ruleset"), ruleset_for(&[]));
        let after = fake.github();
        assert_eq!(after.required, ["riff/verify"]);
        assert!(after.ci.is_empty(), "{after:?}");
        assert!(
            checks(&after).iter().all(Check::passed),
            "{:#?}",
            lines(&after)
        );
        assert_eq!(full_check_of(&after.required), Vec::<String>::new());
    }

    #[test]
    fn the_ruleset_keeps_the_checks_that_the_rules_require_already() {
        let fake = Fake::new(&[(
            "static_rules",
            r#"[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"checks / docs"}]}}]"#,
        )]);
        let mut github = fake.github();
        let done = setup(&fake.forge(), &[Step::Ruleset], &mut github);
        assert!(done[0].1.is_ok(), "{done:?}");
        assert_eq!(fake.json("ruleset"), ruleset_for(&["checks / docs".into()]));
        // It asked for no pull request: the rules named the checks.
        assert!(!fake.calls().iter().any(|c| c.contains("pulls?state=all")));
    }

    /// The pull request of `riff.toml` is often the first one that has
    /// check runs, and its runs come after riff opens it.
    #[test]
    fn the_ruleset_waits_for_the_runs_of_the_pull_request_of_riff_toml() {
        let fake = Fake::new(&[
            ("late", "3"),
            (
                "checks_5e7a777",
                r#"{"check_runs":[{"name":"build"},{"name":"riff/verify"}]}"#,
            ),
        ]);
        let mut github = fake.github();
        let steps = [Step::Config, Step::Ruleset];
        let forge = Forge {
            waits: Waits {
                ci: Duration::from_secs(30),
                merge: Duration::ZERO,
                poll: Duration::from_millis(1),
            },
            ..fake.forge()
        };
        let done = setup(&forge, &steps, &mut github);
        assert!(done.iter().all(|(_, done)| done.is_ok()), "{done:?}");
        assert_eq!(fake.json("ruleset"), ruleset_for(&["build".into()]));

        // No wait: the runs are not there, and the ruleset requires the verify alone.
        let fake = Fake::new(&[
            ("late", "3"),
            ("checks_5e7a777", r#"{"check_runs":[{"name":"build"}]}"#),
        ]);
        let mut github = fake.github();
        let done = setup(&fake.forge(), &steps, &mut github);
        assert!(done.iter().all(|(_, done)| done.is_ok()), "{done:?}");
        assert_eq!(fake.json("ruleset"), ruleset_for(&[]));
    }

    #[test]
    fn a_pull_request_from_a_run_that_failed_is_reused_and_a_branch_is_reset() {
        // A branch and no pull request: the step resets the branch.
        let fake = Fake::new(&[("branch", "")]);
        let mut github = fake.github();
        let done = setup(&fake.forge(), &[Step::Config], &mut github);
        assert!(done[0].1.is_ok(), "{done:?}");
        let starts: Vec<String> = fake
            .changes()
            .iter()
            .map(|c| c.split(" --").next().unwrap().to_owned())
            .collect();
        assert!(
            starts.contains(
                &"api -X PATCH repos/{owner}/{repo}/git/refs/heads/riff-setup".to_owned()
            ),
            "{starts:#?}"
        );
        assert!(
            !starts.contains(&"api -X POST repos/{owner}/{repo}/git/refs".to_owned()),
            "{starts:#?}"
        );

        // An open pull request: the step opens none, and merges it.
        let open = r#"[{"number":7,"html_url":"https://github.com/acme/app/pull/7","head":{"ref":"riff-setup","sha":"5e7a777"},"base":{"ref":"main"}}]"#;
        let fake = Fake::new(&[("open_pulls", open), ("branch", "")]);
        let mut github = fake.github();
        let done = setup(&fake.forge(), &[Step::Config], &mut github);
        assert_eq!(
            done[0].1.as_deref(),
            Ok("found https://github.com/acme/app/pull/7 and merged it")
        );
        let starts: Vec<String> = fake
            .changes()
            .iter()
            .map(|c| c.split(" --").next().unwrap().to_owned())
            .collect();
        assert_eq!(
            starts,
            [
                "api -X PUT repos/{owner}/{repo}/pulls/7/merge",
                "api -X DELETE repos/{owner}/{repo}/git/refs/heads/riff-setup"
            ]
        );
        // Another pull request is not the pull request of riff.
        let other = open.replace("riff-setup", "feature");
        let fake = Fake::new(&[("open_pulls", &other)]);
        let mut github = fake.github();
        let done = setup(&fake.forge(), &[Step::Config], &mut github);
        assert!(
            done[0].1.as_deref().unwrap().starts_with("opened "),
            "{done:?}"
        );
    }

    #[test]
    fn github_may_need_a_moment_before_it_merges_a_new_pull_request() {
        let fake = Fake::new(&[("busy", "2")]);
        let mut github = fake.github();
        let forge = Forge {
            waits: Waits {
                ci: Duration::ZERO,
                merge: Duration::from_secs(30),
                poll: Duration::from_millis(1),
            },
            ..fake.forge()
        };
        let done = setup(&forge, &[Step::Config], &mut github);
        assert!(done[0].1.is_ok(), "{done:?}");
        let merges = fake.calls().iter().filter(|c| c.contains("/merge")).count();
        assert_eq!(merges, 3);

        // With no wait, one try: the step names the pull request.
        let fake = Fake::new(&[("busy", "1")]);
        let mut github = fake.github();
        let done = setup(&fake.forge(), &[Step::Config], &mut github);
        let why = done[0].1.clone().unwrap_err();
        assert!(
            why.starts_with(
                "riff opened https://github.com/acme/app/pull/7, but GitHub did not merge it. \
                 Merge it yourself, then run riff init again: "
            ),
            "{why}"
        );
        assert!(why.contains("HTTP 405"), "{why}");
        assert!(!github.has("riff.toml"), "a failed step changes no fact");
    }

    #[test]
    fn a_step_that_fails_says_why_and_the_next_step_runs() {
        let fake = Fake::new(&[("fail", "rulesets")]);
        let mut github = fake.github();
        let steps = steps_of(&checks(&github));
        let done = setup(&fake.forge(), &steps, &mut github);
        let failed: Vec<(Step, bool)> = done.iter().map(|(s, d)| (*s, d.is_ok())).collect();
        assert_eq!(
            failed,
            [
                (Step::Merge, true),
                (Step::Config, true),
                (Step::Backlog, true),
                (Step::Wave, true),
                (Step::Ruleset, false),
            ]
        );
        let why = done[4].1.clone().unwrap_err();
        assert!(why.contains("Must have admin rights"), "{why}");
        assert!(github.required.is_empty(), "a failed step changes no fact");
    }

    #[test]
    fn the_title_of_a_step_says_what_it_does() {
        let github = ready();
        let titles: Vec<String> = [
            Step::Merge,
            Step::Config,
            Step::Backlog,
            Step::Wave,
            Step::Ruleset,
        ]
        .iter()
        .map(|s| s.title(&github))
        .collect();
        assert_eq!(
            titles,
            [
                "allow auto-merge and squash merge",
                "make riff.toml on main with a pull request",
                "make the milestone Backlog",
                "make the milestone Wave 1",
                "make the ruleset riff on main",
            ]
        );
    }

    #[test]
    fn the_ruleset_requires_what_riff_checks() {
        let ruleset: serde_json::Value = serde_json::from_str(RULESET).unwrap();
        let rules: Vec<Rule> = serde_json::from_value(ruleset["rules"].clone()).unwrap();
        let required: Vec<String> = rules.iter().flat_map(Rule::required).collect();
        assert_eq!(required, [VERIFY_CONTEXT]);
        // The ruleset of the step passes the check of the ruleset.
        let github = Github {
            required,
            ..ready()
        };
        assert!(checks(&github).iter().all(Check::passed));
    }

    #[test]
    fn the_full_check_is_each_required_check_but_the_verify_and_else_none() {
        let names = |list: &[&str]| -> Vec<String> { list.iter().map(|c| (*c).into()).collect() };
        // The ruleset of riff: its checks, and the check of the verify.
        assert_eq!(
            full_check_of(&names(&["Gate", "Hygiene", "riff/verify"])),
            ["Gate", "Hygiene"]
        );
        // The four checks of a repository with no job Gate (#698).
        let four = [
            "checks / lint and test",
            "checks / docs",
            "checks / the lowest Rust version",
            "checks / build x86_64-unknown-linux-musl",
        ];
        let mut required = names(&four);
        required.push("riff/verify".into());
        assert_eq!(full_check_of(&required), four);
        // A check that two rulesets require counts one time.
        assert_eq!(full_check_of(&names(&["A", "B", "A"])), ["A", "B"]);
        // No other check: the repository has no CI, and the verify is the only check.
        assert!(full_check_of(&names(&["riff/verify"])).is_empty());
        assert!(full_check_of(&[]).is_empty());
    }

    /// 01M5SMNKDWBD1J6Y18ENQG87CK: with no required check and no CI, the full
    /// check has no name: `riff/verify` is the only check that gates a merge,
    /// and no check run is read for it.
    #[test]
    fn with_no_ci_the_full_check_is_empty_and_the_verify_is_the_only_check() {
        let dir = tempfile::tempdir().unwrap();
        let script = |rules: &str| {
            format!(
                r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/calls"
case "$*" in
  'api repos/acme/app') printf '%s' '{{"default_branch":"main"}}' ;;
  'api repos/acme/app/rules/branches/main?per_page=100') printf '%s' '{rules}' ;;
  *) echo "gh: no such call: $*" >&2; exit 1 ;;
esac
"#
            )
        };
        let verify_only = r#"[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"riff/verify"}]}}]"#;
        for (name, rules) in [("none", "[]"), ("verify", verify_only)] {
            let gh = Gh::at(isolated::write_program(
                dir.path().join(name),
                script(rules),
            ));
            let full = full_check(&gh, "acme/app").unwrap();
            assert!(full.is_empty(), "{name}: {full:?}");
            // An empty full check is done: the wave starts the verify, and
            // `riff verify pass` needs no check run.
            let head = "1a2b3c4";
            let gate = crate::pr::gate_of(&gh, "acme/app", &full, head).unwrap();
            assert_eq!(gate, crate::pr::Gate::Passed, "{name}");
        }
        let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
        assert!(!calls.contains("check-runs"), "{calls}");
        assert!(!calls.contains("Gate"), "{calls}");
    }

    #[test]
    fn the_required_checks_come_from_the_default_branch_of_the_named_repository() {
        let dir = tempfile::tempdir().unwrap();
        let script = r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/calls"
case "$*" in
  'api repos/acme/app') printf '%s' '{"default_branch":"feature/x"}' ;;
  'api repos/acme/app/rules/branches/feature%2Fx?per_page=100')
    printf '%s' '[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"checks / docs"},{"context":"riff/verify"}]}}]' ;;
  *) echo "gh: no such call: $*" >&2; exit 1 ;;
esac
"#;
        let gh = Gh::at(isolated::write_program(dir.path().join("gh"), script));
        assert_eq!(
            required(&gh, "acme/app").unwrap(),
            ["checks / docs", "riff/verify"]
        );
        assert_eq!(full_check(&gh, "acme/app").unwrap(), ["checks / docs"]);
        let why = format!("{:#}", full_check(&gh, "acme/other").unwrap_err());
        assert!(
            why.starts_with(
                "riff cannot read the required checks of the default branch of acme/other: "
            ),
            "{why}"
        );
        assert!(why.contains("no such call"), "{why}");
    }

    #[test]
    fn a_failed_step_says_what_gh_said() {
        let dir = tempfile::tempdir().unwrap();
        let gh = Gh::at(isolated::write_program(
            dir.path().join("gh"),
            "#!/bin/sh\necho 'HTTP 403: Must have admin rights to Repository.' >&2\nexit 1\n",
        ));
        let forge = Forge {
            gh,
            waits: Waits::NONE,
        };
        let mut github = ready();
        let done = setup(&forge, &[Step::Merge], &mut github);
        let why = done[0].1.clone().unwrap_err();
        assert!(why.contains("Must have admin rights"), "{why}");
    }

    #[test]
    fn read_says_why_when_gh_cannot_read() {
        let dir = tempfile::tempdir().unwrap();
        let gh = Gh::at(isolated::write_program(
            dir.path().join("gh"),
            "#!/bin/sh\necho 'gh: not logged in' >&2\nexit 4\n",
        ));
        let why = read(&gh).unwrap_err();
        assert!(why.contains("not logged in"), "{why}");
        let failed = checks_of(&Err(why));
        assert_eq!(failed.len(), 1);
        assert!(
            failed[0]
                .line()
                .starts_with("fail  the repository on GitHub: riff cannot read the repository: ")
        );
        assert!(failed[0].line().ends_with("gh auth login. The account needs write access to the repository: riff needs gh for the pull requests and the waves"));
    }

    #[test]
    fn the_body_of_a_reply_is_the_text_after_the_headers() {
        assert_eq!(
            body_of("HTTP/2.0 200 OK\r\nA: b\r\n\r\n{\"a\":1}"),
            "{\"a\":1}"
        );
        assert_eq!(body_of("HTTP/2.0 200 OK\n\n{}"), "{}");
        assert_eq!(body_of("{}"), "{}");
        assert_eq!(ref_of("feature/x"), "feature/x");
        assert_eq!(ref_of("a b"), "a%20b");
    }
}
