//! The wave of the daemon in a repository whose full check is not named
//! `Gate` (#698, 01M5QYDHN27RC5SRHAB56PFKK0): the look of the wave reads
//! the required checks of the ruleset of the default branch from a fake
//! `gh`, and the wave starts the verify of a pull request when each of
//! them is done on its head. The repository has no check run `Gate`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use riff::pr::Gh;
use riff::wave::{self, Work};

const REPO: &str = "o/r";

/// A fake `gh` for the repository `o/r` with a ruleset that requires four
/// checks and `riff/verify`. It logs each call to the file `calls`. The
/// file `docs` holds the check runs of `checks / docs`, and the file
/// `rules` the rules of `main`. The three other checks passed, and the
/// head has no check run `Gate`.
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

    // One failed: the check is done, so the wave starts the verify.
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

/// A repository whose ruleset requires no check but `riff/verify` has
/// the check `Gate` as its full check: the wave waits for it.
#[test]
fn a_ruleset_with_no_other_check_leaves_the_gate_as_the_full_check() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path());
    std::fs::write(
        dir.path().join("rules"),
        r#"[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"riff/verify"}]}}]"#,
    )
    .unwrap();
    // The fake forge has no run of `Gate`: the verify waits for it.
    let waiting = look(&gh).unwrap();
    assert!(!waiting.pulls[&5].gate_done);
    let calls = calls(dir.path());
    assert!(
        calls.contains(&"api repos/o/r/commits/eeee555/check-runs?check_name=Gate".to_owned()),
        "{calls:?}"
    );
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
