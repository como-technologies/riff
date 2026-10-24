//! The free work of a wave has no build of an issue whose body has no
//! line `Done when:` (01M5R0HEP1MXWYY643F1N1DWEM, #1391): the look of the
//! wave reads the open issues from a fake `gh`. A session stops at once
//! without the line, so a start would only spend one of the 3 starts of
//! the item. A verify is not changed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use riff::pr::Gh;
use riff::wave::{self, Work};

const REPO: &str = "o/r";

/// A fake `gh`: the milestones, the file `pulls` for `pr list`, the file
/// `issues` for `issue list`, and a passed Gate for each commit.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
case "$1 $2" in
  api*)
    case "$2" in
      */milestones*) echo '[{"title":"Backlog"},{"title":"Wave 1"}]' ;;
      */check-runs*) echo '{"check_runs":[{"status":"completed","conclusion":"success","head_sha":"x"}]}' ;;
      *) exit 1 ;;
    esac ;;
  "pr list") cat "$dir/pulls" ;;
  "issue list") cat "$dir/issues" ;;
  *) exit 1 ;;
esac
"#;

/// The issue `number` of the wave with `body`.
fn issue(number: u64, body: &str) -> String {
    format!(
        r#"{{"number":{number},"title":"t","body":"{body}","comments":[],"milestone":{{"title":"Wave 1"}},"labels":[]}}"#
    )
}

/// The pull request of issue 4, which waits for a verify.
const PULL: &str = r#"[{"number":44,"headRefName":"worktree-issue-4","headRefOid":"1a2b3c4","isDraft":false,"statusCheckRollup":[],"mergeable":"MERGEABLE","autoMergeRequest":null,"body":"Closes #4\n"}]"#;

/// The fake `gh` in `dir`, with the open issues `issues` and the open pull
/// requests `pulls`.
fn forge(dir: &Path, issues: &[String], pulls: &str) -> Gh {
    let path = dir.join("gh");
    isolated::write_program(&path, FAKE_GH);
    std::fs::write(dir.join("issues"), format!("[{}]", issues.join(","))).unwrap();
    std::fs::write(dir.join("pulls"), pulls).unwrap();
    Gh::at(path)
}

/// The look of the wave from `gh`, with no claim and the holds `held`.
fn looks(gh: &Gh, held: &[&str]) -> wave::Look {
    let held = held.iter().map(|h| (*h).to_owned()).collect();
    wave::look_on(gh, REPO, BTreeSet::new(), held).unwrap()
}

#[test]
fn an_issue_with_no_done_when_line_is_not_free_work_and_the_same_issue_with_the_line_is() {
    let dir = tempfile::tempdir().unwrap();
    let issues = [
        issue(1, "Text.\\n\\nDone when: it works."),
        issue(2, "No line here."),
        // Waits for 2: the lead hears of 3 when 2 is done.
        issue(3, "Needs: #2"),
        // A verify waits: the pull request needs no line.
        issue(4, "No line either."),
        issue(5, "Done when:\\n- it works"),
        issue(6, "It is done when it works."),
    ];
    let gh = forge(dir.path(), &issues, PULL);
    let look = looks(&gh, &[]);
    assert_eq!(look.free, ["verify-issue-4", "issue-1", "issue-5"]);
    assert_eq!(look.no_done_when, BTreeSet::from([2, 6]));
    // The verify comes first. While it runs, the first build is issue 1.
    let none = BTreeSet::new();
    let verify = Work::Verify {
        issue: 4,
        pull: 44,
        commit: "1a2b3c4".into(),
    };
    assert_eq!(wave::pick(&look, &none, &BTreeMap::new()), Some(verify));
    let running = BTreeSet::from(["verify-issue-4".to_owned()]);
    assert_eq!(
        wave::pick(&look, &running, &BTreeMap::new()),
        Some(Work::Build { issue: 1 })
    );

    // The lead adds the line to issue 2: the next look takes it.
    let issues = [
        issue(1, "Done when: it works."),
        issue(2, "No line here.\\n\\nDone when: it works."),
    ];
    let look = looks(&forge(dir.path(), &issues, PULL), &[]);
    assert_eq!(look.free, ["verify-issue-4", "issue-1", "issue-2"]);
    assert!(look.no_done_when.is_empty());
}

#[test]
fn a_held_issue_with_no_done_when_line_gets_no_message() {
    let dir = tempfile::tempdir().unwrap();
    let issues = [issue(1, "No line."), issue(2, "No line.")];
    let look = looks(&forge(dir.path(), &issues, "[]"), &["issue-1"]);
    assert!(look.free.is_empty());
    assert_eq!(look.no_done_when, BTreeSet::from([2]));
}
