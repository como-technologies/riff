//! The wave of the daemon rebases a pull request with a conflict (#1082):
//! the look of the wave reads the open pull requests from a fake `gh`
//! (01M57FJ3WWMQSDBFRRHXN3SMFQ). A pull request with auto-merge on and a
//! conflict with `main` is a rebase of its item, also after its verify
//! passed, one time for each head commit. Its verify waits for the new
//! head. A pull request with auto-merge off gets no rebase.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use riff::pr::Gh;
use riff::wave::{self, Conflict, Work};

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

/// The issues 1 to 5 of the wave.
const ISSUES: &str = r#"[
{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":2,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":3,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":4,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":5,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[]}]"#;

/// The pull request of `issue` at `head`, with the status `verify` of
/// `riff/verify` (or none), the state `mergeable`, and auto-merge on when
/// `auto` is true.
fn pull(issue: u64, head: &str, verify: Option<&str>, mergeable: &str, auto: bool) -> String {
    let checks = match verify {
        Some(state) => format!(r#"[{{"context":"riff/verify","state":"{state}"}}]"#),
        None => "[]".into(),
    };
    let auto = match auto {
        true => r#"{"enabledAt":"2026-10-18T08:00:00Z","mergeMethod":"SQUASH"}"#,
        false => "null",
    };
    format!(
        r#"{{"number":{},"headRefName":"worktree-issue-{issue}","headRefOid":"{head}","isDraft":false,"statusCheckRollup":{checks},"mergeable":"{mergeable}","autoMergeRequest":{auto},"body":"Closes #{issue}\n\nIssue: #{issue}\nMilestone: Wave 1\n"}}"#,
        40 + issue
    )
}

/// The fake `gh` in `dir`, with the open pull requests `pulls`.
fn forge(dir: &Path, pulls: &[String]) -> Gh {
    let path = dir.join("gh");
    isolated::write_program(&path, FAKE_GH);
    std::fs::write(dir.join("issues"), ISSUES).unwrap();
    std::fs::write(dir.join("pulls"), format!("[{}]", pulls.join(","))).unwrap();
    Gh::at(path)
}

/// The look of the wave from `gh`, with no claim and no hold.
fn look(gh: &Gh) -> wave::Look {
    wave::look_on(gh, REPO, BTreeSet::new(), BTreeSet::new()).unwrap()
}

/// Issue 1 passed its verify, and issue 2 waits for one: each has a
/// conflict with `main`. Issue 3 has a conflict, and auto-merge off.
/// Issue 4 is free, and issue 5 waits for a verify with no conflict.
#[test]
fn the_look_of_the_wave_rebases_each_pull_request_with_auto_merge_and_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(
        dir.path(),
        &[
            pull(1, "aaaa111", Some("SUCCESS"), "CONFLICTING", true),
            pull(2, "bbbb222", None, "CONFLICTING", true),
            pull(3, "cccc333", Some("SUCCESS"), "CONFLICTING", false),
            pull(5, "eeee555", None, "MERGEABLE", true),
        ],
    );
    let look = look(&gh);
    // The rebases first, then the verify of 5, then the free item 4. No
    // verify of the head of 2: it has a conflict.
    assert_eq!(
        look.free,
        ["issue-1", "issue-2", "verify-issue-5", "issue-4"]
    );
    let conflict = |pull, commit: &str| Conflict {
        pull,
        commit: commit.into(),
    };
    assert_eq!(
        look.conflicts,
        BTreeMap::from([(1, conflict(41, "aaaa111")), (2, conflict(42, "bbbb222"))])
    );

    // One rebase for each head: the starts of a rebase name its head.
    let none = BTreeSet::new();
    let mut starts = BTreeMap::new();
    let first = wave::pick(&look, &none, &starts).unwrap();
    assert_eq!(
        first,
        Work::Rebase {
            issue: 1,
            pull: 41,
            commit: "aaaa111".into()
        }
    );
    assert_eq!(first.item(), "issue-1");
    starts.insert(first.key(), 1);
    let second = wave::pick(&look, &none, &starts).unwrap();
    assert_eq!(
        second,
        Work::Rebase {
            issue: 2,
            pull: 42,
            commit: "bbbb222".into()
        }
    );
    starts.insert(second.key(), 1);
    assert_eq!(
        wave::pick(&look, &none, &starts),
        Some(Work::Verify {
            issue: 5,
            pull: 45,
            commit: "eeee555".into()
        })
    );
}

/// The rebase of 1 pushed a new head with no conflict: its verify waits
/// for a new session, and no rebase is left.
#[test]
fn a_new_head_with_no_conflict_waits_for_its_verify() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path(), &[pull(1, "aaaa999", None, "MERGEABLE", true)]);
    let look = look(&gh);
    assert_eq!(
        look.free,
        ["verify-issue-1", "issue-2", "issue-3", "issue-4", "issue-5"]
    );
    assert!(look.conflicts.is_empty());
    assert_eq!(
        wave::pick(&look, &BTreeSet::new(), &BTreeMap::new()),
        Some(Work::Verify {
            issue: 1,
            pull: 41,
            commit: "aaaa999".into()
        })
    );
}

/// A pull request whose verify failed is free work as a build, with the
/// failed result: no rebase.
#[test]
fn a_failed_verify_with_a_conflict_is_a_build() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(
        dir.path(),
        &[pull(1, "aaaa111", Some("FAILURE"), "CONFLICTING", true)],
    );
    let look = look(&gh);
    assert!(look.conflicts.is_empty());
    assert_eq!(
        wave::pick(&look, &BTreeSet::new(), &BTreeMap::new()),
        Some(Work::Build { issue: 1 })
    );
}

/// The release issue (the label `release`) is no work (#1213): the wave
/// starts no build of it, no verify of its pull request, and no rebase
/// of it. Issue 1 is a plain item and still free.
#[test]
fn the_look_of_the_wave_leaves_out_the_release_issue() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(
        dir.path(),
        &[
            pull(2, "bbbb222", None, "MERGEABLE", true),
            pull(3, "cccc333", Some("SUCCESS"), "CONFLICTING", true),
        ],
    );
    std::fs::write(
        dir.path().join("issues"),
        r#"[
{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":2,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[{"name":"release"}]},
{"number":3,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[{"name":"release"}]},
{"number":4,"body":"","comments":[],"milestone":{"title":"Wave 1"},"labels":[{"name":"release"}]}]"#,
    )
    .unwrap();
    let look = look(&gh);
    assert_eq!(look.free, ["issue-1"]);
    assert!(look.conflicts.is_empty(), "{:?}", look.conflicts);
    let none = BTreeSet::new();
    assert_eq!(
        wave::pick(&look, &none, &BTreeMap::new()),
        Some(Work::Build { issue: 1 })
    );
}
