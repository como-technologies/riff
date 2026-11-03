//! The wave of the daemon acts for a pull request that cannot merge by
//! itself (#279): the look of the wave reads the open pull requests from a
//! fake `gh`. A failed check after a passed verify is a build of its item,
//! one time for each head commit. A verified pull request with auto-merge
//! off, and a release with a failed check, are a message to the lead. A
//! conflict with auto-merge on is still a rebase. `riff top` and `riff
//! wave list` show the item in the group `stuck`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use riff::pr::Gh;
use riff::rollout::{OtherCheck, Review, Run, Stuck};
use riff::wave::{self, Conflict, Failed, Notice, Work};
use riff_client::sessions::Sessions;
use riff_client::waves::Board;

const REPO: &str = "o/r";

/// A fake `gh`: the milestones, the file `pulls` for `pr list`, the file
/// `issues` for `issue list`, a passed Gate for each commit, the file `runs`
/// for the runs of Actions of a commit, and the file `calls` with each read
/// of them.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
case "$1 $2" in
  api*)
    case "$2" in
      repos/o/r) echo '{"default_branch":"main"}' ;;
      */rules/branches/main*) echo '[]' ;;
      */milestones*) echo '[{"title":"Backlog"},{"title":"Wave 1"}]' ;;
      */check-runs*) echo '{"check_runs":[{"status":"completed","conclusion":"success","head_sha":"x"}]}' ;;
      */actions/runs*) echo "$2" >> "$dir/calls"; cat "$dir/runs" ;;
      *) exit 1 ;;
    esac ;;
  "pr list") cat "$dir/pulls" ;;
  "issue list") cat "$dir/issues" ;;
  *) exit 1 ;;
esac
"#;

/// The issues 1 to 6 of the wave. The issue 5 has the label `release`.
const ISSUES: &str = r#"[
{"number":1,"title":"One","body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":2,"title":"Two","body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":3,"title":"Three","body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":4,"title":"Four","body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":5,"title":"Release","body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[{"name":"release"}]},
{"number":6,"title":"Six","body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]}]"#;

/// The status `riff/verify` with `state`.
fn verify(state: &str) -> String {
    format!(r#"{{"__typename":"StatusContext","context":"riff/verify","state":"{state}"}}"#)
}

/// The check run `name` of GitHub Actions with `conclusion`.
fn run(name: &str, conclusion: &str) -> String {
    format!(
        r#"{{"__typename":"CheckRun","name":"{name}","conclusion":"{conclusion}","workflowName":"CI","detailsUrl":"https://github.com/o/r/actions/runs/9/job/1"}}"#
    )
}

/// The check run `name` of CircleCI, an app that is not GitHub Actions,
/// with `conclusion`. GitHub gives it no workflow.
fn circle(name: &str, conclusion: &str) -> String {
    format!(
        r#"{{"__typename":"CheckRun","name":"{name}","conclusion":"{conclusion}","workflowName":"","detailsUrl":"https://app.circleci.com/workflow/9?utm_source=github-checks-link"}}"#
    )
}

/// The status `context` with `state`, as the older apps of CI set it.
fn status(context: &str, state: &str) -> String {
    format!(
        r#"{{"__typename":"StatusContext","context":"{context}","state":"{state}","targetUrl":"https://buildkite.com/o/r/builds/7"}}"#
    )
}

/// A pull request on the branch `branch` for `issue` at `head`, with the
/// rollup `checks`, the state `mergeable`, and auto-merge on when `auto`
/// is true.
fn pull_on(
    branch: &str,
    issue: u64,
    head: &str,
    checks: &[String],
    mergeable: &str,
    auto: bool,
) -> String {
    let auto = match auto {
        true => r#"{"enabledAt":"2026-10-18T08:00:00Z","mergeMethod":"SQUASH"}"#,
        false => "null",
    };
    format!(
        r#"{{"number":{},"headRefName":"{branch}","headRefOid":"{head}","isDraft":false,"statusCheckRollup":[{}],"mergeable":"{mergeable}","autoMergeRequest":{auto},"body":"Closes #{issue}\n\nIssue: #{issue}\nMilestone: Wave 1\n"}}"#,
        40 + issue,
        checks.join(",")
    )
}

/// A pull request for `issue` at `head`, with auto-merge on and no conflict,
/// the rollup `checks`, and what GitHub says of the merge: `reviewDecision`
/// `review` and `mergeStateStatus` `state`.
fn pull_held(issue: u64, head: &str, checks: &[String], review: &str, state: &str) -> String {
    format!(
        r#"{{"number":{},"headRefName":"worktree-issue-{issue}","headRefOid":"{head}","isDraft":false,"statusCheckRollup":[{}],"mergeable":"MERGEABLE","mergeStateStatus":"{state}","reviewDecision":"{review}","autoMergeRequest":{{"enabledAt":"2026-10-18T08:00:00Z","mergeMethod":"SQUASH"}},"body":"Closes #{issue}\n\nIssue: #{issue}\nMilestone: Wave 1\n"}}"#,
        40 + issue,
        checks.join(",")
    )
}

/// [`pull_on`] the branch of `issue`, with auto-merge on and no conflict.
fn pull(issue: u64, head: &str, checks: &[String]) -> String {
    pull_on(
        &format!("worktree-issue-{issue}"),
        issue,
        head,
        checks,
        "MERGEABLE",
        true,
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
fn looks(gh: &Gh) -> wave::Look {
    wave::look_on(gh, REPO, BTreeSet::new(), BTreeSet::new()).unwrap()
}

fn checks_of(issue: u64, pull: u64, commit: &str, names: &[&str]) -> Work {
    Work::Checks {
        issue,
        pull,
        commit: commit.into(),
        checks: names.iter().map(|n| (*n).to_owned()).collect(),
    }
}

/// The Gate of issue 1 failed after a pass. The Gate and Hygiene of issue
/// 2 failed, and its auto-merge is off. Issue 3 passed all, and issue 4
/// has a cancelled Gate and a check that goes: they did not fail.
#[test]
fn a_check_that_failed_after_a_pass_is_a_build_with_its_name_one_time_for_each_head() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(
        dir.path(),
        &[
            pull(1, "aaaa111", &[verify("SUCCESS"), run("Gate", "FAILURE")]),
            pull_on(
                "worktree-issue-2",
                2,
                "bbbb222",
                &[
                    verify("SUCCESS"),
                    run("Gate", "TIMED_OUT"),
                    run("Hygiene", "FAILURE"),
                    run("Gate", "TIMED_OUT"),
                ],
                "MERGEABLE",
                false,
            ),
            pull(3, "cccc333", &[verify("SUCCESS"), run("Gate", "SUCCESS")]),
            pull(
                4,
                "dddd444",
                &[verify("SUCCESS"), run("Gate", "CANCELLED"), run("Lint", "")],
            ),
        ],
    );
    let look = looks(&gh);
    // The fix of 1 and of 2 come first, in the order of the issues.
    assert_eq!(look.free, ["issue-1", "issue-2", "issue-6"]);
    assert_eq!(
        look.fixes.failed,
        BTreeMap::from([
            (
                1,
                Failed {
                    pull: 41,
                    commit: "aaaa111".into(),
                    checks: vec!["Gate".into()]
                }
            ),
            (
                2,
                Failed {
                    pull: 42,
                    commit: "bbbb222".into(),
                    checks: vec!["Gate".into(), "Hygiene".into()]
                }
            ),
        ])
    );
    assert!(look.fixes.conflicts.is_empty());
    // A failed check comes before the auto-merge: no message for 2 yet.
    assert!(look.fixes.notices.is_empty(), "{:?}", look.fixes.notices);

    // One session for each head: the starts name the head.
    let none = BTreeSet::new();
    let mut starts = BTreeMap::new();
    let first = wave::pick(&look, &none, &starts).unwrap();
    assert_eq!(first, checks_of(1, 41, "aaaa111", &["Gate"]));
    assert_eq!(first.item(), "issue-1");
    starts.insert(first.key(), 1);
    let second = wave::pick(&look, &none, &starts).unwrap();
    assert_eq!(second, checks_of(2, 42, "bbbb222", &["Gate", "Hygiene"]));
    starts.insert(second.key(), 1);
    assert_eq!(
        wave::pick(&look, &none, &starts),
        Some(Work::Build { issue: 6 })
    );

    // A new head with a failed check is a new state.
    let gh = forge(
        dir.path(),
        &[pull(
            1,
            "aaaa999",
            &[verify("SUCCESS"), run("Gate", "FAILURE")],
        )],
    );
    let again = wave::pick(&looks(&gh), &none, &starts).unwrap();
    assert_eq!(again, checks_of(1, 41, "aaaa999", &["Gate"]));
}

/// A verified pull request with auto-merge off tells the lead: the
/// notice has the pull request, the head and the reason. A pull request
/// that waits for its verify, and one with auto-merge on, have none.
#[test]
fn a_verified_pull_request_with_auto_merge_off_is_a_notice_for_the_lead() {
    let dir = tempfile::tempdir().unwrap();
    let off = |issue, head: &str, checks: &[String]| {
        pull_on(
            &format!("worktree-issue-{issue}"),
            issue,
            head,
            checks,
            "MERGEABLE",
            false,
        )
    };
    let gh = forge(
        dir.path(),
        &[
            off(1, "aaaa111", &[verify("SUCCESS")]),
            // It waits for its verify: no message yet.
            off(2, "bbbb222", &[]),
            pull(3, "cccc333", &[verify("SUCCESS")]),
            // A conflict, and auto-merge off: nothing merges it either.
            pull_on(
                "worktree-issue-4",
                4,
                "dddd444",
                &[verify("SUCCESS")],
                "CONFLICTING",
                false,
            ),
        ],
    );
    let look = looks(&gh);
    let notice = |pull, issue, commit: &str| Notice {
        pull,
        branch: format!("worktree-issue-{issue}"),
        commit: commit.into(),
        stuck: Stuck::AutoMergeOff,
    };
    assert_eq!(
        look.fixes.notices,
        [notice(41, 1, "aaaa111"), notice(44, 4, "dddd444")]
    );
    // No session starts for them: no rebase, no build, no verify of 1.
    assert!(look.fixes.conflicts.is_empty());
    assert!(look.fixes.failed.is_empty());
    assert_eq!(look.free, ["verify-issue-2", "issue-6"]);
    // Each notice is a message with the reason and the command.
    let body = riff::text::wave_stuck(
        REPO,
        41,
        "worktree-issue-1",
        "main",
        "aaaa111",
        &Stuck::AutoMergeOff,
    );
    assert!(body.contains("auto-merge is off"), "{body}");
    assert!(body.ends_with("gh pr merge --auto --squash 41"), "{body}");
    // One key for each pull request, head and reason.
    assert_eq!(look.fixes.notices[0].key(), "#41@aaaa111:auto-merge off");
}

/// A conflict with auto-merge on is still a rebase, also with a failed
/// check: its new head has new checks (no change of #1082).
#[test]
fn a_conflict_with_auto_merge_on_still_starts_the_rebase() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(
        dir.path(),
        &[pull_on(
            "worktree-issue-1",
            1,
            "aaaa111",
            &[verify("SUCCESS"), run("Gate", "FAILURE")],
            "CONFLICTING",
            true,
        )],
    );
    let look = looks(&gh);
    assert_eq!(
        look.fixes.conflicts,
        BTreeMap::from([(
            1,
            Conflict {
                pull: 41,
                commit: "aaaa111".into()
            }
        )])
    );
    assert!(look.fixes.failed.is_empty());
    assert!(look.fixes.notices.is_empty());
    assert_eq!(
        wave::pick(&look, &BTreeSet::new(), &BTreeMap::new()),
        Some(Work::Rebase {
            issue: 1,
            pull: 41,
            commit: "aaaa111".into()
        })
    );
}

/// A failed verify is free work as a build already: a failed check on the
/// head is no second state. A held item gets no session.
#[test]
fn a_failed_verify_and_a_held_item_get_no_fix_of_checks() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(
        dir.path(),
        &[
            pull(1, "aaaa111", &[verify("FAILURE"), run("Gate", "FAILURE")]),
            pull(2, "bbbb222", &[verify("SUCCESS"), run("Gate", "FAILURE")]),
        ],
    );
    let held: BTreeSet<String> = ["issue-2".to_owned()].into();
    let look = wave::look_on(&gh, REPO, BTreeSet::new(), held).unwrap();
    assert!(look.fixes.failed.contains_key(&2));
    assert!(!look.fixes.failed.contains_key(&1));
    assert_eq!(
        wave::pick(&look, &BTreeSet::new(), &BTreeMap::new()),
        Some(Work::Build { issue: 1 })
    );
}

/// #1427: the fix of a failed check reads the run with `gh run view`.
/// That works for GitHub Actions only. A failed check of another app (a
/// check run of CircleCI, a status of Buildkite) starts no session: the
/// lead gets a notice with the check, its app and its link. A failed run
/// of Actions on the same head still starts one session.
#[test]
fn a_failed_check_of_another_app_is_a_notice_with_its_link_and_no_build() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(
        dir.path(),
        &[
            // A check run of CircleCI.
            pull(1, "aaaa111", &[verify("SUCCESS"), circle("ci", "FAILURE")]),
            // A status of Buildkite.
            pull(
                2,
                "bbbb222",
                &[verify("SUCCESS"), status("buildkite/riff", "FAILURE")],
            ),
            // A run of Actions and a check run of CircleCI.
            pull(
                3,
                "cccc333",
                &[
                    verify("SUCCESS"),
                    run("Gate", "FAILURE"),
                    circle("ci", "FAILURE"),
                    circle("ci", "FAILURE"),
                ],
            ),
        ],
    );
    let look = looks(&gh);
    // One session, for the run of Actions of 3.
    assert_eq!(
        look.fixes.failed,
        BTreeMap::from([(
            3,
            Failed {
                pull: 43,
                commit: "cccc333".into(),
                checks: vec!["Gate".into()]
            }
        )])
    );
    assert_eq!(
        wave::pick(&look, &BTreeSet::new(), &BTreeMap::new()),
        Some(checks_of(3, 43, "cccc333", &["Gate"]))
    );
    assert!(
        !look.free.contains(&"issue-1".to_owned()),
        "{:?}",
        look.free
    );
    assert!(
        !look.free.contains(&"issue-2".to_owned()),
        "{:?}",
        look.free
    );

    // The lead hears of each other check one time, with its app and link.
    let other = |pull, issue, commit: &str, name: &str, app: &str, link: &str| Notice {
        pull,
        branch: format!("worktree-issue-{issue}"),
        commit: commit.into(),
        stuck: Stuck::Other(OtherCheck {
            name: name.into(),
            app: Some(app.into()),
            link: Some(link.into()),
        }),
    };
    let circle_link = "https://app.circleci.com/workflow/9?utm_source=github-checks-link";
    let notices = [
        other(41, 1, "aaaa111", "ci", "app.circleci.com", circle_link),
        other(
            42,
            2,
            "bbbb222",
            "buildkite/riff",
            "buildkite.com",
            "https://buildkite.com/o/r/builds/7",
        ),
        other(43, 3, "cccc333", "ci", "app.circleci.com", circle_link),
    ];
    assert_eq!(look.fixes.notices, notices);
    assert_eq!(look.fixes.notices[0].key(), "#41@aaaa111:checks:ci");

    // The message names the check, the app and the link.
    let body = riff::text::wave_stuck(
        REPO,
        41,
        "worktree-issue-1",
        "main",
        "aaaa111",
        &look.fixes.notices[0].stuck,
    );
    assert!(
        body.contains("the check ci of the app app.circleci.com failed"),
        "{body}"
    );
    assert!(body.contains("It is not a run of GitHub Actions"), "{body}");
    assert!(body.ends_with(&format!("Link: {circle_link}")), "{body}");

    // "Fix a failed check of another app" in the book shows the message.
    let example = Stuck::Other(OtherCheck {
        name: "ci".into(),
        app: Some("app.circleci.com".into()),
        link: Some("https://app.circleci.com/workflow/9".into()),
    });
    let message = riff::text::wave_stuck(
        REPO,
        40,
        "worktree-issue-12",
        "main",
        "1a2b3c4d5e",
        &example,
    );
    let book = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/src/development.md"
    ))
    .unwrap();
    assert!(book.contains(&format!("\n{message}\n")), "{message}");

    // `riff top` shows the item as stuck with the checks.
    let issues = riff::top::issues_from_gh(&gh, REPO).unwrap();
    assert_eq!(issues.stuck[&1], "checks");
    assert_eq!(issues.stuck[&2], "checks");
}

/// No session builds a release (the label `release`, or a branch
/// `release-…`): a failed check of its pull request is a notice, at any
/// state of its verify, also when the pull request names no issue.
#[test]
fn a_release_pull_request_with_a_failed_check_is_a_notice_and_no_build() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(
        dir.path(),
        &[
            // The issue 5 has the label `release`.
            pull_on(
                "release-v0.8.0",
                5,
                "eeee555",
                &[run("Gate", "SUCCESS"), run("Hygiene", "FAILURE")],
                "MERGEABLE",
                true,
            ),
            // A branch `release-…` that names no issue.
            r#"{"number":99,"headRefName":"release-v0.9.0","headRefOid":"ffff666","isDraft":false,"statusCheckRollup":[{"__typename":"CheckRun","name":"Hygiene","conclusion":"FAILURE"}],"mergeable":"MERGEABLE","autoMergeRequest":{"mergeMethod":"SQUASH"},"body":""}"#.to_owned(),
        ],
    );
    let look = looks(&gh);
    assert!(look.fixes.failed.is_empty());
    let checks = Stuck::Checks(vec!["Hygiene".into()]);
    let notice = |pull, branch: &str, commit: &str| Notice {
        pull,
        branch: branch.into(),
        commit: commit.into(),
        stuck: checks.clone(),
    };
    assert_eq!(
        look.fixes.notices,
        [
            notice(45, "release-v0.8.0", "eeee555"),
            notice(99, "release-v0.9.0", "ffff666")
        ]
    );
    assert_eq!(
        look.free,
        ["issue-1", "issue-2", "issue-3", "issue-4", "issue-6"]
    );
}

/// `riff top` and `riff wave list` show the item of a stuck pull request
/// in the group `stuck`, with the reason, while no session claims it.
#[test]
fn the_board_shows_a_stuck_pull_request_with_its_reason() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(
        dir.path(),
        &[
            pull(1, "aaaa111", &[verify("SUCCESS"), run("Gate", "FAILURE")]),
            pull_on(
                "worktree-issue-2",
                2,
                "bbbb222",
                &[verify("SUCCESS")],
                "MERGEABLE",
                false,
            ),
            pull_on("worktree-issue-3", 3, "cccc333", &[], "CONFLICTING", true),
            pull(4, "dddd444", &[verify("SUCCESS")]),
        ],
    );
    let issues = riff::top::issues_from_gh(&gh, REPO).unwrap();
    assert_eq!(issues.stuck[&1], "checks");
    assert_eq!(issues.stuck[&2], "auto-merge off");
    assert_eq!(issues.stuck[&3], "conflict");
    assert!(!issues.stuck.contains_key(&4), "it waits for the merge");

    let sessions = Sessions::new(vec![], vec![]);
    let board = Board::of(REPO, &issues, &sessions, &BTreeMap::new()).unwrap();
    assert_eq!(
        board.stuck,
        [
            (1, "checks".to_owned()),
            (2, "auto-merge off".to_owned()),
            (3, "conflict".to_owned())
        ]
    );
    assert_eq!(board.verify, [4]);
    assert_eq!(
        board.groups(false),
        [
            "free: #5 #6",
            "verify: #4",
            "stuck: #1 (checks), #2 (auto-merge off), #3 (conflict)"
        ]
    );

    // A session that claims the item shows in `claimed`, not in `stuck`.
    let claim: riff_core::wire::SessionInfo = serde_json::from_value(serde_json::json!({
        "uri": "riff://mike@thelio/o/r?session=a1&claim=issue-1", "live": true,
    }))
    .unwrap();
    let sessions = Sessions::new(vec![claim], vec![]);
    let board = Board::of(REPO, &issues, &sessions, &BTreeMap::new()).unwrap();
    assert_eq!(board.claimed, [1]);
    assert_eq!(board.stuck.len(), 2);
}

/// The runs of Actions that the fake `gh` gives for each commit.
fn runs(dir: &Path, json: &str) {
    std::fs::write(dir.join("runs"), json).unwrap();
}

/// The commits whose runs the look read, one for each read.
fn read_runs(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("calls"))
        .unwrap_or_default()
        .lines()
        .map(|l| l.split(['=', '&']).nth(1).unwrap_or(l).to_owned())
        .collect()
}

/// #1433: a pull request with a passed verify that GitHub holds for a
/// review never merges by itself. The look reads `reviewDecision` and the
/// merge state, and the lead gets a notice with the reason and the step
/// of a person, as the book shows. A pull request with no review rule, one
/// that is approved, one whose Gate goes, and one that waits for its
/// verify, have none.
#[test]
fn a_verified_pull_request_that_needs_a_review_is_a_notice_with_the_step_of_a_person() {
    let dir = tempfile::tempdir().unwrap();
    let held = pull_held;
    let gate = || run("Gate", "SUCCESS");
    let gh = forge(
        dir.path(),
        &[
            held(
                1,
                "aaaa111",
                &[verify("SUCCESS"), gate()],
                "REVIEW_REQUIRED",
                "BLOCKED",
            ),
            // The count of reviews is 0, and a commit has an author with no
            // GitHub account: GitHub says no decision and blocks the merge.
            held(2, "bbbb222", &[verify("SUCCESS"), gate()], "", "BLOCKED"),
            // The Gate goes: it explains the block.
            held(
                3,
                "cccc333",
                &[verify("SUCCESS"), run("Gate", "")],
                "",
                "BLOCKED",
            ),
            // The review came.
            held(
                4,
                "dddd444",
                &[verify("SUCCESS"), gate()],
                "APPROVED",
                "CLEAN",
            ),
            // It waits for its verify: no message yet.
            held(6, "ffff666", &[gate()], "REVIEW_REQUIRED", "BLOCKED"),
        ],
    );
    // No run of Actions waits.
    runs(dir.path(), r#"{"total_count":0,"workflow_runs":[]}"#);
    let look = looks(&gh);
    let notice = |pull, issue, commit: &str, why| Notice {
        pull,
        branch: format!("worktree-issue-{issue}"),
        commit: commit.into(),
        stuck: Stuck::Review(why),
    };
    assert_eq!(
        look.fixes.notices,
        [
            notice(41, 1, "aaaa111", Review::Required),
            notice(42, 2, "bbbb222", Review::Blocked)
        ]
    );
    // No session starts for them: no rebase, no build, no verify.
    assert!(look.fixes.conflicts.is_empty());
    assert!(look.fixes.failed.is_empty());
    assert_eq!(look.free, ["verify-issue-6"]);
    // One key for each pull request and head: the reason is the word.
    assert_eq!(look.fixes.notices[0].key(), "#41@aaaa111:review");
    // The look read the runs of the commits that GitHub blocks with no
    // check open, and of no other.
    assert_eq!(read_runs(dir.path()), ["aaaa111", "bbbb222"]);

    // The message has the reason and the step.
    let body = riff::text::wave_stuck(
        REPO,
        41,
        "worktree-issue-1",
        "main",
        "aaaa111",
        &look.fixes.notices[0].stuck,
    );
    assert!(
        body.contains("needs an approving review of a person"),
        "{body}"
    );
    assert!(body.ends_with("gh pr review 41 --approve"), "{body}");
    let body = riff::text::wave_stuck(
        REPO,
        42,
        "worktree-issue-2",
        "main",
        "bbbb222",
        &look.fixes.notices[1].stuck,
    );
    assert!(body.contains("an author with no GitHub account"), "{body}");

    // "See a pull request that cannot merge" in the book shows the message.
    let message = riff::text::wave_stuck(
        REPO,
        40,
        "worktree-issue-12",
        "main",
        "1a2b3c4d5e",
        &Stuck::Review(Review::Required),
    );
    let book = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/src/development.md"
    ))
    .unwrap();
    assert!(book.contains(&format!("\n{message}\n")), "{message}");
    assert!(book.contains("\ngh pr review 40 --approve\n"));
    let message = riff::text::wave_stuck(
        REPO,
        40,
        "worktree-issue-12",
        "main",
        "1a2b3c4d5e",
        &Stuck::Review(Review::Blocked),
    );
    assert!(book.contains(&format!("\n{message}\n")), "{message}");

    // `riff top` and `riff wave list` show the review in the group `stuck`.
    let issues = riff::top::issues_from_gh(&gh, REPO).unwrap();
    assert_eq!(issues.stuck[&1], "review");
    assert_eq!(issues.stuck[&2], "review");
    assert!(!issues.stuck.contains_key(&3), "the Gate goes");
    assert!(!issues.stuck.contains_key(&4), "it is approved");
    let sessions = Sessions::new(vec![], vec![]);
    let board = Board::of(REPO, &issues, &sessions, &BTreeMap::new()).unwrap();
    assert!(
        board
            .groups(false)
            .contains(&"stuck: #1 #2 (review)".to_owned())
    );
}

/// #1433: the run of a pull request from a fork waits in `action_required`
/// for a maintainer. It makes no check run, so GitHub counts its required
/// check as not reported, and blocks the merge. The look reads the runs of
/// the head, and the lead gets a notice with the run and the step. Only
/// the newest run of a workflow counts.
#[test]
fn a_run_that_waits_for_approval_is_a_notice_with_the_run_and_the_step() {
    let dir = tempfile::tempdir().unwrap();
    let gate = || run("Gate", "SUCCESS");
    let gh = forge(
        dir.path(),
        &[pull_held(
            1,
            "aaaa111",
            &[verify("SUCCESS"), gate()],
            "",
            "BLOCKED",
        )],
    );
    // The newest Hygiene (40151175338) waits. The older run passed, and an
    // older run of the Gate waits still, but a newer one passed.
    runs(
        dir.path(),
        r#"{"total_count":4,"workflow_runs":[
            {"id":40151175338,"workflow_id":2,"name":"Hygiene","status":"completed","conclusion":"action_required","html_url":"https://github.com/o/r/actions/runs/40151175338"},
            {"id":40151174504,"workflow_id":2,"name":"Hygiene","status":"completed","conclusion":"action_required","html_url":"https://github.com/o/r/actions/runs/40151174504"},
            {"id":40100000002,"workflow_id":1,"name":"CI","status":"completed","conclusion":"success","html_url":"https://github.com/o/r/actions/runs/40100000002"},
            {"id":40100000001,"workflow_id":1,"name":"CI","status":"completed","conclusion":"action_required","html_url":"https://github.com/o/r/actions/runs/40100000001"}]}"#,
    );
    let look = looks(&gh);
    let hygiene = Run {
        id: 40151175338,
        name: "Hygiene".into(),
        link: Some("https://github.com/o/r/actions/runs/40151175338".into()),
    };
    assert_eq!(
        look.fixes.notices,
        [Notice {
            pull: 41,
            branch: "worktree-issue-1".into(),
            commit: "aaaa111".into(),
            stuck: Stuck::Approve(hygiene),
        }]
    );
    assert_eq!(
        look.fixes.notices[0].key(),
        "#41@aaaa111:approve run:40151175338"
    );
    assert!(look.fixes.failed.is_empty());
    assert_eq!(read_runs(dir.path()), ["aaaa111"]);

    // The message has the run and the step.
    let body = riff::text::wave_stuck(
        REPO,
        41,
        "worktree-issue-1",
        "main",
        "aaaa111",
        &look.fixes.notices[0].stuck,
    );
    assert!(
        body.contains("the run Hygiene (run 40151175338) of a pull request from a fork"),
        "{body}"
    );
    assert!(
        body.ends_with("gh api -X POST repos/o/r/actions/runs/40151175338/approve"),
        "{body}"
    );

    // "See a pull request that cannot merge" in the book shows the message
    // and the step.
    let example = Stuck::Approve(Run {
        id: 40151174504,
        name: "Hygiene".into(),
        link: Some("https://github.com/como-technologies/riff/actions/runs/40151174504".into()),
    });
    let message = riff::text::wave_stuck(
        "como-technologies/riff",
        40,
        "worktree-issue-12",
        "main",
        "1a2b3c4d5e",
        &example,
    );
    let book = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/src/development.md"
    ))
    .unwrap();
    assert!(book.contains(&format!("\n{message}\n")), "{message}");
    assert!(book.contains(
        "\ngh api -X POST repos/como-technologies/riff/actions/runs/40151174504/approve\n"
    ));

    // After the approval the run goes on: no run waits, and the pull
    // request is a block with no cause for the wave (a review, likely).
    runs(
        dir.path(),
        r#"{"total_count":1,"workflow_runs":[{"id":40151175338,"workflow_id":2,"name":"Hygiene","status":"in_progress","conclusion":null}]}"#,
    );
    let look = looks(&gh);
    assert_eq!(look.fixes.notices[0].stuck, Stuck::Review(Review::Blocked));

    // A fault of `gh` for the runs gives no run, and no fault of the look.
    std::fs::remove_file(dir.path().join("runs")).unwrap();
    assert_eq!(
        looks(&gh).fixes.notices[0].stuck,
        Stuck::Review(Review::Blocked)
    );

    // `riff top` shows the item in the group `stuck`.
    runs(
        dir.path(),
        r#"{"workflow_runs":[{"id":9,"workflow_id":2,"name":"Hygiene","status":"completed","conclusion":"action_required"}]}"#,
    );
    let issues = riff::top::issues_from_gh(&gh, REPO).unwrap();
    assert_eq!(issues.stuck[&1], "approve run");
}

/// The check run `name` of GitHub Actions with `conclusion`, started at
/// `started`. The rollup keeps each run of a check on the head.
fn run_at(name: &str, conclusion: &str, started: &str) -> String {
    format!(
        r#"{{"__typename":"CheckRun","name":"{name}","conclusion":"{conclusion}","workflowName":"CI","detailsUrl":"https://github.com/o/r/actions/runs/9/job/1","startedAt":"{started}","completedAt":"{started}"}}"#
    )
}

/// #1548, 01M6G0BFAJJV9YNQZT502P1KGJ: the rollup keeps an old failed run
/// next to the newer run of the same check that passed. Such a pull
/// request is settled, as one with no old run: the look reads the runs of
/// its head, and the lead hears of the run that waits for approval one
/// time. A pull request whose newest run failed is not settled: it gets a
/// build, and the look reads no runs for it.
#[test]
fn a_run_that_waits_for_approval_tells_the_lead_once_next_to_an_old_failed_run_of_a_passed_check() {
    let dir = tempfile::tempdir().unwrap();
    let gate_old = || run_at("Gate", "FAILURE", "2026-11-03T05:50:00Z");
    let gate_new = || run_at("Gate", "SUCCESS", "2026-11-03T05:55:00Z");
    let gh = forge(
        dir.path(),
        &[
            // Each check has an old failed run and a newer passed run.
            pull_held(
                1,
                "aaaa111",
                &[
                    run_at("Hygiene", "FAILURE", "2026-11-03T05:51:00Z"),
                    verify("SUCCESS"),
                    gate_old(),
                    run_at("Hygiene", "SUCCESS", "2026-11-03T05:56:00Z"),
                    gate_new(),
                ],
                "",
                "BLOCKED",
            ),
            // The newest run of the Gate failed.
            pull_held(
                2,
                "bbbb222",
                &[
                    verify("SUCCESS"),
                    gate_old(),
                    gate_new(),
                    run_at("Gate", "FAILURE", "2026-11-03T06:00:00Z"),
                ],
                "",
                "BLOCKED",
            ),
        ],
    );
    runs(
        dir.path(),
        r#"{"total_count":1,"workflow_runs":[
            {"id":40151175338,"workflow_id":2,"name":"Hygiene","status":"completed","conclusion":"action_required","html_url":"https://github.com/o/r/actions/runs/40151175338"}]}"#,
    );
    let look = looks(&gh);
    let hygiene = Run {
        id: 40151175338,
        name: "Hygiene".into(),
        link: Some("https://github.com/o/r/actions/runs/40151175338".into()),
    };
    assert_eq!(
        look.fixes.notices,
        [Notice {
            pull: 41,
            branch: "worktree-issue-1".into(),
            commit: "aaaa111".into(),
            stuck: Stuck::Approve(hygiene),
        }]
    );
    assert_eq!(
        look.fixes.failed,
        BTreeMap::from([(
            2,
            Failed {
                pull: 42,
                commit: "bbbb222".into(),
                checks: vec!["Gate".into()]
            }
        )]),
        "the newest run of the Gate failed"
    );
    // The look read the runs of the settled head only.
    assert_eq!(read_runs(dir.path()), ["aaaa111"]);

    // The next look gives the same key: the lead hears of the run one time.
    let key = look.fixes.notices[0].key();
    assert_eq!(key, "#41@aaaa111:approve run:40151175338");
    assert_eq!(looks(&gh).fixes.notices[0].key(), key);

    // `riff top` shows the item in the group `stuck`.
    let issues = riff::top::issues_from_gh(&gh, REPO).unwrap();
    assert_eq!(issues.stuck[&1], "approve run");
}
