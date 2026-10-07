//! The tokens and the models of each issue (#330): the sum of a claim
//! (01M3Y1YP0QY11VR28RF9MKPN0G), its comment on the issue
//! (01M3Y1YP1ZA5TBRA01MKWM3VC6) and `riff usage`
//! (01M3Y1YP3QMKS6B35PJ42KNYXX, 01M3Y1YP45VQS5HMJCXKRN3CCR,
//! 01M3Y1YP4KVHK1DTZ85YNGDG0T).
//!
//! A fake `gh` on `PATH` keeps the comments of each issue in a file, as
//! GitHub does. A real `riff-server` with no sign-in holds the claims.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use isolated::Isolated;
use riff::api::Api;
use riff::mcp::Tools;
use riff::pr::Gh;
use riff::usage::Meter;
use riff_core::name::SessionUri;
use rmcp::model::CallToolRequestParams;
use rmcp::{ServiceExt, service::RunningService};
use serde_json::json;

use crate::book;

/// A fake `gh`: it logs each call to `gh.log`, keeps each comment of
/// the issue N on one line of `comments-N`, and answers
/// `gh issue list` from the file `wave`. The file `login` names who
/// writes: the default is `mike`. With the file `down`, each call fails.
/// A write reads its body from the file after `--input`. With the file
/// `empty-reply`, the next POST of a total fails as a reply with no body
/// does, and keeps nothing. With the file `bad-gateway`, each POST of a
/// total fails with the status 502.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "gh $*" >> "$dir/gh.log"
if [ -f "$dir/down" ]; then echo "you are not logged in" >&2; exit 1; fi
if [ "$1 $2 $3" = "api -X POST" ] && grep -q 'riff:usage-total' "$7" 2>/dev/null; then
    if [ -f "$dir/empty-reply" ]; then
        rm "$dir/empty-reply"; echo "unexpected end of JSON input" >&2; exit 1
    fi
    if [ -f "$dir/bad-gateway" ]; then
        printf 'HTTP/2.0 502 Bad Gateway\r\n\r\n'; echo "unexpected end of JSON input" >&2; exit 1
    fi
fi
login=$(cat "$dir/login" 2>/dev/null || echo mike)
issues=repos/como-technologies/riff/issues/
case "$1 $2" in
'api user') echo "$login" ;;
'api --paginate')
    n=${3#"$issues"}; n=${n%/comments}
    cat "$dir/comments-$n" 2>/dev/null ;;
'api -X')
    [ "$5 $6" = "--include --input" ] || { echo "no body file" >&2; exit 1; }
    body=$(cat "$7"); rest=${body#\{}
    case "$3" in
    POST)
        n=${4#"$issues"}; n=${n%/comments}
        id=$(cat "$dir"/comments-* 2>/dev/null | wc -l); id=$((id + 1))
        printf '{"id":%s,"login":"%s",%s\n' "$id" "$login" "$rest" >> "$dir/comments-$n" ;;
    PATCH)
        id=${4##*/}
        for f in "$dir"/comments-*; do
            LINE="{\"id\":$id,\"login\":\"$login\",$rest" awk -v start="{\"id\":$id," \
                'index($0, start) == 1 { print ENVIRON["LINE"]; next } { print }' "$f" > "$dir/new"
            mv "$dir/new" "$f"
        done ;;
    esac ;;
'issue list') cat "$dir/wave" ;;
esac
exit 0
"#;

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// One reply of `model` in a transcript, at the time `at`, with the
/// four kinds of tokens.
fn reply(at: &str, id: &str, model: &str, tokens: [u64; 4]) -> String {
    let [input, output, write, read] = tokens;
    json!({
        "type": "assistant",
        "timestamp": at,
        "sessionId": "claude-1",
        "cwd": "/home/mike/secret-path",
        "message": {
            "id": id,
            "model": model,
            "content": [{ "type": "text", "text": "the secret text of the reply" }],
            "usage": {
                "input_tokens": input,
                "output_tokens": output,
                "cache_creation_input_tokens": write,
                "cache_read_input_tokens": read,
            },
        },
    })
    .to_string()
}

/// The time now, as a transcript has it.
fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
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
    async fn new() -> Machine {
        let bin = tempfile::tempdir().unwrap();
        let gh = bin.path().join("gh");
        std::fs::write(&gh, FAKE_GH).unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        Machine {
            env: Isolated::new(),
            bin,
            repo: repo(),
            server: server().await,
        }
    }

    /// `riff ARGS` as the session `session` of mike, with the fake `gh`
    /// first in `PATH`.
    fn riff(&self, session: &str, args: &[&str]) -> Command {
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
        cmd
    }

    async fn run(&self, session: &str, args: &[&str]) -> Output {
        let mut cmd = self.riff(session, args);
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

    /// Runs the hook `event` of `session` with `input` on stdin.
    async fn hook(&self, session: &str, event: &str, input: serde_json::Value) {
        let mut cmd = self.riff(session, &["hook", event]);
        let out = tokio::task::spawn_blocking(move || {
            let mut hook = cmd
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            hook.stdin
                .take()
                .unwrap()
                .write_all(input.to_string().as_bytes())
                .unwrap();
            hook.wait_with_output().unwrap()
        })
        .await
        .unwrap();
        assert!(out.status.success(), "{}", text(&out.stderr));
    }

    /// The transcript of `session`.
    fn transcript(&self, session: &str) -> PathBuf {
        self.repo.path().join(format!("{session}.jsonl"))
    }

    /// Starts `session`: its start hook gets the path of its transcript.
    async fn start(&self, session: &str) {
        let input = json!({
            "session_id": "claude-1",
            "source": "startup",
            "transcript_path": self.transcript(session),
        });
        self.hook(session, "session-start", input).await;
    }

    /// Makes the riff run, with `session` as the lead.
    async fn resume(&self, session: &str) {
        self.ok(session, &["lead"]).await;
        self.ok(session, &["resume", "--riff"]).await;
    }

    /// Writes `lines` at the end of the transcript of `session`.
    fn says(&self, session: &str, lines: &[String]) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.transcript(session))
            .unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
    }

    /// `session` claims `item`, the model replies with `replies` (the
    /// model and its four kinds of tokens), and the session releases the
    /// item. Returns the output of the release.
    async fn works(&self, session: &str, item: &str, replies: &[(&str, [u64; 4])]) -> String {
        self.ok(session, &["claim", item]).await;
        let lines: Vec<String> = replies
            .iter()
            .enumerate()
            .map(|(i, (model, tokens))| reply(&now(), &format!("{item}-{i}"), model, *tokens))
            .collect();
        self.says(session, &lines);
        self.ok(session, &["release", item]).await
    }

    /// The comments of the issue `n`, one on each line.
    fn comments(&self, n: u64) -> String {
        std::fs::read_to_string(self.bin.path().join(format!("comments-{n}"))).unwrap_or_default()
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.bin.path().join("gh.log")).unwrap_or_default()
    }
}

/// Waits until `done` is true, for at most 20 seconds.
async fn wait_for(what: &str, done: impl Fn() -> bool) {
    let span = isolated::Span::start();
    while !done() {
        assert!(span.within(Duration::from_secs(20)), "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

const OPUS: &str = "claude-opus-5-5";
const HAIKU: &str = "claude-haiku-4-5";

#[tokio::test]
async fn usage_shows_the_sum_of_a_claim_for_each_model_and_the_total() {
    let machine = Machine::new().await;
    machine.start("s1").await;
    machine.resume("s1").await;
    // Before the claim, and far after its end.
    machine.says(
        "s1",
        &[
            reply("2020-01-01T00:00:00.000Z", "before", OPUS, [9000; 4]),
            reply("2099-01-01T00:00:00.000Z", "after", HAIKU, [7000; 4]),
        ],
    );
    let released = machine
        .works(
            "s1",
            "issue-12",
            &[
                (OPUS, [1, 20, 300, 4000]),
                (HAIKU, [5, 60, 700, 8000]),
                (OPUS, [10, 200, 3000, 40000]),
            ],
        )
        .await;
    assert!(
        released.contains(
            "The claim of issue-12 took 56,296 tokens (input 16, output 280, cache write 4,000, \
             cache read 52,000). riff put them on #12 as a comment."
        ),
        "{released}"
    );

    let shown = machine.ok("s1", &["usage", "issue-12"]).await;
    let lines: Vec<&str> = shown.lines().collect();
    assert_eq!(
        lines[0],
        "#12: 56,296 tokens (input 16, output 280, cache write 4,000, cache read 52,000) in 1 claim"
    );
    assert_eq!(lines[1], "work: 56,296 tokens");
    assert!(
        lines[2].starts_with("  issue-12, work, session s1, "),
        "{shown}"
    );
    assert!(
        lines[2].ends_with("Models: claude-haiku-4-5, claude-opus-5-5. Comment of mike."),
        "{shown}"
    );
    assert_eq!(
        lines[3],
        "    claude-haiku-4-5: 8,765 tokens (input 5, output 60, cache write 700, cache read 8,000)"
    );
    assert_eq!(
        lines[4],
        "    claude-opus-5-5: 47,531 tokens (input 11, output 220, cache write 3,300, cache read \
         44,000)"
    );
    assert_eq!(lines.len(), 5, "{shown}");
    // Each form of the issue.
    for issue in ["12", "#12"] {
        assert_eq!(machine.ok("s1", &["usage", issue]).await, shown);
    }

    // The comment holds only numbers and names (01M3Y1YP2CSNHCWV7T4CE9HZ4Y).
    let comments = machine.comments(12);
    assert_eq!(comments.lines().count(), 1, "{comments}");
    for mark in ["secret", ".jsonl", "/home", "claude-1", "@", "thelio"] {
        assert!(!comments.contains(mark), "{mark} is in {comments}");
    }
    assert!(comments.contains("<details><summary>riff:usage</summary>"));
}

#[tokio::test]
async fn usage_shows_the_work_claim_and_the_verify_claim_and_one_total() {
    let machine = Machine::new().await;
    machine.start("author").await;
    machine.start("verifier").await;
    machine.resume("author").await;
    machine
        .works("author", "issue-12", &[(OPUS, [1, 2, 3, 4])])
        .await;
    // The verifier is a session of another person on GitHub.
    std::fs::write(machine.bin.path().join("login"), "brett").unwrap();
    let released = machine
        .works("verifier", "verify-issue-12", &[(HAIKU, [10, 20, 30, 40])])
        .await;
    assert!(released.contains("riff put them on #12"), "{released}");

    let shown = machine.ok("author", &["usage", "12"]).await;
    let lines: Vec<&str> = shown.lines().collect();
    assert_eq!(
        lines[0],
        "#12: 110 tokens (input 11, output 22, cache write 33, cache read 44) in 2 claims"
    );
    assert_eq!(lines[1], "work: 10 tokens");
    assert!(
        lines[2].starts_with("  issue-12, work, session author, ")
            && lines[2].ends_with("Comment of mike."),
        "{shown}"
    );
    assert_eq!(lines[4], "verify: 100 tokens");
    assert!(
        lines[5].starts_with("  verify-issue-12, verify, session verifier, ")
            && lines[5].ends_with("Comment of brett."),
        "{shown}"
    );
    assert_eq!(lines.len(), 7, "{shown}");
}

#[tokio::test]
async fn usage_of_a_wave_lists_each_issue_with_its_total() {
    let machine = Machine::new().await;
    machine.start("s1").await;
    machine.resume("s1").await;
    machine
        .works("s1", "issue-12", &[(OPUS, [1, 2, 3, 4])])
        .await;
    machine
        .works("s1", "issue-14", &[(OPUS, [100, 200, 300, 400])])
        .await;
    std::fs::write(
        machine.bin.path().join("wave"),
        r#"[{"number":14,"title":"Fix the tail"},{"number":12,"title":"Show the wave"},{"number":13,"title":"Not started"}]"#,
    )
    .unwrap();

    let shown = machine.ok("s1", &["usage", "--wave", "Wave 3"]).await;
    assert_eq!(
        shown,
        "Wave 3: 1,010 tokens (input 101, output 202, cache write 303, cache read 404) in 3 issues\n\
         \x20 #12 Show the wave: 10 tokens\n\
         \x20 #13 Not started: 0 tokens\n\
         \x20 #14 Fix the tail: 1,000 tokens\n"
    );
    assert!(
        machine
            .log()
            .contains("gh issue list --repo como-technologies/riff --milestone Wave 3 --state all"),
        "{}",
        machine.log()
    );

    // An issue and a wave at the same time are refused.
    let both = machine
        .run("s1", &["usage", "12", "--wave", "Wave 3"])
        .await;
    assert_eq!(both.status.code(), Some(2));
}

#[tokio::test]
async fn a_release_with_no_gh_keeps_the_tokens_on_the_machine_and_does_not_fail() {
    let machine = Machine::new().await;
    machine.start("s1").await;
    machine.resume("s1").await;
    machine.says("s1", &[reply(&now(), "idle", OPUS, [1, 1, 1, 1])]);
    std::fs::write(machine.bin.path().join("down"), "").unwrap();
    let released = machine
        .works("s1", "issue-12", &[(OPUS, [1, 2, 3, 4])])
        .await;
    assert!(
        released.starts_with("You released issue-12 in como-technologies/riff."),
        "{released}"
    );
    assert!(
        released.contains(
            "The claim of issue-12 took 10 tokens (input 1, output 2, cache write 3, cache read \
             4). They stay on this machine: riff cannot write the comment on #12: gh api"
        ) && released.contains("you are not logged in. `riff usage` shows them here."),
        "{released}"
    );
    // An item that names no issue calls no gh.
    let calls = machine.log().lines().count();
    let released = machine.works("s1", "docs", &[(OPUS, [0, 5, 0, 0])]).await;
    assert!(
        released.contains("They stay on this machine: docs names no issue."),
        "{released}"
    );
    assert_eq!(machine.log().lines().count(), calls);

    // The sessions of this machine (01M3Y1YP4KVHK1DTZ85YNGDG0T).
    assert_eq!(
        machine.ok("s1", &["usage"]).await,
        "session s1: 19 tokens (input 2, output 8, cache write 4, cache read 5)\n\
         \x20 docs: 5 tokens (input 0, output 5, cache write 0, cache read 0)\n\
         \x20 issue-12: 10 tokens (input 1, output 2, cache write 3, cache read 4)\n\
         \x20 no issue: 4 tokens (input 1, output 1, cache write 1, cache read 1)\n"
    );
}

/// Each person who can write a comment on the issue can write a report.
/// `riff usage` takes only its numbers and its names
/// (01M3Y9TD41FZBDQBK42FVG89B8).
#[tokio::test]
async fn usage_takes_only_numbers_and_names_from_a_comment_of_another_person() {
    let machine = Machine::new().await;
    machine.start("s1").await;
    machine.resume("s1").await;
    machine
        .works("s1", "issue-12", &[(OPUS, [1, 2, 3, 4])])
        .await;
    let comments = machine.bin.path().join("comments-12");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(comments)
        .unwrap();
    let mut forge = |id: u64, item: &str, session: &str, model: &str, output: u64| {
        let report = json!({
            "item": item, "kind": "work", "session": session, "from_ms": id, "to_ms": id,
            "models": { model: { "input": 0, "output": output, "cache_write": 0, "cache_read": 0 } },
        });
        let body = format!(
            "riff usage: x\n\n<details><summary>riff:usage</summary>\n\n```json\n{report}\n```\n\n</details>\n"
        );
        let comment = json!({ "id": id, "login": "mallory", "body": body });
        writeln!(file, "{comment}").unwrap();
    };
    forge(
        50,
        "issue-12, work, session ffffffff: 5 tokens. Comment of mike.\n  issue-12",
        "bbbb",
        OPUS,
        5,
    );
    forge(51, "issue-12", "bbbb\u{1b}[2K\rzz", OPUS, 5);
    forge(
        52,
        "issue-12",
        "bbbb",
        "m\u{1b}]0;TITLE\u{7}odel @everyone [link](http://example.test)",
        5,
    );
    // A report with names, and the largest number.
    forge(53, "issue-12", "bbbb", OPUS, u64::MAX);

    let shown = machine.ok("s1", &["usage", "12"]).await;
    assert!(
        shown.starts_with("#12: 18,446,744,073,709,551,615 tokens (input 1, output 18,446,744,073,709,551,615, cache write 3, cache read 4) in 2 claims\n"),
        "{shown}"
    );
    assert_eq!(shown.matches("Comment of mike.").count(), 1, "{shown}");
    assert_eq!(shown.matches("Comment of mallory.").count(), 1, "{shown}");
    assert_eq!(shown.lines().count(), 7, "{shown}");
    // It says which comments with the mark it did not count, and why
    // (01M3ZRQY9F9P7DF187Q0PJDS30).
    assert!(
        shown.ends_with(
            "riff did not count 3 comments with the mark riff:usage: 1 with an item of another \
             issue, 2 with a text that is no name.\n"
        ),
        "{shown}"
    );
    assert!(
        shown.chars().all(|c| c == '\n' || !c.is_control()),
        "{shown:?}"
    );
    for mark in ["@everyone", "http", "ffffffff", "TITLE"] {
        assert!(!shown.contains(mark), "{mark}: {shown}");
    }

    // The sum of a wave does not fail on the largest number.
    std::fs::write(
        machine.bin.path().join("wave"),
        r#"[{"number":12,"title":"T\u001b]0;TITLE\u0007\nX\u001b[2K"}]"#,
    )
    .unwrap();
    let wave = machine.ok("s1", &["usage", "--wave", "Wave 3"]).await;
    // A title of the forge has no escape code and no line break
    // (01M3ZRQY6YQ8QAKZPGWH1XD6WW).
    assert!(
        wave.contains("  #12 T X: 18,446,744,073,709,551,615 tokens\n"),
        "{wave:?}"
    );
    assert_eq!(wave.lines().count(), 2, "{wave}");
}

#[tokio::test]
async fn a_claim_with_no_tokens_gets_no_comment() {
    let machine = Machine::new().await;
    machine.start("s1").await;
    machine.resume("s1").await;
    let released = machine.works("s1", "issue-12", &[]).await;
    assert!(
        released.contains("riff counted no tokens for the claim of issue-12."),
        "{released}"
    );
    assert_eq!(machine.log(), "");
    assert_eq!(
        machine.ok("s1", &["usage", "12"]).await,
        "#12 has no comment with tokens.\n"
    );
}

#[tokio::test]
async fn a_new_start_reports_the_claim_that_it_frees() {
    let machine = Machine::new().await;
    machine.start("s1").await;
    machine.resume("s1").await;
    machine.ok("s1", &["claim", "issue-12"]).await;
    machine.says("s1", &[reply(&now(), "m1", OPUS, [1, 2, 3, 4])]);

    // `/clear`: a new transcript, and the claims of the session are free.
    let cleared = json!({
        "session_id": "claude-2",
        "source": "clear",
        "transcript_path": machine.repo.path().join("s1-cleared.jsonl"),
    });
    machine.hook("s1", "session-start", cleared).await;
    wait_for("the comment of the freed claim", || {
        !machine.comments(12).is_empty()
    })
    .await;
    let shown = machine.ok("s1", &["usage", "12"]).await;
    assert!(
        shown.starts_with(
            "#12: 10 tokens (input 1, output 2, cache write 3, cache read 4) in 1 claim\n"
        ),
        "{shown}"
    );

    // A compaction frees no claim.
    machine.ok("s1", &["claim", "issue-14"]).await;
    machine.says("s1", &[reply(&now(), "m2", OPUS, [1, 1, 1, 1])]);
    let compacted = json!({ "session_id": "claude-2", "source": "compact" });
    machine.hook("s1", "session-start", compacted).await;
    assert_eq!(machine.comments(14), "");
    // The end of the session frees it.
    let ended = json!({ "session_id": "claude-2", "reason": "prompt_input_exit" });
    machine.hook("s1", "session-end", ended).await;
    wait_for("the comment at the end of the session", || {
        !machine.comments(14).is_empty()
    })
    .await;
    assert_eq!(machine.comments(14).lines().count(), 1);
}

const MERGED: &str = r#"{"state":"MERGED","mergeCommit":{"oid":"9f8e7d6c"}}"#;

/// Makes the fake `gh` of `machine` also know pull request 40 of the
/// issue 12, merged.
fn knows_pr_40(machine: &Machine) {
    let gh = machine.bin.path().join("gh");
    let with_pr = FAKE_GH.replace(
        "case \"$1 $2\" in\n",
        &format!(
            "case \"$*\" in\n\
             'pr view 40 --json state,mergeCommit') echo '{MERGED}'; exit 0 ;;\n\
             'pr view 40 --json body') printf '%s' '{{\"body\":\"Closes #12\\n\\nIssue: #12\\nMilestone: Wave 3\\n\"}}'; exit 0 ;;\n\
             esac\n\
             case \"$1 $2\" in\n"
        ),
    );
    std::fs::write(&gh, with_pr).unwrap();
}

/// A POST of the total that fails one time with an empty reply: the
/// second try writes it (01M49HF07WQC3M5HGAQNHR8WA0). A POST that fails
/// each time: the line names the HTTP status (01M49HF057R08DGW6X5A8EHR42).
/// Then `riff usage 14 --total` writes the missing total
/// (01M49HF0ADEQ83XTJCQT0PK3RS).
#[tokio::test]
async fn a_failed_post_of_the_total_is_tried_again_and_names_its_http_status() {
    let machine = Machine::new().await;
    knows_pr_40(&machine);
    machine.start("author").await;
    machine.resume("author").await;
    machine
        .works("author", "issue-12", &[(OPUS, [1, 2, 3, 4])])
        .await;
    let total = |comments: &str| {
        comments
            .lines()
            .filter(|c| c.contains("riff:usage-total"))
            .count()
    };

    std::fs::write(machine.bin.path().join("empty-reply"), "").unwrap();
    let out = machine.run("author", &["pr", "wait", "40"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("The total of #12 is on the issue: 10 tokens"),
        "{}",
        text(&out.stderr)
    );
    assert!(!machine.bin.path().join("empty-reply").exists());
    let comments = machine.comments(12);
    assert_eq!(total(&comments), 1, "{comments}");
    assert!(
        machine.log().contains("--include --input "),
        "{}",
        machine.log()
    );

    // An issue with no total, and a forge that gives 502 to each POST.
    std::fs::write(machine.bin.path().join("bad-gateway"), "").unwrap();
    machine
        .works("author", "issue-14", &[(OPUS, [5, 0, 0, 0])])
        .await;
    let out = machine.run("author", &["usage", "14", "--total"]).await;
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains(
            "gh api -X POST repos/como-technologies/riff/issues/14/comments: HTTP 502: \
             unexpected end of JSON input"
        ),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(total(&machine.comments(14)), 0);

    // The forge is good again: the command writes the missing total.
    std::fs::remove_file(machine.bin.path().join("bad-gateway")).unwrap();
    let shown = machine.ok("author", &["usage", "14", "--total"]).await;
    assert!(
        shown.starts_with(
            "The total of #14 is on the issue: 5 tokens (input 5, output 0, cache write 0, \
             cache read 0).\n#14: 5 tokens"
        ),
        "{shown}"
    );
    let comments = machine.comments(14);
    assert_eq!(total(&comments), 1, "{comments}");
    assert!(
        comments.contains("riff usage total of #14: 5 tokens"),
        "{comments}"
    );
}

#[tokio::test]
async fn pr_wait_writes_the_total_and_the_release_replaces_the_comment_of_its_claim() {
    let machine = Machine::new().await;
    knows_pr_40(&machine);
    machine.start("verifier").await;
    machine.start("author").await;
    machine.resume("author").await;
    machine
        .works("verifier", "verify-issue-12", &[(HAIKU, [10, 20, 30, 40])])
        .await;
    machine.ok("author", &["claim", "issue-12"]).await;
    machine.says("author", &[reply(&now(), "a1", OPUS, [1, 2, 3, 4])]);

    let out = machine.run("author", &["pr", "wait", "40"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "9f8e7d6c\n");
    assert!(
        text(&out.stderr).contains(
            "The total of #12 is on the issue: 110 tokens (input 11, output 22, cache write 33, \
             cache read 44)."
        ),
        "{}",
        text(&out.stderr)
    );
    let comments = machine.comments(12);
    assert_eq!(comments.lines().count(), 3, "{comments}");
    assert!(
        comments.contains(
            "riff usage total of #12: 110 tokens (input 11, output 22, cache write 33, cache \
             read 44) in 2 claims. Work: 10 tokens. Verify: 100 tokens. Models: \
             claude-haiku-4-5, claude-opus-5-5."
        ),
        "{comments}"
    );

    // The author writes its "done" note, then releases: a second report
    // of the same claim (01M3Y1YP2TVYQC7GCCAMN6111K).
    machine.says("author", &[reply(&now(), "a2", OPUS, [1000, 0, 0, 0])]);
    machine.ok("author", &["release", "issue-12"]).await;
    let comments = machine.comments(12);
    assert_eq!(comments.lines().count(), 3, "{comments}");
    assert!(
        comments.contains("riff usage total of #12: 1,110 tokens")
            && comments.contains("in 2 claims"),
        "{comments}"
    );
    let shown = machine.ok("author", &["usage", "12"]).await;
    assert!(
        shown.starts_with("#12: 1,110 tokens (input 1,011, output 22, cache write 33, cache read 44) in 2 claims\n"),
        "{shown}"
    );

    // Another account cannot edit the total comment of mike. The text
    // says that the comment of the claim is on the issue, and why the
    // total is not updated (01M3ZRQY9F9P7DF187Q0PJDS30).
    std::fs::write(machine.bin.path().join("login"), "brett").unwrap();
    machine.ok("author", &["claim", "issue-12"]).await;
    machine.says("author", &[reply(&now(), "a3", OPUS, [5, 0, 0, 0])]);
    let why = "The total comment of #12 is not updated: the total comment is of the account \
               mike, and GitHub lets only that account edit it.";
    let out = machine.run("author", &["pr", "wait", "40"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains(&format!(
            "The comment of the claim of #12 is on the issue. {why}"
        )),
        "{}",
        text(&out.stderr)
    );
    let released = machine.ok("author", &["release", "issue-12"]).await;
    assert!(
        released.contains(&format!("riff put them on #12 as a comment. {why}")),
        "{released}"
    );
    let comments = machine.comments(12);
    assert_eq!(comments.lines().count(), 4, "{comments}");
    assert!(
        comments.contains("riff usage total of #12: 1,110 tokens"),
        "{comments}"
    );
}

async fn call(
    client: &RunningService<rmcp::RoleClient, ()>,
    tool: &str,
    args: serde_json::Value,
) -> String {
    let params = CallToolRequestParams::new(tool.to_owned())
        .with_arguments(args.as_object().unwrap().clone());
    let result = client.call_tool(params).await.unwrap();
    assert_ne!(result.is_error, Some(true), "{result:?}");
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The fake `gh` in `bin`, and the meter of a machine with the marks in
/// `marks`.
fn meter(bin: &Path, marks: &Path) -> Meter {
    let gh = bin.join("gh");
    std::fs::write(&gh, FAKE_GH).unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    Meter {
        dir: marks.to_owned(),
        gh: Gh::at(gh),
    }
}

#[tokio::test]
async fn the_release_tool_puts_the_tokens_of_its_claim_on_the_issue() {
    let (bin, marks) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let api = Api::new(&server().await);
    let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a1#issue-12"
        .parse()
        .unwrap();
    api.register(&me).await.unwrap();
    api.set_riff(&me, riff_core::wire::RiffState::Running)
        .await
        .unwrap();
    let tools = Tools::new(api.clone(), me).with_meter(Some(meter(bin.path(), marks.path())));
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move { tools.serve(server_io).await.unwrap().waiting().await });
    let client = ().serve(client_io).await.unwrap();

    let transcript = marks.path().join("a1.jsonl");
    riff::usage::saw(marks.path(), "a1", &transcript).unwrap();
    let claimed = call(&client, "claim", json!({ "item": "issue-12" })).await;
    assert!(claimed.contains("You hold issue-12"), "{claimed}");
    std::fs::write(&transcript, reply(&now(), "m1", OPUS, [1, 2, 3, 4])).unwrap();
    let released = call(&client, "release", json!({ "item": "issue-12" })).await;
    assert_eq!(
        released,
        "You released issue-12 in como-technologies/riff.\nThe claim of issue-12 took 10 tokens \
         (input 1, output 2, cache write 3, cache read 4). riff put them on #12 as a comment."
    );
    let comments = std::fs::read_to_string(bin.path().join("comments-12")).unwrap();
    assert!(
        comments.contains("riff usage: issue-12, work, session a1, "),
        "{comments}"
    );
}

#[test]
fn the_book_shows_how_to_see_the_tokens_of_an_issue() {
    let commands = book::commands_of_part("development.md", "See the tokens of an issue");
    for command in [
        "riff usage 12",
        "riff usage --wave \"Wave 3\"",
        "riff usage",
        "riff usage 12 --total",
    ] {
        assert!(
            commands.iter().any(|c| c == command),
            "{command}: {commands:?}"
        );
    }
    let part = book::part("development.md", "See the tokens of an issue");
    assert!(
        part.contains("public"),
        "the numbers are public on the issue"
    );
    book::each_is_real(
        &commands
            .into_iter()
            .filter(|c| !c.contains('"'))
            .collect::<Vec<_>>(),
    );
}
