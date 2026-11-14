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
use std::sync::Arc;
use std::time::Duration;

use isolated::Isolated;

use riff::ghcalls::Calls;
use riff::pr::{self, Gh};

use crate::book;

/// A fake `gh` in `bin` that logs `gh ARGS` to `bin/gh.log`, copies its
/// stdin to the log for `--body-file -`, and runs `cases`: the arms of a
/// `case "$*" in`. After them come the reads of the council of pull
/// request 40 ([`COUNCIL_READS`]).
fn fake_gh(bin: &Path, cases: &str) {
    let path = bin.join("gh");
    isolated::write_program(
        &path,
        format!(
            "#!/bin/sh\ndir=$(dirname \"$0\")\necho \"gh $*\" >> \"$dir/gh.log\"\n\
             case \"$*\" in *'--body-file -'*) cat > \"$dir/body\"; cat \"$dir/body\" >> \"$dir/gh.log\" ;; esac\n\
             case \"$*\" in\n{cases}\n{COUNCIL_READS}\nesac\nexit 0\n"
        ),
    );
}

/// The reads of `riff council show 40` (01M7D11KGEJ8ACFJK6Y10WENJZ): the
/// base branch `main`, the path `src/lib.rs` and no label. With the base
/// branch of [`repo`], the council is design, testing, docs and rust.
const COUNCIL_READS: &str = r#"*'pr view 40 --json baseRefName,body,labels'*) printf '%s' '{"baseRefName":"main","body":"Closes #12\n\nIssue: #12\n","labels":[]}' ;;
*'pulls/40/files?per_page=100 --paginate'*) echo src/lib.rs ;;
*'issue view 12 --json labels'*) echo '{"labels":[]}' ;;"#;

/// A result with a line for each member of the council of
/// [`COUNCIL_READS`].
const RESULT: &str = "1. The wave shows.\n\n## The council\n\n- design: no finding.\n\
                      - testing: no finding.\n- docs: no finding.\n- rust: no finding.\n";

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
/// is `como-technologies/riff`, with the riff config of riff, and the
/// base branch `origin/main` with no member file of the council
/// ([`crate::common::base_branch`]).
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
    crate::common::base_branch(dir.path());
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
        self.run_in(self.repo.path(), session, args).await
    }

    /// Runs `riff ARGS` as `session` in the folder `dir`. An empty
    /// `session` is a person in a terminal.
    async fn run_in(&self, dir: &Path, session: &str, args: &[&str]) -> Output {
        let path = format!(
            "{}:{}",
            self.bin.path().display(),
            std::env::var("PATH").unwrap()
        );
        crate::common::sign_in(&self.server, &self.env.riff_home(), "mike");
        let mut cmd = self.env.riff();
        cmd.args(args)
            .current_dir(dir)
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
        self.name_lead(session).await;
        self.ok(session, &["resume", "--riff"]).await;
        self.ok(session, &["claim", item]).await;
    }

    /// The person mike names the session `session` the lead, as the riff
    /// daemon does for a lead that it starts (01M77Y6ZMFZJGK2DM6RAM94C7Q).
    async fn name_lead(&self, session: &str) {
        let dir = self.repo.path();
        crate::common::lead_by_person(&self.server, dir, "thelio", "mike", session).await;
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

/// A wait of one hour for a pull request that stays open and pending,
/// at `--every` of `every` seconds (01M6JE9ME6YSHV538GNPH7ZRGQ). The fake
/// `gh` stays open for a clock of one hour that moves with each wait
/// and never sleeps, and `Calls` counts the points of each call. The one
/// look that sees the merge is in the count. Returns the seconds of each
/// wait and the points.
fn wait_an_hour(every: u64) -> (Vec<u64>, u32) {
    let bin = tempfile::tempdir().unwrap();
    fake_gh(
        bin.path(),
        r#"*'pr view 40 --json state,mergeCommit'*)
    if [ -e "$dir/merged" ]; then echo '{"state":"MERGED","mergeCommit":{"oid":"9f8e7d6c"}}'
    else echo '{"state":"OPEN","mergeCommit":null}'; fi ;;
*'pr checks 40 --required --json name,bucket'*) echo '[{"name":"Gate","bucket":"pass"},{"name":"riff/verify","bucket":"pending"}]' ;;"#,
    );
    let calls = Arc::new(Calls::default());
    let gh = Gh::at(bin.path().join("gh")).with_calls(calls.clone());
    let mut clock = Duration::ZERO;
    let mut sleeps = Vec::new();
    let commit = pr::wait_with(&gh, 40, Duration::from_secs(every), |wait| {
        sleeps.push(wait.as_secs());
        clock += wait;
        if clock >= Duration::from_secs(3600) {
            std::fs::write(bin.path().join("merged"), "").unwrap();
        }
    })
    .unwrap();
    assert_eq!(commit, "9f8e7d6c");
    let (count, points) = calls.shown(riff::daemon::now_ms()).total();
    assert_eq!(count, 2 * (sleeps.len() as u32 + 1), "{sleeps:?}");
    assert_eq!(points, pr::LOOK_POINTS * (sleeps.len() as u32 + 1));
    (sleeps, points)
}

/// At the default pace, one pull request that waits an hour uses at most
/// 60 GraphQL points (01M6JE9ME6YSHV538GNPH7ZRGQ).
#[test]
fn pr_wait_uses_at_most_60_points_in_an_hour_at_the_default_pace() {
    let (sleeps, points) = wait_an_hour(30);
    // 4 waits of 30 s, then waits of 5 minutes.
    assert_eq!(&sleeps[..6], [30, 30, 30, 30, 300, 300]);
    assert!(sleeps[4..].iter().all(|s| *s == 300), "{sleeps:?}");
    assert!(points <= pr::WAIT_POINTS_AN_HOUR, "{points} points");
}

/// A shorter `--every` costs more: the example of the book, `--every
/// 10`, uses 72 points in the first hour, over the bound of the default
/// pace (01M6JE9ME6YSHV538GNPH7ZRGQ).
#[test]
fn pr_wait_with_every_10_uses_72_points_in_the_first_hour() {
    let (sleeps, points) = wait_an_hour(10);
    // 12 waits of 10 s in the first 2 minutes, then waits of 5 minutes.
    assert!(sleeps[..12].iter().all(|s| *s == 10), "{sleeps:?}");
    assert!(sleeps[12..].iter().all(|s| *s == 300), "{sleeps:?}");
    // 24 looks in the first hour, and the look after it that sees the
    // merge.
    assert_eq!(sleeps.len(), 24, "{sleeps:?}");
    assert_eq!(pr::LOOK_POINTS * sleeps.len() as u32, 72);
    assert_eq!(points, 72 + pr::LOOK_POINTS);
    assert!(points - pr::LOOK_POINTS > pr::WAIT_POINTS_AN_HOUR);
}

/// A wait with a long `--every` is never faster than `--every`.
#[test]
fn pr_wait_with_a_long_every_waits_that_long() {
    let bin = tempfile::tempdir().unwrap();
    fake_gh(
        bin.path(),
        r#"*'pr view 40 --json state,mergeCommit'*)
    n=$(cat "$dir/views" 2>/dev/null || echo 0); n=$((n + 1)); echo $n > "$dir/views"
    if [ $n -lt 4 ]; then echo '{"state":"OPEN","mergeCommit":null}'
    else echo '{"state":"MERGED","mergeCommit":{"oid":"9f8e7d6c"}}'; fi ;;"#,
    );
    let gh = Gh::at(bin.path().join("gh"));
    let mut sleeps = Vec::new();
    let every = Duration::from_secs(600);
    pr::wait_with(&gh, 40, every, |wait| sleeps.push(wait.as_secs())).unwrap();
    assert_eq!(sleeps, [600, 600, 600]);
}

/// The fake forge of pull request 40 "Show the wave" with `Closes #12`
/// and auto-merge off, whose Gate passed on its head 1a2b3c4d. Issue 12
/// has no line `Live:` ([`PLAIN_ISSUE`]). Each other call, for example
/// `pr merge`, works and prints nothing.
const VERIFY: &str = r#"*'pr view 40 --json headRefOid,body'*) printf '%s' '{"headRefOid":"1a2b3c4d","title":"Show the wave","autoMergeRequest":null,"body":"Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n"}' ;;
*'pr comment 40 --body-file -'*) echo 'https://github.com/como-technologies/riff/pull/40#issuecomment-7' ;;
*'/statuses/'*) echo '{}' ;;
*'api repos/como-technologies/riff') echo '{"default_branch":"main"}' ;;
*'/rules/branches/main?per_page=100'*) echo '[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"Gate"},{"context":"riff/verify"}]}}]' ;;
*'/check-runs?check_name=Gate'*) echo '{"total_count":1,"check_runs":[{"name":"Gate","status":"completed","conclusion":"success"}]}' ;;
*'issue view 12 --json body'*) printf '%s' '{"body":"Make it.\n\n## Done when:\n\n- A test.\n"}' ;;"#;

/// `riff verify VERDICT` makes one comment, one status on the head
/// commit and one post to the holder of the issue.
async fn verify(verdict: &str, state: &str) {
    let machine = Machine::new(VERIFY).await;
    machine.claim("author", "issue-12").await;
    let result = machine.write("result.md", RESULT);
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
    // A pass gives the squash message first (01M748RKC7BYP8FC6W2E20SQA7),
    // a fail gives none.
    let merges = calls(&log)
        .into_iter()
        .filter(|call| call.starts_with("gh pr merge"))
        .count();
    assert_eq!(merges, usize::from(verdict == "pass"), "{log}");

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

/// Each call of the fake `gh` in `log`, in order, with no text of its
/// stdin.
fn calls(log: &str) -> Vec<&str> {
    log.lines().filter(|line| line.starts_with("gh ")).collect()
}

/// The stdin of the first call of `log` that starts with `call`.
fn stdin_of(log: &str, call: &str) -> String {
    let mut lines = log.lines().skip_while(|line| !line.starts_with(call));
    assert!(lines.next().is_some(), "no call {call}: {log}");
    lines
        .take_while(|line| !line.starts_with("gh "))
        .map(|line| format!("{line}\n"))
        .collect()
}

/// The call of `riff verify pass 40` that gives the squash message of
/// [`VERIFY`] to the auto-merge (01M748RKC7BYP8FC6W2E20SQA7).
const SQUASH: &str = "gh pr merge 40 --auto --squash --match-head-commit 1a2b3c4d \
                      --subject Show the wave (#40) --body-file -";

/// The body of [`VERIFY`] as JSON text.
const VERIFY_BODY: &str = r#""Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n""#;

/// The pass of pull request 40 on `fake`, by the verifier of a machine
/// where the author holds issue 12. It gives the log of `gh` and the
/// output.
async fn pass(fake: &str) -> (String, std::process::Output) {
    let machine = Machine::new(fake).await;
    machine.claim("author", "issue-12").await;
    let result = machine.write("result.md", RESULT);
    let args = [
        "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
    ];
    let out = machine.run("verifier", &args).await;
    (log(machine.bin.path()), out)
}

/// #1807, #1816, 01M748RKC7BYP8FC6W2E20SQA7: `riff verify pass` gives
/// the subject `TITLE (#N)` and the body of the pull request to `gh pr
/// merge N --auto --squash` before its comment and its status. The
/// trailers end the body as one last paragraph, so GitHub adds no
/// paragraph of its own and the `Co-authored-by:` of the person counts.
/// The pull request of [`VERIFY`] has auto-merge off, and the pass turns
/// it on with that one call.
#[tokio::test]
async fn verify_pass_gives_the_squash_message_before_the_status() {
    let body = r#""Refs #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n\nRiff-Session: author\nCo-authored-by: Mike Sandman <7+mike@users.noreply.github.com>\n""#;
    let (log, out) = pass(&VERIFY.replace(VERIFY_BODY, body)).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let calls = calls(&log);
    let at = |start: &str| {
        calls
            .iter()
            .position(|call| call.starts_with(start))
            .unwrap_or_else(|| panic!("no call {start}: {log}"))
    };
    assert_eq!(calls[at("gh pr merge")], SQUASH);
    assert!(at("gh pr merge") < at("gh pr comment"), "{log}");
    assert!(at("gh pr comment") < at("gh api repos/como-technologies/riff/statuses/"));
    let given = stdin_of(&log, SQUASH);
    assert_eq!(
        given,
        "Refs #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\nRiff-Session: author\n\
         Co-authored-by: Mike Sandman <7+mike@users.noreply.github.com>\n"
    );
    let trailers = given.trim_end().rsplit("\n\n").next().unwrap();
    assert!(
        trailers.ends_with("Co-authored-by: Mike Sandman <7+mike@users.noreply.github.com>"),
        "{given}"
    );
    assert!(trailers.starts_with("Issue: #12\n"), "{given}");
    // An auto-merge that was off gets no read after the call.
    assert!(!log.contains("--json autoMergeRequest"), "{log}");
}

/// 01M6Y95BMYDXFDYG3DKE2KG2PP: an auto-merge that has the message
/// already gets no call. One that was on with another message gets the
/// call. When it keeps the other message, riff turns auto-merge off and
/// on again with the message, before the comment and the status.
#[tokio::test]
async fn verify_pass_turns_an_auto_merge_with_another_message_off_and_on() {
    let message = r#"{"mergeMethod":"SQUASH","commitHeadline":"Show the wave (#40)","commitBody":"Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n"}"#;
    let other = r#"{"mergeMethod":"SQUASH","commitHeadline":null,"commitBody":null}"#;
    let auto = |before: &str, after: &str| {
        format!(
            "{}\n*'pr view 40 --json autoMergeRequest'*) printf '%s' '{{\"autoMergeRequest\":{after}}}' ;;",
            VERIFY.replace(
                r#""autoMergeRequest":null"#,
                &format!(r#""autoMergeRequest":{before}"#)
            )
        )
    };
    let merges = |log: &str| -> Vec<String> {
        calls(log)
            .into_iter()
            .filter(|call| call.starts_with("gh pr merge") || call.contains("autoMergeRequest"))
            .map(str::to_owned)
            .collect()
    };
    let view = "gh pr view 40 --json autoMergeRequest";

    // The message is there: no call.
    let (log, out) = pass(&auto(message, message)).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        merges(&log),
        ["gh pr view 40 --json headRefOid,body,title,autoMergeRequest"]
    );

    // The call changes the message.
    let (log, out) = pass(&auto(other, message)).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(merges(&log)[1..], [SQUASH, view], "{log}");

    // The call keeps the other message: off, and on again.
    let (log, out) = pass(&auto(other, other)).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        merges(&log)[1..],
        [SQUASH, view, "gh pr merge 40 --disable-auto", SQUASH],
        "{log}"
    );
    assert_eq!(stdin_of(&log, "gh pr merge 40 --disable-auto"), "");
    let calls = calls(&log);
    let last = calls.iter().rposition(|call| *call == SQUASH).unwrap();
    let comment = calls
        .iter()
        .position(|call| call.starts_with("gh pr comment"));
    assert!(comment.is_some_and(|comment| last < comment), "{log}");
}

/// 01M6Y95BMYDXFDYG3DKE2KG2PP: when the squash message does not go to
/// the auto-merge, `riff verify pass` makes no comment, no status and no
/// post, and prints the error of `gh`.
#[tokio::test]
async fn verify_pass_reports_nothing_when_the_squash_message_does_not_go() {
    let fake = format!(
        "*'pr merge 40'*) echo 'GraphQL: Pull request Auto merge is not allowed for this repository' >&2; exit 1 ;;\n{VERIFY}"
    );
    let machine = Machine::new(&fake).await;
    machine.claim("author", "issue-12").await;
    let result = machine.write("result.md", RESULT);
    let args = [
        "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
    ];
    let out = machine.run("verifier", &args).await;
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains(&riff::text::verify_no_squash(40)),
        "{stderr}"
    );
    assert!(
        stderr.contains("Auto merge is not allowed for this repository"),
        "{stderr}"
    );
    let log = log(machine.bin.path());
    assert!(log.contains(SQUASH), "{log}");
    assert!(!log.contains("gh pr comment"), "{log}");
    assert!(!log.contains("/statuses/"), "{log}");
    let inbox = machine.ok("author", &["read"]).await;
    assert!(!inbox.contains("verify result:"), "{inbox}");
}

/// The author released the item at its verify request. So no session
/// holds the issue, and the result wakes the lead of the verifier: a
/// result is never silent (01M3Z9N70J4H79VJN4ZKKH3G6S). With a holder,
/// the lead does not wake.
#[tokio::test]
async fn a_verify_result_wakes_the_lead_when_no_session_holds_the_item() {
    let machine = Machine::new(VERIFY).await;
    machine.ok("lead", &["whoami"]).await;
    machine.name_lead("lead").await;
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

/// A person in a terminal has no repository in the URI, in any folder
/// (01M6FYW8NG0E9HYPWTG2RV5EAM). The thread of the pull request gives
/// the repository: with no holder, the lead of the project wakes in it.
/// With a holder in the repository, only the holder wakes. The person
/// runs in a folder of its own, not in the clone of a session.
#[tokio::test]
async fn a_verify_from_a_terminal_in_another_folder_wakes_the_lead_of_the_repository() {
    let machine = Machine::new(VERIFY).await;
    machine.ok("lead", &["whoami"]).await;
    machine.name_lead("lead").await;
    machine.ok("lead", &["resume", "--riff"]).await;
    let terminal = repo();
    let result = machine.write("result.md", RESULT);
    let verify = [
        "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
    ];
    let run = || machine.run_in(terminal.path(), "", &verify);

    let out = run().await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let out = text(&out.stdout);
    assert!(out.contains("Woke mike@thelio:riff (lead)"), "{out}");
    assert!(!out.contains("repo=-"), "{out}");
    let inbox = machine.ok("lead", &["read"]).await;
    assert!(
        inbox.contains("verify result: PASS for issue-12, PR #40, commit 1a2b3c4d."),
        "{inbox}"
    );

    // A session holds the item in the repository: only it wakes.
    machine.ok("author", &["claim", "issue-12"]).await;
    let out = run().await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let out = text(&out.stdout);
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
             in the instructions of the riff tools.",
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
        machine.name_lead("lead").await;
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
    let result = machine.write("result.md", RESULT);
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
    assert!(!log.contains("gh pr merge"), "{log}");
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
        let result = machine.write("result.md", RESULT);
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
        assert!(!calls.contains("gh pr merge"), "{calls}");
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

/// The body of issue 12 in [`VERIFY`]: its "Done when" has no line
/// `Live:`.
const PLAIN_ISSUE: &str = r#"Make it.\n\n## Done when:\n\n- A test.\n"#;

/// The body of an issue 12 with a line `Live:` in "Done when": a check
/// after the release.
const LIVE_ISSUE: &str = r#"Make the Apps.\n\n## Done when:\n\n- A test with a fake GitHub.\n- Live: the owner runs riff github check, and it passes.\n"#;

/// #1747, 01M6WW5WSSRMTBP882FQP3TS2Q: the body of pull request 40 has
/// `Closes #12`, and issue 12 has a line `Live:` in "Done when". `riff
/// verify pass` refuses with the fix, and makes no comment, no status
/// and no post. A fail needs no such check. With `Refs #12` the pass is
/// reported, and an issue with no `Live:` line takes `Closes #12`.
#[tokio::test]
async fn verify_pass_refuses_a_body_that_closes_an_issue_with_a_live_line() {
    let issue = |body: &str| VERIFY.replace(PLAIN_ISSUE, body);
    let machine = Machine::new(&issue(LIVE_ISSUE)).await;
    machine.claim("author", "issue-12").await;
    let result = machine.write("result.md", RESULT);
    let pass = [
        "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
    ];
    let out = machine.run("verifier", &pass).await;
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains(&riff::text::verify_closes_live(40, 12)),
        "{stderr}"
    );
    assert!(
        stderr.contains("Fix: change the first line of the body to `Refs #12`"),
        "{stderr}"
    );
    let calls = log(machine.bin.path());
    assert!(calls.contains("gh issue view 12 --json body"), "{calls}");
    assert!(!calls.contains("gh pr comment"), "{calls}");
    assert!(!calls.contains("gh pr merge"), "{calls}");
    assert!(!calls.contains("/statuses/"), "{calls}");
    let inbox = machine.ok("author", &["read"]).await;
    assert!(!inbox.contains("verify result:"), "{inbox}");

    // A fail needs no such check.
    let fail = [
        "verify", "fail", "40", "--file", &result, "--commit", "1a2b3c4d",
    ];
    machine.ok("verifier", &fail).await;
    assert!(log(machine.bin.path()).contains("-f state=failure"));

    // The body with `Refs #12` passes, and reads no body of an issue.
    let refs = issue(LIVE_ISSUE).replace(r#""body":"Closes #12\n"#, r#""body":"Refs #12\n"#);
    assert!(refs.contains(r#""body":"Refs #12\n"#), "{refs}");
    let machine = Machine::new(&refs).await;
    machine.claim("author", "issue-12").await;
    let result = machine.write("result.md", RESULT);
    let pass = [
        "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
    ];
    machine.ok("verifier", &pass).await;
    let calls = log(machine.bin.path());
    assert!(calls.contains("-f state=success"), "{calls}");
    assert!(!calls.contains("gh issue view 12 --json body"), "{calls}");

    // An issue with no `Live:` line takes `Closes #12`.
    let machine = Machine::new(&issue(PLAIN_ISSUE)).await;
    machine.claim("author", "issue-12").await;
    let result = machine.write("result.md", RESULT);
    let pass = [
        "verify", "pass", "40", "--file", &result, "--commit", "1a2b3c4d",
    ];
    machine.ok("verifier", &pass).await;
    let calls = log(machine.bin.path());
    assert!(calls.contains("gh issue view 12 --json body"), "{calls}");
    assert!(calls.contains("-f state=success"), "{calls}");
}

/// 01M7CFBZSQEEZNZBR9H1S54GS1, 01M7D11KGEJ8ACFJK6Y10WENJZ: `riff verify
/// pass` reads the council of the pull request as `riff council show`
/// does: design, testing, docs and rust for a change of `src/lib.rs`. A
/// result with no part `## The council` gets no squash message, no
/// comment, no status and no post, and the line names each member. A
/// result with no line for docs names docs only. `riff verify fail`
/// reports the result with no part.
#[tokio::test]
async fn verify_pass_refuses_a_result_with_no_line_for_a_member_of_the_council() {
    let machine = Machine::new(VERIFY).await;
    machine.claim("author", "issue-12").await;
    let names = |names: &[&str]| -> Vec<riff::council::Name> {
        names.iter().map(|name| name.parse().unwrap()).collect()
    };
    let no_part = machine.write("result.md", "1. The wave shows.\n");
    let no_docs = machine.write("no-docs.md", &RESULT.replace("- docs: no finding.\n", ""));
    for (result, members) in [
        (&no_part, names(&["design", "testing", "docs", "rust"])),
        (&no_docs, names(&["docs"])),
    ] {
        let pass = [
            "verify", "pass", "40", "--file", result, "--commit", "1a2b3c4d",
        ];
        let out = machine.run("verifier", &pass).await;
        assert_eq!(out.status.code(), Some(1), "{result}");
        let stderr = text(&out.stderr);
        let line = riff::text::verify_no_council_line(&members);
        let refusals: Vec<&str> = stderr
            .lines()
            .filter(|l| l.contains("riff verify pass:"))
            .collect();
        assert_eq!(refusals.len(), 1, "one line: {stderr}");
        assert!(refusals[0].ends_with(&line), "{line}\n{stderr}");
        let calls = log(machine.bin.path());
        assert!(
            calls.contains("gh pr view 40 --json baseRefName,body,labels"),
            "{calls}"
        );
        assert!(!calls.contains("gh pr merge"), "{calls}");
        assert!(!calls.contains("gh pr comment"), "{calls}");
        assert!(!calls.contains("/statuses/"), "{calls}");
        let inbox = machine.ok("author", &["read"]).await;
        assert!(!inbox.contains("verify result:"), "{inbox}");
    }
    assert!(
        riff::text::verify_no_council_line(&names(&["design", "testing", "docs", "rust"]))
            .contains("for design, testing, docs and rust.")
    );

    // A fail can stop early: it needs no part `## The council`.
    let fail = [
        "verify", "fail", "40", "--file", &no_part, "--commit", "1a2b3c4d",
    ];
    machine.ok("verifier", &fail).await;
    let calls = log(machine.bin.path());
    assert!(calls.contains("-f state=failure"), "{calls}");
    let inbox = machine.ok("author", &["read"]).await;
    assert!(
        inbox.contains("verify result: FAIL for issue-12, PR #40, commit 1a2b3c4d."),
        "{inbox}"
    );
}

/// The fake forge of a pull request that changes `riff.toml` from the
/// pin of `ghcr.io/o/i` to the pin `head`.
fn pin_cases(head: &str) -> String {
    let file = |image: &str| format!(r#"printf '[session]\nimage = "{image}"\n'"#);
    format!(
        "{VERIFY}\n\
         *'/pulls/40/files'*) printf 'src/lib.rs\\nriff.toml\\n' ;;\n\
         *'pr view 40 --json baseRefName --jq'*) echo main ;;\n\
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
        let result = machine.write("result.md", RESULT);
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
        assert!(!calls.contains("gh pr merge"), "{calls}");
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
    let result = machine.write("result.md", RESULT);
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
    let result = machine.write("result.md", RESULT);
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
    // The pull request, the comment, the status, and the read of issue 12.
    let mut cases: Vec<String> = VERIFY
        .lines()
        .take(3)
        .chain(VERIFY.lines().last())
        .map(str::to_owned)
        .collect();
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
    let result = machine.write("result.md", RESULT);
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
    let result = machine.write("result.md", RESULT);
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
        let result = machine.write("result.md", RESULT);
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
        let result = machine.write("result.md", RESULT);
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
        assert!(!calls.contains("gh pr merge"), "{calls}");
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
        let result = machine.write("result.md", RESULT);
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
        assert!(!calls.contains("gh pr merge"), "{calls}");
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
    let result = machine.write("result.md", RESULT);
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
*'pr create'*) echo 'https://github.com/como-technologies/riff/pull/40' ;;
'api user') echo '{"login":"mike","id":7,"name":"Mike Sandman"}' ;;"#;

/// The trailers of the session `author` of mike at the end of each body
/// (01M6WZ2CB56PNSYKEX4E0ZJV3Y).
const SIGNED: &str =
    "Riff-Session: author\nCo-authored-by: Mike Sandman <7+mike@users.noreply.github.com>\n";

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
        format!(
            "Closes #12\n\nShow the wave in riff who.\n\nIssue: #12\nMilestone: Wave 3\n{SIGNED}"
        )
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
    assert_eq!(
        body,
        format!("Closes #12\n\nIssue: #12\nMilestone: Wave 3\n{SIGNED}")
    );
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
    assert_eq!(
        body,
        format!("Refs #12\n\nIssue: #12\nMilestone: Wave 3\n{SIGNED}")
    );
}

/// #1747, 01M6WW5WSSRMTBP882FQP3TS2Q: issue 12 has a line `Live:` in
/// "Done when". `riff pr open` links with `Refs #12`, with and with no
/// `--refs`. A summary with `Closes #12` is refused with the fix, and
/// riff opens nothing.
#[tokio::test]
async fn pr_open_links_an_issue_with_a_live_line_with_refs() {
    // printf, not echo: the echo of sh reads `\n` as a new line.
    let live = OPEN.replace(
        r#"echo '{"number":12,"state":"OPEN","milestone":{"title":"Wave 3"}}'"#,
        &format!(
            r#"printf '%s' '{{"number":12,"state":"OPEN","milestone":{{"title":"Wave 3"}},"body":"{LIVE_ISSUE}"}}'"#
        ),
    );
    assert!(live.contains("Live: the owner"), "{live}");
    for refs in [false, true] {
        let machine = Machine::new(&live).await;
        machine.claim("author", "issue-12").await;
        let summary = machine.write("summary.md", "Make the Apps.\n");
        let mut args = vec!["pr", "open", "--title", "Make the Apps", "--file", &summary];
        if refs {
            args.push("--refs");
        }
        machine.ok("author", &args).await;
        let (_, body) = created(machine.bin.path());
        assert_eq!(
            body,
            format!("Refs #12\n\nMake the Apps.\n\nIssue: #12\nMilestone: Wave 3\n{SIGNED}"),
            "--refs {refs}"
        );
        let calls = log(machine.bin.path());
        assert!(
            calls.contains("gh issue view 12 --json number,state,milestone,body"),
            "{calls}"
        );
    }

    let machine = Machine::new(&live).await;
    machine.claim("author", "issue-12").await;
    let summary = machine.write("summary.md", "Closes #12\n\nMake the Apps.\n");
    let out = machine
        .run(
            "author",
            &["pr", "open", "--title", "Make the Apps", "--file", &summary],
        )
        .await;
    assert_eq!(out.status.code(), Some(1));
    let err = text(&out.stderr);
    assert!(
        err.contains(&format!(
            "the body has the line `Closes #12`, but this pull request needs `Refs #12`. {}",
            riff::text::live_link_fix(12)
        )),
        "{err}"
    );
    assert!(!log(machine.bin.path()).contains("gh pr create"));
}

/// A body that has the link line and the trailers gets none of them a
/// second time (01M3W2627GYXR8CFW76KB6CB9W, 01M6WZ2CB56PNSYKEX4E0ZJV3Y).
#[tokio::test]
async fn pr_open_adds_no_line_that_the_body_has() {
    let full = format!(
        "Closes #12\n\nShow the wave in riff who.\n\nIssue: #12\nMilestone: Wave 3\n{SIGNED}"
    );
    let session_only =
        "Show the wave in riff who.\n\nIssue: #12\nMilestone: Wave 3\nRiff-Session: author\n";
    for content in [
        &full,
        "Show the wave in riff who.\n\nIssue: #12\nMilestone: Wave 3\n",
        "Closes #12\n\nShow the wave in riff who.\n\nIssue: #12\n",
        session_only,
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
        for line in [
            "Closes #12\n",
            "Issue: #12\n",
            "Milestone: Wave 3\n",
            "Riff-Session: author\n",
            "Co-authored-by: ",
        ] {
            assert_eq!(body.matches(line).count(), 1, "{line} in {body}");
        }
    }
}

/// A person in a terminal is the author: the body has no trailer of a
/// session (01M6WZ2CB56PNSYKEX4E0ZJV3Y), and riff reads no account.
#[tokio::test]
async fn pr_open_of_a_person_adds_no_trailer_of_a_session() {
    let machine = Machine::new(OPEN).await;
    machine
        .ok(
            "",
            &["pr", "open", "--title", "Show the wave", "--issue", "12"],
        )
        .await;
    let (_, body) = created(machine.bin.path());
    assert_eq!(body, "Closes #12\n\nIssue: #12\nMilestone: Wave 3\n");
    assert!(!log(machine.bin.path()).contains("gh api user"));
}

/// With no name on GitHub, the login is the name of the line
/// `Co-authored-by:` (01M6WZ2CB56PNSYKEX4E0ZJV3Y).
#[tokio::test]
async fn pr_open_takes_the_login_when_the_person_has_no_name() {
    let no_name = OPEN.replace(r#""name":"Mike Sandman""#, r#""name":null"#);
    let machine = Machine::new(&no_name).await;
    machine.claim("author", "issue-12").await;
    machine
        .ok("author", &["pr", "open", "--title", "Show the wave"])
        .await;
    let (_, body) = created(machine.bin.path());
    assert!(
        body.ends_with(
            "Riff-Session: author\nCo-authored-by: mike <7+mike@users.noreply.github.com>\n"
        ),
        "{body}"
    );
}

/// When riff cannot read the account of the person, for example with a
/// token of a GitHub App, the body has the session and no line
/// `Co-authored-by:`, and riff says why (01M6WZ2CG9DFB45E3BQCGQQYNY).
#[tokio::test]
async fn pr_open_says_why_the_body_has_no_co_author() {
    let refused = OPEN.replace(
        r#"'api user') echo '{"login":"mike","id":7,"name":"Mike Sandman"}' ;;"#,
        r#"'api user') echo 'Resource not accessible by integration (HTTP 403)' >&2; exit 1 ;;"#,
    );
    let machine = Machine::new(&refused).await;
    machine.claim("author", "issue-12").await;
    let out = machine
        .run("author", &["pr", "open", "--title", "Show the wave"])
        .await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let err = text(&out.stderr);
    assert!(
        err.contains("riff cannot read your account on GitHub (the read of the person on GitHub failed: Resource not accessible by integration (HTTP 403)). The body of the pull request gets no line Co-authored-by."),
        "{err}"
    );
    let (_, body) = created(machine.bin.path());
    assert_eq!(
        body,
        "Closes #12\n\nIssue: #12\nMilestone: Wave 3\nRiff-Session: author\n"
    );
}

/// A body with the line `Riff-Session:` of another session is refused,
/// and riff opens nothing (01M6WZ2CB56PNSYKEX4E0ZJV3Y).
#[tokio::test]
async fn pr_open_refuses_the_session_line_of_another_session() {
    let machine = Machine::new(OPEN).await;
    machine.claim("author", "issue-12").await;
    let summary = machine.write(
        "summary.md",
        "Text.\n\nIssue: #12\nRiff-Session: 9f8e7d6c\n",
    );
    let out = machine
        .run(
            "author",
            &["pr", "open", "--title", "Show the wave", "--file", &summary],
        )
        .await;
    assert_eq!(out.status.code(), Some(1));
    let err = text(&out.stderr);
    assert!(
        err.contains(
            "the body has the line `Riff-Session: 9f8e7d6c`, but this pull request needs \
             `Riff-Session: author`."
        ),
        "{err}"
    );
    assert!(!log(machine.bin.path()).contains("gh pr create"));
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

/// A `gh` with no sign-in ends before it reads the body of `pr create`.
/// `riff pr open` shows the error of `gh`, and not a broken pipe
/// (01M6Y4XHQADF27SHMMMV8RQ7Q1). The body is more than a pipe holds, so
/// the write meets the closed pipe.
#[tokio::test]
async fn pr_open_shows_the_error_of_a_gh_that_ends_before_it_reads_the_body() {
    let machine = Machine::new(OPEN).await;
    machine.claim("author", "issue-12").await;
    let cases = OPEN.replace(
        "*'pr create'*) echo 'https://github.com/como-technologies/riff/pull/40' ;;",
        "*'pr create'*) echo 'To get started with GitHub CLI, please run:  gh auth login' >&2; exit 4 ;;",
    );
    isolated::write_program(
        machine.bin.path().join("gh"),
        format!("#!/bin/sh\ncase \"$*\" in\n{cases}\nesac\nexit 0\n"),
    );
    let summary = machine.write("summary.md", &"Show the wave in riff who.\n".repeat(4096));
    let out = machine
        .run(
            "author",
            &["pr", "open", "--title", "Show the wave", "--file", &summary],
        )
        .await;
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("please run:  gh auth login"), "{stderr}");
    assert!(!stderr.contains("Broken pipe"), "{stderr}");
}

/// A worker writes `Tests: ...` at the end of its summary, and the
/// rule reads the line as a trailer. A line of 300 characters stops the
/// pull request with the rule `trailer-wrap`, and riff opens nothing. The
/// same text as a plain sentence opens it (01M6H3EK1J3DEEV09FAKKKFPEB).
#[tokio::test]
async fn pr_open_refuses_a_tests_line_of_300_characters_and_takes_the_same_text_as_a_sentence() {
    let tests = format!("{}done", "unit tests pass, ".repeat(17));
    let line = format!("Tests: {tests}");
    assert_eq!(line.chars().count(), 300);

    let machine = Machine::new(OPEN).await;
    machine.claim("author", "issue-12").await;
    let summary = machine.write("summary.md", &format!("Show the wave.\n\n{line}\n"));
    let out = machine
        .run(
            "author",
            &["pr", "open", "--title", "Show the wave", "--file", &summary],
        )
        .await;
    assert_eq!(out.status.code(), Some(1));
    let err = text(&out.stderr);
    assert!(err.contains("trailer-wrap: "), "{err}");
    assert!(err.contains("has 300 characters"), "{err}");
    assert!(err.contains(&line), "{err}");
    assert!(!log(machine.bin.path()).contains("gh pr create"));

    // The same text as a plain sentence.
    let sentence = format!("I ran the checks, and {tests}.");
    let machine = Machine::new(OPEN).await;
    machine.claim("author", "issue-12").await;
    let summary = machine.write("summary.md", &format!("Show the wave.\n\n{sentence}\n"));
    let out = machine
        .ok(
            "author",
            &["pr", "open", "--title", "Show the wave", "--file", &summary],
        )
        .await;
    assert!(out.starts_with("Opened pull request #40"), "{out}");
    let (_, body) = created(machine.bin.path());
    assert_eq!(
        body,
        format!(
            "Closes #12\n\nShow the wave.\n\n{sentence}\n\nIssue: #12\nMilestone: Wave 3\n{SIGNED}"
        )
    );
    let pr = hygiene::PullRequest::new("Show the wave", &body, Some("Wave 3"));
    let issue = hygiene::Issue::new(12, "OPEN", Some("Wave 3"));
    assert_eq!(hygiene::check_pr(&pr, Some(&issue)), []);
}

/// The issue 12 of the fake `gh` has no milestone.
const OPEN_NO_WAVE: &str = r#"*'issue view 12 --json number,state,milestone'*) echo '{"number":12,"state":"OPEN","milestone":null}' ;;
*'pr create'*) echo 'https://github.com/como-technologies/riff/pull/40' ;;
'api user') echo '{"login":"mike","id":7,"name":"Mike Sandman"}' ;;"#;

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
        assert_eq!(
            body,
            format!("Closes #12\n\nIssue: #12\n{SIGNED}"),
            "{config:?}"
        );
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
        format!("Closes #12\n\nShow the wave. It fixes #13.\n\nIssue: #12\n{SIGNED}")
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

/// The `###` how-to `heading` of how-it-works.md, to the next heading. A
/// line that starts with `#` in a code block is no heading.
fn how_to(heading: &str) -> String {
    let page = book::page("how-it-works.md");
    let start = page
        .find(&format!("\n### {heading}\n"))
        .unwrap_or_else(|| panic!("no how-to {heading:?}"));
    let rest = &page[start + 1..];
    let (mut fenced, mut at) = (false, 0);
    for line in rest.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced && at > 0 && line.starts_with('#') {
            return rest[..at - 1].to_owned();
        }
        at += line.len();
    }
    rest.to_owned()
}

/// 01M6Y6B8VBM3TKC3YHZKSA4PFZ, 01M6WZ2C8JXEA888XYMRXTS17V,
/// 01M6WZ2CB56PNSYKEX4E0ZJV3Y: the how-to "Find the commits of a
/// session" of development.md finds the commits of a session by the
/// trailer `Riff-Session:` on its branch and on `main`, and the commits
/// of each session by the name of the bot, in copyable `sh` blocks. Its
/// author is the one of the daemon, and the line `Co-authored-by:` of
/// "Open a pull request" is the one of riff.
#[test]
fn the_book_shows_how_to_find_the_commits_of_a_session() {
    let found = &commits_how_to();
    let commands = book::commands_in(found);
    let (name, email) = riff::daemon::gh_token::author("riff-como-technologies[bot]", Some(1101));
    for command in [
        "git log --grep 'Riff-Session: 21c026a1' origin/worktree-issue-12".to_owned(),
        "git log --grep 'Riff-Session: 21c026a1' origin/main".to_owned(),
        format!("git log -F --author='{name}' origin/worktree-issue-12"),
    ] {
        assert!(commands.contains(&command), "{command}\n{commands:?}");
    }
    assert!(found.contains(&format!("`{name}`")), "{found}");
    assert!(found.contains(&format!("`{email}`")), "{found}");

    let mike = pr::GitHubUser {
        login: "mike".into(),
        id: 1234567,
        name: Some("Mike".into()),
    };
    let line = mike.co_author().unwrap();
    assert!(how_to("Open a pull request").contains(&line), "{line}");
}

/// The how-to "Find the commits of a session" of development.md.
fn commits_how_to() -> String {
    let page = book::page("development.md");
    let start = page
        .find("\n### Find the commits of a session\n")
        .expect("the how-to Find the commits of a session");
    let rest = &page[start + 1..];
    rest[..rest[4..].find("\n#").map_or(rest.len(), |i| i + 4)].to_owned()
}

/// 01M6Y6B8VBM3TKC3YHZKSA4PFZ: each command of "Find the commits of a
/// session" runs in a real git repository. The search for the trailer
/// finds the commit of the session and no other, and the search for the
/// bot finds each commit of the bot and none of a person.
#[test]
fn the_commands_of_the_book_find_the_commits_of_a_session() {
    let env = Isolated::new();
    let repo = env.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let run = |cmd: &mut Command| {
        let out = cmd.current_dir(&repo).output().unwrap();
        assert!(out.status.success(), "{out:?}");
        text(&out.stdout)
    };
    run(env.command("git").args(["init", "-q"]));
    let (bot, email) = riff::daemon::gh_token::author("riff-como-technologies[bot]", Some(1101));
    let session = "Riff-Session: 21c026a1-ee2a-4451-9729-59e6332cb974";
    let other = "Riff-Session: 9de7b86a-39a2-4f79-aa5e-b9508cc7a0a2";
    for (name, email, subject, trailer) in [
        (bot.as_str(), email.as_str(), "of the session", session),
        (bot.as_str(), email.as_str(), "of another session", other),
        ("Mike", "mike@example.com", "of a person", "Issue: #12"),
    ] {
        run(env.command("git").args([
            "-c",
            &format!("user.name={name}"),
            "-c",
            &format!("user.email={email}"),
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            subject,
            "--trailer",
            trailer,
        ]));
    }
    for branch in ["origin/worktree-issue-12", "origin/main"] {
        let to = format!("refs/remotes/{branch}");
        run(env.command("git").args(["update-ref", &to, "HEAD"]));
    }
    let found = |command: &str| run(env.command("sh").args(["-c", command]));
    let commands = book::commands_in(&commits_how_to());
    for command in commands.iter().filter(|c| c.contains("--grep")) {
        let log = found(command);
        assert!(log.contains("of the session"), "{command}\n{log}");
        assert!(
            !log.contains("another") && !log.contains("person"),
            "{command}\n{log}"
        );
    }
    let by_bot = commands
        .iter()
        .find(|c| c.contains("--author"))
        .expect("a search by the bot");
    let log = found(by_bot);
    assert!(
        log.contains("of the session") && log.contains("of another session"),
        "{log}"
    );
    assert!(!log.contains("of a person"), "{log}");
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

/// 01M7CFBZSQEEZNZBR9H1S54GS1: "Report a verify" says that `riff verify
/// pass` refuses a result with no line for a member of the council, and
/// shows the line that it prints.
#[test]
fn the_book_shows_the_line_of_a_pass_with_no_line_for_a_member_of_the_council() {
    let part = how_to("Report a verify");
    let members = ["docs".parse().unwrap(), "rust".parse().unwrap()];
    let line = riff::text::verify_no_council_line(&members);
    assert!(
        part.contains(&format!("```text\n{line}\n```")),
        "{line}\n{part}"
    );
    let flat = part.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "When the part has no line for a member, or the result has no such part, `riff verify \
         pass` reports nothing and prints one line that names each member with no line",
        "A fail needs no part `## The council`",
        "Its PASS with no line for a member is a result with no verdict: your lead gets a \
         message that names each member with no line, and the pull request waits for a new \
         verify.",
    ] {
        assert!(flat.contains(words), "no {words:?} in {flat}");
    }
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
            "Set how often a wait looks",
            &["riff pr wait 40 --every 10"],
        ),
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
    let pace = how_to("Set how often a wait looks");
    assert!(pace.contains("`--every SECONDS`"), "{pace}");
    let points = format!("at most {} points an hour", pr::WAIT_POINTS_AN_HOUR);
    assert!(pace.contains(&points), "{pace}");
    assert!(pace.contains("default pace"), "{pace}");
    assert!(pace.contains("72 points in the first hour"), "{pace}");
}

/// 01M6JGF6Z6Y8FP86Q9VAS3NPZ7 (#1582): the calls of `gh` of `riff pr wait`
/// are in the tally of the daemon, as the caller `pr wait`, with their kind
/// and GraphQL points. `riff daemon status` shows the tally.
#[tokio::test]
async fn pr_wait_adds_its_calls_to_the_tally_of_the_daemon() {
    let open_then_merged = r#"*'pr view 40 --json state,mergeCommit'*)
    n=$(cat "$dir/views" 2>/dev/null || echo 0); n=$((n + 1)); echo $n > "$dir/views"
    if [ $n -lt 2 ]; then echo '{"state":"OPEN","mergeCommit":null}'
    else echo '{"state":"MERGED","mergeCommit":{"oid":"9f8e7d6c"}}'; fi ;;
*'pr checks 40 --required --json name,bucket'*) echo '[{"name":"Gate","bucket":"pass"}]' ;;"#;
    let machine = Machine::new(open_then_merged).await;
    let out = machine
        .run("s1", &["pr", "wait", "40", "--every", "1"])
        .await;
    assert!(out.status.success(), "{}", text(&out.stderr));

    let folder = machine.env.riff_home().join("daemon");
    let calls = riff::ghcalls::Tally::in_folder(&folder).read(riff::daemon::now_ms());
    let kinds = |caller: &str| -> Vec<(&str, u32)> {
        calls
            .iter()
            .filter(|c| c.caller == caller)
            .map(|c| (c.kind.as_str(), c.points))
            .collect()
    };
    // The two looks, and the read of the issue after the merge.
    let wait = kinds("pr wait");
    assert_eq!(
        wait.iter().filter(|c| **c == ("pr view", 1)).count(),
        3,
        "{calls:?}"
    );
    assert_eq!(
        wait.iter().filter(|c| **c == ("pr checks", 2)).count(),
        2,
        "{calls:?}"
    );
    // The total of the tokens after the merge is the usage meter.
    assert_eq!(kinds("usage"), [("pr view", 1)], "{calls:?}");
    // Each call of `gh` that uses a limit is in the tally one time.
    let log = log(machine.bin.path());
    assert_eq!(log.matches("gh pr view 40").count(), 4, "{log}");
    assert_eq!(log.matches("gh pr checks 40").count(), 2, "{log}");
    assert_eq!(calls.len(), 6, "{calls:?}");
}
