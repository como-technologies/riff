//! The state of a session from facts (#424), against a real server: the
//! hooks write what a session does, the keep-alive carries it, the facts
//! of the forge make `waiting`, and a block wakes the lead, then wakes
//! it again, then tells the person.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use isolated::Isolated;
use riff::look::{self, MainWatch, Notifier, PullWatch};
use riff::pr::Gh;
use riff::top::Top;
use riff_client::api::Api;
use riff_client::sessions::Sessions;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{RiffOwner, RiffState, SessionInfo, SessionState, Waits};

async fn start_server() -> crate::common::Signed {
    crate::common::start_signed().await
}

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

/// A server with no sign-in, for the looks that post as the person: `look::once`
/// takes one client, and a token acts as one caller only.
async fn start_open() -> (String, (Api, Api, SessionUri, SessionUri)) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    let api = Api::new(&url);
    let seeded = seed(api.clone(), api.clone(), api).await;
    (url, seeded)
}

/// The lead of mike and one worker of mike, in a running riff.
async fn riff(server: &crate::common::Signed) -> (Api, Api, SessionUri, SessionUri) {
    let al = server.api("mike", Some("l1"));
    let aw = server.api("mike", Some("w1"));
    let ap = server.api("mike", None);
    seed(al, aw, ap).await
}

async fn seed(al: Api, aw: Api, ap: Api) -> (Api, Api, SessionUri, SessionUri) {
    let lead = uri("riff://mike@pangolin/como-technologies/riff?session=l1");
    let worker = uri("riff://mike@pangolin/como-technologies/riff?session=w1#issue-12");
    al.register(&lead).await.unwrap();
    aw.register(&worker).await.unwrap();
    crate::common::name_lead(&ap, &lead).await;
    al.set_riff(&lead, RiffState::Running).await.unwrap();
    (al, aw, lead, worker)
}

/// `riff` in `dir` with this server.
fn riff_cmd(server: &str, dir: &Path, session: &str) -> assert_cmd::Command {
    crate::common::sign_in(server, dir, "mike");
    let mut cmd = Isolated::shared().assert_riff();
    cmd.current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_HOME", dir)
        .env("RIFF_SESSION", session)
        .env_remove("RIFF_WORKER")
        .env_remove("CLAUDE_CODE_SESSION_ID");
    cmd
}

/// A script `name` in `dir` that runs `body`.
fn script(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    isolated::write_program(&path, format!("#!/bin/sh\n{body}\n"));
    path
}

async fn info(al: &Api, lead: &SessionUri, me: &SessionUri) -> SessionInfo {
    let sessions = al.who(lead, false).await.unwrap();
    sessions
        .into_iter()
        .find(|s| s.uri.who() == me.who())
        .unwrap()
}

/// The board of `riff top`, with no color.
fn top(sessions: &[SessionInfo]) -> String {
    let sessions = &Sessions::new(sessions.to_vec(), Vec::new());
    let running = RiffState::Running.into();
    let top = Top {
        pauses: &running,
        owner: &RiffOwner::NoSignIn,
        server: None,
        sessions,
        people: &[],
        issues: &Default::default(),
        holds: &Default::default(),
        repo: Some("como-technologies/riff"),
        show: &Default::default(),
        width: 200,
        fault: None,
        machines: &[],
    };
    anstream::adapter::strip_str(&top.view()).to_string()
}

/// The facts of the forge make `waiting`, with no `status` call
/// (01M41FZP2C4Z4J6WKRXZ5B31EH, 01M41FZP9A50CH4A2VX344DW49).
#[tokio::test(flavor = "multi_thread")]
async fn the_facts_of_the_forge_make_waiting() {
    let server = start_server().await;
    let (al, aw, lead, worker) = riff(&server).await;
    aw.claim(&worker, &repo(), "issue-12").await.unwrap();
    let _watch = aw.watch(&worker).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let pulls = r#"[{"number":418,"headRefName":"worktree-issue-12","headRefOid":"1a2b","isDraft":false,"statusCheckRollup":[]}]"#;
    let gh = script(
        dir.path(),
        "gh",
        &format!("case \"$1\" in pr) echo '{pulls}' ;; *) echo '[]' ;; esac"),
    );
    let gh = Arc::new(Gh::at(gh));
    look::once(
        &al,
        &lead,
        &gh,
        Duration::from_secs(600),
        None,
        &mut PullWatch::default(),
        &mut MainWatch::default(),
    )
    .await
    .unwrap();
    let w1 = info(&al, &lead, &worker).await;
    assert_eq!(w1.state, Some(SessionState::Waiting));
    assert_eq!(w1.waits, Some(Waits::Verify { pull: 418 }));

    // The merge: the pull request is gone from the open list.
    let gh = Arc::new(Gh::at(script(dir.path(), "gh2", "echo '[]'")));
    look::once(
        &al,
        &lead,
        &gh,
        Duration::from_secs(600),
        None,
        &mut PullWatch::default(),
        &mut MainWatch::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        info(&al, &lead, &worker).await.state,
        Some(SessionState::Busy)
    );
}

/// A need is met when its issue is closed, or when the issue has a
/// comment `Merged in #`. Only an open need with no such comment makes
/// `waiting` (01M41FZP9A50CH4A2VX344DW49, 01M49HAW3NXNXNX02ETDZD3YCN).
#[tokio::test(flavor = "multi_thread")]
async fn a_need_with_a_merged_in_comment_is_met() {
    let server = start_server().await;
    let (al, aw, lead, worker) = riff(&server).await;
    aw.claim(&worker, &repo(), "issue-12").await.unwrap();
    let _watch = aw.watch(&worker).await.unwrap();
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
        &al,
        &lead,
        &gh,
        Duration::from_secs(600),
        None,
        &mut PullWatch::default(),
        &mut MainWatch::default(),
    )
    .await
    .unwrap();
    let w1 = info(&al, &lead, &worker).await;
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
        &al,
        &lead,
        &gh,
        Duration::from_secs(600),
        None,
        &mut PullWatch::default(),
        &mut MainWatch::default(),
    )
    .await
    .unwrap();
    let w1 = info(&al, &lead, &worker).await;
    assert_eq!(w1.state, Some(SessionState::Busy));
    assert_eq!(w1.waits, None);
}

/// Each unread message of `me` that the person posted, as `TO: BODY`.
/// The look posts as the person, so the lead sees it too.
async fn unread(al: &Api, me: &SessionUri) -> Vec<String> {
    al.inbox(me, None, false)
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
async fn look_at(al: &Api, lead: &SessionUri, gh: &Arc<Gh>, watch: &mut PullWatch) {
    look::once(
        al,
        lead,
        gh,
        Duration::from_secs(600),
        None,
        watch,
        &mut MainWatch::default(),
    )
    .await
    .unwrap();
}

/// A pull request with auto-merge on and a conflict tells the session
/// that holds its item, or the lead when no session holds it, one time
/// for each head (01M49Q30XMVRFX42YTM1PHX0RZ, 01M49Q31FASDM7CG3JEGPYCZB9).
#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_with_a_conflict_tells_its_holder_or_the_lead_once() {
    let (_server, (al, aw, lead, worker)) = start_open().await;
    aw.claim(&worker, &repo(), "issue-12").await.unwrap();
    al.inbox(&lead, None, false).await.unwrap();
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

    look_at(&al, &lead, &gh, &mut watch).await;
    assert_eq!(
        unread(&al, &lead).await,
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
    let mine = unread(&aw, &worker).await;
    assert!(
        mine.iter().any(|m| m.contains("#40 of issue-12")),
        "{mine:?}"
    );

    // The same state again: no message.
    look_at(&al, &lead, &gh, &mut watch).await;
    assert!(unread(&al, &lead).await.is_empty());

    // A new push that has a conflict again is a new state.
    let gh = forge(dir.path(), "gh2", &pulls("9d8c7b6a5f"));
    look_at(&al, &lead, &gh, &mut watch).await;
    let again = unread(&al, &lead).await;
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
    let (_server, (al, aw, lead, worker)) = start_open().await;
    aw.claim(&worker, &repo(), "verify-issue-13").await.unwrap();
    al.inbox(&lead, None, false).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(
        dir.path(),
        "gh",
        r#"[{"number":40,"headRefName":"worktree-issue-12","headRefOid":"1a2b3c4d5e","isDraft":false,"statusCheckRollup":[],"mergeable":"MERGEABLE","autoMergeRequest":{}},
            {"number":41,"headRefName":"worktree-issue-13","headRefOid":"5e6f7a8b9c","isDraft":false,"statusCheckRollup":[],"mergeable":"MERGEABLE","autoMergeRequest":{}}]"#,
    );
    let mut watch = PullWatch::new(Duration::from_secs(1));

    // Not yet the wait: no message.
    look_at(&al, &lead, &gh, &mut watch).await;
    assert!(unread(&al, &lead).await.is_empty());

    tokio::time::sleep(Duration::from_millis(1100)).await;
    look_at(&al, &lead, &gh, &mut watch).await;
    assert_eq!(
        unread(&al, &lead).await,
        [
            "session=l1: Pull request #40 of issue-12 waits for a verify of commit 1a2b3c4 for 1 \
          minute, and no session claims verify-issue-12. Give the verify to a free session."
        ]
    );
    look_at(&al, &lead, &gh, &mut watch).await;
    assert!(unread(&al, &lead).await.is_empty(), "one time");
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
    let (al, aw, lead, worker) = riff(&server).await;
    aw.claim(&worker, &repo(), "issue-12").await.unwrap();
    let _watch = aw.watch(&worker).await.unwrap();
    al.inbox(&lead, None, false).await.unwrap();
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
        let (mut watch, mut main) = (PullWatch::default(), MainWatch::default());
        look::once(
            &al,
            &lead,
            &gh,
            wake,
            Some(&notifier),
            &mut watch,
            &mut main,
        )
        .await
    };
    let bodies = |inbox: Vec<riff_client::api::Inbox>| -> Vec<String> {
        inbox
            .into_iter()
            .flat_map(|i| i.messages)
            .map(|c| c.message.body)
            .collect()
    };

    // One command: the block, and the first wake of the lead.
    assert_eq!(
        aw.blocked(&worker, "which design?").await.unwrap(),
        riff_client::api::Told::Lead
    );
    let first = bodies(al.inbox(&lead, None, false).await.unwrap());
    assert_eq!(first, ["blocked: which design?"]);
    assert_eq!(
        info(&al, &lead, &worker).await.state,
        Some(SessionState::Blocked)
    );

    // No answer for the wake time: the second wake of the lead.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(look().await.unwrap().is_empty());
    let second = bodies(al.inbox(&lead, None, false).await.unwrap());
    assert_eq!(
        second,
        ["blocked: w1 (issue-12) has no answer after 1 minute: which design?"]
    );

    // No answer again: unanswered, one notification, and the line of
    // `riff top`.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let unanswered = look().await.unwrap();
    assert_eq!(unanswered.len(), 1);
    let sessions = al.who(&lead, false).await.unwrap();
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
    al.tell(&lead, "w1", "take the first one").await.unwrap();
    let w1 = info(&al, &lead, &worker).await;
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
    aw.alive_with(&worker, Some(work), None).await.unwrap();
    let w1 = info(&al, &lead, &worker).await;
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

/// `riff blocked REASON` sets the block and wakes the lead, in one
/// command (01M41FZPGEK4TNPSM2051W4VMS).
#[tokio::test(flavor = "multi_thread")]
async fn riff_blocked_sets_the_block_and_tells_the_lead() {
    let server = start_server().await;
    let (al, _, lead, worker) = riff(&server).await;
    al.inbox(&lead, None, false).await.unwrap();
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
    let block = info(&al, &lead, &worker).await.blocked.unwrap();
    assert_eq!(block.reason, "which design?");
    let inbox = al.inbox(&lead, None, false).await.unwrap();
    let bodies: Vec<String> = inbox
        .into_iter()
        .flat_map(|i| i.messages)
        .map(|c| c.message.body)
        .collect();
    assert_eq!(bodies, ["blocked: which design?"]);
}
