//! `hygiene pr` with a fake `gh` on the PATH, and `hygiene commit` in a
//! git repository, as the workflow `hygiene.yml` and a person run them.

use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Runs `cmd` to its end.
fn run(cmd: &mut Command) -> Output {
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap()
}

const GOOD: &str = "Closes #77\n\nPause the riff.\n\nIssue: #77\nMilestone: Wave 3\n";

/// A directory with a fake `gh`. It prints `pr.json` for `gh pr view`,
/// and `issue-N.json` for `gh issue view N`. A missing file fails. It
/// appends each call to `calls.txt`.
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
        "#!/bin/sh\nd='{}'\necho \"$*\" >> \"$d/calls.txt\"\ncase \"$1 $2\" in\n\
         'pr view') cat \"$d/pr.json\" ;;\n\
         'issue view') cat \"$d/issue-$3.json\" 2>/dev/null || {{ echo \"no issue $3\" >&2; exit 1; }} ;;\n\
         *) echo \"unknown: $*\" >&2; exit 1 ;;\nesac\n",
        dir.path().display()
    );
    let gh = dir.path().join("gh");
    isolated::write_program(&gh, script);
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

/// 01M3JFEXS6M5549TC6MH0G2MS0: the squash of a pull request with a
/// co-author adds a paragraph `Co-authored-by:` after the trailers of
/// the body. The check reads the commits, and the pull request passes.
#[test]
fn a_pull_request_with_a_co_author_passes() {
    let mut pr = pr_json(GOOD, Some("Wave 3"));
    pr["author"] = serde_json::json!({"id": "x", "is_bot": false, "login": "ana", "name": "Ana"});
    pr["commits"] = serde_json::json!([
        {"oid": "a", "authors": [
            {"email": "ana@example.com", "id": "x", "login": "ana", "name": "Ana"},
            {"email": "noreply@z.ai", "id": "", "login": "", "name": "GLM-5.3-Flash"},
        ]},
    ]);
    let gh = fake_gh(&pr, &[issue_json(77, "OPEN", Some("Wave 3"))]);
    let out = hygiene(&["pr", "90"], Some(gh.path()), None);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "ok: pull request #90\n");
    let calls = std::fs::read_to_string(gh.path().join("calls.txt")).unwrap();
    assert!(
        calls.contains("pr view 90 --json title,body,milestone,author,commits"),
        "{calls}"
    );
}

#[test]
fn a_pull_request_with_a_co_author_and_a_last_paragraph_of_text_fails() {
    let body = format!("{GOOD}\nA last paragraph of text.\n");
    let mut pr = pr_json(&body, Some("Wave 3"));
    pr["commits"] = serde_json::json!([
        {"oid": "a", "authors": [{"email": "noreply@z.ai", "id": "", "login": "", "name": "GLM-5.3-Flash"}]},
    ]);
    let gh = fake_gh(&pr, &[issue_json(77, "OPEN", Some("Wave 3"))]);
    let out = hygiene(&["pr", "90"], Some(gh.path()), None);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    for rule in ["issue-trailer:", "milestone-trailer:"] {
        assert!(stderr.contains(rule), "no {rule} in:\n{stderr}");
    }
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

/// 01M518A04JWYJE5FA1VR309SAA: the lead renamed the wave after the
/// pull request got its trailer. The trailer names the wave by its
/// number, so the check passes. A trailer of another wave fails.
#[test]
fn a_trailer_of_the_old_title_passes_after_a_rename_of_the_wave() {
    let renamed = Some("Wave 28: cloud, fixes that slow the work");
    let old = GOOD.replace("Wave 3", "Wave 28: after 1.4.0");
    let gh = fake_gh(&pr_json(&old, renamed), &[issue_json(77, "OPEN", renamed)]);
    let out = hygiene(&["pr", "90"], Some(gh.path()), None);
    assert!(out.status.success(), "{}", text(&out.stderr));

    let other = GOOD.replace("Wave 3", "Wave 27: after 1.4.0");
    let gh = fake_gh(
        &pr_json(&other, renamed),
        &[issue_json(77, "OPEN", renamed)],
    );
    let out = hygiene(&["pr", "90"], Some(gh.path()), None);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains(
            "error: milestone-trailer: `Milestone: Wave 27: after 1.4.0` is not the milestone of \
             the pull request (Wave 28: cloud, fixes that slow the work)"
        ),
        "{stderr}"
    );
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

/// 01M3JFEXS6M5549TC6MH0G2MS0: GitHub adds a paragraph `Co-authored-by:`
/// after the trailers of the body. The commit on `main` must still pass.
#[test]
fn a_squash_commit_with_a_co_author_after_the_trailers_passes() {
    let body = format!(
        "{GOOD}Generated-by: GLM-5.3-Flash\n\nCo-authored-by: GLM-5.3-Flash <noreply@z.ai>\n"
    );
    let dir = repo(&format!("Pause and resume the riff (#90)\n\n{body}"));
    let out = hygiene(&["commit"], None, Some(dir.path()));
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "ok: commit HEAD\n");
}

#[test]
fn a_squash_commit_with_the_trailers_before_a_paragraph_of_text_fails() {
    let body = format!("{GOOD}\nA last paragraph of text.\n\nCo-authored-by: A <a@b.c>\n");
    let dir = repo(&format!("Pause and resume the riff (#90)\n\n{body}"));
    let out = hygiene(&["commit"], None, Some(dir.path()));
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    for rule in ["issue-trailer:", "milestone-trailer:"] {
        assert!(stderr.contains(rule), "no {rule} in:\n{stderr}");
    }
}
