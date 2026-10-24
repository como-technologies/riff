//! The setup of a repository on GitHub that riff needs: three checks of
//! `riff init` (01M5QVF5535YRN1EF3DZAB4W8G).
//!
//! # Design
//!
//! `riff init` checks three facts of the repository of the current
//! folder, and names the step of each fact that fails. A person does
//! the step, for example from the how-to "Add another repository to the
//! riff" in the book. Each failed check also carries its [`Step`]: the
//! code of the step, so a later change of `riff init` can do the step
//! after a question, with no new code per step. The step and the how-to
//! make the same change: the ruleset of the how-to is [`RULESET`]
//! (01M5QWA0E9A4ZJ8X49EM2MJM5Q).
//!
//! ```mermaid
//! flowchart LR
//!     O["remote origin<br/>github.com/OWNER/REPO"] --> R["gh api repos/OWNER/REPO"]
//!     O --> B["gh api repos/OWNER/REPO/rules/branches/BRANCH"]
//!     O --> M["gh api repos/OWNER/REPO/milestones"]
//!     R --> C["the three checks"]
//!     B --> C
//!     M --> C
//!     C --> L["pass or fail lines of riff init"]
//! ```
//!
//! | Check | riff reads | Pass |
//! |---|---|---|
//! | auto-merge and squash merge | `allow_auto_merge` and `allow_squash_merge` of the repository | both are on: the work sessions run `gh pr merge --auto --squash` |
//! | the ruleset of the default branch | the rules that apply to the `default_branch` of the repository ([`rules_path`]) | a `required_status_checks` rule has the check [`VERIFY_CONTEXT`]: without it, nothing on GitHub makes a merge wait for the verify |
//! | the milestone `Backlog` | the open milestones ([`crate::waves::open`]) | one has the title [`BACKLOG`]: `riff wave move --to Backlog` and `riff wave end --move-open-to-backlog` need it |
//!
//! The fix of the ruleset names the checks [`GATE`] and [`VERIFY_CONTEXT`]:
//! the wave starts a verify only when a check run named [`GATE`] passed
//! on the head of the pull request ([`crate::pr::gate_of`]).
//!
//! A folder with no remote `origin` on GitHub has no such check
//! ([`crate::identity::github_origin`]). When `gh` cannot read the
//! repository, one check fails ([`crate::text::INIT_GITHUB`]) and the
//! others do not run.
//!
//! ```
//! use riff::repo_setup::{Github, checks};
//!
//! let mut github = Github {
//!     repo: "acme/app".into(),
//!     default_branch: "main".into(),
//!     auto_merge: true,
//!     squash_merge: true,
//!     required: vec!["Gate".into(), "riff/verify".into()],
//!     milestones: vec!["Backlog".into(), "Wave 1".into()],
//! };
//! assert!(checks(&github).iter().all(|c| c.passed()));
//!
//! github.auto_merge = false;
//! github.required = vec!["Gate".into()];
//! github.milestones = vec!["Wave 1".into()];
//! let failed: Vec<String> = checks(&github).iter().filter(|c| !c.passed()).map(|c| c.name.clone()).collect();
//! assert_eq!(failed, ["auto-merge and squash merge", "the ruleset of main", "the milestone Backlog"]);
//! ```

use std::time::Duration;

use serde::Deserialize;

use crate::first_run::Check;
use crate::pr::{GATE, Gh, VERIFY_CONTEXT};
use crate::text;
use crate::waves::{self, BACKLOG};

/// The time that riff waits for each call of `gh`.
pub const LIMIT: Duration = Duration::from_secs(30);

/// The ruleset that [`Step::Ruleset`] makes: a pull request with squash
/// merges only, no bypass, and the checks [`GATE`] and [`VERIFY_CONTEXT`]
/// on the default branch. The how-to "Add another repository to the
/// riff" shows the same text, and a test checks that they agree.
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
          { "context": "Gate" },
          { "context": "riff/verify" }
        ]
      }
    }
  ]
}"#;

/// The step that fixes a failed check of the repository on GitHub. Each
/// step needs the rights of an admin of the repository, and it is safe
/// to do again.
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
/// };
/// let steps: Vec<Step> = checks(&github).iter().filter_map(|c| c.step).collect();
/// assert_eq!(steps, [Step::Merge, Step::Ruleset, Step::Backlog]);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Allow auto-merge and squash merge: `gh api -X PATCH`.
    Merge,
    /// Make the ruleset [`RULESET`]: `gh api -X POST .../rulesets`, or
    /// `PUT` when a ruleset named `riff` is there.
    Ruleset,
    /// Make the open milestone [`BACKLOG`]
    /// ([`crate::waves::make_backlog`]).
    Backlog,
}

impl Step {
    /// Does the step in the repository of `gh`.
    pub fn apply(self, gh: &Gh) -> anyhow::Result<()> {
        match self {
            Step::Merge => gh
                .run(
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
                )
                .map(drop),
            Step::Ruleset => {
                let body: serde_json::Value = serde_json::from_str(RULESET)?;
                let rulesets: Vec<serde_json::Value> =
                    gh.json(&["api", "repos/{owner}/{repo}/rulesets?per_page=100"])?;
                let name = body["name"].as_str();
                let there = rulesets.iter().find(|r| r["name"].as_str() == name);
                match there.and_then(|r| r["id"].as_u64()) {
                    Some(id) => gh.send(
                        "PUT",
                        &format!("repos/{{owner}}/{{repo}}/rulesets/{id}"),
                        &body,
                    ),
                    None => gh.send("POST", "repos/{owner}/{repo}/rulesets", &body),
                }
                .map(drop)
            }
            Step::Backlog => waves::make_backlog(gh).map(drop),
        }
    }
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

/// The path of the rules that apply to `branch`: GitHub gives the active
/// rules of each ruleset, with no other detail. A name goes into the path
/// with each byte but a letter, a digit and `-_.` as `%XX`.
///
/// ```
/// use riff::repo_setup::rules_path;
///
/// assert_eq!(rules_path("main"), "repos/{owner}/{repo}/rules/branches/main?per_page=100");
/// assert_eq!(rules_path("release/2.x"), "repos/{owner}/{repo}/rules/branches/release%2F2.x?per_page=100");
/// ```
pub fn rules_path(branch: &str) -> String {
    let mut name = String::new();
    for byte in branch.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => name.push(byte as char),
            other => name.push_str(&format!("%{other:02X}")),
        }
    }
    format!("repos/{{owner}}/{{repo}}/rules/branches/{name}?per_page=100")
}

/// Reads the facts of the repository of `gh` with three calls, or says
/// why it cannot.
pub fn read(gh: &Gh) -> Result<Github, String> {
    let why = |e: anyhow::Error| format!("{e:#}");
    let repo: Repository = gh.json(&["api", "repos/{owner}/{repo}"]).map_err(why)?;
    let rules: Vec<Rule> = gh
        .json(&["api", &rules_path(&repo.default_branch)])
        .map_err(why)?;
    let milestones = waves::open(gh).map_err(why)?;
    Ok(Github {
        repo: repo.full_name,
        default_branch: repo.default_branch,
        auto_merge: repo.allow_auto_merge.unwrap_or(false),
        squash_merge: repo.allow_squash_merge.unwrap_or(false),
        required: rules.iter().flat_map(Rule::required).collect(),
        milestones: milestones.into_iter().map(|m| m.title).collect(),
    })
}

/// The facts of the repository of the folder `dir`, with the `gh` of
/// the machine: `None` when the folder has no remote `origin` on GitHub.
pub fn here(dir: &std::path::Path) -> Option<Result<Github, String>> {
    let repo = crate::identity::github_origin(dir)?;
    Some(read(&Gh::for_repo(&repo).with_limit(LIMIT)))
}

/// The checks on the facts `github`, in order: the merge settings, the
/// ruleset of the default branch and the milestone `Backlog`. An `Err`
/// is one failed check ([`text::INIT_GITHUB`]).
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

/// The three checks on `github`.
pub fn checks(github: &Github) -> Vec<Check> {
    let repo = &github.repo;
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
    let branch = &github.default_branch;
    let ruleset = text::init_ruleset(branch);
    let ruleset = if github.required.iter().any(|c| c == VERIFY_CONTEXT) {
        Check::pass(&ruleset, text::init_ruleset_on(VERIFY_CONTEXT))
    } else {
        Check::fail(
            &ruleset,
            text::init_ruleset_off(branch, VERIFY_CONTEXT),
            text::init_fix_ruleset(branch, GATE, VERIFY_CONTEXT),
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
    vec![merge, ruleset, backlog]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ready() -> Github {
        Github {
            repo: "acme/app".into(),
            default_branch: "main".into(),
            auto_merge: true,
            squash_merge: true,
            required: vec!["Gate".into(), "riff/verify".into()],
            milestones: vec!["Backlog".into(), "Wave 1".into()],
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
                "pass  auto-merge and squash merge: acme/app allows both",
                "pass  the ruleset of main: it requires the check riff/verify",
                "pass  the milestone Backlog: acme/app has it open",
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
        let line = |github: &Github, at: usize| lines(github).swap_remove(at);
        assert_eq!(
            line(&no_merge, 0),
            "fail  auto-merge and squash merge: acme/app allows no auto-merge and no squash \
             merge. Fix: allow them: gh api -X PATCH repos/acme/app -F allow_auto_merge=true -F \
             allow_squash_merge=true"
        );
        let rule = line(&no_rule, 1);
        assert!(
            rule.starts_with(
                "fail  the ruleset of trunk: no ruleset of trunk requires the check riff/verify, \
                 so a merge does not wait for the verify. Fix: make a ruleset of trunk that \
                 requires a pull request and the checks Gate and riff/verify. See "
            ),
            "{rule}"
        );
        assert_eq!(
            line(&no_backlog, 2),
            "fail  the milestone Backlog: acme/app has no open milestone Backlog. Fix: make it: \
             gh api -X POST repos/acme/app/milestones -f title=Backlog. riff wave make makes it \
             too, with the next wave"
        );
        // One failure leaves the other two checks as they were.
        let failed = |github: &Github| checks(github).iter().filter(|c| !c.passed()).count();
        assert_eq!(
            (failed(&no_merge), failed(&no_rule), failed(&no_backlog)),
            (1, 1, 1)
        );
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

    /// A fake `gh` in `dir` with the answers of the three calls.
    fn fake_gh(dir: &Path, repo: &str, rules: &str, milestones: &str) -> Gh {
        let script = format!(
            "#!/bin/sh\necho \"$*\" >> \"$(dirname \"$0\")/calls\"\ncase \"$*\" in\n\
             'api repos/{{owner}}/{{repo}}') printf '%s' '{repo}' ;;\n\
             'api repos/{{owner}}/{{repo}}/rules/branches/main?per_page=100') printf '%s' '{rules}' ;;\n\
             'api repos/{{owner}}/{{repo}}/milestones?state=open&per_page=100') printf '%s' '{milestones}' ;;\n\
             *) echo \"gh: no such call: $*\" >&2; exit 1 ;;\nesac\n"
        );
        Gh::at(isolated::write_program(dir.join("gh"), script))
    }

    const REPO: &str = r#"{"full_name":"acme/app","default_branch":"main","allow_auto_merge":true,"allow_squash_merge":false}"#;
    const RULES: &str = r#"[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"riff/verify"}]}}]"#;
    const MILESTONES: &str = r#"[{"number":1,"title":"Wave 1","open_issues":0,"closed_issues":0}]"#;

    #[test]
    fn read_gets_the_facts_with_three_calls() {
        let dir = tempfile::tempdir().unwrap();
        let github = read(&fake_gh(dir.path(), REPO, RULES, MILESTONES)).unwrap();
        assert_eq!(
            github,
            Github {
                repo: "acme/app".into(),
                default_branch: "main".into(),
                auto_merge: true,
                squash_merge: false,
                required: vec!["riff/verify".into()],
                milestones: vec!["Wave 1".into()],
            }
        );
        let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
        assert_eq!(calls.lines().count(), 3, "{calls}");
    }

    /// A fake `gh` in `dir` for the steps. It logs each call to
    /// `dir/calls`, lists the rulesets of `dir/rulesets.json`, and keeps
    /// the body of a ruleset in `dir/body.json`.
    fn fake_steps(dir: &Path, rulesets: &str) -> Gh {
        std::fs::write(dir.join("rulesets.json"), rulesets).unwrap();
        let script = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/calls"
case "$*" in
  'api repos/{owner}/{repo}/rulesets?per_page=100') cat "$dir/rulesets.json" ;;
  'api -X POST repos/{owner}/{repo}/rulesets --include --input '*|'api -X PUT repos/{owner}/{repo}/rulesets/77 --include --input '*)
    for last; do :; done
    cp "$last" "$dir/body.json"
    printf 'HTTP/2.0 200 OK\r\n\r\n{}' ;;
  'api -X PATCH repos/{owner}/{repo} '*) ;;
  'api -X POST repos/{owner}/{repo}/milestones -f title=Backlog') printf '{"number":5,"title":"Backlog","open_issues":0,"closed_issues":0}' ;;
  *) echo "gh: no such call: $*" >&2; exit 1 ;;
esac
"#;
        Gh::at(isolated::write_program(dir.join("gh"), script))
    }

    fn calls(dir: &Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(Into::into)
            .collect()
    }

    #[test]
    fn the_ruleset_requires_what_riff_checks() {
        let ruleset: serde_json::Value = serde_json::from_str(RULESET).unwrap();
        let rules: Vec<Rule> = serde_json::from_value(ruleset["rules"].clone()).unwrap();
        let required: Vec<String> = rules.iter().flat_map(Rule::required).collect();
        assert_eq!(required, [GATE, VERIFY_CONTEXT]);
        // The ruleset of the step passes the check of the ruleset.
        let github = Github {
            required,
            ..ready()
        };
        assert!(checks(&github).iter().all(Check::passed));
    }

    #[test]
    fn the_step_of_the_merge_settings_turns_on_both() {
        let dir = tempfile::tempdir().unwrap();
        Step::Merge.apply(&fake_steps(dir.path(), "[]")).unwrap();
        assert_eq!(
            calls(dir.path()),
            [
                "api -X PATCH repos/{owner}/{repo} -F allow_auto_merge=true -F allow_squash_merge=true"
            ]
        );
    }

    #[test]
    fn the_step_of_the_ruleset_makes_it_and_then_updates_the_same_one() {
        let dir = tempfile::tempdir().unwrap();
        Step::Ruleset.apply(&fake_steps(dir.path(), "[]")).unwrap();
        let log = calls(dir.path());
        assert_eq!(log[0], "api repos/{owner}/{repo}/rulesets?per_page=100");
        assert!(
            log[1].starts_with("api -X POST repos/{owner}/{repo}/rulesets --include --input "),
            "{log:?}"
        );
        let sent: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("body.json")).unwrap())
                .unwrap();
        assert_eq!(
            sent,
            serde_json::from_str::<serde_json::Value>(RULESET).unwrap()
        );

        let other = tempfile::tempdir().unwrap();
        let there = r#"[{"id":5,"name":"other"},{"id":77,"name":"riff"}]"#;
        Step::Ruleset
            .apply(&fake_steps(other.path(), there))
            .unwrap();
        let log = calls(other.path());
        assert!(
            log[1].starts_with("api -X PUT repos/{owner}/{repo}/rulesets/77 --include --input "),
            "{log:?}"
        );
    }

    #[test]
    fn the_step_of_the_backlog_makes_the_milestone() {
        let dir = tempfile::tempdir().unwrap();
        Step::Backlog.apply(&fake_steps(dir.path(), "[]")).unwrap();
        assert_eq!(
            calls(dir.path()),
            ["api -X POST repos/{owner}/{repo}/milestones -f title=Backlog"]
        );
    }

    #[test]
    fn a_failed_step_says_what_gh_said() {
        let dir = tempfile::tempdir().unwrap();
        let gh = Gh::at(isolated::write_program(
            dir.path().join("gh"),
            "#!/bin/sh\necho 'HTTP 403: Must have admin rights to Repository.' >&2\nexit 1\n",
        ));
        let why = Step::Merge.apply(&gh).unwrap_err().to_string();
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
}
