//! The wave of the daemon in a repository whose full check is not named
//! `Gate` (#698, 01M5QYDHN27RC5SRHAB56PFKK0): the look of the wave reads
//! the required checks of the ruleset of the default branch from a fake
//! `gh`, and the wave starts the verify of a pull request when each of
//! them is done on its head. The repository has no check run `Gate`. A
//! head with a failed check gets the fix of the check, not a verify
//! (#1575). A held verify does not start when the checks passed (#1816).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use riff::pr::Gh;
use riff::rollout::{OtherCheck, Stuck};
use riff::wave::{self, Failed, Notice, Work};

const REPO: &str = "o/r";

/// A fake `gh` for the repository `o/r` with a ruleset that requires four
/// checks and `riff/verify`. It logs each call to the file `calls`. The
/// file `docs` holds the check runs of `checks / docs`, the file `hygiene`
/// those of `Hygiene`, and the file `rules` the rules of `main`. The three
/// other checks passed, and the head has no check run `Gate`.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/calls"
case "$1 $2" in
  api*)
    case "$2" in
      repos/o/r) echo '{"default_branch":"main"}' ;;
      */rules/branches/main*) cat "$dir/rules" ;;
      */milestones*) echo '[{"title":"Backlog"},{"title":"Wave 1"}]' ;;
      *check_name=Gate) echo '{"total_count":0,"check_runs":[]}' ;;
      *check_name=Hygiene) cat "$dir/hygiene" ;;
      *check_name=checks%20%2F%20docs) cat "$dir/docs" ;;
      *check_name=checks%20%2F%20*) echo '{"check_runs":[{"status":"completed","conclusion":"success"}]}' ;;
      *) exit 1 ;;
    esac ;;
  "pr list") cat "$dir/pulls" ;;
  "issue list") cat "$dir/issues" ;;
  *) exit 1 ;;
esac
"#;

/// The rules of `main`: a pull request, and four checks with `riff/verify`.
const RULES: &str = r#"[
{"type":"pull_request","parameters":{"required_approving_review_count":0}},
{"type":"required_status_checks","parameters":{"required_status_checks":[
  {"context":"checks / lint and test"},{"context":"checks / docs"},
  {"context":"checks / the lowest Rust version"},
  {"context":"checks / build x86_64-unknown-linux-musl"},{"context":"riff/verify"}]}}]"#;

/// The check runs of `checks / docs` in the state `status`, with the
/// `conclusion` as JSON.
fn docs(status: &str, conclusion: &str) -> String {
    format!(r#"{{"check_runs":[{{"status":"{status}","conclusion":{conclusion}}}]}}"#)
}

/// The issue 5 of the wave, and its pull request at the head `eeee555`
/// with no `riff/verify` status: it waits for a verify.
fn forge(dir: &Path) -> Gh {
    let path = dir.join("gh");
    isolated::write_program(&path, FAKE_GH);
    std::fs::write(
        dir.join("issues"),
        r#"[{"number":5,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[]}]"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("pulls"),
        r#"[{"number":45,"headRefName":"worktree-issue-5","headRefOid":"eeee555","isDraft":false,"statusCheckRollup":[],"mergeable":"MERGEABLE","autoMergeRequest":{"enabledAt":"2026-10-24T08:00:00Z","mergeMethod":"SQUASH"},"body":"Closes #5\n\nIssue: #5\nMilestone: Wave 1\n"}]"#,
    )
    .unwrap();
    std::fs::write(dir.join("rules"), RULES).unwrap();
    std::fs::write(dir.join("docs"), docs("completed", r#""success""#)).unwrap();
    std::fs::write(dir.join("hygiene"), docs("completed", r#""success""#)).unwrap();
    Gh::at(path)
}

fn calls(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("calls"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn look(gh: &Gh) -> anyhow::Result<wave::Look> {
    wave::look_on(gh, REPO, BTreeSet::new(), BTreeSet::new())
}

/// The wave starts the verify of the pull request of issue 5 when the
/// four required checks passed on its head, and not before: a check that
/// runs holds it. A check that ended with a failure is done too: the
/// verify starts, and the verifier reports the failure. The look never
/// asks for a check named `Gate`.
#[test]
fn the_wave_starts_the_verify_when_the_required_checks_of_the_default_branch_are_done() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path());
    let none = BTreeSet::new();
    let starts = BTreeMap::new();
    let verify = Work::Verify {
        issue: 5,
        pull: 45,
        commit: "eeee555".into(),
    };

    // One of the four runs: the item is free work, and the pick waits.
    std::fs::write(dir.path().join("docs"), docs("in_progress", "null")).unwrap();
    let running = look(&gh).unwrap();
    assert_eq!(running.free, ["verify-issue-5"]);
    assert!(!running.pulls[&5].gate_done);
    assert_eq!(wave::pick(&running, &none, &starts), None);

    // One has no run yet.
    std::fs::write(dir.path().join("docs"), r#"{"check_runs":[]}"#).unwrap();
    assert_eq!(wave::pick(&look(&gh).unwrap(), &none, &starts), None);

    // The four passed: the wave starts the verify.
    std::fs::write(dir.path().join("docs"), docs("completed", r#""success""#)).unwrap();
    let passed = look(&gh).unwrap();
    assert!(passed.pulls[&5].gate_done);
    assert_eq!(wave::pick(&passed, &none, &starts), Some(verify.clone()));

    // One ended with a failure, and the rollup of the pull request shows no
    // failed check: the check is done, so the wave starts the verify.
    std::fs::write(dir.path().join("docs"), docs("completed", r#""failure""#)).unwrap();
    assert_eq!(
        wave::pick(&look(&gh).unwrap(), &none, &starts),
        Some(verify)
    );

    let calls = calls(dir.path());
    assert!(
        calls.contains(&"api repos/o/r/rules/branches/main?per_page=100".to_owned()),
        "{calls:?}"
    );
    assert!(
        calls.contains(
            &"api repos/o/r/commits/eeee555/check-runs?check_name=checks%20%2F%20docs".to_owned()
        ),
        "{calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.contains("check_name=Gate")),
        "{calls:?}"
    );
}

/// 01M5SMNKDWBD1J6Y18ENQG87CK: a repository whose ruleset requires no check
/// but `riff/verify` has no CI and no full check: the wave starts the verify
/// at once, and reads no check run, also not one named `Gate`.
#[test]
fn a_ruleset_with_no_other_check_has_no_full_check_and_the_verify_starts_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path());
    std::fs::write(
        dir.path().join("rules"),
        r#"[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"riff/verify"}]}}]"#,
    )
    .unwrap();
    // The fake forge has no run of any check: the verify does not wait.
    let waiting = look(&gh).unwrap();
    assert!(waiting.pulls[&5].gate_done);
    let calls = calls(dir.path());
    assert!(!calls.iter().any(|c| c.contains("check-runs")), "{calls:?}");
    assert!(!calls.iter().any(|c| c.contains("Gate")), "{calls:?}");
}

/// A look with no verify reads no rules, and a look that cannot read the
/// rules fails with the reason.
#[test]
fn the_look_reads_the_rules_only_for_a_verify_and_says_why_when_it_cannot() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path());
    std::fs::write(dir.path().join("pulls"), "[]").unwrap();
    assert!(look(&gh).unwrap().pulls.is_empty());
    assert!(
        !calls(dir.path()).iter().any(|c| c.contains("/rules/")),
        "no verify, no rules"
    );

    std::fs::write(dir.path().join("rules"), "not json").unwrap();
    std::fs::write(
        dir.path().join("pulls"),
        r#"[{"number":45,"headRefName":"worktree-issue-5","headRefOid":"eeee555","isDraft":false,"statusCheckRollup":[],"mergeable":"MERGEABLE","autoMergeRequest":null,"body":"Closes #5\n"}]"#,
    )
    .unwrap();
    let why = format!("{:#}", look(&gh).unwrap_err());
    assert!(
        why.starts_with("riff cannot read the required checks of the default branch of o/r"),
        "{why}"
    );
}

/// The rules of `main` for #1575: the jobs `Hygiene` and `Gate`, and
/// `riff/verify`.
const RULES_GATE: &str = r#"[
{"type":"required_status_checks","parameters":{"required_status_checks":[
  {"context":"Hygiene"},{"context":"Gate"},{"context":"riff/verify"}]}}]"#;

/// The pull request 45 of issue 5 at `eeee555` with the check run
/// `Hygiene` in its rollup, with `conclusion`.
fn pull_with_hygiene(dir: &Path, conclusion: &str) {
    pull_with_runs(dir, &[("Hygiene", conclusion)]);
}

/// #1575, PR #1573 at its commit cfed717: `Hygiene` failed, and the job
/// `Gate` had no run yet, as it starts when each other job ended. A failed
/// check next to a check with no run was "done", and the wave started a
/// verify that could only fail. Now the wave starts no verify of the head:
/// it starts the fix of the failed check. After the fix, the verify waits
/// for the `Gate`.
#[test]
fn a_failed_check_next_to_a_check_with_no_run_gets_its_fix_and_no_verify() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path());
    std::fs::write(dir.path().join("rules"), RULES_GATE).unwrap();
    std::fs::write(
        dir.path().join("hygiene"),
        docs("completed", r#""failure""#),
    )
    .unwrap();
    pull_with_hygiene(dir.path(), "FAILURE");
    let none = BTreeSet::new();
    let starts = BTreeMap::new();

    // `Hygiene` failed, `Gate` has no run: the fix, and no verify.
    let failed = look(&gh).unwrap();
    assert_eq!(failed.free, ["issue-5"]);
    assert!(failed.pulls.is_empty(), "{:?}", failed.pulls);
    assert_eq!(
        failed.fixes.failed[&5],
        Failed {
            pull: 45,
            commit: "eeee555".into(),
            checks: vec!["Hygiene".into()],
        }
    );
    let fix = Work::Checks {
        issue: 5,
        pull: 45,
        commit: "eeee555".into(),
        checks: vec!["Hygiene".into()],
    };
    assert_eq!(wave::pick(&failed, &none, &starts), Some(fix.clone()));
    // The same head gets one fix.
    let spent = [(fix.key(), 1)].into();
    assert_eq!(wave::pick(&failed, &none, &spent), None);

    // The fix pushed: the new head has `Hygiene` passed and no `Gate` run.
    // The verify waits for the `Gate`.
    std::fs::write(
        dir.path().join("hygiene"),
        docs("completed", r#""success""#),
    )
    .unwrap();
    pull_with_hygiene(dir.path(), "SUCCESS");
    let waiting = look(&gh).unwrap();
    assert_eq!(waiting.free, ["verify-issue-5"]);
    assert!(!waiting.pulls[&5].gate_done);
    assert_eq!(wave::pick(&waiting, &none, &starts), None);
    assert!(waiting.fixes.failed.is_empty());
}

/// #1816, 01M4ZV3B6M7PZRJ39XTXBZP3ZH: a held `verify-issue-5` starts no
/// verify of pull request 45, whose four required checks passed. A free
/// of the hold starts the verify at the next look. Auto-merge off holds
/// nothing: with no hold, the verify of a pull request with auto-merge
/// off starts too, and its pass turns auto-merge on
/// (01M748RKC7BYP8FC6W2E20SQA7).
#[test]
fn a_held_verify_starts_no_verify_of_a_pull_request_whose_checks_passed_and_a_free_starts_it() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path());
    let none = BTreeSet::new();
    let starts = BTreeMap::new();
    let verify = Work::Verify {
        issue: 5,
        pull: 45,
        commit: "eeee555".into(),
    };
    assert!(look(&gh).unwrap().pulls[&5].gate_done, "the checks passed");

    // The lead holds the verify: it is no free work, and nothing starts.
    let held: BTreeSet<String> = ["verify-issue-5".to_owned()].into();
    let on_hold = wave::look_on(&gh, REPO, BTreeSet::new(), held).unwrap();
    assert!(on_hold.free.is_empty(), "{:?}", on_hold.free);
    assert_eq!(wave::pick(&on_hold, &none, &starts), None);

    // The lead frees the hold: the next look starts the verify.
    let freed = look(&gh).unwrap();
    assert_eq!(freed.free, ["verify-issue-5"]);
    assert_eq!(wave::pick(&freed, &none, &starts), Some(verify.clone()));

    // Auto-merge off, and no hold: the verify starts.
    let pulls = std::fs::read_to_string(dir.path().join("pulls")).unwrap();
    let off = pulls.replace(
        r#""autoMergeRequest":{"enabledAt":"2026-10-24T08:00:00Z","mergeMethod":"SQUASH"}"#,
        r#""autoMergeRequest":null"#,
    );
    assert_ne!(off, pulls);
    std::fs::write(dir.path().join("pulls"), off).unwrap();
    assert_eq!(
        wave::pick(&look(&gh).unwrap(), &none, &starts),
        Some(verify)
    );
}

/// #1816: the book has the how-to of a hold of a pull request for a check
/// on a host, with its commands, and says that a pass turns auto-merge on.
#[test]
fn the_book_holds_a_pull_request_with_a_hold_of_its_verify() {
    let page = crate::book::page("development.md");
    let start = page
        .find("\n### Hold a pull request for a check on a host\n")
        .expect("the how-to");
    let how_to = &page[start..];
    let how_to = &how_to[..how_to[1..].find("\n### ").unwrap() + 1];
    let commands = crate::book::commands_in(how_to);
    for command in [
        "riff plan hold verify-issue-12 waits for the test on thelio",
        "riff plan free verify-issue-12",
    ] {
        assert!(commands.contains(&command.to_owned()), "{commands:?}");
    }
    assert!(how_to.contains("Auto-merge off does not hold a pull request."));
    let riff: Vec<String> = commands
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    crate::book::each_is_real(&riff);

    let start = page
        .find("\n#### Turn on auto-merge for a pull request\n")
        .expect("the how-to");
    assert!(page[start..].contains("\nA pass turns auto-merge on."));
}

/// The pull request 45 of issue 5 at `eeee555` with the check runs `runs`
/// of GitHub Actions in its rollup: each a name and a conclusion.
fn pull_with_runs(dir: &Path, runs: &[(&str, &str)]) {
    let runs: Vec<String> = runs
        .iter()
        .map(|(name, conclusion)| {
            format!(
                r#"{{"__typename":"CheckRun","name":"{name}","conclusion":"{conclusion}","workflowName":"CI","detailsUrl":"https://github.com/o/r/actions/runs/9/job/1"}}"#
            )
        })
        .collect();
    std::fs::write(
        dir.join("pulls"),
        format!(
            r#"[{{"number":45,"headRefName":"worktree-issue-5","headRefOid":"eeee555","isDraft":false,"statusCheckRollup":[{}],"mergeable":"MERGEABLE","autoMergeRequest":{{"enabledAt":"2026-10-24T08:00:00Z","mergeMethod":"SQUASH"}},"body":"Closes #5\n\nIssue: #5\nMilestone: Wave 1\n"}}]"#,
            runs.join(",")
        ),
    )
    .unwrap();
}

/// 01M749P62T24VDPX20D40KRJEK, strata PR #774: the new workflow `knurl
/// adapter` failed on the head, and the ruleset of `main` does not require
/// it. Each of the four required checks passed. The look reads the
/// ruleset, and the wave starts the verify of the head, not a fix: the
/// lead gets one notice for the run. A failed required check still gets
/// the fix and no verify.
#[test]
fn a_failed_run_that_the_ruleset_does_not_require_holds_no_verify() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path());
    let none = BTreeSet::new();
    let starts = BTreeMap::new();
    let knurl = Notice {
        pull: 45,
        branch: "worktree-issue-5".into(),
        commit: "eeee555".into(),
        stuck: Stuck::Optional(OtherCheck {
            name: "knurl adapter".into(),
            app: Some("github.com".into()),
            link: Some("https://github.com/o/r/actions/runs/9/job/1".into()),
        }),
    };

    pull_with_runs(
        dir.path(),
        &[("checks / docs", "SUCCESS"), ("knurl adapter", "FAILURE")],
    );
    let optional = look(&gh).unwrap();
    assert_eq!(optional.free, ["verify-issue-5"]);
    assert!(optional.fixes.failed.is_empty());
    assert!(optional.pulls[&5].gate_done);
    assert_eq!(
        wave::pick(&optional, &none, &starts),
        Some(Work::Verify {
            issue: 5,
            pull: 45,
            commit: "eeee555".into(),
        })
    );
    assert_eq!(optional.fixes.notices, std::slice::from_ref(&knurl));
    assert_eq!(
        optional.fixes.notices[0].key(),
        "#45@eeee555:optional check:knurl adapter"
    );
    assert!(
        calls(dir.path())
            .iter()
            .any(|c| c.contains("/rules/branches/main")),
        "the look reads the ruleset"
    );
    // `riff top` reads the same pull requests, and shows no stuck one.
    let pulls = riff::rollout::pulls(&gh, REPO).unwrap();
    assert_eq!(pulls[0].stuck(false), None);

    // A required check failed too: the fix of it, no verify, and the same
    // notice.
    std::fs::write(dir.path().join("docs"), docs("completed", r#""failure""#)).unwrap();
    pull_with_runs(
        dir.path(),
        &[("checks / docs", "FAILURE"), ("knurl adapter", "FAILURE")],
    );
    let required = look(&gh).unwrap();
    assert_eq!(required.free, ["issue-5"]);
    assert!(required.pulls.is_empty(), "{:?}", required.pulls);
    assert_eq!(
        wave::pick(&required, &none, &starts),
        Some(Work::Checks {
            issue: 5,
            pull: 45,
            commit: "eeee555".into(),
            checks: vec!["checks / docs".into()],
        })
    );
    assert_eq!(required.fixes.notices, [knurl]);

    // The message names the run and its link, and says that the verify
    // goes on. "See a failed check that no rule requires" in the book shows
    // it.
    let example = Stuck::Optional(OtherCheck {
        name: "knurl adapter".into(),
        app: Some("github.com".into()),
        link: Some("https://github.com/como-technologies/riff/actions/runs/9/job/1".into()),
    });
    let message = riff::text::wave_stuck(
        REPO,
        40,
        "worktree-issue-12",
        "main",
        "1a2b3c4d5e",
        &example,
    );
    assert!(
        message.contains("so the verify and the merge go on"),
        "{message}"
    );
    let book = crate::book::page("development.md");
    assert!(book.contains(&format!("\n{message}\n")), "{message}");
}
