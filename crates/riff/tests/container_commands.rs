//! #1733: the riff commands in a container session, with `RIFF_SOCKET`
//! and no `RIFF_SERVER`, over a fake riff daemon on the socket. Each
//! command that asks riff-server goes through the socket of the session,
//! and a step of the host refuses at once ([`riff::reach::COMMANDS`]).
//! Nothing listens at the riff-server of the machine: a command that
//! called it would say `cannot reach riff-server`.

use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use isolated::Isolated;
use riff::reach::{COMMANDS, Way};
use serde_json::{Value, json};

/// Each ask that a fake daemon got, as JSON. The count of the calls of
/// `gh` is not in it.
type Asked = Arc<Mutex<Vec<Value>>>;

/// A fake riff daemon on a socket in a temp folder. For each `server`
/// ask, `reply` gives the answer to the call, or the text of a refusal.
/// It answers each other ask as the daemon does for a session that is not
/// the lead. The folder holds the socket: keep it until the end of the
/// test.
fn daemon(
    reply: impl Fn(&Value) -> Result<Value, String> + Send + 'static,
) -> (tempfile::TempDir, String, Asked) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("riff.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let asked = Asked::default();
    let log = asked.clone();
    std::thread::spawn(move || {
        use std::io::{BufRead, Write};
        for conn in listener.incoming().flatten() {
            let mut line = String::new();
            let _ = std::io::BufReader::new(&conn).read_line(&mut line);
            let Ok(ask) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let say = match ask["ask"].as_str() {
                Some("gh_calls") => continue,
                Some("server") => match reply(&ask["call"]) {
                    Ok(reply) => json!({"say": "served", "reply": reply}),
                    Err(error) => json!({"say": "failed", "error": error}),
                },
                Some("caller") => {
                    json!({"say": "caller", "lead": false, "role": null, "role_unknown": false})
                }
                Some("watch" | "follow") => json!({"say": "line", "text": "a wake"}),
                Some("denied") => json!({"say": "posted", "count": 0}),
                Some("prompt" | "idle") => json!({"say": "done"}),
                _ => json!({"say": "failed", "error": "the fake daemon"}),
            };
            log.lock().unwrap().push(ask);
            let _ = (&conn).write_all(format!("{say}\n").as_bytes());
        }
    });
    (dir, path.to_string_lossy().into_owned(), asked)
}

/// The names of the `server` calls of `asked`, in order.
fn calls(asked: &Asked) -> Vec<String> {
    asked
        .lock()
        .unwrap()
        .iter()
        .filter(|ask| ask["ask"] == "server")
        .map(|ask| ask["call"]["call"].as_str().unwrap().to_owned())
        .collect()
}

/// The `server` call `name` of `asked`.
fn call(asked: &Asked, name: &str) -> Value {
    let asked = asked.lock().unwrap();
    let found = asked.iter().find(|ask| ask["call"]["call"] == name);
    found.unwrap_or_else(|| panic!("no call {name}"))["call"].clone()
}

/// The fake forge of pull request 40 "Show the wave" of issue 12, whose
/// Gate passed on its head 1a2b3c4d, and of the milestone Wave 3. Each
/// other call prints `[]`. The script logs each call in `gh.log`.
const GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "gh $*" >> "$dir/gh.log"
case "$*" in *'--body-file -'*) cat > /dev/null ;; esac
case "$*" in
*'pr view 40 --json headRefOid,body'*) printf '%s' '{"headRefOid":"1a2b3c4d","title":"Show the wave","autoMergeRequest":null,"body":"Closes #12\n\nShow the wave.\n\nIssue: #12\nMilestone: Wave 3\n"}' ;;
*'pr view 40 --json state,mergeCommit'*) echo '{"state":"MERGED","mergeCommit":{"oid":"9f8e7d6c"}}' ;;
*'pr view 40 --json body'*) printf '%s' '{"body":"Show the wave.\n\nIssue: #12\n"}' ;;
*'pr comment 40 --body-file -'*) echo 'https://github.com/como-technologies/riff/pull/40#issuecomment-7' ;;
*'/statuses/'*) echo '{}' ;;
*'api repos/como-technologies/riff') echo '{"default_branch":"main"}' ;;
*'/rules/branches/main?per_page=100'*) echo '[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"Gate"},{"context":"riff/verify"}]}}]' ;;
*'/check-runs?check_name=Gate'*) echo '{"total_count":1,"check_runs":[{"name":"Gate","status":"completed","conclusion":"success"}]}' ;;
*'issue view 12 --json body'*) printf '%s' '{"body":"Make it.\n\n## Done when:\n\n- A test.\n"}' ;;
*milestones*) echo '[{"number":3,"title":"Wave 3","open_issues":1,"closed_issues":1}]' ;;
*) echo '[]' ;;
esac
exit 0
"#;

/// The longest run of one command: a command that waits for more, for
/// example the relay, ends then.
const WAIT: Duration = Duration::from_secs(20);

/// A clone of a GitHub repository with a release cycle, and the fake
/// `gh`, in the environment of one test.
struct Container {
    env: Isolated,
    repo: tempfile::TempDir,
    bin: tempfile::TempDir,
}

impl Container {
    fn new() -> Container {
        let repo = tempfile::tempdir().unwrap();
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
                .current_dir(repo.path())
                .status();
            assert!(status.unwrap().success());
        }
        std::fs::write(repo.path().join("riff.toml"), "[release]\n").unwrap();
        std::fs::write(repo.path().join("result.md"), "1. The wave shows.\n").unwrap();
        let bin = tempfile::tempdir().unwrap();
        isolated::write_program(bin.path().join("gh"), GH);
        Container {
            env: Isolated::new(),
            repo,
            bin,
        }
    }

    /// `riff ARGS` in the container: the socket of the session, the
    /// session ID, the fake `gh`, and no `RIFF_SERVER`. `None` when it
    /// runs for longer than [`WAIT`]: riff ends it then.
    async fn riff(&self, socket: &str, args: &[&str]) -> Option<Output> {
        assert!(
            std::path::Path::new(socket).starts_with(std::env::temp_dir()),
            "RIFF_SOCKET of a test is not in a temp folder: {socket}"
        );
        let path = format!(
            "{}:{}",
            self.bin.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = self.env.tokio_riff();
        cmd.args(args)
            .current_dir(self.repo.path())
            .env("RIFF_SOCKET", socket)
            .env("RIFF_SESSION", "c1")
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "thelio")
            .env("PATH", path)
            .env_remove("RIFF_SERVER")
            .stdin(Stdio::null())
            .kill_on_drop(true);
        tokio::time::timeout(WAIT, cmd.output())
            .await
            .ok()
            .map(Result::unwrap)
    }

    /// `riff ARGS` in the container, which ends in [`WAIT`].
    async fn run(&self, socket: &str, args: &[&str]) -> Output {
        let out = self.riff(socket, args).await;
        out.unwrap_or_else(|| panic!("riff {} runs on", args.join(" ")))
    }

    /// The calls of the fake `gh`.
    fn gh_log(&self) -> String {
        std::fs::read_to_string(self.bin.path().join("gh.log")).unwrap_or_default()
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

const LEAD: &str = "riff://mike@thelio/como-technologies/riff?session=c1";
const AUTHOR: &str = "riff://mike@pangolin/como-technologies/riff?session=a1&claim=issue-12";

/// What the daemon reads for `riff who` and `riff top`: the riff runs,
/// and has the lead and the author of issue-12.
fn who_facts() -> Value {
    json!({
        "pauses": {"state": "running"},
        "who": {
            "owner": {"kind": "owner", "user": "mike", "email": "mike@example.com"},
            "sessions": [
                {"uri": format!("{LEAD}&lead=1"), "live": true, "container": true},
                {"uri": AUTHOR, "live": true, "status": {"step": "writing the tests", "age_secs": 60}},
            ],
        },
    })
}

/// #1733: `riff top --once` in the container prints the table of the
/// sessions. The daemon reads the sessions and the holds for it.
#[tokio::test(flavor = "multi_thread")]
async fn top_once_prints_the_table_through_the_socket() {
    let container = Container::new();
    let (_dir, socket, asked) = daemon(|call| match call["call"].as_str() {
        Some("who_facts") => Ok(who_facts()),
        Some("holds") => Ok(json!({})),
        _ => Err("no such call".into()),
    });
    let out = container
        .run(&socket, &["top", "--once", "--color", "never"])
        .await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let table = text(&out.stdout);
    assert!(table.contains("c1  container"), "{table}");
    assert!(table.contains("a1  busy"), "{table}");
    assert!(table.contains("writing the tests"), "{table}");
    assert!(
        !text(&out.stderr).contains("cannot reach"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(calls(&asked), ["who_facts", "holds"]);
    assert_eq!(call(&asked, "who_facts")["all"], false);
    assert_eq!(call(&asked, "holds")["thread"], "como-technologies/riff");
}

/// #1733: `riff read` in the container reads the messages of the session
/// through the socket. The daemon joins the thread of the repository and
/// gives the text.
#[tokio::test(flavor = "multi_thread")]
async fn read_reads_the_messages_through_the_socket() {
    let container = Container::new();
    let inbox = "como-technologies/riff\n  mike@pangolin:riff#issue-12 (a1): the tests pass";
    let (_dir, socket, asked) = daemon(move |_| Ok(Value::String(inbox.into())));
    let out = container.run(&socket, &["read"]).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stderr), "");
    assert_eq!(text(&out.stdout), format!("{inbox}\n"));

    let out = container
        .run(&socket, &["read", "--thread", "dm:a1", "--all"])
        .await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    let asked: Vec<Value> = asked
        .lock()
        .unwrap()
        .iter()
        .map(|a| a["call"].clone())
        .collect();
    assert_eq!(
        asked,
        [
            json!({"call": "read", "thread": null, "all": false}),
            json!({"call": "read", "thread": "dm:a1", "all": true}),
        ]
    );
}

/// The daemon of a verify: it reads the sessions, with the author that
/// holds issue-12, and the post wakes it.
fn verify_daemon(post: Result<Value, String>) -> (tempfile::TempDir, String, Asked) {
    daemon(move |call| match call["call"].as_str() {
        Some("who") => Ok(who_facts()["who"].clone()),
        Some("fact") => Ok(Value::Null),
        Some("post") => post.clone(),
        _ => Err("no such call".into()),
    })
}

/// #1733: `riff verify fail` in the container comments on GitHub, sets
/// the status, and posts the result to the session of the issue through
/// the socket.
#[tokio::test(flavor = "multi_thread")]
async fn verify_fail_posts_to_the_session_of_the_issue_through_the_socket() {
    let container = Container::new();
    let woke = "Woke mike@pangolin:riff#issue-12 (a1).";
    let (_dir, socket, asked) = verify_daemon(Ok(Value::String(woke.into())));
    let args = [
        "verify",
        "fail",
        "40",
        "--file",
        "result.md",
        "--commit",
        "1a2b3c4d",
    ];
    let out = container.run(&socket, &args).await;
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stdout).ends_with(&format!("{woke}\n")),
        "{}",
        text(&out.stdout)
    );
    let log = container.gh_log();
    assert_eq!(log.matches("gh pr comment 40").count(), 1, "{log}");
    assert!(log.contains("-f state=failure"), "{log}");

    assert_eq!(calls(&asked), ["fact", "who", "post"]);
    assert_eq!(
        call(&asked, "fact"),
        json!({"call": "fact", "issue": 12, "number": 40, "state": "failed"})
    );
    let post = call(&asked, "post");
    assert_eq!(post["thread"], "como-technologies/riff");
    assert_eq!(post["to"], json!([{"claim": "issue-12"}]));
    assert_eq!(post["kind"], "message");
    let body = post["body"].as_str().unwrap();
    assert!(
        body.starts_with("verify result: FAIL for issue-12, PR #40, commit 1a2b3c4d."),
        "{body}"
    );
    assert!(body.ends_with("1. The wave shows."), "{body}");
}

/// #1733: when the post of `riff verify` fails after its part on GitHub,
/// the error says that the comment and the status are on GitHub.
#[tokio::test(flavor = "multi_thread")]
async fn verify_says_which_part_was_done_when_the_post_fails() {
    let container = Container::new();
    let (_dir, socket, _) = verify_daemon(Err("riff-server is away".into()));
    let args = [
        "verify",
        "fail",
        "40",
        "--file",
        "result.md",
        "--commit",
        "1a2b3c4d",
    ];
    let out = container.run(&socket, &args).await;
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    let done = riff::pr::Reported {
        issue: 12,
        commit: "1a2b3c4d".into(),
        url: "https://github.com/como-technologies/riff/pull/40#issuecomment-7".into(),
    };
    let line =
        riff::text::verify_not_posted(riff::pr::Verdict::Fail, 40, &done, "riff-server is away");
    assert!(text(&out.stderr).contains(&line), "{}", text(&out.stderr));
    assert!(container.gh_log().contains("gh pr comment 40"));
}

/// #1733: the Sessions and the Waves tabs of the TUI look through the
/// socket in a container: the sessions, the state of the riff and the
/// holds of each board.
#[tokio::test(flavor = "multi_thread")]
async fn the_look_of_the_tui_goes_through_the_socket() {
    let (_dir, socket, asked) = daemon(|call| match call["call"].as_str() {
        Some("who_facts") => Ok(who_facts()),
        Some("holds") => Ok(json!({"issue-13": {"reason": "a reading", "at_ms": 0}})),
        _ => Err("no such call".into()),
    });
    let reach = riff::reach::Reach::Daemon(socket.into());
    // Nothing listens at the riff-server of this client.
    let api = riff_client::api::Api::new("http://127.0.0.1:9");
    let me = LEAD.parse().unwrap();
    let boards = ["como-technologies/riff".to_owned()];
    let look = riff::tui::look(&api, &me, &reach, &boards).await.unwrap();
    assert_eq!(look.who.sessions.len(), 2);
    assert_eq!(look.holds["como-technologies/riff"].len(), 1);
    assert_eq!(calls(&asked), ["who_facts", "holds"]);
}

/// The rows of [`COMMANDS`] that this test does not run, and where a
/// test shows their way: each asks riff-server only after its steps on
/// GitHub, or after a hook keeps its input.
const RUN_ELSEWHERE: [(&[&str], &str); 3] = [
    (
        &["release", "cut", "3.4.0"],
        "release_cycle::cut_in_the_lead_container_tells_the_chat_through_the_socket",
    ),
    (
        &["release", "status"],
        "release_cycle: the holds of the release come through the socket",
    ),
    (
        &["relay", "denied"],
        "session_relay: the relay sends the kept refusals on the socket",
    ),
];

/// #1733: each riff command of [`COMMANDS`] in a container session. A
/// command with [`Way::Socket`] asks the riff daemon on the socket, and
/// never the riff-server of the machine. A command with [`Way::Host`]
/// refuses, and asks nothing. A command with [`Way::Local`] asks no
/// riff-server: this test does not run it, as it can act on the
/// machine, for example `riff daemon install`.
#[tokio::test(flavor = "multi_thread")]
async fn each_command_in_a_container_takes_the_socket_or_refuses() {
    let container = Container::new();
    for (args, _) in RUN_ELSEWHERE {
        let row = COMMANDS.iter().find(|row| row.args == args);
        assert_eq!(row.map(|row| row.way), Some(Way::Socket), "{args:?}");
    }
    let mut faults = Vec::new();
    for row in COMMANDS {
        let elsewhere = RUN_ELSEWHERE.iter().any(|(args, _)| *args == row.args);
        // `riff init` runs on Linux only.
        let init = row.args[0] == "init" && !cfg!(target_os = "linux");
        if row.way == Way::Local || elsewhere || init {
            continue;
        }
        let (_dir, socket, asked) = daemon(|call| Err(format!("the fake daemon refuses {call}")));
        let words = format!("riff {}", row.args.join(" "));
        let out = container.riff(&socket, row.args).await;
        let said = out
            .as_ref()
            .map(|out| format!("{}{}", text(&out.stdout), text(&out.stderr)))
            .unwrap_or_default();
        let asked = asked.lock().unwrap().len();
        if said.contains("cannot reach riff-server") || said.contains("127.0.0.1:7878") {
            faults.push(format!(
                "{words} asks the riff-server of the machine: {said}"
            ));
        }
        match row.way {
            Way::Socket if asked == 0 => faults.push(format!("{words} asks no socket: {said}")),
            Way::Host if asked > 0 => faults.push(format!("{words} asks the socket")),
            Way::Host if out.as_ref().is_none_or(|out| out.status.success()) => {
                faults.push(format!("{words} does not refuse: {said}"));
            }
            _ => {}
        }
    }
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

/// The text of the part `### heading` of the book page `name`, up to the
/// next heading of its level or above.
fn book_part(name: &str, heading: &str) -> String {
    let page = crate::book::page(name);
    let start = page
        .find(&format!("\n### {heading}\n"))
        .unwrap_or_else(|| panic!("{name} has no \"### {heading}\""));
    let rest = &page[start + 1..];
    let (mut fenced, mut at) = (false, 0);
    for line in rest.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced && at > 0 && (line.starts_with("## ") || line.starts_with("### ")) {
            return rest[..at].to_owned();
        }
        at += line.len();
    }
    rest.to_owned()
}

/// #1733: the list of the commands of the lead container in the book
/// names each command that takes the socket, and each command that
/// refuses there. `riff relay` is the riff of the container itself: no
/// person runs it.
#[test]
fn the_book_names_each_command_of_the_lead_container() {
    let part = book_part(
        "how-it-works.md",
        "The riff commands in the container of the lead",
    )
    .replace('\n', " ");
    let mut missing: Vec<&str> = COMMANDS
        .iter()
        .filter(|row| row.way != Way::Local && row.args[0] != "relay")
        .map(|row| row.args[0])
        .filter(|name| !part.contains(&format!("`riff {name}")))
        .collect();
    missing.dedup();
    assert!(missing.is_empty(), "the book names no riff {missing:?}");
}
