//! `hygiene pr` with a fake `gh` on the PATH, and `hygiene commit` in a
//! git repository, as the workflow `hygiene.yml` and a person run them.

use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::SPAWN;

/// Runs `cmd` to its end. It starts under [`SPAWN`].
fn run(cmd: &mut Command) -> Output {
    let child = {
        let _lock = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    child.wait_with_output().unwrap()
}

const GOOD: &str = "Closes #77\n\nPause the riff.\n\nIssue: #77\nMilestone: Wave 3\n";

/// A directory with a fake `gh`. It prints `pr.json` for `gh pr view`,
/// and `issue-N.json` for `gh issue view N`. A missing file fails.
fn fake_gh(pr: &serde_json::Value, issues: &[serde_json::Value]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, value: &serde_json::Value| {
        std::fs::write(dir.path().join(name), value.to_string()).unwrap();
    };
    write("pr.json", pr);
    for issue in issues {
        write(&format!("issue-{}.json", issue["number"]), issue);
    }
    let script = format!(
        "#!/bin/sh\nd='{}'\ncase \"$1 $2\" in\n\
         'pr view') cat \"$d/pr.json\" ;;\n\
         'issue view') cat \"$d/issue-$3.json\" 2>/dev/null || {{ echo \"no issue $3\" >&2; exit 1; }} ;;\n\
         *) echo \"unknown: $*\" >&2; exit 1 ;;\nesac\n",
        dir.path().display()
    );
    let gh = dir.path().join("gh");
    let _lock = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
    std::fs::write(&gh, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}

fn hygiene(args: &[&str], path_first: Option<&Path>, cwd: Option<&Path>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hygiene"));
    cmd.args(args);
    if let Some(dir) = path_first {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![dir.to_path_buf()];
        paths.extend(std::env::split_paths(&path));
        cmd.env("PATH", std::env::join_paths(paths).unwrap());
    }
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    run(&mut cmd)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn pr_json(body: &str, milestone: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "title": "Pause and resume the riff",
        "body": body,
        "milestone": milestone.map(|t| serde_json::json!({"number": 8, "title": t, "description": "", "dueOn": null})),
    })
}

fn issue_json(number: u64, state: &str, milestone: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "number": number,
        "state": state,
        "milestone": milestone.map(|t| serde_json::json!({"number": 8, "title": t})),
    })
}

#[test]
fn a_good_pull_request_passes() {
    let gh = fake_gh(
        &pr_json(GOOD, Some("Wave 3")),
        &[issue_json(77, "OPEN", Some("Wave 3"))],
    );
    let out = hygiene(&["pr", "90"], Some(gh.path()), None);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "ok: pull request #90\n");
}

#[test]
fn a_broken_pull_request_fails_with_each_rule() {
    let body = GOOD.replace("Milestone: Wave 3\n", "");
    let gh = fake_gh(
        &pr_json(&body, Some("Wave 3")),
        &[issue_json(77, "CLOSED", Some("Wave 4"))],
    );
    let out = hygiene(&["pr", "90"], Some(gh.path()), None);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    for line in [
        "error: milestone-trailer: the message does not end with the trailer `Milestone: M`",
        "error: milestone: the pull request has milestone `Wave 3`, and issue #77 has `Wave 4`",
        "error: issue-open: issue #77 is closed",
        "pull request #90 breaks 3 rule(s) of issue hygiene.",
    ] {
        assert!(stderr.contains(line), "no {line:?} in:\n{stderr}");
    }
}

#[test]
fn a_pull_request_of_an_unknown_issue_is_a_tool_error() {
    let gh = fake_gh(&pr_json(GOOD, Some("Wave 3")), &[]);
    let out = hygiene(&["pr", "90"], Some(gh.path()), None);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("gh issue view 77"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn a_bad_number_or_command_prints_the_usage() {
    for args in [&["pr", "x"][..], &["pr"], &[]] {
        let out = hygiene(args, None, None);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            text(&out.stderr).contains("usage: hygiene pr NUMBER"),
            "{args:?}"
        );
    }
}

/// A git repository with one commit of `message`.
fn repo(message: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let out = run(Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir.path()));
        assert!(out.status.success(), "git {args:?}: {}", text(&out.stderr));
    };
    git(&["init", "-q"]);
    git(&["commit", "-q", "--allow-empty", "-m", message]);
    dir
}

#[test]
fn a_squash_commit_of_a_pull_request_passes() {
    let dir = repo(&format!("Pause and resume the riff (#90)\n\n{GOOD}"));
    let out = hygiene(&["commit"], None, Some(dir.path()));
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "ok: commit HEAD\n");
}

#[test]
fn a_commit_of_the_old_flow_fails() {
    let dir = repo("Pause and resume the riff (R1, #77)\n\nText.\n");
    let out = hygiene(&["commit", "HEAD"], None, Some(dir.path()));
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    for rule in ["issue-trailer:", "milestone-trailer:", "commit-title:"] {
        assert!(stderr.contains(rule), "no {rule} in:\n{stderr}");
    }
}

#[test]
fn an_unknown_commit_is_a_tool_error() {
    let dir = repo("One (#90)\n\nIssue: #77\nMilestone: Wave 3\n");
    let out = hygiene(&["commit", "no-such-rev"], None, Some(dir.path()));
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("git log no-such-rev"),
        "{}",
        text(&out.stderr)
    );
}
