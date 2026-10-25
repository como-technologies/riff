//! The steps of a pull request on GitHub (#201): `riff pr open`
//! (01M3NB6FTGPD0S5JTXXXNGNNDT), `riff pr wait`
//! (01M3NB6FWMGBQ9VTY6RCBPKBHK) and `riff verify`
//! (01M3NB6FYXXKX80VHEVA5CV6RY).
//!
//! A fake `gh` on `PATH` logs each call, and the body that it gets on
//! stdin, and answers from a `case` of the test. A real `riff-server`
//! with sign-in holds the claims and the posts.

use std::path::Path;
use std::process::{Command, Output};

use isolated::Isolated;

use crate::book;

/// A fake `gh` in `bin` that logs `gh ARGS` to `bin/gh.log`, copies its
/// stdin to the log for `--body-file -`, and runs `cases`: the arms of a
/// `case "$*" in`.
fn fake_gh(bin: &Path, cases: &str) {
    let path = bin.join("gh");
    isolated::write_program(
        &path,
        format!(
            "#!/bin/sh\ndir=$(dirname \"$0\")\necho \"gh $*\" >> \"$dir/gh.log\"\n\
             case \"$*\" in *'--body-file -'*) cat > \"$dir/body\"; cat \"$dir/body\" >> \"$dir/gh.log\" ;; esac\n\
             case \"$*\" in\n{cases}\nesac\nexit 0\n"
        ),
    );
}

fn log(bin: &Path) -> String {
    std::fs::read_to_string(bin.join("gh.log")).unwrap_or_default()
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// The riff config of riff: it asks for each rule of the hygiene check
/// (01M5SMSMWE89SC3Q7XFBQCFY32).
const RIFF_TOML: &str = "[hygiene]\nrules = \"riff\"\n";

/// A git repository with a GitHub origin, so the place of each session
/// is `como-technologies/riff`, with the riff config of riff.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/como-technologies/riff.git",
        ],
    ] {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status();
        assert!(status.unwrap().success());
    }
    std::fs::write(dir.path().join("riff.toml"), RIFF_TOML).unwrap();
    dir
}

/// One machine: its environment, the fake `gh` and a clone.
struct Machine {
    env: Isolated,
    bin: tempfile::TempDir,
    repo: tempfile::TempDir,
    server: crate::common::Signed,
}

impl Machine {
    async fn new(cases: &str) -> Machine {
        let bin = tempfile::tempdir().unwrap();
        fake_gh(bin.path(), cases);
        Machine {
            env: Isolated::new(),
            bin,
            repo: repo(),
            server: crate::common::start_signed().await,
        }
    }

    /// Runs `riff ARGS` as the session `session` of mike, with the fake
    /// `gh` first in `PATH`, away from the runtime of the server.
    async fn run(&self, session: &str, args: &[&str]) -> Output {
        let path = format!(
            "{}:{}",
            self.bin.path().display(),
            std::env::var("PATH").unwrap()
        );
        crate::common::sign_in(&self.server, &self.env.riff_home(), "mike");
        let mut cmd = self.env.riff();
        cmd.args(args)
            .current_dir(self.repo.path())
            .env("RIFF_SERVER", &*self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "thelio")
            .env("RIFF_SESSION", session)
            .env("PATH", path);
        tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap()
    }

    /// Runs `riff ARGS` as `session`, and checks that it succeeds.
    async fn ok(&self, session: &str, args: &[&str]) -> String {
        let out = self.run(session, args).await;
        assert!(out.status.success(), "riff {args:?}: {}", text(&out.stderr));
        text(&out.stdout)
    }

    /// Makes `session` the lead, resumes the new riff, and claims `item`
    /// as `session`.
    async fn claim(&self, session: &str, item: &str) {
        self.ok(session, &["whoami"]).await;
        self.ok("", &["lead", "--session", session]).await;
        self.ok(session, &["resume", "--riff"]).await;
        self.ok(session, &["claim", item]).await;
    }

    /// Sets the riff config of the clone: `None` removes the file.
    fn riff_toml(&self, config: Option<&str>) {
        let path = self.repo.path().join("riff.toml");
        match config {
            Some(config) => std::fs::write(path, config).unwrap(),
            None => std::fs::remove_file(path).unwrap(),
        }
    }

    fn write(&self, name: &str, content: &str) -> String {
        let path = self.repo.path().join(name);
        std::fs::write(&path, content).unwrap();
        path.display().to_string()
    }
}

const MERGED: &str = r#"*'pr view 40 --json state,mergeCommit'*) echo '{"state":"MERGED","mergeCommit":{"oid":"9f8e7d6c"}}' ;;"#;

#[tokio::test]
async fn pr_wait_prints_the_merge_commit_of_a_merged_pull_request() {
    let machine = Machine::new(MERGED).await;
    let out = machine.run("s1", &["pr", "wait", "40"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "9f8e7d6c\n");
}

#[tokio::test]
async fn pr_wait_waits_while_the_pull_request_is_open() {
    let open_then_merged = r#"*'pr view 40 --json state,mergeCommit'*)
    n=$(cat "$dir/views" 2>/dev/null || echo 0); n=$((n + 1)); echo $n > "$dir/views"
    if [ $n -lt 3 ]; then echo '{"state":"OPEN","mergeCommit":null}'
    else echo '{"state":"MERGED","mergeCommit":{"oid":"9f8e7d6c"}}'; fi ;;
*'pr checks 40 --required --json name,bucket'*) echo '[{"name":"Gate","bucket":"pass"},{"name":"riff/verify","bucket":"pending"}]' ;;"#;
    let machine = Machine::new(open_then_merged).await;
    let out = machine
        .run("s1", &["pr", "wait", "40", "--every", "1"])
        .await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "9f8e7d6c\n");
    let log = log(machine.bin.path());
    assert_eq!(log.matches("gh pr view 40 --json state").count(), 3);
}

/// After one good look, a look of `gh` that fails does not end the
/// wait. It prints one line, and looks again
/// (01M3Z8GG5EGEYAVEXG0HS46ACT).
#[tokio::test]
async fn pr_wait_goes_on_after_a_look_that_fails() {
    let open_away_merged = r#"*'pr view 40 --json state,mergeCommit'*)
    n=$(cat "$dir/views" 2>/dev/null || echo 0); n=$((n + 1)); echo $n > "$dir/views"
    if [ $n -eq 1 ]; then echo '{"state":"OPEN","mergeCommit":null}'
    elif [ $n -lt 4 ]; then echo 'error connecting to api.github.com' >&2; exit 1
    else echo '{"state":"MERGED","mergeCommit":{"oid":"9f8e7d6c"}}'; fi ;;
*'pr checks 40 --required --json name,bucket'*) echo '[{"name":"Gate","bucket":"pass"}]' ;;"#;
    let machine = Machine::new(open_away_merged).await;
    let out = machine
        .run("s1", &["pr", "wait", "40", "--every", "1"])
        .await;
    let stderr = text(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert_eq!(text(&out.stdout), "9f8e7d6c\n");
    let line = "riff: cannot look at pull request #40: gh pr view 40 --json state,mergeCommit: \
                error connecting to api.github.com. Trying again every 1 seconds.";
    assert_eq!(stderr.matches(line).count(), 1, "{stderr}");
    assert_eq!(stderr.matches("cannot look at").count(), 1, "{stderr}");
    let log = log(machine.bin.path());
    assert_eq!(log.matches("gh pr view 40 --json state").count(), 4);
}

/// When the first look of `gh` fails, the wait ends with the error
/// (01M3Z8GG5EGEYAVEXG0HS46ACT).
#[tokio::test]
async fn pr_wait_ends_when_the_first_look_fails() {
    let away =
        r#"*'pr view 40 --json state,mergeCommit'*) echo 'no pull requests found' >&2; exit 1 ;;"#;
    let machine = Machine::new(away).await;
    let out = machine
        .run("s1", &["pr", "wait", "40", "--every", "1"])
        .await;
    let stderr = text(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("no pull requests found"), "{stderr}");
    assert!(!stderr.contains("cannot look at"), "{stderr}");
    let log = log(machine.bin.path());
    assert_eq!(log.matches("gh pr view 40 --json state").count(), 1);
}

#[tokio::test]
async fn pr_wait_fails_for_a_closed_pull_request() {
    let closed = r#"*'pr view 40 --json state,mergeCommit'*) echo '{"state":"CLOSED","mergeCommit":null}' ;;"#;
    let machine = Machine::new(closed).await;
    let out = machine.run("s1", &["pr", "wait", "40"]).await;
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(
        text(&out.stderr).contains("pull request #40 is closed, and not merged"),
        "{}",
        text(&out.stderr)
    );
}

#[tokio::test]
async fn pr_wait_fails_for_a_failed_required_check() {
    let failed = r#"*'pr view 40 --json state,mergeCommit'*) echo '{"state":"OPEN","mergeCommit":null}' ;;
*'pr checks 40 --required --json name,bucket'*) echo '[{"name":"Gate","bucket":"fail"},{"name":"Hygiene","bucket":"pass"}]' ;;"#;
    let machine = Machine::new(failed).await;
    let out = machine.run("s1", &["pr", "wait", "40"]).await;
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("a required check failed: Gate"),
        "{}",
        text(&out.stderr)
    );
}

const VERIFY: &str = r#"*'pr view 40 --json headRefOid,body'*) printf '%s' '{"headRefOid":"1a2b3c4d","body":"Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n"}' ;;
*'pr comment 40 --body-file -'*) echo 'https://github.com/como-technologies/riff/pull/40#issuecomment-7' ;;
*'/statuses/'*) echo '{}' ;;
*'api repos/como-technologies/riff') echo '{"default_branch":"main"}' ;;
*'/rules/branches/main?per_page=100'*) echo '[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"Gate"},{"context":"riff/verify"}]}}]' ;;
*'/check-runs?check_name=Gate'*) echo '{"total_count":1,"check_runs":[{"name":"Gate","status":"completed","conclusion":"success"}]}' ;;"#;

/// `riff verify VERDICT` makes one comment, one status on the head
/// commit and one post to the holder of the issue.
async fn verify(verdict: &str, state: &str) {
    let machine = Machine::new(VERIFY).await;
    machine.claim("author", "issue-12").await;
    let result = machine.write("result.md", "1. The wave shows.\n");
    let out = machine
        .ok(
            "verifier",
            &[
                "verify", verdict, "40", "--file", &result, "--commit", "1a2b3c4d",
            ],
        )
        .await;
    assert!(out.contains("Woke mike@thelio:"), "{out}");

    let log = log(machine.bin.path());
    assert_eq!(log.matches("gh pr comment").count(), 1, "{log}");
    let word = verdict.to_uppercase();
    assert!(
        log.contains(&format!(
            "verify result: {word} for issue-12, commit 1a2b3c4d.\n\n1. The wave shows.\n"
        )),
        "{log}"
    );
    let statuses: Vec<&str> = log.lines().filter(|l| l.contains("/statuses/")).collect();
    assert_eq!(
        statuses,
        [format!(
            "gh api repos/como-technologies/riff/statuses/1a2b3c4d -f state={state} \
             -f context=riff/verify -f description={word}: verify-issue-12 \
             -f target_url=https://github.com/como-technologies/riff/pull/40#issuecomment-7"
        )]
    );

    let inbox = machine.ok("author", &["read"]).await;
    assert_eq!(inbox.matches("verify result:").count(), 1, "{inbox}");
    assert!(
        inbox.contains(&format!(
            "verify result: {word} for issue-12, PR #40, commit 1a2b3c4d."
        )),
        "{inbox}"
    );
    assert!(inbox.contains("to claim=issue-12"), "{inbox}");
}

#[tokio::test]
async fn verify_pass_comments_sets_success_and_posts_to_the_claim() {
    verify("pass", "success").await;
}

#[tokio::test]
async fn verify_fail_sets_failure() {
    verify("fail", "failure").await;
}

/// The author released the item at its verify request. So no session
/// holds the issue, and the result wakes the lead of the verifier: a
/// result is never silent (01M3Z9N70J4H79VJN4ZKKH3G6S). With a holder,
/// the lead does not wake.
#[tokio::test]
async fn a_verify_result_wakes_the_lead_when_no_session_holds_the_item() {
    let machine = Machine::new(VERIFY).await;
    machine.ok("lead", &["whoami"]).await;
    machine.ok("", &["lead", "--session", "lead"]).await;
    machine.ok("lead", &["resume", "--riff"]).await;
    let result = machine.write("result.md", "1. The wave does not show.\n");
    let verify = [
        "verify", "fail", "40", "--file", &result, "--commit", "1a2b3c4d",
    ];
    let out = machine.ok("verifier", &verify).await;
    assert!(out.contains("Woke mike@thelio:riff (lead)"), "{out}");
    assert!(out.contains("No session matches claim=issue-12"), "{out}");
    let inbox = machine.ok("lead", &["read"]).await;
    assert!(
        inbox.contains("verify result: FAIL for issue-12, PR #40, commit 1a2b3c4d."),
        "{inbox}"
    );

    // A session holds the item: only it wakes.
    machine.ok("author", &["claim", "issue-12"]).await;
    let out = machine.ok("verifier", &verify).await;
    assert!(out.contains("Woke mike@thelio:riff (author)"), "{out}");
    assert!(!out.contains("(lead)"), "{out}");
    assert!(!out.contains("No session matches"), "{out}");
}

/// The fake `gh` gives one open pull request for the branch of issue 12,
/// with the status `riff/verify` of `STATUS` on its head.
fn pulls(status: &str) -> String {
    format!(
        r#"*'pr list'*) echo '[{{"number":40,"headRefName":"worktree-issue-12","headRefOid":"1a2b3c4d5e6f","isDraft":false,"statusCheckRollup":[{{"__typename":"CheckRun","name":"Gate","conclusion":"SUCCESS"}}{status}]}}]' ;;"#
    )
}

/// A granted claim of an item names its open pull request and the state
/// of its verify (01M3Z9N6SPWPSSCBEVDCKBESSV): the author released the
/// item at the verify request, so the next session must know it. riff
/// asks `gh` only for an item with a pushed branch.
#[tokio::test]
async fn a_claim_names_the_pull_request_of_the_item_and_its_verify() {
    let status = |state: &str| {
        format!(
            r#",{{"__typename":"StatusContext","context":"riff/verify","state":"{state}","targetUrl":"https://github.com/como-technologies/riff/pull/40#issuecomment-7"}}"#
        )
    };
    for (status, line) in [
        (
            status("FAILURE"),
            "The verify of pull request #40 of issue-12 failed for commit 1a2b3c4: \
             https://github.com/como-technologies/riff/pull/40#issuecomment-7. Read the result, \
             go on from the branch, and send a new verify request: see \"Pick up dropped work\" \
             in the riff skill.",
        ),
        (
            String::new(),
            "Pull request #40 of issue-12 waits for a verify of commit 1a2b3c4. The build is \
             done: do not build it again. Release issue-12. To verify the work, claim \
             verify-issue-12.",
        ),
        (
            status("SUCCESS"),
            "The verify of pull request #40 of issue-12 passed for commit 1a2b3c4, and the merge \
             waits. The build is done: do not build it again. Release issue-12.",
        ),
    ] {
        let machine = Machine::new(&pulls(&status)).await;
        machine.ok("lead", &["whoami"]).await;
        machine.ok("", &["lead", "--session", "lead"]).await;
        machine.ok("lead", &["resume", "--riff"]).await;

        // No pushed branch: riff does not ask `gh`.
        let out = machine.ok("next", &["claim", "issue-12"]).await;
        assert_eq!(out, "You hold issue-12 in como-technologies/riff.\n");
        assert!(!log(machine.bin.path()).contains("pr list"));
        machine.ok("next", &["release", "issue-12"]).await;

        // The branch of the author is on `origin`.
        let head = commit(&machine);
        let git = Command::new("git")
            .args(["update-ref", "refs/remotes/origin/worktree-issue-12", &head])
            .current_dir(machine.repo.path())
            .status();
        assert!(git.unwrap().success());
        let out = machine.ok("next", &["claim", "issue-12"]).await;
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3, "{out}");
        assert!(lines[1].starts_with("Earlier work on issue-12: the pushed branch"));
        assert_eq!(lines[2], line);
        assert!(
            log(machine.bin.path()).contains(
                "gh pr list --repo como-technologies/riff --state open --limit 100 --json \
                 number,headRefName,headRefOid,isDraft,statusCheckRollup"
            ),
            "{}",
            log(machine.bin.path())
        );

        // A verify claim, and an item with no pull request, get no line.
        let out = machine.ok("verifier", &["claim", "verify-issue-12"]).await;
        assert_eq!(out, "You hold verify-issue-12 in como-technologies/riff.\n");
        let out = machine.ok("verifier", &["claim", "issue-13"]).await;
        assert_eq!(out, "You hold issue-13 in como-technologies/riff.\n");
    }
}

/// Makes one empty commit in the clone of `machine`, and returns it.
fn commit(machine: &Machine) -> String {
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .current_dir(machine.repo.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", text(&out.stderr));
        text(&out.stdout).trim().to_owned()
    };
    git(&["commit", "-q", "--allow-empty", "-m", "tested"]);
    git(&["rev-parse", "HEAD"])
}

/// A verify counts only for its commit: when the head of the pull
/// request is not HEAD of the verifier, riff makes no comment, no
/// status and no post.
#[tokio::test]
async fn verify_refuses_when_the_head_is_not_the_tested_commit() {
    let machine = Machine::new(VERIFY).await;
    machine.claim("author", "issue-12").await;
    let tested = commit(&machine);
    let result = machine.write("result.md", "1. The wave shows.\n");
    for args in [
        &["verify", "pass", "40", "--file", &result][..],
        &[
            "verify", "fail", "40", "--file", &result, "--commit", "9f8e7d6",
        ],
    ] {
        let out = machine.run("verifier", args).await;
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(
            text(&out.stderr).contains("but the head of the pull request is 1a2b3c4d"),
            "{}",
            text(&out.stderr)
        );
    }
    assert!(
        text(
            &machine
                .run("verifier", &["verify", "pass", "40", "--file", &result])
                .await
                .stderr
        )
        .contains(&format!("you tested {tested},"))
    );
    let log = log(machine.bin.path());
    assert!(!log.contains("gh pr comment"), "{log}");
    assert!(!log.contains("/statuses/"), "{log}");
    let inbox = machine.ok("author", &["read"]).await;
    assert!(!inbox.contains("verify result:"), "{inbox}");
}

/// A pass needs a success of the Gate of the head commit
/// (01M49HAZ7P3JMWNCG1SWCMAXQP): the Gate is the one full test run, and
/// the verifier does not run it again. While the Gate runs, failed or
/// has not started, `riff verify pass` makes no comment, no status and
/// no post. A fail needs no Gate.
#[tokio::test]
async fn verify_pass_refuses_while_the_gate_of_the_head_has_no_success() {
    let gate = |runs: &str| {
        VERIFY.replace(
            r#"[{"name":"Gate","status":"completed","conclusion":"success"}]"#,
            runs,
        )
    };
    let cases = [
        (
            gate(r#"[{"name":"Gate","status":"in_progress","conclusion":null}]"#),
            "the full check of commit 1a2b3c4 (Gate) runs still.",
        ),
        (
            gate(r#"[{"name":"Gate","status":"completed","conclusion":"failure"}]"#),
            "the full check of commit 1a2b3c4 (Gate) ended with failure.",
        ),
        (
            gate("[]"),
            "the full check of commit 1a2b3c4 (Gate) has not started.",
        ),
    ];
    for (fake, line) in cases {
        let machine = Machine::new(&fake).await;
        machine.claim("author", "issue-12").await;
        let result = machine.write("result.md", "1. The wave shows.\n");
        let pass = [
            "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
        ];
        let out = machine.run("verifier", &pass).await;
        assert_eq!(out.status.code(), Some(1), "{line}");
        let stderr = text(&out.stderr);
        assert!(
            stderr.contains(&format!(
                "riff verify pass: {line} A pass needs a success of each check of it: see gh pr checks 40. \
                 Nothing is reported."
            )),
            "{stderr}"
        );
        assert_eq!(stderr.trim().lines().count(), 1, "one line: {stderr}");
        let calls = log(machine.bin.path());
        assert!(
            calls.contains(
                "gh api repos/como-technologies/riff/commits/1a2b3c4d/check-runs?check_name=Gate"
            ),
            "{calls}"
        );
        assert!(!calls.contains("gh pr comment"), "{calls}");
        assert!(!calls.contains("/statuses/"), "{calls}");
        let inbox = machine.ok("author", &["read"]).await;
        assert!(!inbox.contains("verify result:"), "{inbox}");

        // A fail needs no Gate.
        let fail = [
            "verify", "fail", "40", "--file", &result, "--commit", "1a2b3c4d",
        ];
        machine.ok("verifier", &fail).await;
        let calls = log(machine.bin.path());
        assert!(calls.contains("-f state=failure"), "{calls}");
    }
}

/// The fake forge of a pull request that changes `riff.toml` from the
/// pin of `ghcr.io/o/i` to the pin `head`.
fn pin_cases(head: &str) -> String {
    let file = |image: &str| format!(r#"printf '[session]\nimage = "{image}"\n'"#);
    format!(
        "{VERIFY}\n\
         *'/pulls/40/files'*) printf 'src/lib.rs\\nriff.toml\\n' ;;\n\
         *'pr view 40 --json baseRefName'*) echo main ;;\n\
         *'contents/riff.toml?ref=1a2b3c4d') {} ;;\n\
         *'contents/riff.toml?ref=main') {} ;;",
        file(head),
        file(&format!("ghcr.io/o/i@sha256:{}", "0".repeat(64))),
    )
}

/// 01M5SMRRAG1WPBJMQKKTPSRHRP: a pull request that pins a new session
/// image that a registry does not give with no login gets no pass. The
/// line has the fix. Nothing is commented, set or posted.
#[tokio::test]
async fn verify_pass_refuses_a_new_image_that_a_host_cannot_pull_with_no_login() {
    // A package that is private, one that is not there, and a registry
    // that gives no anonymous token.
    for (manifest, token, answer) in [(403, 200, 403_u16), (404, 200, 404), (200, 401, 401)] {
        let (host, seen) = crate::common::registry(manifest, token).await;
        let head = format!("{host}/o/i@sha256:{}", "1".repeat(64));
        let machine = Machine::new(&pin_cases(&head)).await;
        machine.claim("author", "issue-12").await;
        let result = machine.write("result.md", "1. The wave shows.\n");
        let pass = [
            "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
        ];
        let out = machine.run("verifier", &pass).await;
        assert_eq!(out.status.code(), Some(1), "{answer}");
        let stderr = text(&out.stderr);
        assert_eq!(stderr.trim().lines().count(), 1, "one line: {stderr}");
        let image: riff::session_image::Image = head.parse().unwrap();
        let status = reqwest::StatusCode::from_u16(answer).unwrap();
        let why = format!("the registry answered {status}");
        let line = riff::text::verify_image_closed(40, &image, &why);
        assert!(stderr.contains(&line), "{line}\n{stderr}");
        assert!(
            stderr.contains(&format!(
                "Fix: make the package public, or run podman login {host} on each host of the wave."
            )),
            "{stderr}"
        );
        assert!(stderr.contains("Nothing is reported."), "{stderr}");
        assert!(seen.load(std::sync::atomic::Ordering::SeqCst) > 0);
        let calls = log(machine.bin.path());
        assert!(calls.contains("contents/riff.toml?ref=1a2b3c4d"), "{calls}");
        assert!(!calls.contains("gh pr comment"), "{calls}");
        assert!(!calls.contains("/statuses/"), "{calls}");
        let inbox = machine.ok("author", &["read"]).await;
        assert!(!inbox.contains("verify result:"), "{inbox}");

        // A fail needs no such check.
        let fail = [
            "verify", "fail", "40", "--file", &result, "--commit", "1a2b3c4d",
        ];
        machine.ok("verifier", &fail).await;
        assert!(log(machine.bin.path()).contains("-f state=failure"));
    }
}

/// A new image that the registry gives to the anonymous token passes, and
/// a pin that is the pin of the base branch is not asked at all.
#[tokio::test]
async fn verify_pass_reports_a_new_image_that_a_host_can_pull_and_asks_no_old_pin() {
    let (host, seen) = crate::common::registry(200, 200).await;
    let head = format!("{host}/o/i@sha256:{}", "1".repeat(64));
    let machine = Machine::new(&pin_cases(&head)).await;
    machine.claim("author", "issue-12").await;
    let result = machine.write("result.md", "1. The wave shows.\n");
    let pass = [
        "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
    ];
    let out = machine.ok("verifier", &pass).await;
    assert!(out.contains("Woke mike@thelio:"), "{out}");
    assert!(
        log(machine.bin.path()).contains("statuses/1a2b3c4d -f state=success"),
        "{}",
        log(machine.bin.path())
    );
    assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 3);

    // The head pins the image of the base: the registry says no, and
    // riff asks nothing.
    let (host, seen) = crate::common::registry(403, 200).await;
    let same = format!("ghcr.io/o/i@sha256:{}", "0".repeat(64));
    let machine = Machine::new(&pin_cases(&same)).await;
    machine.claim("author", "issue-12").await;
    let result = machine.write("result.md", "1. The wave shows.\n");
    let pass = [
        "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
    ];
    machine.ok("verifier", &pass).await;
    assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 0, "{host}");
}

/// The four checks of the forge of a repository whose full check is not
/// named `Gate` (#698).
const FOUR: [&str; 4] = [
    "checks / lint and test",
    "checks / docs",
    "checks / the lowest Rust version",
    "checks / build x86_64-unknown-linux-musl",
];

/// The fake forge of such a repository: the ruleset of `main` requires
/// [`FOUR`] and `riff/verify`, and the head commit has no check run
/// `Gate`. `run` gives the check run of each of the four, as JSON.
fn without_gate(run: impl Fn(&str) -> String) -> String {
    let mut cases: Vec<String> = VERIFY.lines().take(3).map(str::to_owned).collect();
    let contexts: Vec<String> = FOUR
        .iter()
        .chain(&["riff/verify"])
        .map(|c| format!(r#"{{"context":"{c}"}}"#))
        .collect();
    cases
        .push(r#"*'api repos/como-technologies/riff') echo '{"default_branch":"main"}' ;;"#.into());
    cases.push(format!(
        r#"*'/rules/branches/main?per_page=100'*) echo '[{{"type":"required_status_checks","parameters":{{"required_status_checks":[{}]}}}}]' ;;"#,
        contexts.join(",")
    ));
    for check in FOUR {
        cases.push(format!(
            "*'/check-runs?check_name={}'*) echo '{{\"check_runs\":[{}]}}' ;;",
            riff::pr::percent(check),
            run(check)
        ));
    }
    cases.push(
        r#"*'/check-runs?check_name=Gate'*) echo '{"total_count":0,"check_runs":[]}' ;;"#.into(),
    );
    cases.join("\n")
}

/// The check runs of each of the four, by its name.
type Runs = fn(&str) -> String;

/// The check run `name` with the `status` and the `conclusion`.
fn check_run(name: &str, status: &str, conclusion: &str) -> String {
    format!(r#"{{"name":"{name}","status":"{status}","conclusion":{conclusion}}}"#)
}

/// A repository with no check named `Gate` and four required checks that
/// pass (#698, 01M5QYDE41FVFX26EC8K5QGYH4): `riff verify pass` reads the
/// required checks of the ruleset of the default branch, and reports. It
/// never asks for a check named `Gate`.
#[tokio::test]
async fn verify_pass_needs_the_required_checks_of_a_repository_with_no_gate() {
    let fake = without_gate(|name| check_run(name, "completed", r#""success""#));
    let machine = Machine::new(&fake).await;
    machine.claim("author", "issue-12").await;
    let result = machine.write("result.md", "1. The wave shows.\n");
    let pass = [
        "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
    ];
    let out = machine.ok("verifier", &pass).await;
    assert!(out.contains("Woke mike@thelio:"), "{out}");

    let calls = log(machine.bin.path());
    assert!(
        calls.contains("gh api repos/como-technologies/riff/rules/branches/main?per_page=100"),
        "{calls}"
    );
    let run = "gh api repos/como-technologies/riff/commits/1a2b3c4d/check-runs?check_name=";
    for check in [
        "checks%20%2F%20lint%20and%20test",
        "checks%20%2F%20docs",
        "checks%20%2F%20the%20lowest%20Rust%20version",
        "checks%20%2F%20build%20x86_64-unknown-linux-musl",
    ] {
        assert!(calls.contains(&format!("{run}{check}")), "{check}\n{calls}");
    }
    assert!(!calls.contains(&format!("{run}Gate")), "{calls}");
    assert!(
        calls.contains("statuses/1a2b3c4d -f state=success"),
        "{calls}"
    );
}

/// #1393, 01M5R13DE5XRWB13E0BQ17C80F: a required check that ends
/// `skipped` or `neutral` counts as a pass on GitHub, so it counts for
/// `riff verify pass` too.
#[tokio::test]
async fn verify_pass_counts_a_skipped_or_neutral_required_check_as_a_pass() {
    let fake = without_gate(|name| {
        let conclusion = match name {
            "checks / docs" => r#""skipped""#,
            "checks / the lowest Rust version" => r#""neutral""#,
            _ => r#""success""#,
        };
        check_run(name, "completed", conclusion)
    });
    let machine = Machine::new(&fake).await;
    machine.claim("author", "issue-12").await;
    let result = machine.write("result.md", "1. The wave shows.\n");
    let pass = [
        "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
    ];
    let out = machine.ok("verifier", &pass).await;
    assert!(out.contains("Woke mike@thelio:"), "{out}");
    let calls = log(machine.bin.path());
    assert!(
        calls.contains("statuses/1a2b3c4d -f state=success"),
        "{calls}"
    );
}

/// #1417, 01M5RGM8HBVQMY25H5PYESK17J: a commit can have two runs of one
/// check, for example a run that failed before a maintainer set the
/// milestone and a run that passed after it. `riff verify pass` reads the
/// newest run of each check, as `gh pr checks` does, in any order of the
/// list.
#[tokio::test]
async fn verify_pass_reads_the_newest_run_of_each_check() {
    let run = |started: &str, completed: &str, conclusion: &str| {
        format!(
            r#"{{"name":"Gate","status":"completed","conclusion":"{conclusion}","started_at":"{started}","completed_at":"{completed}"}}"#
        )
    };
    let old = |conclusion| run("2026-10-24T17:01:00Z", "2026-10-24T17:04:30Z", conclusion);
    let new = |conclusion| run("2026-10-24T17:22:00Z", "2026-10-24T17:26:50Z", conclusion);
    let gate = |runs: [String; 2]| {
        VERIFY.replace(
            r#"[{"name":"Gate","status":"completed","conclusion":"success"}]"#,
            &format!("[{}]", runs.join(",")),
        )
    };
    let passes = [
        gate([old("failure"), new("success")]),
        gate([new("success"), old("failure")]),
    ];
    for fake in passes {
        let machine = Machine::new(&fake).await;
        machine.claim("author", "issue-12").await;
        let result = machine.write("result.md", "1. The wave shows.\n");
        let pass = [
            "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
        ];
        let out = machine.ok("verifier", &pass).await;
        assert!(out.contains("Woke mike@thelio:"), "{out}");
        let calls = log(machine.bin.path());
        assert!(
            calls.contains("statuses/1a2b3c4d -f state=success"),
            "{calls}"
        );
    }

    let refusals = [
        gate([old("success"), new("failure")]),
        gate([new("failure"), old("success")]),
    ];
    for fake in refusals {
        let machine = Machine::new(&fake).await;
        machine.claim("author", "issue-12").await;
        let result = machine.write("result.md", "1. The wave shows.\n");
        let pass = [
            "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
        ];
        let out = machine.run("verifier", &pass).await;
        assert_eq!(out.status.code(), Some(1));
        let stderr = text(&out.stderr);
        assert!(
            stderr.contains("the full check of commit 1a2b3c4 (Gate) ended with failure."),
            "{stderr}"
        );
        let calls = log(machine.bin.path());
        assert!(!calls.contains("gh pr comment"), "{calls}");
        assert!(!calls.contains("/statuses/"), "{calls}");
    }
}

/// A required check that runs or failed stops the pass, and
/// the line names the four checks. A fail needs none of them.
#[tokio::test]
async fn verify_pass_refuses_while_a_required_check_has_no_success() {
    let names = FOUR.join(", ");
    let cases: [(Runs, &str); 2] = [
        (
            |name| {
                let (status, conclusion) = match name {
                    "checks / docs" => ("in_progress", "null"),
                    _ => ("completed", r#""success""#),
                };
                check_run(name, status, conclusion)
            },
            "runs still",
        ),
        (
            |name| {
                let conclusion = match name {
                    "checks / the lowest Rust version" => r#""failure""#,
                    _ => r#""success""#,
                };
                check_run(name, "completed", conclusion)
            },
            "ended with failure",
        ),
    ];
    for (run, state) in cases {
        let machine = Machine::new(&without_gate(run)).await;
        machine.claim("author", "issue-12").await;
        let result = machine.write("result.md", "1. The wave shows.\n");
        let pass = [
            "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
        ];
        let out = machine.run("verifier", &pass).await;
        assert_eq!(out.status.code(), Some(1), "{state}");
        let stderr = text(&out.stderr);
        assert!(
            stderr.contains(&format!(
                "riff verify pass: the full check of commit 1a2b3c4 ({names}) {state}. A pass \
                 needs a success of each check of it: see gh pr checks 40. Nothing is reported."
            )),
            "{stderr}"
        );
        let calls = log(machine.bin.path());
        assert!(!calls.contains("gh pr comment"), "{calls}");
        assert!(!calls.contains("/statuses/"), "{calls}");

        let fail = [
            "verify", "fail", "40", "--file", &result, "--commit", "1a2b3c4d",
        ];
        machine.ok("verifier", &fail).await;
        assert!(log(machine.bin.path()).contains("-f state=failure"));
    }
}

/// With no --commit, the tested commit is HEAD of the verifier.
#[tokio::test]
async fn verify_takes_head_as_the_tested_commit() {
    let machine = Machine::new(VERIFY).await;
    machine.claim("author", "issue-12").await;
    let tested = commit(&machine);
    fake_gh(machine.bin.path(), &VERIFY.replace("1a2b3c4d", &tested));
    let result = machine.write("result.md", "1. The wave shows.\n");
    machine
        .ok("verifier", &["verify", "pass", "40", "--file", &result])
        .await;
    let log = log(machine.bin.path());
    assert!(
        log.contains(&format!(
            "gh api repos/como-technologies/riff/statuses/{tested} -f state=success"
        )),
        "{log}"
    );
}

const OPEN: &str = r#"*'issue view 12 --json number,state,milestone'*) echo '{"number":12,"state":"OPEN","milestone":{"title":"Wave 3"}}' ;;
*'pr create'*) echo 'https://github.com/como-technologies/riff/pull/40' ;;"#;

/// The title and the body that the fake `gh` got for `pr create`.
fn created(bin: &Path) -> (String, String) {
    let log = log(bin);
    let line = log.lines().find(|l| l.starts_with("gh pr create")).unwrap();
    let title = line
        .split("--title ")
        .nth(1)
        .unwrap()
        .split(" --milestone")
        .next()
        .unwrap();
    (
        title.to_owned(),
        std::fs::read_to_string(bin.join("body")).unwrap(),
    )
}

#[tokio::test]
async fn pr_open_makes_a_body_that_passes_the_hygiene_check_and_turns_on_auto_merge() {
    let machine = Machine::new(OPEN).await;
    machine.claim("author", "issue-12").await;
    let summary = machine.write("summary.md", "Show the wave in riff who.\n");
    let out = machine
        .ok(
            "author",
            &["pr", "open", "--title", "Show the wave", "--file", &summary],
        )
        .await;
    assert_eq!(
        out,
        "Opened pull request #40 with auto-merge on: https://github.com/como-technologies/riff/pull/40\n"
    );

    let (title, body) = created(machine.bin.path());
    assert_eq!(title, "Show the wave");
    assert_eq!(
        body,
        "Closes #12\n\nShow the wave in riff who.\n\nIssue: #12\nMilestone: Wave 3\n"
    );
    // The check of `hygiene pr`, on what gh got.
    let pr = hygiene::PullRequest::new(&title, &body, Some("Wave 3"));
    let issue = hygiene::Issue::new(12, "OPEN", Some("Wave 3"));
    assert_eq!(hygiene::check_pr(&pr, Some(&issue)), []);

    let log = log(machine.bin.path());
    let create = log.find("gh pr create --title Show the wave --milestone Wave 3 --body-file -");
    let merge = log.find("gh pr merge 40 --auto --squash");
    assert!(create.is_some() && create < merge, "{log}");
}

/// The trailer names a wave with a name by its number, so a rename of
/// the wave breaks no pull request (01M518A07QF1YACBYT91M0DD1R).
#[tokio::test]
async fn pr_open_names_a_wave_with_a_name_by_its_number() {
    let named = OPEN.replace(r#""title":"Wave 3""#, r#""title":"Wave 3: Cloud""#);
    let machine = Machine::new(&named).await;
    machine.claim("author", "issue-12").await;
    machine
        .ok("author", &["pr", "open", "--title", "Show the wave"])
        .await;
    let (_, body) = created(machine.bin.path());
    assert_eq!(body, "Closes #12\n\nIssue: #12\nMilestone: Wave 3\n");
    let log = log(machine.bin.path());
    assert!(
        log.contains("gh pr create --title Show the wave --milestone Wave 3: Cloud --body-file -"),
        "{log}"
    );
    for renamed in ["Wave 3: Cloud", "Wave 3: The view"] {
        let pr = hygiene::PullRequest::new("Show the wave", &body, Some(renamed));
        let issue = hygiene::Issue::new(12, "OPEN", Some(renamed));
        assert_eq!(hygiene::check_pr(&pr, Some(&issue)), [], "{renamed}");
    }
}

#[tokio::test]
async fn pr_open_with_refs_links_with_refs() {
    let machine = Machine::new(OPEN).await;
    machine
        .ok(
            "author",
            &[
                "pr", "open", "--title", "Part one", "--refs", "--issue", "12",
            ],
        )
        .await;
    let (_, body) = created(machine.bin.path());
    assert_eq!(body, "Refs #12\n\nIssue: #12\nMilestone: Wave 3\n");
}

/// A body that has the link line and the trailers gets none of them a
/// second time (01M3W2627GYXR8CFW76KB6CB9W).
#[tokio::test]
async fn pr_open_adds_no_line_that_the_body_has() {
    let full = "Closes #12\n\nShow the wave in riff who.\n\nIssue: #12\nMilestone: Wave 3\n";
    for content in [
        full,
        "Show the wave in riff who.\n\nIssue: #12\nMilestone: Wave 3\n",
        "Closes #12\n\nShow the wave in riff who.\n\nIssue: #12\n",
    ] {
        let machine = Machine::new(OPEN).await;
        machine.claim("author", "issue-12").await;
        let summary = machine.write("summary.md", content);
        machine
            .ok(
                "author",
                &["pr", "open", "--title", "Show the wave", "--file", &summary],
            )
            .await;
        let (_, body) = created(machine.bin.path());
        assert_eq!(body, full, "{content}");
        for line in ["Closes #12\n", "Issue: #12\n", "Milestone: Wave 3\n"] {
            assert_eq!(body.matches(line).count(), 1, "{line} in {body}");
        }
    }
}

/// A body with a line for another issue or another milestone is
/// refused: the message names the line, and riff opens nothing
/// (01M3W2627GYXR8CFW76KB6CB9W).
#[tokio::test]
async fn pr_open_refuses_a_body_with_a_line_that_is_not_of_the_claim() {
    for (content, refs, line, need) in [
        ("Text.\n\nIssue: #9\n", false, "`Issue: #9`", "`Issue: #12`"),
        (
            "Text.\n\nIssue: #12\nMilestone: Wave 4\n",
            false,
            "`Milestone: Wave 4`",
            "`Milestone: Wave 3`",
        ),
        ("Closes #9\n\nText.\n", false, "`Closes #9`", "`Closes #12`"),
        ("Closes #12\n\nText.\n", true, "`Closes #12`", "`Refs #12`"),
    ] {
        let machine = Machine::new(OPEN).await;
        machine.claim("author", "issue-12").await;
        let summary = machine.write("summary.md", content);
        let mut args = vec!["pr", "open", "--title", "Show the wave", "--file", &summary];
        if refs {
            args.push("--refs");
        }
        let out = machine.run("author", &args).await;
        assert_eq!(out.status.code(), Some(1), "{content}");
        let err = text(&out.stderr);
        assert!(
            err.contains(&format!(
                "the body has the line {line}, but this pull request needs {need}."
            )),
            "{err}"
        );
        assert!(!log(machine.bin.path()).contains("gh pr create"));
    }
}

#[tokio::test]
async fn pr_open_refuses_a_title_that_breaks_the_hygiene_check() {
    let machine = Machine::new(OPEN).await;
    machine.claim("author", "issue-12").await;
    let out = machine
        .run("author", &["pr", "open", "--title", "Show the wave (#12)"])
        .await;
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("title: "),
        "{}",
        text(&out.stderr)
    );
    assert!(!log(machine.bin.path()).contains("gh pr create"));
}

/// The issue 12 of the fake `gh` has no milestone.
const OPEN_NO_WAVE: &str = r#"*'issue view 12 --json number,state,milestone'*) echo '{"number":12,"state":"OPEN","milestone":null}' ;;
*'pr create'*) echo 'https://github.com/como-technologies/riff/pull/40' ;;"#;

/// The riff config of a repository that is not riff: it has no table
/// `[hygiene]`.
const OTHER_TOML: &str = "[session]\nimage = \"ghcr.io/acme/app-session@sha256:0000000000000000000000000000000000000000000000000000000000000000\"\n";

/// A repository with no table `[hygiene]` gets the rules that riff
/// reads and no others: a title with `(#12)` and an issue with no
/// milestone pass. The body has `Closes #12` and `Issue: #12`
/// (01M5SMSHNQ9E4P9W6RCB8E1WTN).
#[tokio::test]
async fn pr_open_keeps_only_what_riff_reads_in_a_repository_with_no_hygiene_table() {
    for config in [None, Some(OTHER_TOML), Some("[hygiene]\n")] {
        let machine = Machine::new(OPEN_NO_WAVE).await;
        machine.riff_toml(config);
        machine.claim("author", "issue-12").await;
        let out = machine
            .ok("author", &["pr", "open", "--title", "Show the wave (#12)"])
            .await;
        assert!(out.starts_with("Opened pull request #40"), "{out}");

        let log = log(machine.bin.path());
        assert!(
            log.contains("gh pr create --title Show the wave (#12) --body-file -\n"),
            "{log}"
        );
        assert!(!log.contains("--milestone"), "{log}");
        let body = std::fs::read_to_string(machine.bin.path().join("body")).unwrap();
        assert_eq!(body, "Closes #12\n\nIssue: #12\n", "{config:?}");
        let create = log.find("gh pr create");
        let merge = log.find("gh pr merge 40 --auto --squash");
        assert!(create.is_some() && create < merge, "{log}");
    }
}

/// An issue with a milestone gives it to the pull request, because riff
/// reads the milestone for the lists of a wave. The body has no
/// trailer `Milestone:`, and a closing keyword in the text passes.
#[tokio::test]
async fn pr_open_gives_the_milestone_but_no_trailer_in_a_repository_with_no_hygiene_table() {
    let machine = Machine::new(OPEN).await;
    machine.riff_toml(None);
    machine.claim("author", "issue-12").await;
    let summary = machine.write("summary.md", "Show the wave. It fixes #13.\n");
    machine
        .ok(
            "author",
            &["pr", "open", "--title", "Show the wave", "--file", &summary],
        )
        .await;
    let (_, body) = created(machine.bin.path());
    assert_eq!(
        body,
        "Closes #12\n\nShow the wave. It fixes #13.\n\nIssue: #12\n"
    );
    let log = log(machine.bin.path());
    assert!(
        log.contains("gh pr create --title Show the wave --milestone Wave 3 --body-file -"),
        "{log}"
    );
}

/// What riff reads still holds in a repository with no table: a line for
/// another issue is refused, and nothing opens.
#[tokio::test]
async fn pr_open_still_refuses_a_line_for_another_issue_in_a_repository_with_no_hygiene_table() {
    let machine = Machine::new(OPEN_NO_WAVE).await;
    machine.riff_toml(None);
    machine.claim("author", "issue-12").await;
    let summary = machine.write("summary.md", "Text.\n\nIssue: #9\n");
    let out = machine
        .run(
            "author",
            &["pr", "open", "--title", "Show the wave", "--file", &summary],
        )
        .await;
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains(
            "the body has the line `Issue: #9`, but this pull request needs `Issue: #12`."
        ),
        "{}",
        text(&out.stderr)
    );
    assert!(!log(machine.bin.path()).contains("gh pr create"));
}

/// In riff, with `rules = "riff"`, each rule holds: an issue with no
/// milestone, a title with `(#12)` and a closing keyword each stop the
/// pull request (01M5SMSMWE89SC3Q7XFBQCFY32).
#[tokio::test]
async fn pr_open_keeps_each_rule_in_a_repository_with_the_rules_of_riff() {
    let machine = Machine::new(OPEN_NO_WAVE).await;
    machine.claim("author", "issue-12").await;
    let out = machine
        .run("author", &["pr", "open", "--title", "Show the wave"])
        .await;
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("issue #12 has no milestone"),
        "{}",
        text(&out.stderr)
    );

    let machine = Machine::new(OPEN).await;
    machine.claim("author", "issue-12").await;
    let summary = machine.write("summary.md", "Show the wave. It fixes #13.\n");
    let out = machine
        .run(
            "author",
            &[
                "pr",
                "open",
                "--title",
                "Show the wave (#12)",
                "--file",
                &summary,
            ],
        )
        .await;
    assert_eq!(out.status.code(), Some(1));
    let err = text(&out.stderr);
    assert!(err.contains("keyword: "), "{err}");
    assert!(err.contains("title: "), "{err}");
    assert!(!log(machine.bin.path()).contains("gh pr create"));
}

/// A value of `rules` that riff does not know, and a riff config that is
/// not TOML, stop the command before any call of `gh`.
#[tokio::test]
async fn pr_open_refuses_a_riff_config_it_cannot_read() {
    for (config, said) in [
        (
            "[hygiene]\nrules = \"strict\"\n",
            "[hygiene] rules is \"strict\". The one value is \"riff\".",
        ),
        ("[hygiene]\nrules = true\n", "[hygiene] rules is true."),
        ("[hygiene\n", "is not valid TOML"),
    ] {
        let machine = Machine::new(OPEN).await;
        machine.riff_toml(Some(config));
        machine.claim("author", "issue-12").await;
        let out = machine
            .run("author", &["pr", "open", "--title", "Show the wave"])
            .await;
        assert_eq!(out.status.code(), Some(1), "{config}");
        let err = text(&out.stderr);
        assert!(err.contains(said), "{err}");
        assert!(err.contains("riff.toml"), "{err}");
        assert!(log(machine.bin.path()).is_empty(), "{config}");
    }
}

#[tokio::test]
async fn pr_open_needs_a_claimed_issue() {
    let machine = Machine::new(OPEN).await;
    let out = machine
        .run("author", &["pr", "open", "--title", "Show the wave"])
        .await;
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("holds no claim issue-N"),
        "{}",
        text(&out.stderr)
    );
    assert!(log(machine.bin.path()).is_empty());
}

/// The `###` how-to `heading` of how-it-works.md, to the next heading.
fn how_to(heading: &str) -> String {
    let page = book::page("how-it-works.md");
    let start = page
        .find(&format!("\n### {heading}\n"))
        .unwrap_or_else(|| panic!("no how-to {heading:?}"));
    let rest = &page[start + 1..];
    rest[..rest[4..].find("\n#").map_or(rest.len(), |i| i + 4)].to_owned()
}

/// The help of `riff pr open` and of `--title` says what holds in each
/// repository: the trailer `Milestone:` and the rule for a title that
/// ends with `(#N)` hold only with `rules = "riff"`
/// (01M5SQ45MQVHFX8ZSFSF02SFKH). "Choose the rules of a repository"
/// says the same.
#[test]
fn the_help_of_pr_open_says_what_holds_in_each_repository() {
    let flat = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let out = Isolated::new()
        .riff()
        .args(["pr", "open", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let help = flat(&text(&out.stdout));
    for said in [
        "The body has the link line and the trailer Issue: of the issue. \
         With `rules = \"riff\"` in the table `[hygiene]` of riff.toml, \
         the body also has the trailer Milestone:, which names a wave by \
         its number, for example Wave 3. \
         Without it, riff writes no trailer Milestone:.",
        "--title <TITLE> The title. \
         With `rules = \"riff\"` in the table `[hygiene]` of riff.toml, \
         do not end it with (#N). Without it, that is allowed",
    ] {
        assert!(help.contains(said), "no {said:?} in {help}");
    }
    for old in ["trailers Issue: and Milestone:", "The title. Do not end"] {
        assert!(!help.contains(old), "{old:?} is in {help}");
    }

    let page = book::page("development.md");
    let start = page
        .find("\n### Choose the rules of a repository\n")
        .unwrap();
    let rest = &page[start + 1..];
    let part = flat(&rest[..rest[4..].find("\n#").map_or(rest.len(), |i| i + 4)]);
    for said in [
        "With `rules = \"riff\"`, the body that `riff pr open` writes has the \
         trailer `Milestone:`, and a title that ends with `(#12)` breaks a rule. \
         In a repository with no table, `riff pr open` writes no trailer \
         `Milestone:`, and it accepts that title.",
        "`riff pr open --help` says the same.",
    ] {
        assert!(part.contains(said), "no {said:?} in {part}");
    }
}

/// The book shows the flow of an item with a mermaid diagram: the work
/// of a worker ends at the verify request, and the verifier does the
/// steps after the merge (01M3Z9N6AK6W9KCA1MN72X78B6,
/// 01M3Z9N6GQK0NYCMGQ66FW406V). Its how-to shows the lines of a claim
/// as the code gives them (01M3Z9N6SPWPSSCBEVDCKBESSV).
#[test]
fn the_book_shows_how_a_session_works_on_an_item() {
    let part = book::part("how-it-works.md", "How a session works on an item");
    let flat = part.split_whitespace().collect::<Vec<_>>().join(" ");
    for text in [
        "```mermaid sequenceDiagram",
        "A->>E: post verify request A->>G: comment on issue 6: the state A->>E: release issue-6",
        "Note over A: the turn ends, riff clears the context",
        "V->>E: note \"done issue-6\" V->>V: remove the worktree and the branch of issue-6",
        "Note over E: issue-6 is free, with its branch",
        "E-->>N: granted, and the failed verify of PR #40",
        "One context holds one item.",
        "| `failure` | is free, with its earlier work | a build |",
        "A session that is not a worker keeps the rule of before",
    ] {
        assert!(flat.contains(text), "no {text:?} in {part}");
    }

    let how_to = how_to("See the pull request of an item at a claim");
    assert_eq!(book::commands_in(&how_to), ["riff claim issue-12"]);
    let url = "https://github.com/como-technologies/riff/pull/40#issuecomment-7";
    let pull = |checks| riff::rollout::Pull {
        number: 40,
        branch: "worktree-issue-12".into(),
        head: "1a2b3c4d5e6f".into(),
        checks,
        ..riff::rollout::Pull::default()
    };
    let verify = |state: &str| riff::rollout::Check {
        url: Some(url.into()),
        ..riff::rollout::Check::verify(state)
    };
    for checks in [vec![], vec![verify("SUCCESS")], vec![verify("FAILURE")]] {
        let line = riff::rollout::claim_line("issue-12", &[pull(checks)]).unwrap();
        assert!(
            how_to.contains(&format!("\n{line}\n```")),
            "no {line:?} in {how_to}"
        );
    }
    assert!(how_to.contains(&format!(
        "waits at most {} seconds",
        riff::rollout::PULL_WAIT.as_secs()
    )));
}

/// The refusal in "Report a verify" is the line that `riff verify pass`
/// prints (01M5QYDE41FVFX26EC8K5QGYH4), and the how-to says what riff waits
/// for.
#[test]
fn the_book_shows_the_line_of_a_refused_pass_and_what_riff_waits_for() {
    let part = how_to("Report a verify");
    let checks = ["Gate".to_owned(), "Hygiene".to_owned()];
    let line = riff::text::gate_not_passed(40, "1a2b3c4d", &checks, &riff::pr::Gate::Running);
    assert!(part.contains(&format!("{line}\n")), "{line}\n{part}");
    assert!(part.contains(
        "each check that the ruleset of the default branch\nrequires, but `riff/verify`"
    ));
}

/// 01M5SMRRAG1WPBJMQKKTPSRHRP: the refusal for a session image that a
/// host cannot pull is in "Report a verify", as `riff verify pass` prints
/// it.
#[test]
fn the_book_shows_the_line_of_a_pass_refused_for_an_image_that_a_host_cannot_pull() {
    let part = how_to("Report a verify");
    let image: riff::session_image::Image =
        format!("ghcr.io/acme/session@sha256:{}", "ab".repeat(32))
            .parse()
            .unwrap();
    let line = riff::text::verify_image_closed(40, &image, "the registry answered 403 Forbidden")
        .replace(image.digest(), "sha256:…");
    assert!(
        part.contains(&format!("```text\n{line}\n```")),
        "{line}\n{part}"
    );
    assert!(part.contains("(start-a-riff.md#check-that-each-host-can-pull-the-image)"));
}

/// Each command has a how-to with a copyable `sh` block, and each
/// command and flag of the how-to is in `--help`.
#[test]
fn the_book_has_a_how_to_for_each_command() {
    for (heading, commands) in [
        (
            "Open a pull request",
            &[
                "riff pr open --title \"Show the wave\" --file summary.md",
                "riff pr open --title \"Show the wave\" --file summary.md --refs",
            ][..],
        ),
        ("Wait for the merge", &["riff pr wait 40"]),
        (
            "Report a verify",
            &[
                "gh pr checks 40",
                "riff verify pass 40 --file result.md",
                "riff verify fail 40 --file result.md",
            ],
        ),
    ] {
        let part = how_to(heading);
        assert_eq!(book::commands_in(&part), commands, "{part}");
    }
    let help = |args: &[&str]| {
        let out = Isolated::new()
            .riff()
            .args(args)
            .arg("--help")
            .output()
            .unwrap();
        assert!(out.status.success());
        text(&out.stdout)
    };
    let open = help(&["pr", "open"]);
    for flag in [
        "--title <TITLE>",
        "--file <FILE>",
        "--refs",
        "--issue <ISSUE>",
    ] {
        assert!(open.contains(flag), "{flag} is not in riff pr open --help");
    }
    assert!(help(&["pr", "wait"]).contains("--every <EVERY>"));
    let verify = help(&["verify"]);
    assert!(verify.contains("[possible values: pass, fail]"), "{verify}");
    assert!(verify.contains("--file <FILE>"), "{verify}");
    assert!(verify.contains("--commit <SHA>"), "{verify}");
    assert!(how_to("Report a verify").contains("`--commit 1a2b3c4`"));
    assert!(how_to("Open a pull request").contains("`--issue 12`"));
    assert!(how_to("Wait for the merge").contains("`--every SECONDS`"));
}
