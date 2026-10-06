//! The state of a session from facts (#424), against a real server: the
//! hooks write what a session does, the keep-alive carries it, the facts
//! of the forge make `waiting`, and a block wakes the lead, then wakes
//! it again, then tells the person.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use isolated::Isolated;
use riff::api::Api;
use riff::look::{self, Notifier, PullWatch};
use riff::pr::Gh;
use riff::top::Top;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{RiffOwner, RiffState, SessionInfo, SessionState, Waits};

async fn start_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    format!("http://{addr}")
}

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

/// The lead of mike and one worker of mike, in a running riff.
async fn riff(server: &str) -> (Api, SessionUri, SessionUri) {
    let api = Api::new(server);
    let lead = uri("riff://mike@pangolin/como-technologies/riff?session=l1");
    let worker = uri("riff://mike@pangolin/como-technologies/riff?session=w1#issue-12");
    for me in [&lead, &worker] {
        api.register(me).await.unwrap();
    }
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    (api, lead, worker)
}

/// `riff` in `dir` with this server.
fn riff_cmd(server: &str, dir: &Path, session: &str) -> assert_cmd::Command {
    let mut cmd = Isolated::shared().assert_riff();
    cmd.current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_HOME", dir)
        .env("RIFF_SESSION", session)
        .env_remove("RIFF_WORKER")
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .env_remove("CLAUDE_CODE_SESSION_ID");
    cmd
}

/// A script `name` in `dir` that runs `body`.
fn script(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

async fn info(api: &Api, lead: &SessionUri, me: &SessionUri) -> SessionInfo {
    let sessions = api.who(lead, false).await.unwrap();
    sessions
        .into_iter()
        .find(|s| s.uri.who() == me.who())
        .unwrap()
}

/// The board of `riff top`, with no color.
fn top(sessions: &[SessionInfo]) -> String {
    let running = RiffState::Running.into();
    let top = Top {
        pauses: &running,
        owner: &RiffOwner::NoSignIn,
        server: None,
        sessions,
        people: &[],
        issues: &Default::default(),
        repo: Some("como-technologies/riff"),
        show: &Default::default(),
        width: 200,
        fault: None,
        machines: &[],
    };
    anstream::adapter::strip_str(&top.view()).to_string()
}

/// The hooks write the facts, and the keep-alive carries them: `who`
/// shows the work with no `status` call (01M41FZNTPXQNCZ1S99HE42PYQ).
#[tokio::test(flavor = "multi_thread")]
async fn the_hooks_and_the_keep_alive_show_the_work() {
    let server = start_server().await;
    let (api, lead, worker) = riff(&server).await;
    api.claim(&worker, &repo(), "issue-12").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let input = r#"{"session_id":"x","tool_name":"Bash","tool_input":{"command":"just ci","description":"Run just ci"}}"#;
    riff_cmd(&server, dir.path(), "w1")
        .args(["hook", "tool"])
        .write_stdin(input)
        .assert()
        .success();

    // `riff mcp` reads the file at each keep-alive.
    let state = dir.path().join("state");
    let activity = riff::activity::read(&state, "w1", riff::activity::now_ms()).unwrap();
    assert_eq!(activity.tool.as_deref(), Some("Bash: Run just ci"));
    api.alive_with(&worker, Some(activity), None).await.unwrap();
    let _watch = api.watch(&worker).await.unwrap();
    let w1 = info(&api, &lead, &worker).await;
    assert_eq!(w1.state, Some(SessionState::Busy));
    let lines: Vec<String> = riff::state::detail(&w1, &|_| None)
        .into_iter()
        .map(|(line, _)| line)
        .collect();
    assert!(
        lines[1].starts_with("runs Bash: Run just ci for "),
        "{lines:?}"
    );

    // A riff tool is no work: the fact stays.
    let read = r#"{"tool_name":"mcp__riff__read","tool_input":{}}"#;
    riff_cmd(&server, dir.path(), "w1")
        .args(["hook", "tool", "--done"])
        .write_stdin(read)
        .assert()
        .success();
    let activity = riff::activity::read(&state, "w1", riff::activity::now_ms()).unwrap();
    assert_eq!(activity.tool.as_deref(), Some("Bash: Run just ci"));

    // The end of the turn.
    riff_cmd(&server, dir.path(), "w1")
        .args(["hook", "stop"])
        .write_stdin(r#"{"session_id":"x"}"#)
        .assert()
        .success();
    let activity = riff::activity::read(&state, "w1", riff::activity::now_ms()).unwrap();
    assert!(!activity.turn && activity.tool.is_none(), "{activity:?}");
}

/// The facts of the forge make `waiting`, with no `status` call
/// (01M41FZP2C4Z4J6WKRXZ5B31EH, 01M41FZP9A50CH4A2VX344DW49).
#[tokio::test(flavor = "multi_thread")]
async fn the_facts_of_the_forge_make_waiting() {
    let server = start_server().await;
    let (api, lead, worker) = riff(&server).await;
    api.claim(&worker, &repo(), "issue-12").await.unwrap();
    let _watch = api.watch(&worker).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let pulls = r#"[{"number":418,"headRefName":"worktree-issue-12","headRefOid":"1a2b","isDraft":false,"statusCheckRollup":[]}]"#;
    let gh = script(
        dir.path(),
        "gh",
        &format!("case \"$1\" in pr) echo '{pulls}' ;; *) echo '[]' ;; esac"),
    );
    let gh = Arc::new(Gh::at(gh));
    look::once(
        &api,
        &lead,
        &gh,
        Duration::from_secs(600),
        None,
        &mut PullWatch::default(),
    )
    .await
    .unwrap();
    let w1 = info(&api, &lead, &worker).await;
    assert_eq!(w1.state, Some(SessionState::Waiting));
    assert_eq!(w1.waits, Some(Waits::Verify { pull: 418 }));

    // The merge: the pull request is gone from the open list.
    let gh = Arc::new(Gh::at(script(dir.path(), "gh2", "echo '[]'")));
    look::once(
        &api,
        &lead,
        &gh,
        Duration::from_secs(600),
        None,
        &mut PullWatch::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        info(&api, &lead, &worker).await.state,
        Some(SessionState::Busy)
    );
}

/// A need is met when its issue is closed, or when the issue has a
/// comment `Merged in #`. Only an open need with no such comment makes
/// `waiting` (01M41FZP9A50CH4A2VX344DW49, 01M49HAW3NXNXNX02ETDZD3YCN).
#[tokio::test(flavor = "multi_thread")]
async fn a_need_with_a_merged_in_comment_is_met() {
    let server = start_server().await;
    let (api, lead, worker) = riff(&server).await;
    api.claim(&worker, &repo(), "issue-12").await.unwrap();
    let _watch = api.watch(&worker).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    // #12 needs #9 (closed: not in the open list), #10 (open, merged)
    // and #11 (open, with no comment).
    let look = |name: &str, issues: &str| {
        let gh = script(
            dir.path(),
            name,
            &format!("case \"$1\" in issue) echo '{issues}' ;; *) echo '[]' ;; esac"),
        );
        Arc::new(Gh::at(gh))
    };
    let gh = look(
        "gh",
        r#"[{"number":12,"body":"Needs: #9, #10, #11","comments":[]},
            {"number":10,"body":"","comments":[{"author":{"login":"m"},"body":"Merged in #517 (7b47efa)"}]},
            {"number":11,"body":"","comments":[{"author":{"login":"m"},"body":"Not merged in #518."}]}]"#,
    );
    look::once(
        &api,
        &lead,
        &gh,
        Duration::from_secs(600),
        None,
        &mut PullWatch::default(),
    )
    .await
    .unwrap();
    let w1 = info(&api, &lead, &worker).await;
    assert_eq!(w1.state, Some(SessionState::Waiting));
    assert_eq!(w1.waits, Some(Waits::Needs { issues: vec![11] }));

    // #11 gets its comment: each need is met.
    let gh = look(
        "gh2",
        r#"[{"number":12,"body":"Needs: #9, #10, #11","comments":[]},
            {"number":10,"body":"","comments":[{"body":"Merged in #517 (7b47efa)"}]},
            {"number":11,"body":"","comments":[{"body":"  Merged in #518 (1a2b3c4)"}]}]"#,
    );
    look::once(
        &api,
        &lead,
        &gh,
        Duration::from_secs(600),
        None,
        &mut PullWatch::default(),
    )
    .await
    .unwrap();
    let w1 = info(&api, &lead, &worker).await;
    assert_eq!(w1.state, Some(SessionState::Busy));
    assert_eq!(w1.waits, None);
}

/// Each unread message of `me` that the person posted, as `TO: BODY`.
/// The look posts as the person, so the lead sees it too.
async fn unread(api: &Api, me: &SessionUri) -> Vec<String> {
    api.inbox(me, None, false)
        .await
        .unwrap()
        .into_iter()
        .flat_map(|i| i.messages)
        .map(|c| c.message)
        .filter(|m| m.from.who().session().is_none())
        .map(|m| {
            let to: Vec<String> = m.to.iter().map(ToString::to_string).collect();
            format!("{}: {}", to.join(" or "), m.body)
        })
        .collect()
}

/// A fake `gh` in `dir` named `name`: `pulls` for `gh pr list`, and no
/// issue.
fn forge(dir: &Path, name: &str, pulls: &str) -> Arc<Gh> {
    let gh = script(
        dir,
        name,
        &format!("case \"$1\" in pr) echo '{pulls}' ;; *) echo '[]' ;; esac"),
    );
    Arc::new(Gh::at(gh))
}

/// One look of the lead with `gh` and `watch`.
async fn look_at(api: &Api, lead: &SessionUri, gh: &Arc<Gh>, watch: &mut PullWatch) {
    look::once(api, lead, gh, Duration::from_secs(600), None, watch)
        .await
        .unwrap();
}

/// A pull request with auto-merge on and a conflict tells the session
/// that holds its item, or the lead when no session holds it, one time
/// for each head (01M49Q30XMVRFX42YTM1PHX0RZ, 01M49Q31FASDM7CG3JEGPYCZB9).
#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_with_a_conflict_tells_its_holder_or_the_lead_once() {
    let server = start_server().await;
    let (api, lead, worker) = riff(&server).await;
    api.claim(&worker, &repo(), "issue-12").await.unwrap();
    api.inbox(&lead, None, false).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    // Each verify passed, so no pull request waits for a verify. #42 has
    // no auto-merge, and #43 is mergeable.
    let pull = |number, issue, head: &str, mergeable: &str, auto: &str| {
        format!(
            r#"{{"number":{number},"headRefName":"worktree-issue-{issue}","headRefOid":"{head}","isDraft":false,"statusCheckRollup":[{{"context":"riff/verify","state":"SUCCESS"}}],"mergeable":"{mergeable}","autoMergeRequest":{auto}}}"#
        )
    };
    let pulls = |head40: &str| {
        format!(
            "[{},{},{},{}]",
            pull(40, 12, head40, "CONFLICTING", r#"{"mergeMethod":"SQUASH"}"#),
            pull(
                41,
                13,
                "5e6f7a8b9c",
                "CONFLICTING",
                r#"{"mergeMethod":"SQUASH"}"#
            ),
            pull(42, 14, "0a0b0c0d0e", "CONFLICTING", "null"),
            pull(
                43,
                15,
                "1f1e1d1c1b",
                "MERGEABLE",
                r#"{"mergeMethod":"SQUASH"}"#
            ),
        )
    };
    let gh = forge(dir.path(), "gh", &pulls("1a2b3c4d5e"));
    let mut watch = PullWatch::default();

    look_at(&api, &lead, &gh, &mut watch).await;
    assert_eq!(
        unread(&api, &lead).await,
        [
            "repo=como-technologies/riff,claim=issue-12: Pull request #40 of issue-12 has a \
             conflict with the default branch at commit 1a2b3c4, so it cannot merge. Rebase it on \
             a fresh default branch, push it, and send a new verify request.",
            "session=l1: Pull request #41 of issue-13 has a conflict with the default branch at \
             commit 5e6f7a8, so it cannot merge. No session holds issue-13: give it to a free \
             session to rebase."
        ]
    );
    // The holder reads its message.
    let mine = unread(&api, &worker).await;
    assert!(
        mine.iter().any(|m| m.contains("#40 of issue-12")),
        "{mine:?}"
    );

    // The same state again: no message.
    look_at(&api, &lead, &gh, &mut watch).await;
    assert!(unread(&api, &lead).await.is_empty());

    // A new push that has a conflict again is a new state.
    let gh = forge(dir.path(), "gh2", &pulls("9d8c7b6a5f"));
    look_at(&api, &lead, &gh, &mut watch).await;
    let again = unread(&api, &lead).await;
    assert_eq!(again.len(), 1, "{again:?}");
    assert!(
        again[0].starts_with("repo=como-technologies/riff,claim=issue-12: ")
            && again[0].contains("at commit 9d8c7b6"),
        "{again:?}"
    );
}

/// A pull request that waits for a verify with no `verify-` claim for the
/// wait tells the lead one time. A claimed verify tells nothing
/// (01M49Q316RXNATJP587DWGDNCD, 01M49Q31FASDM7CG3JEGPYCZB9).
#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_with_no_verify_claim_tells_the_lead_once() {
    let server = start_server().await;
    let (api, lead, worker) = riff(&server).await;
    api.claim(&worker, &repo(), "verify-issue-13")
        .await
        .unwrap();
    api.inbox(&lead, None, false).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(
        dir.path(),
        "gh",
        r#"[{"number":40,"headRefName":"worktree-issue-12","headRefOid":"1a2b3c4d5e","isDraft":false,"statusCheckRollup":[],"mergeable":"MERGEABLE","autoMergeRequest":{}},
            {"number":41,"headRefName":"worktree-issue-13","headRefOid":"5e6f7a8b9c","isDraft":false,"statusCheckRollup":[],"mergeable":"MERGEABLE","autoMergeRequest":{}}]"#,
    );
    let mut watch = PullWatch::new(Duration::from_secs(1));

    // Not yet the wait: no message.
    look_at(&api, &lead, &gh, &mut watch).await;
    assert!(unread(&api, &lead).await.is_empty());

    tokio::time::sleep(Duration::from_millis(1100)).await;
    look_at(&api, &lead, &gh, &mut watch).await;
    assert_eq!(
        unread(&api, &lead).await,
        [
            "session=l1: Pull request #40 of issue-12 waits for a verify of commit 1a2b3c4 for 1 \
          minute, and no session claims verify-issue-12. Give the verify to a free session."
        ]
    );
    look_at(&api, &lead, &gh, &mut watch).await;
    assert!(unread(&api, &lead).await.is_empty(), "one time");
}

/// A block wakes the lead, then wakes it again, then `riff top` shows
/// "the lead gave no answer" and one desktop notification tells the
/// person. An answer ends the line at once, and the next work ends the
/// block
/// (01M41FZPGEK4TNPSM2051W4VMS, 01M41FZQ545HQ9Q75CSKX8HF8H,
/// 01M41FZQCHWY1YVGAZ60ZHJK21, 01M41FZQKZKW131Z8822G31T5G).
#[tokio::test(flavor = "multi_thread")]
async fn a_block_with_no_answer_wakes_the_lead_again_then_tells_the_person() {
    let server = start_server().await;
    let (api, lead, worker) = riff(&server).await;
    api.claim(&worker, &repo(), "issue-12").await.unwrap();
    let _watch = api.watch(&worker).await.unwrap();
    api.inbox(&lead, None, false).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let gh = Arc::new(Gh::at(script(dir.path(), "gh", "echo '[]'")));
    let shown = dir.path().join("shown");
    let notify = script(
        dir.path(),
        "notify-send",
        &format!("echo \"$@\" >> {}", shown.display()),
    );
    let notifier = Notifier { program: notify };
    let wake = Duration::from_secs(1);
    let look = || async {
        let mut watch = PullWatch::default();
        look::once(&api, &lead, &gh, wake, Some(&notifier), &mut watch).await
    };
    let bodies = |inbox: Vec<riff::api::Inbox>| -> Vec<String> {
        inbox
            .into_iter()
            .flat_map(|i| i.messages)
            .map(|c| c.message.body)
            .collect()
    };

    // One command: the block, and the first wake of the lead.
    assert_eq!(
        api.blocked(&worker, "which design?").await.unwrap(),
        riff::api::Told::Lead
    );
    let first = bodies(api.inbox(&lead, None, false).await.unwrap());
    assert_eq!(first, ["blocked: which design?"]);
    assert_eq!(
        info(&api, &lead, &worker).await.state,
        Some(SessionState::Blocked)
    );

    // No answer for the wake time: the second wake of the lead.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(look().await.unwrap().is_empty());
    let second = bodies(api.inbox(&lead, None, false).await.unwrap());
    assert_eq!(
        second,
        ["blocked: w1 (issue-12) has no answer after 1 minute: which design?"]
    );

    // No answer again: unanswered, one notification, and the line of
    // `riff top`.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let unanswered = look().await.unwrap();
    assert_eq!(unanswered.len(), 1);
    let sessions = api.who(&lead, false).await.unwrap();
    let board = top(&sessions);
    let line = board
        .lines()
        .find(|l| l.starts_with("blocked  w1 issue-12, "));
    let line = line.unwrap_or_else(|| panic!("{board}"));
    assert!(
        line.ends_with("s, the lead gave no answer: which design?"),
        "{board}"
    );
    assert!(look().await.unwrap().is_empty(), "one time");
    let lines = std::fs::read_to_string(&shown).unwrap();
    assert_eq!(
        lines,
        "--app-name=riff riff: the lead gave no answer w1 (issue-12) is blocked: which design?\n"
    );

    // The answer ends the line at once, before the next work. The
    // session stays blocked until that work.
    api.tell(&lead, "w1", "take the first one").await.unwrap();
    let w1 = info(&api, &lead, &worker).await;
    assert_eq!(w1.state, Some(SessionState::Blocked));
    let board = top(&[w1]);
    assert!(board.contains("blocked  w1 issue-12, "), "{board}");
    assert!(!board.contains("the lead gave no answer"), "{board}");

    // Then work: the block ends.
    let work = riff_core::wire::Activity {
        tool: Some("Edit".into()),
        turn: true,
        secs: 0,
    };
    api.alive_with(&worker, Some(work), None).await.unwrap();
    let w1 = info(&api, &lead, &worker).await;
    assert_eq!(w1.state, Some(SessionState::Busy));
    assert!(!top(&[w1]).contains("the lead gave no answer"));
}

/// No desktop: no notification, and nothing fails.
#[test]
fn a_machine_with_no_desktop_gets_no_notification() {
    let notifier = Notifier {
        program: "/no/such/notify-send".into(),
    };
    assert!(!notifier.here());
}

/// `riff lead blocked` shows and sets the wake time and the notification
/// (01M41FZQ545HQ9Q75CSKX8HF8H, 01M41FZQKZKW131Z8822G31T5G).
#[tokio::test(flavor = "multi_thread")]
async fn riff_lead_blocked_shows_and_sets_the_settings() {
    let server = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let show = |args: &[&str]| {
        let out = riff_cmd(&server, dir.path(), "l1")
            .args(args)
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let shown = show(&["lead", "blocked"]);
    assert!(shown.starts_with("lead.wake  15  ("), "{shown}");
    assert!(shown.contains("\nlead.notify  true  ("), "{shown}");
    let set = show(&["lead", "blocked", "--wake", "30", "--notify", "off"]);
    assert!(set.starts_with("lead.wake  30  ("), "{set}");
    assert!(set.contains("\nlead.notify  false  ("), "{set}");
    assert!(set.contains("riff lead blocked --notify on"), "{set}");
}

/// `riff blocked REASON` sets the block and wakes the lead, in one
/// command (01M41FZPGEK4TNPSM2051W4VMS).
#[tokio::test(flavor = "multi_thread")]
async fn riff_blocked_sets_the_block_and_tells_the_lead() {
    let server = start_server().await;
    let (api, lead, worker) = riff(&server).await;
    api.inbox(&lead, None, false).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
    };
    git(&["init", "-q"]);
    git(&[
        "remote",
        "add",
        "origin",
        "https://github.com/como-technologies/riff.git",
    ]);
    let out = riff_cmd(&server, dir.path(), "w1")
        .args(["blocked", "which", "design?"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let said = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        said,
        "You are blocked: which design? The lead has the reason.\n"
    );
    let block = info(&api, &lead, &worker).await.blocked.unwrap();
    assert_eq!(block.reason, "which design?");
    let inbox = api.inbox(&lead, None, false).await.unwrap();
    let bodies: Vec<String> = inbox
        .into_iter()
        .flat_map(|i| i.messages)
        .map(|c| c.message.body)
        .collect();
    assert_eq!(bodies, ["blocked: which design?"]);
}
