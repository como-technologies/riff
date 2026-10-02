//! The steps of a pull request on GitHub (#201): `riff pr open`
//! (01M3NB6FTGPD0S5JTXXXNGNNDT), `riff pr wait`
//! (01M3NB6FWMGBQ9VTY6RCBPKBHK) and `riff verify`
//! (01M3NB6FYXXKX80VHEVA5CV6RY).
//!
//! A fake `gh` on `PATH` logs each call, and the body that it gets on
//! stdin, and answers from a `case` of the test. A real `riff-server`
//! with no sign-in holds the claims and the posts.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

use isolated::Isolated;

mod book;

/// A fake `gh` in `bin` that logs `gh ARGS` to `bin/gh.log`, copies its
/// stdin to the log for `--body-file -`, and runs `cases`: the arms of a
/// `case "$*" in`.
fn fake_gh(bin: &Path, cases: &str) {
    let path = bin.join("gh");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\ndir=$(dirname \"$0\")\necho \"gh $*\" >> \"$dir/gh.log\"\n\
             case \"$*\" in *'--body-file -'*) cat > \"$dir/body\"; cat \"$dir/body\" >> \"$dir/gh.log\" ;; esac\n\
             case \"$*\" in\n{cases}\nesac\nexit 0\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn log(bin: &Path) -> String {
    std::fs::read_to_string(bin.join("gh.log")).unwrap_or_default()
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// A real riff-server of this build, with no sign-in.
async fn server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, riff_server::router()).await.unwrap() });
    format!("http://{addr}")
}

/// A git repository with a GitHub origin, so the place of each session
/// is `como-technologies/riff`.
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
    dir
}

/// One machine: its environment, the fake `gh` and a clone.
struct Machine {
    env: Isolated,
    bin: tempfile::TempDir,
    repo: tempfile::TempDir,
    server: String,
}

impl Machine {
    async fn new(cases: &str) -> Machine {
        let bin = tempfile::tempdir().unwrap();
        fake_gh(bin.path(), cases);
        Machine {
            env: Isolated::new(),
            bin,
            repo: repo(),
            server: server().await,
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
        let mut cmd = self.env.riff();
        cmd.args(args)
            .current_dir(self.repo.path())
            .env("RIFF_SERVER", &self.server)
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
        self.ok(session, &["lead"]).await;
        self.ok(session, &["resume", "--riff"]).await;
        self.ok(session, &["claim", item]).await;
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
*'/statuses/'*) echo '{}' ;;"#;

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
    machine.ok("lead", &["lead"]).await;
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
        machine.ok("lead", &["lead"]).await;
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
