//! The merge queue of como-technologies/riff (01M5TKPHDNHY75BHPD4XVBJRN9,
//! 01M5TKPHH2627K9NDS4P1Y1N3T): CI and Hygiene run on the event
//! `merge_group`, a job sets `riff/verify` on the merge group commit, and
//! `deploy/merge-queue.sh` turns the queue on or off in a ruleset. The
//! tests read the workflows and run the scripts with a fake `gh`. The fake
//! prints what the `--jq` filter of the real `gh` prints: the filters are
//! checked by a read of the live API, not here.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn text(path: &str) -> String {
    std::fs::read_to_string(top().join(path)).unwrap()
}

/// The lines of the block `on:` of a workflow.
fn triggers(workflow: &str) -> String {
    let rest = workflow.split_once("\non:\n").unwrap().1;
    rest.lines()
        .take_while(|line| line.is_empty() || line.starts_with(' ') || line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn ci_and_hygiene_run_on_the_merge_group() {
    for file in [".github/workflows/ci.yml", ".github/workflows/hygiene.yml"] {
        let on = triggers(&text(file));
        assert!(on.contains("\n  merge_group:"), "{file}: {on}");
    }
}

#[test]
fn a_job_of_ci_sets_the_verify_of_the_merge_group_with_the_script() {
    let ci = text(".github/workflows/ci.yml");
    let job = ci
        .split_once("\n  merge_verify:\n")
        .expect("ci.yml has the job merge_verify")
        .1;
    let job = job.split_once("\n  static:").unwrap().0;
    assert!(
        job.contains("if: github.event_name == 'merge_group'"),
        "{job}"
    );
    assert!(job.contains("statuses: write"), "{job}");
    assert!(
        job.contains("run: deploy/merge-group-verify.sh \"$HEAD_REF\" \"$HEAD_SHA\""),
        "{job}"
    );
    assert!(job.contains("github.event.merge_group.head_ref"), "{job}");
    assert!(job.contains("github.event.merge_group.head_sha"), "{job}");
}

#[test]
fn hygiene_checks_the_commit_of_the_merge_group() {
    let hygiene = text(".github/workflows/hygiene.yml");
    let step = hygiene
        .split_once("if: github.event_name == 'merge_group'")
        .expect("a step runs on the merge group")
        .1;
    let step = &step[..step.find("\n\n").unwrap_or(step.len())];
    assert!(step.contains("hygiene -- commit \"$GITHUB_SHA\""), "{step}");
}

/// A fake `gh` in `dir` that logs each call to `calls` and answers from
/// the files of `dir`.
fn gh(dir: &Path, script: &str) {
    isolated::write_program(
        dir.join("gh"),
        format!("#!/bin/sh\ndir=$(dirname \"$0\")\necho \"$*\" >> \"$dir/calls\"\n{script}"),
    );
}

fn run(script: &str, dir: &Path, args: &[&str]) -> Output {
    let path = format!(
        "{}:{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    Command::new("bash")
        .arg(top().join(script))
        .args(args)
        .env("PATH", path)
        .env("GH_REPO", "o/r")
        .env_remove("LEAD_APP")
        .output()
        .unwrap()
}

fn calls(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("calls"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The fake `gh` of `deploy/merge-group-verify.sh`. The statuses of the
/// head answer as the filter asks: the newest `riff/verify` of the bot of
/// the lead App (`lead`), else the newest `riff/verify` of each creator
/// (`status`).
const VERIFY_GH: &str = r#"case "$*" in
  'api repos/o/r/pulls/1234 --jq .head.sha') cat "$dir/head" ;;
  'api repos/o/r/commits/abc1234def/statuses?per_page=100 --jq '*'.creator.login == "riff-como-technologies-lead[bot]"'*) cat "$dir/lead" ;;
  'api repos/o/r/commits/abc1234def/statuses?per_page=100 --jq '*) cat "$dir/status" ;;
  'api -X POST repos/o/r/statuses/'*) ;;
  *) echo "no such call: $*" >&2; exit 1 ;;
esac
"#;

const GROUP: &str = "refs/heads/gh-readonly-queue/main/pr-1234-5e6f7a8";

/// The folder of a fake `gh` whose pull request has the head `abc1234def`,
/// whose newest `riff/verify` of the lead App is the status line `lead`,
/// and whose newest `riff/verify` of each creator is the line `status`.
fn forge_of(lead: &str, status: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    gh(dir.path(), VERIFY_GH);
    std::fs::write(dir.path().join("head"), "abc1234def\n").unwrap();
    std::fs::write(dir.path().join("lead"), lead).unwrap();
    std::fs::write(dir.path().join("status"), status).unwrap();
    dir
}

/// The folder of a fake `gh` whose newest `riff/verify` is the status
/// line `status`, from the lead App.
fn verify_forge(status: &str) -> tempfile::TempDir {
    forge_of(status, status)
}

/// The post of the failure of the merge group commit of pull request
/// #1234: no passed verify of the lead App at its head.
const FAILURE: &str = "api -X POST repos/o/r/statuses/9999aaa -f state=failure \
                       -f context=riff/verify -f description=Pull request #1234 has no passed \
                       verify of riff-como-technologies-lead[bot] at abc1234";

#[test]
fn a_passed_verify_of_the_head_is_success_on_the_merge_group_commit() {
    let dir = verify_forge("success\thttps://github.com/o/r/pull/1234#c1\n");
    let out = run(
        "deploy/merge-group-verify.sh",
        dir.path(),
        &[GROUP, "9999aaa"],
    );
    assert!(out.status.success(), "{out:?}");
    let calls = calls(dir.path());
    assert_eq!(calls.len(), 3, "{calls:?}");
    // 01M7GQKYSE0XZ1QFXCVR73JQ5R: the newest riff/verify of the lead App.
    assert!(
        calls[1].contains(
            "[.[] | select(.context == \"riff/verify\" and .creator.login == \
             \"riff-como-technologies-lead[bot]\")][0]"
        ),
        "{calls:?}"
    );
    assert_eq!(
        calls[2],
        "api -X POST repos/o/r/statuses/9999aaa -f state=success -f context=riff/verify \
         -f description=The verify of pull request #1234 passed at abc1234 \
         -f target_url=https://github.com/o/r/pull/1234#c1"
    );
}

#[test]
fn a_pull_request_with_no_passed_verify_is_failure_on_the_merge_group_commit() {
    for status in ["\t\n", "failure\thttps://c\n", "pending\t\n"] {
        let dir = verify_forge(status);
        let out = run(
            "deploy/merge-group-verify.sh",
            dir.path(),
            &[GROUP, "9999aaa"],
        );
        assert_eq!(out.status.code(), Some(1), "{status:?}: {out:?}");
        let post = calls(dir.path()).pop().unwrap();
        assert_eq!(post, FAILURE, "{status:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("pull request #1234"));
    }
}

/// 01M7GQKYSE0XZ1QFXCVR73JQ5R: a success that another creator set, for
/// example a workflow of the pull request, is no pass. The script copies
/// nothing: it sets failure, and fails.
#[test]
fn a_success_of_another_creator_is_failure_on_the_merge_group_commit() {
    for lead in ["\t\n", "failure\thttps://github.com/o/r/pull/1234#c1\n"] {
        let dir = forge_of(lead, "success\thttps://evil.example\n");
        let out = run(
            "deploy/merge-group-verify.sh",
            dir.path(),
            &[GROUP, "9999aaa"],
        );
        assert_eq!(out.status.code(), Some(1), "{lead:?}: {out:?}");
        let posts: Vec<String> = calls(dir.path())
            .into_iter()
            .filter(|c| c.contains("-X POST"))
            .collect();
        assert_eq!(posts, [FAILURE], "{lead:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("has no passed riff/verify of riff-como-technologies-lead[bot]"),
            "{stderr}"
        );
    }
}

/// `LEAD_APP` names another lead App: only a status of its bot counts.
#[test]
fn lead_app_names_the_creator_of_a_verify_that_counts() {
    let dir = verify_forge("success\t\n");
    let path = format!(
        "{}:{}",
        dir.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("bash")
        .arg(top().join("deploy/merge-group-verify.sh"))
        .args([GROUP, "9999aaa"])
        .env("PATH", path)
        .env("GH_REPO", "o/r")
        .env("LEAD_APP", "riff-acme-lead")
        .output()
        .unwrap();
    // The fake knows only the bot of riff-como-technologies-lead: the
    // filter of riff-acme-lead[bot] gets the status of each creator.
    assert!(out.status.success(), "{out:?}");
    let calls = calls(dir.path());
    assert!(
        calls[1].contains(".creator.login == \"riff-acme-lead[bot]\""),
        "{calls:?}"
    );
}

#[test]
fn the_merge_group_branch_of_another_base_names_its_pull_request() {
    let dir = verify_forge("success\t\n");
    let branch = "refs/heads/gh-readonly-queue/release/1.0/pr-1234-5e6f7a8";
    let out = run(
        "deploy/merge-group-verify.sh",
        dir.path(),
        &[branch, "9999aaa"],
    );
    assert!(out.status.success(), "{out:?}");
    assert!(calls(dir.path())[0].contains("pulls/1234"));
}

#[test]
fn a_branch_that_is_no_merge_group_sets_no_status() {
    let dir = verify_forge("success\t\n");
    for args in [["refs/heads/main", "9999aaa"], [GROUP, ""], ["", ""]] {
        let out = run("deploy/merge-group-verify.sh", dir.path(), &args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
    }
    assert!(calls(dir.path()).is_empty(), "{:?}", calls(dir.path()));
}

const QUEUE_GH: &str = r#"case "$*" in
  'api repos/o/r/rulesets?per_page=100 --jq '*) cat "$dir/id" ;;
  'api repos/o/r/rulesets/77') cat "$dir/before" ;;
  'api repos/o/r/rulesets/77 --jq {name'*) cat "$dir/after" ;;
  'api -X PUT repos/o/r/rulesets/77 --input '*) cp "${*##* }" "$dir/put" ;;
  'api repos/o/r/rulesets/77 --jq [.rules'*) echo "deletion, merge_queue" ;;
  *) echo "no such call: $*" >&2; exit 1 ;;
esac
"#;

fn queue_forge(id: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    gh(dir.path(), QUEUE_GH);
    std::fs::write(dir.path().join("id"), id).unwrap();
    std::fs::write(dir.path().join("before"), "{\"rules\":[\"before\"]}\n").unwrap();
    std::fs::write(dir.path().join("after"), "{\"rules\":[\"after\"]}\n").unwrap();
    dir
}

#[test]
fn on_adds_the_queue_rule_to_the_ruleset_and_keeps_a_copy_of_the_old_one() {
    let dir = queue_forge("77\n");
    let out = run("deploy/merge-queue.sh", dir.path(), &["on", "o/r"]);
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("The rules of the ruleset main: deletion, merge_queue"),
        "{stdout}"
    );
    // The update sends the body of the filter, and only that.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("put")).unwrap(),
        "{\"rules\":[\"after\"]}\n"
    );
    // The old ruleset is in the file that the script names.
    let saved = stdout
        .split("as it was: ")
        .nth(1)
        .and_then(|rest| rest.lines().next())
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(saved).unwrap(),
        "{\"rules\":[\"before\"]}\n"
    );
    let filter = calls(dir.path())
        .into_iter()
        .find(|c| c.contains("--jq {name"))
        .unwrap();
    for part in [
        "\"type\":\"merge_queue\"",
        "\"merge_method\":\"SQUASH\"",
        "select(.type != \"merge_queue\")",
    ] {
        assert!(filter.contains(part), "{part}: {filter}");
    }
}

#[test]
fn off_sends_the_filter_without_the_queue_rule() {
    let dir = queue_forge("77\n");
    let out = run("deploy/merge-queue.sh", dir.path(), &["off", "o/r", "main"]);
    assert!(out.status.success(), "{out:?}");
    let filter = calls(dir.path())
        .into_iter()
        .find(|c| c.contains("--jq {name"))
        .unwrap();
    assert!(!filter.contains("\"merge_method\""), "{filter}");
    assert!(
        filter.contains("select(.type != \"merge_queue\")] + []"),
        "{filter}"
    );
    assert!(dir.path().join("put").exists());
}

#[test]
fn a_ruleset_that_is_not_there_changes_nothing() {
    let dir = queue_forge("\n");
    let out = run("deploy/merge-queue.sh", dir.path(), &["on", "o/r", "other"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("o/r has no ruleset named other"));
    assert!(!dir.path().join("put").exists());
    assert!(calls(dir.path()).iter().all(|c| !c.contains("PUT")));
}

#[test]
fn a_wrong_mode_or_no_repository_is_a_usage_error() {
    let dir = queue_forge("77\n");
    for args in [&["maybe", "o/r"][..], &["on"], &[]] {
        let out = run("deploy/merge-queue.sh", dir.path(), args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {out:?}");
    }
    assert!(calls(dir.path()).is_empty());
}
