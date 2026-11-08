//! The wave orders the verifies of the pull requests that change the same
//! files (#1710): the look reads the files of each pull request from a fake
//! `gh`, and the order of the verify requests from the bodies of the
//! thread. The verify of one starts, and the others wait until it merges.
//! Pull requests that change other files get their verifies side by side.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use riff::pr::Gh;
use riff::wave::{self, Work};

const REPO: &str = "o/r";

/// A fake `gh`: the milestones, the file `pulls` for `pr list`, the file
/// `issues` for `issue list`, a passed check for each commit, and the file
/// `files-N` for the files of the pull request N.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
case "$1 $2" in
  api*)
    case "$2" in
      repos/o/r) echo '{"default_branch":"main"}' ;;
      */rules/branches/main*) echo '[]' ;;
      */milestones*) echo '[{"title":"Backlog"},{"title":"Wave 1"}]' ;;
      */check-runs*) echo '{"check_runs":[{"status":"completed","conclusion":"success","head_sha":"x"}]}' ;;
      */pulls/*/files*)
        n=$(echo "$2" | sed 's|.*/pulls/\([0-9]*\)/files.*|\1|')
        cat "$dir/files-$n" ;;
      *) exit 1 ;;
    esac ;;
  "pr list") cat "$dir/pulls" ;;
  "issue list") cat "$dir/issues" ;;
  *) exit 1 ;;
esac
"#;

/// The issues 1 to 3 of the wave.
const ISSUES: &str = r#"[
{"number":1,"body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":2,"body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":3,"body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]}]"#;

/// The pull request `40 + issue` of `issue` at `head`, with auto-merge on,
/// no conflict, and the status `verify` of `riff/verify` (or none).
fn pull(issue: u64, head: &str, verify: Option<&str>) -> String {
    let checks = match verify {
        Some(state) => format!(r#"[{{"context":"riff/verify","state":"{state}"}}]"#),
        None => "[]".into(),
    };
    format!(
        r#"{{"number":{},"headRefName":"worktree-issue-{issue}","headRefOid":"{head}","isDraft":false,"statusCheckRollup":{checks},"mergeable":"MERGEABLE","autoMergeRequest":{{"mergeMethod":"SQUASH"}},"body":"Closes #{issue}\n\nIssue: #{issue}\nMilestone: Wave 1\n"}}"#,
        40 + issue
    )
}

/// The verify request of `issue` at `head`, as the riff daemon posts it.
fn request(issue: u64, head: &str) -> String {
    format!(
        "verify request: issue-{issue}, PR #{}, branch worktree-issue-{issue}, commit {}, from the riff daemon (session 65f510d7).",
        40 + issue,
        &head[..7]
    )
}

/// The fake `gh` in `dir`, with the open pull requests `pulls`, and the
/// files of each pull request (by number).
fn forge(dir: &Path, pulls: &[String], files: &[(u64, &[&str])]) -> Gh {
    let path = dir.join("gh");
    isolated::write_program(&path, FAKE_GH);
    std::fs::write(dir.join("issues"), ISSUES).unwrap();
    std::fs::write(dir.join("pulls"), format!("[{}]", pulls.join(","))).unwrap();
    for (number, names) in files {
        let lines: String = names.iter().map(|n| format!("{n}\n")).collect();
        std::fs::write(dir.join(format!("files-{number}")), lines).unwrap();
    }
    Gh::at(path)
}

/// The look of the wave from `gh`, with no claim and no hold, and the
/// messages `bodies` of the thread, oldest first.
fn look(gh: &Gh, bodies: &[String]) -> wave::Look {
    let bodies: Vec<&str> = bodies.iter().map(String::as_str).collect();
    wave::look_with(gh, REPO, BTreeSet::new(), BTreeSet::new(), &bodies).unwrap()
}

fn verify(issue: u64, head: &str) -> Work {
    Work::Verify {
        issue,
        pull: 40 + issue,
        commit: head.into(),
    }
}

/// Issues 1 and 2 each change `tui.rs`, and the verify request of 2 came
/// first. The verify of 2 starts, and the verify of 1 waits for #42: while
/// the verify of 2 runs, while it waits for the merge after its pass, and
/// until it merges.
#[test]
fn the_verify_whose_request_came_first_starts_and_the_other_waits_for_its_merge() {
    let dir = tempfile::tempdir().unwrap();
    let (one, two) = ("aaaa1111", "bbbb2222");
    let tui = ["crates/riff/src/tui.rs"];
    let both = ["crates/riff/src/tui.rs", "crates/riff/src/tui/view.rs"];
    let files: [(u64, &[&str]); 2] = [(41, &tui), (42, &both)];
    let gh = forge(
        dir.path(),
        &[pull(1, one, None), pull(2, two, None)],
        &files,
    );
    let bodies = [request(2, two), request(1, one)];
    let first = look(&gh, &bodies);
    assert_eq!(first.lanes[&2].asked, Some(0));
    assert_eq!(first.lanes[&1].files, BTreeSet::from(tui.map(String::from)));
    let none = BTreeSet::new();
    let starts = BTreeMap::new();
    assert_eq!(wave::pick(&first, &none, &starts), Some(verify(2, two)));
    assert_eq!(
        wave::order(&first, &none, &starts),
        BTreeMap::from([(1, 42)])
    );

    // The verify of 2 runs: the wave builds 3, and the verify of 1 waits.
    let running: BTreeSet<String> = ["verify-issue-2".to_owned()].into();
    assert_eq!(
        wave::pick(&first, &running, &starts),
        Some(Work::Build { issue: 3 })
    );

    // The verify of 2 passed: 1 waits for the merge.
    forge(
        dir.path(),
        &[pull(1, one, None), pull(2, two, Some("SUCCESS"))],
        &files,
    );
    let passed = look(&gh, &bodies);
    assert!(passed.lanes[&2].passed);
    assert_eq!(
        wave::order(&passed, &none, &starts),
        BTreeMap::from([(1, 42)])
    );
    assert_eq!(
        wave::pick(&passed, &none, &starts),
        Some(Work::Build { issue: 3 })
    );

    // 2 merged: the verify of 1 starts.
    forge(dir.path(), &[pull(1, one, None)], &files);
    let merged = look(&gh, &bodies);
    assert!(wave::order(&merged, &none, &starts).is_empty());
    assert_eq!(wave::pick(&merged, &none, &starts), Some(verify(1, one)));
}

/// The verify of 2 failed: the verify of 1 waits for nobody.
#[test]
fn a_failed_verify_lets_the_next_verify_start() {
    let dir = tempfile::tempdir().unwrap();
    let (one, two) = ("aaaa1111", "bbbb2222");
    let files: [(u64, &[&str]); 2] = [(41, &["a.rs"]), (42, &["a.rs"])];
    let pulls = [pull(1, one, None), pull(2, two, Some("FAILURE"))];
    let gh = forge(dir.path(), &pulls, &files);
    let result = "verify result: FAIL for issue-2, PR #42, commit bbbb222. PR comment https://c.";
    let failed = look(&gh, &[request(2, two), result.into(), request(1, one)]);
    let none = BTreeSet::new();
    assert!(wave::order(&failed, &none, &BTreeMap::new()).is_empty());
    assert_eq!(
        wave::pick(&failed, &none, &BTreeMap::new()),
        Some(verify(1, one))
    );
}

/// Issues 1 and 2 change other files: both verifies start, side by side.
#[test]
fn the_verifies_of_pull_requests_that_change_other_files_start_side_by_side() {
    let dir = tempfile::tempdir().unwrap();
    let (one, two) = ("aaaa1111", "bbbb2222");
    let files: [(u64, &[&str]); 2] = [(41, &["src/a.rs"]), (42, &["src/b.rs"])];
    let gh = forge(
        dir.path(),
        &[pull(1, one, None), pull(2, two, None)],
        &files,
    );
    let look = look(&gh, &[request(1, one), request(2, two)]);
    let none = BTreeSet::new();
    let starts = BTreeMap::new();
    assert!(wave::order(&look, &none, &starts).is_empty());
    let first = wave::pick(&look, &none, &starts).unwrap();
    assert_eq!(first, verify(1, one));
    let running: BTreeSet<String> = [first.item()].into();
    assert_eq!(wave::pick(&look, &running, &starts), Some(verify(2, two)));
}
