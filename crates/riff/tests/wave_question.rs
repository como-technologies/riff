//! The free work of a wave has no build of an issue with the label
//! `question` (01M6QBJHM5BQR59QX2K2BMZVYH, #1605): the work waits for the
//! answer of a person. The look of the wave reads the open issues from a
//! fake `gh`. A verify of an open pull request goes on. When the label
//! goes, the issue is free work at the next look.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use riff::pr::Gh;
use riff::wave::{self, Work};

const REPO: &str = "o/r";

/// A fake `gh`: the milestones, the file `pulls` for `pr list`, the file
/// `issues` for `issue list`, the default branch with no rule (so the
/// full check is `Gate`), and a passed Gate for each commit.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
case "$1 $2" in
  api*)
    case "$2" in
      repos/o/r) echo '{"default_branch":"main"}' ;;
      */rules/branches/main*) echo '[]' ;;
      */milestones*) echo '[{"title":"Backlog"},{"title":"Wave 1"}]' ;;
      */check-runs*) echo '{"check_runs":[{"status":"completed","conclusion":"success","head_sha":"x"}]}' ;;
      *) exit 1 ;;
    esac ;;
  "pr list") cat "$dir/pulls" ;;
  "issue list") cat "$dir/issues" ;;
  *) exit 1 ;;
esac
"#;

/// The issue `number` of the wave with the labels `labels`.
fn issue(number: u64, labels: &[&str]) -> String {
    let labels: Vec<String> = labels
        .iter()
        .map(|l| format!(r#"{{"name":"{l}"}}"#))
        .collect();
    format!(
        r#"{{"number":{number},"title":"t","body":"Done when: x.","comments":[],"milestone":{{"title":"Wave 1"}},"labels":[{}]}}"#,
        labels.join(",")
    )
}

/// The pull request of issue `issue`, which waits for a verify.
fn pull(number: u64, issue: u64, conflict: bool) -> String {
    let mergeable = if conflict { "CONFLICTING" } else { "MERGEABLE" };
    format!(
        r#"{{"number":{number},"headRefName":"worktree-issue-{issue}","headRefOid":"1a2b3c4","isDraft":false,"statusCheckRollup":[],"mergeable":"{mergeable}","autoMergeRequest":{{"enabledAt":"2026-11-05T00:00:00Z"}},"body":"Closes #{issue}\n"}}"#
    )
}

/// The fake `gh` in `dir`, with the open issues `issues` and the open pull
/// requests `pulls`.
fn forge(dir: &Path, issues: &[String], pulls: &[String]) -> Gh {
    let path = dir.join("gh");
    isolated::write_program(&path, FAKE_GH);
    std::fs::write(dir.join("issues"), format!("[{}]", issues.join(","))).unwrap();
    std::fs::write(dir.join("pulls"), format!("[{}]", pulls.join(","))).unwrap();
    Gh::at(path)
}

/// The look of the wave from `gh`, with no claim and the holds `held`.
fn looks(gh: &Gh, held: &[&str]) -> wave::Look {
    let held = held.iter().map(|h| (*h).to_owned()).collect();
    wave::look_on(gh, REPO, BTreeSet::new(), held).unwrap()
}

#[test]
fn an_issue_with_the_label_question_is_not_free_work_and_the_same_issue_without_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let issues = [
        issue(1, &["question"]),
        issue(2, &["bug"]),
        // Its pull request waits for a verify: the verify goes on.
        issue(3, &["question"]),
        // Its pull request has a conflict: no rebase.
        issue(4, &["bug", "question"]),
        issue(5, &[]),
    ];
    let pulls = [pull(43, 3, false), pull(44, 4, true)];
    let gh = forge(dir.path(), &issues, &pulls);
    let look = looks(&gh, &[]);
    assert_eq!(look.free, ["verify-issue-3", "issue-2", "issue-5"]);
    assert!(
        look.fixes.conflicts.contains_key(&4),
        "the conflict is a fact"
    );
    assert!(look.no_done_when.is_empty());
    // The verify comes first. While it runs, the first build is issue 2.
    let none = BTreeSet::new();
    let verify = Work::Verify {
        issue: 3,
        pull: 43,
        commit: "1a2b3c4".into(),
    };
    assert_eq!(wave::pick(&look, &none, &BTreeMap::new()), Some(verify));
    let running = BTreeSet::from(["verify-issue-3".to_owned()]);
    assert_eq!(
        wave::pick(&look, &running, &BTreeMap::new()),
        Some(Work::Build { issue: 2 })
    );

    // The person answered, and the lead removed the label from 1 and 4: the
    // next look takes both. 4 is a rebase.
    let issues = [issue(1, &[]), issue(4, &["bug"])];
    let look = looks(&forge(dir.path(), &issues, &pulls[1..]), &[]);
    assert_eq!(look.free, ["issue-4", "issue-1"]);
    assert!(look.fixes.conflicts.contains_key(&4));
}

#[test]
fn an_issue_with_the_label_question_and_no_done_when_line_gets_no_message() {
    let dir = tempfile::tempdir().unwrap();
    let bare = |number, labels: &str| {
        format!(
            r#"{{"number":{number},"title":"t","body":"No line.","comments":[],"milestone":{{"title":"Wave 1"}},"labels":[{labels}]}}"#
        )
    };
    let issues = [bare(1, r#"{"name":"question"}"#), bare(2, "")];
    let look = looks(&forge(dir.path(), &issues, &[]), &[]);
    assert!(look.free.is_empty());
    assert_eq!(look.no_done_when, BTreeSet::from([2]));
}
