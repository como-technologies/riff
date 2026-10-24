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
use riff::rollout::Stuck;
use riff::wave::{self, Conflict, Failed, Notice, Work};
use riff_client::sessions::Sessions;
use riff_client::waves::Board;

const REPO: &str = "o/r";

/// A fake `gh`: the milestones, the file `pulls` for `pr list`, the file
/// `issues` for `issue list`, and a passed Gate for each commit.
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

/// The check run `name` with `conclusion`.
fn run(name: &str, conclusion: &str) -> String {
    format!(r#"{{"__typename":"CheckRun","name":"{name}","conclusion":"{conclusion}"}}"#)
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
    let body = riff::text::wave_stuck(41, "worktree-issue-1", "aaaa111", &Stuck::AutoMergeOff);
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
