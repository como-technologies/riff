//! The wave orders the verifies of the pull requests that change the same
//! files (#1710): the look reads the files of each pull request from a fake
//! `gh`, and the order of the verify requests from the bodies of the
//! thread. The verify of one starts, and the others wait until it merges.
//! Pull requests that change other files get their verifies side by side.
//! The rebases go in the same order (#1801): of the pull requests with a
//! conflict, only the next of the order gets a rebase. A session of the
//! item of a pull request does not make it go first (#1894). Pull requests
//! that change one file, and whose heads merge with no conflict in a real
//! git, get their verifies side by side (#1895).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

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
    pull_in(issue, head, verify, "MERGEABLE")
}

/// [`pull`] with the state `mergeable`, for example `CONFLICTING`.
fn pull_in(issue: u64, head: &str, verify: Option<&str>, mergeable: &str) -> String {
    let checks = match verify {
        Some(state) => format!(r#"[{{"context":"riff/verify","state":"{state}"}}]"#),
        None => "[]".into(),
    };
    format!(
        r#"{{"number":{},"headRefName":"worktree-issue-{issue}","headRefOid":"{head}","isDraft":false,"statusCheckRollup":{checks},"mergeable":"{mergeable}","autoMergeRequest":{{"mergeMethod":"SQUASH"}},"body":"Closes #{issue}\n\nIssue: #{issue}\nMilestone: Wave 1\n"}}"#,
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

fn rebase(issue: u64, head: &str) -> Work {
    Work::Rebase {
        issue,
        pull: 40 + issue,
        commit: head.into(),
    }
}

/// The open issues `numbers` of the wave, in the file `issues` of the fake
/// `gh` in `dir`: a merge closes its issue.
fn open(dir: &Path, numbers: &[u64]) {
    let issues: Vec<String> = numbers
        .iter()
        .map(|n| {
            format!(
                r#"{{"number":{n},"body":"Done when: x.","comments":[],"milestone":{{"title":"Wave 1"}},"labels":[]}}"#
            )
        })
        .collect();
    std::fs::write(dir.join("issues"), format!("[{}]", issues.join(","))).unwrap();
}

/// #1801: issues 1, 2 and 3 each change `tui.rs`, and their verify
/// requests came in that order. 1 merged, and gave 2 and 3 a conflict.
/// Only 2 gets its rebase. 3 keeps its conflict while a session rebases
/// 2, while the new head of 2 reads `UNKNOWN`, while it waits for its
/// verify and for its merge. After the merge of 2, 3 gets its rebase.
#[test]
fn only_the_next_pull_request_of_the_order_gets_a_rebase() {
    let dir = tempfile::tempdir().unwrap();
    let (two, three, rebased) = ("bbbb2222", "cccc3333", "dddd4444");
    let tui: &[&str] = &["crates/riff/src/tui.rs"];
    let files = [(42, tui), (43, tui)];
    // The open pull requests: 2 with `second`, and 3 with its conflict.
    let at = |second: Option<String>| {
        let third = pull_in(3, three, None, "CONFLICTING");
        let gh = forge(
            dir.path(),
            &[second.into_iter().collect(), vec![third]].concat(),
            &files,
        );
        open(dir.path(), &[2, 3]);
        gh
    };
    let gh = at(Some(pull_in(2, two, None, "CONFLICTING")));
    let mut bodies = vec![request(1, "aaaa1111"), request(2, two), request(3, three)];
    let none = BTreeSet::new();
    let starts = BTreeMap::new();

    let merged = look(&gh, &bodies);
    assert!(merged.lanes[&2].conflict);
    assert_eq!(merged.lanes[&3].asked, Some(2));
    assert_eq!(
        wave::order(&merged, &none, &starts),
        BTreeMap::from([(3, 42)])
    );
    assert_eq!(wave::pick(&merged, &none, &starts), Some(rebase(2, two)));
    // A session rebases 2: 3 waits.
    let rebasing: BTreeSet<String> = ["issue-2".to_owned()].into();
    assert_eq!(wave::pick(&merged, &rebasing, &starts), None);

    // 2 pushed its new head, and sent its verify request. GitHub does not
    // know yet whether it has a conflict: 2 keeps the place of its item.
    bodies.push(request(2, rebased));
    at(Some(pull_in(2, rebased, None, "UNKNOWN")));
    let unknown = look(&gh, &bodies);
    assert!(unknown.lanes[&2].unknown);
    assert_eq!(unknown.lanes[&2].asked, Some(1), "the place of the item");
    assert_eq!(wave::pick(&unknown, &none, &starts), None);

    // The new head has no conflict: its verify starts, and 3 waits.
    at(Some(pull_in(2, rebased, None, "MERGEABLE")));
    let next = look(&gh, &bodies);
    assert_eq!(wave::pick(&next, &none, &starts), Some(verify(2, rebased)));
    assert_eq!(
        wave::order(&next, &none, &starts),
        BTreeMap::from([(3, 42)])
    );

    // The verify of 2 passed: 3 waits for its merge.
    at(Some(pull_in(2, rebased, Some("SUCCESS"), "MERGEABLE")));
    assert_eq!(wave::pick(&look(&gh, &bodies), &none, &starts), None);

    // 2 merged: the rebase of 3 starts.
    at(None);
    open(dir.path(), &[3]);
    let last = look(&gh, &bodies);
    assert!(wave::order(&last, &none, &starts).is_empty());
    assert_eq!(wave::pick(&last, &none, &starts), Some(rebase(3, three)));
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

/// #1894: issues 1 and 2 each change `tui.rs`, and the verify request of
/// 1 came first. A session holds `issue-1`, for example to fix its
/// checks. 1 does not go first, and its verify cannot start: the verify
/// of 2 starts. When the session ends, 1 waits for #42.
#[test]
fn a_session_of_the_item_of_a_pull_request_does_not_make_it_go_first() {
    let dir = tempfile::tempdir().unwrap();
    let (one, two) = ("aaaa1111", "bbbb2222");
    let tui: &[&str] = &["crates/riff/src/tui.rs"];
    let gh = forge(
        dir.path(),
        &[pull(1, one, None), pull(2, two, None)],
        &[(41, tui), (42, tui)],
    );
    let bodies = [request(1, one), request(2, two)];
    let bodies: Vec<&str> = bodies.iter().map(String::as_str).collect();
    let claims: BTreeSet<String> = ["issue-1".to_owned()].into();
    let held = wave::look_with(&gh, REPO, claims, BTreeSet::new(), &bodies).unwrap();
    assert_eq!(held.lanes[&1].asked, Some(0));
    let none = BTreeSet::new();
    let starts = BTreeMap::new();
    assert!(wave::order(&held, &none, &starts).is_empty());
    assert_eq!(wave::pick(&held, &none, &starts), Some(verify(2, two)));

    // The session ended, and the verify of 2 runs: 1 waits for #42.
    let free = wave::look_with(&gh, REPO, BTreeSet::new(), BTreeSet::new(), &bodies).unwrap();
    let running: BTreeSet<String> = ["verify-issue-2".to_owned()].into();
    assert_eq!(
        wave::order(&free, &running, &starts),
        BTreeMap::from([(1, 42)])
    );
    assert_eq!(
        wave::pick(&free, &running, &starts),
        Some(Work::Build { issue: 3 })
    );
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

/// Runs `git ARGS` in `dir` (with an identity and no signing), and gives
/// its output.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// The page of requirements with the sections A and B, and the lines `a`
/// and `b` at the end of each.
fn page(a: &str, b: &str) -> String {
    format!("# Requirements\n\n## A\n\n- one\n{a}\n## B\n\n- two\n{b}")
}

/// The path of the page of requirements in a clone.
const PAGE: &str = "docs/src/requirements.md";

/// A bare `origin` in `dir` that a `file://` URL names, with [`page`] on
/// `main`, and a main clone of it. For each `(a, b)` of `heads`, the head
/// of the pull request `40 + N` of the issue N (from 1) adds `a` and `b`
/// to the page. `origin` has each head only on `refs/pull/40+N/head`, as
/// GitHub has it, so the main clone does not have it. It gives the main
/// clone and the heads.
fn origin(dir: &Path, heads: &[(&str, &str)]) -> (PathBuf, Vec<String>) {
    let (bare, work) = (dir.join("origin.git"), dir.join("work"));
    let url = format!("file://{}", bare.display());
    git(dir, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
    std::fs::create_dir_all(work.join("docs/src")).unwrap();
    git(&work, &["init", "-q", "-b", "main"]);
    std::fs::write(work.join(PAGE), page("", "")).unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "base"]);
    git(&work, &["remote", "add", "origin", &url]);
    git(&work, &["push", "-q", "origin", "main"]);
    let heads = heads
        .iter()
        .zip(41..)
        .map(|((a, b), pull)| {
            git(
                &work,
                &["switch", "-q", "-c", &format!("pr-{pull}"), "main"],
            );
            std::fs::write(work.join(PAGE), page(a, b)).unwrap();
            git(&work, &["commit", "-q", "-am", &format!("pr {pull}")]);
            let to = format!("HEAD:refs/pull/{pull}/head");
            git(&work, &["push", "-q", "origin", &to]);
            git(&work, &["rev-parse", "HEAD"])
        })
        .collect();
    git(dir, &["clone", "-q", "--", &url, "main"]);
    (dir.join("main"), heads)
}

/// True when the clone `dir` has the commit `commit`.
fn has(dir: &Path, commit: &str) -> bool {
    Command::new("git")
        .args(["cat-file", "-e", &format!("{commit}^{{commit}}")])
        .current_dir(dir)
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

/// #1895, Wave 54: issues 1, 2 and 3 each add a requirement to
/// `requirements.md`. 1 and 3 add theirs to the end of section A, and 2
/// to the end of section B. So the heads of 1 and 2 merge with no
/// conflict, and so do the heads of 2 and 3, but the heads of 1 and 3
/// conflict. The merge test fetches the heads into the main clone. The
/// verifies of 1 and 2 start side by side, and the verify of 3 waits for
/// #41.
#[test]
fn pull_requests_that_change_one_file_with_no_conflict_verify_side_by_side() {
    let dir = tempfile::tempdir().unwrap();
    let (main, heads) = origin(
        dir.path(),
        &[("- three\n", ""), ("", "- four\n"), ("- five\n", "")],
    );
    let page: &[&str] = &[PAGE];
    let pulls: Vec<String> = (1..).zip(&heads).map(|(n, h)| pull(n, h, None)).collect();
    let gh = forge(dir.path(), &pulls, &[(41, page), (42, page), (43, page)]);
    let bodies: Vec<String> = (1..).zip(&heads).map(|(n, h)| request(n, h)).collect();
    let none = BTreeSet::new();
    let starts = BTreeMap::new();

    // With no merge test, the three are one group.
    let mut first = look(&gh, &bodies);
    assert_eq!(
        wave::order(&first, &none, &starts),
        BTreeMap::from([(2, 41), (3, 41)])
    );
    assert!(!has(&main, &heads[0]), "origin has the head on no branch");

    riff::order::merges(&main, &mut first.lanes);
    assert!(heads.iter().all(|head| has(&main, head)), "the fetch");
    assert_eq!(first.lanes[&1].clean, BTreeSet::from([42]));
    assert_eq!(first.lanes[&2].clean, BTreeSet::from([41, 43]));
    assert_eq!(first.lanes[&3].clean, BTreeSet::from([42]));
    assert_eq!(
        wave::order(&first, &none, &starts),
        BTreeMap::from([(3, 41)])
    );
    assert_eq!(
        wave::pick(&first, &none, &starts),
        Some(verify(1, &heads[0]))
    );
    let one: BTreeSet<String> = ["verify-issue-1".to_owned()].into();
    assert_eq!(
        wave::pick(&first, &one, &starts),
        Some(verify(2, &heads[1]))
    );
    // The verifies of 1 and 2 run: 3 waits for #41.
    let both: BTreeSet<String> = ["verify-issue-1".to_owned(), "verify-issue-2".to_owned()].into();
    assert_eq!(
        wave::order(&first, &both, &starts),
        BTreeMap::from([(3, 41)])
    );
    assert_eq!(wave::pick(&first, &both, &starts), None);

    // The clone keeps the heads: a second test needs no `origin`.
    std::fs::remove_dir_all(dir.path().join("origin.git")).unwrap();
    let mut again = look(&gh, &bodies);
    riff::order::merges(&main, &mut again.lanes);
    assert_eq!(again.lanes, first.lanes);
}
