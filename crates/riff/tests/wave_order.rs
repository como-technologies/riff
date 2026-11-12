//! The wave orders the verifies of the pull requests that change the same
//! files (#1710): the look reads the files of each pull request from a fake
//! `gh`, and the order of the verify requests from the messages of the
//! thread. The verify of one starts, and the others wait until it merges.
//! Pull requests that change other files get their verifies side by side.
//! The rebases go in the same order (#1801): of the pull requests with a
//! conflict, only the next of the order gets a rebase. A session of the
//! item of a pull request does not make it go first (#1894). Pull requests
//! that change one file, and whose heads merge with no conflict in a real
//! git, get their verifies side by side (#1895). A verify request or a
//! verify result counts only from its right sender (#2027).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use riff::pr::Gh;
use riff::wave::{self, Work};
use riff_core::wire::{Kind, Message};

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

/// The message `body` of the thread from `from`: the URI of the sender
/// as riff-server gives it, with its claims and its lead mark.
fn said(from: &str, body: &str) -> Message {
    Message {
        seq: 0,
        from: from.parse().unwrap(),
        to: Vec::new(),
        body: body.into(),
        at_ms: 0,
        kind: Kind::Message,
        sig: None,
        payload: None,
    }
}

/// The work session of `issue`: it claims `issue-N`.
fn author(issue: u64) -> String {
    format!("riff://ann@heron/o/r?session=b{issue}&claim=issue-{issue}#issue-{issue}")
}

/// The verify session of `issue`: it claims `verify-issue-N`.
fn verifier(issue: u64) -> String {
    format!("riff://ann@heron/o/r?session=v{issue}&claim=verify-issue-{issue}#verify-issue-{issue}")
}

/// The lead of the repository.
const LEAD: &str = "riff://ann@heron/o/r?session=l1&lead=true";

/// The person: the riff daemon posts as the person.
const PERSON: &str = "riff://ann@heron/o/r";

/// The body of the verify request of `issue` at `head`, as the riff
/// daemon posts it.
fn request_body(issue: u64, head: &str) -> String {
    format!(
        "verify request: issue-{issue}, PR #{}, branch worktree-issue-{issue}, commit {}, from the riff daemon (session 65f510d7).",
        40 + issue,
        &head[..7]
    )
}

/// The verify request of `issue` at `head`, as the riff daemon posts it:
/// as the session of the build, which claims the item.
fn request(issue: u64, head: &str) -> Message {
    said(&author(issue), &request_body(issue, head))
}

/// The body of the failed verify result of `issue` at `head`.
fn fail_body(issue: u64, head: &str) -> String {
    format!(
        "verify result: FAIL for issue-{issue}, PR #{}, commit {}. PR comment https://c.",
        40 + issue,
        &head[..7]
    )
}

/// The failed verify result of `issue` at `head`, from its verify session.
fn fail(issue: u64, head: &str) -> Message {
    said(&verifier(issue), &fail_body(issue, head))
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
/// messages of the thread `messages`, oldest first.
fn look(gh: &Gh, messages: &[Message]) -> wave::Look {
    wave::look_with(gh, REPO, BTreeSet::new(), BTreeSet::new(), messages).unwrap()
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
    let mut messages = vec![request(1, "aaaa1111"), request(2, two), request(3, three)];
    let none = BTreeSet::new();
    let starts = BTreeMap::new();

    let merged = look(&gh, &messages);
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
    messages.push(request(2, rebased));
    at(Some(pull_in(2, rebased, None, "UNKNOWN")));
    let unknown = look(&gh, &messages);
    assert!(unknown.lanes[&2].unknown);
    assert_eq!(unknown.lanes[&2].asked, Some(1), "the place of the item");
    assert_eq!(wave::pick(&unknown, &none, &starts), None);

    // The new head has no conflict: its verify starts, and 3 waits.
    at(Some(pull_in(2, rebased, None, "MERGEABLE")));
    let next = look(&gh, &messages);
    assert_eq!(wave::pick(&next, &none, &starts), Some(verify(2, rebased)));
    assert_eq!(
        wave::order(&next, &none, &starts),
        BTreeMap::from([(3, 42)])
    );

    // The verify of 2 passed: 3 waits for its merge.
    at(Some(pull_in(2, rebased, Some("SUCCESS"), "MERGEABLE")));
    assert_eq!(wave::pick(&look(&gh, &messages), &none, &starts), None);

    // 2 merged: the rebase of 3 starts.
    at(None);
    open(dir.path(), &[3]);
    let last = look(&gh, &messages);
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
    let messages = [request(2, two), request(1, one)];
    let first = look(&gh, &messages);
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
    let passed = look(&gh, &messages);
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
    let merged = look(&gh, &messages);
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
    let messages = [request(1, one), request(2, two)];
    let claims: BTreeSet<String> = ["issue-1".to_owned()].into();
    let held = wave::look_with(&gh, REPO, claims, BTreeSet::new(), &messages).unwrap();
    assert_eq!(held.lanes[&1].asked, Some(0));
    let none = BTreeSet::new();
    let starts = BTreeMap::new();
    assert!(wave::order(&held, &none, &starts).is_empty());
    assert_eq!(wave::pick(&held, &none, &starts), Some(verify(2, two)));

    // The session ended, and the verify of 2 runs: 1 waits for #42.
    let free = wave::look_with(&gh, REPO, BTreeSet::new(), BTreeSet::new(), &messages).unwrap();
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
    let failed = look(&gh, &[request(2, two), fail(2, two), request(1, one)]);
    let none = BTreeSet::new();
    assert!(wave::order(&failed, &none, &BTreeMap::new()).is_empty());
    assert_eq!(
        wave::pick(&failed, &none, &BTreeMap::new()),
        Some(verify(1, one))
    );
}

/// The senders that may not send a verify request of issue 2 (#2027): a
/// session with no claim, the work session of issue 1, the verify session
/// of issue 2, a session of another repository that claims `issue-2`, the
/// lead of another repository, and riff-server.
fn not_requesters() -> Vec<String> {
    vec![
        "riff://ann@heron/o/r?session=n1".into(),
        author(1),
        verifier(2),
        "riff://ann@heron/o/x?session=x2&claim=issue-2#issue-2".into(),
        "riff://ann@heron/o/x?session=l2&lead=true".into(),
        "riff://riff@server".into(),
    ]
}

/// The senders that may send a verify request of issue 2 (#2027): its
/// work session, the riff daemon as the person, and the lead.
fn requesters() -> Vec<String> {
    vec![author(2), PERSON.into(), LEAD.into()]
}

/// #2027: issues 1 and 2 each change `tui.rs`, and the verify request of
/// 1 came first. A request of 2 from a wrong sender before them changes
/// nothing in the look: 1 goes first. The same request from a right
/// sender puts 2 first.
#[test]
fn a_verify_request_from_a_wrong_sender_changes_nothing_in_the_order() {
    let dir = tempfile::tempdir().unwrap();
    let (one, two) = ("aaaa1111", "bbbb2222");
    let tui: &[&str] = &["crates/riff/src/tui.rs"];
    let gh = forge(
        dir.path(),
        &[pull(1, one, None), pull(2, two, None)],
        &[(41, tui), (42, tui)],
    );
    let asked = [request(1, one), request(2, two)];
    let plain = look(&gh, &asked);
    let none = BTreeSet::new();
    let starts = BTreeMap::new();
    assert_eq!(wave::pick(&plain, &none, &starts), Some(verify(1, one)));
    let first = |from: &str| [vec![said(from, &request_body(2, two))], asked.to_vec()].concat();
    for from in not_requesters() {
        assert_eq!(look(&gh, &first(&from)), plain, "{from}");
    }
    for from in requesters() {
        let got = look(&gh, &first(&from));
        assert_eq!(got.lanes[&2].asked, Some(0), "{from}");
        assert_eq!(
            wave::pick(&got, &none, &starts),
            Some(verify(2, two)),
            "{from}"
        );
    }
}

/// #2027: the verify of the head of 2 failed. A new verify request for
/// that head from a wrong sender changes nothing in the look: 2 is free
/// as a build. The same request from a right sender answers the fail: 2
/// waits for a verify again (01M5P20DPK4GAQRDEKC1E6V718).
#[test]
fn a_new_verify_request_answers_a_failed_verify_only_from_its_right_sender() {
    let dir = tempfile::tempdir().unwrap();
    let two = "bbbb2222";
    let gh = forge(
        dir.path(),
        &[pull(2, two, Some("FAILURE"))],
        &[(42, &["a.rs"])],
    );
    let failed = [request(2, two), fail(2, two)];
    let build = look(&gh, &failed);
    let (issue, verify) = ("issue-2".to_owned(), "verify-issue-2".to_owned());
    assert!(build.free.contains(&issue), "{:?}", build.free);
    assert!(!build.free.contains(&verify), "{:?}", build.free);
    let again = |from: &str| [failed.to_vec(), vec![said(from, &request_body(2, two))]].concat();
    for from in not_requesters() {
        assert_eq!(look(&gh, &again(&from)), build, "{from}");
    }
    for from in requesters() {
        let free = look(&gh, &again(&from)).free;
        assert!(free.contains(&verify), "{from}: {free:?}");
        assert!(!free.contains(&issue), "{from}: {free:?}");
    }
}

/// #2027: the verify of the head of 2 failed, and a new verify request
/// answered the fail. A verify result for that head from a wrong sender
/// changes nothing in the look: 2 waits for a verify. The same result
/// from its verify session, or from the riff daemon as the person, ends
/// the request: 2 is free as a build.
#[test]
fn a_verify_result_counts_only_from_its_right_sender() {
    let dir = tempfile::tempdir().unwrap();
    let two = "bbbb2222";
    let gh = forge(
        dir.path(),
        &[pull(2, two, Some("FAILURE"))],
        &[(42, &["a.rs"])],
    );
    let asked = [request(2, two), fail(2, two), request(2, two)];
    let waits = look(&gh, &asked);
    assert!(
        waits.free.contains(&"verify-issue-2".to_owned()),
        "{:?}",
        waits.free
    );
    let build = look(&gh, &asked[..2]);
    assert_ne!(waits, build);
    let then = |from: &str| [asked.to_vec(), vec![said(from, &fail_body(2, two))]].concat();
    // A session with no claim, the work session of issue 2, the verify
    // session of issue 1, the lead, a session of another repository that
    // claims `verify-issue-2`, and riff-server.
    let wrong = [
        "riff://ann@heron/o/r?session=n1".into(),
        author(2),
        verifier(1),
        LEAD.into(),
        "riff://ann@heron/o/x?session=x2&claim=verify-issue-2".into(),
        "riff://riff@server".into(),
    ];
    for from in wrong {
        assert_eq!(look(&gh, &then(&from)), waits, "{from}");
    }
    for from in [verifier(2), PERSON.into()] {
        assert_eq!(look(&gh, &then(&from)), build, "{from}");
    }
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
    let messages: Vec<Message> = (1..).zip(&heads).map(|(n, h)| request(n, h)).collect();
    let none = BTreeSet::new();
    let starts = BTreeMap::new();

    // With no merge test, the three are one group.
    let mut first = look(&gh, &messages);
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
    let mut again = look(&gh, &messages);
    riff::order::merges(&main, &mut again.lanes);
    assert_eq!(again.lanes, first.lanes);
}
