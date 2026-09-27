//! A new start is blank (01M3JEE1QQCFS5TMZW5N2DAD2D,
//! 01M3JEE1SWR05DWQA5WQ8AXFTF), over real HTTP and the real start hook:
//! after `/clear`, a resume or a new process, the session has the same
//! ID and no claims, and its item is free. The lead stays the lead. A
//! compaction keeps the claims. The skill finds the pushed branch of an
//! earlier session (01M3JEE1W32CMQP8CP2HJ829E7).

use std::path::{Path, PathBuf};
use std::process::Stdio;

use riff::api::Api;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::RiffState;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// `cmd` with no git settings of the user or the machine.
fn alone(cmd: &mut std::process::Command) -> &mut std::process::Command {
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "riff test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "riff test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = alone(
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir),
    )
    .output()
    .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// A clone of the riff repository on GitHub, so the default thread is
/// `como-technologies/riff`.
fn repo_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(
        dir.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/como-technologies/riff.git",
        ],
    );
    dir
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

fn session(id: &str) -> SessionUri {
    format!("riff://mike@pangolin/como-technologies/riff?session={id}")
        .parse()
        .unwrap()
}

/// Runs the start hook of Claude Code for the session `id` in `dir`.
/// Returns its context.
async fn start_hook(api: &Api, dir: &Path, id: &str, source: &str) -> String {
    let mut hook = Command::new(env!("CARGO_BIN_EXE_riff"))
        .args(["hook", "session-start"])
        .current_dir(dir)
        .env("XDG_RUNTIME_DIR", dir)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SERVER", api.base())
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let input = format!(r#"{{"session_id":"{id}","source":"{source}"}}"#);
    let mut stdin = hook.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).await.unwrap();
    drop(stdin);
    let out = hook.wait_with_output().await.unwrap();
    assert!(out.status.success());
    let out: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    out["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The URI of the session `id` in `who`.
async fn shown(api: &Api, id: &str) -> SessionUri {
    api.who(&session("x0"), false)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.uri.who().session() == Some(id))
        .unwrap_or_else(|| panic!("{id} is not in who"))
        .uri
}

#[tokio::test]
async fn each_new_start_frees_the_claims_and_keeps_the_id_and_the_lead() {
    let api = start_server().await;
    let dir = repo_dir();
    let (lead, worker) = (session("l1"), session("w1"));
    api.register(&lead).await.unwrap();
    api.register(&worker).await.unwrap();
    api.set_riff(&lead, RiffState::Running).await.unwrap();

    for source in ["clear", "resume", "startup"] {
        for me in [&lead, &worker] {
            let id = me.who().session().unwrap();
            let item = format!("issue-{id}");
            assert!(api.claim(me, &repo(), &item).await.unwrap().granted);

            let context = start_hook(&api, dir.path(), id, source).await;
            assert!(
                context.contains(&format!("freed your claims: {item} in {}", repo())),
                "{source}: {context}"
            );
            let now = shown(&api, id).await;
            assert!(now.claims().is_empty(), "{source}: {now}");
            assert_eq!(now.lead(), me == &lead, "{source}: the lead stays");
            let other = if me == &lead { &worker } else { &lead };
            let taken = api.claim(other, &repo(), &item).await.unwrap();
            assert!(taken.granted, "{source}: the item is free at once");
            api.release(other, &repo(), &item).await.unwrap();
        }
    }
}

#[tokio::test]
async fn a_compaction_keeps_the_claims() {
    let api = start_server().await;
    let dir = repo_dir();
    let me = session("c1");
    api.register(&me).await.unwrap();
    api.set_riff(&me, RiffState::Running).await.unwrap();
    api.claim(&me, &repo(), "issue-12").await.unwrap();

    let context = start_hook(&api, dir.path(), "c1", "compact").await;
    assert!(!context.contains("freed"), "{context}");
    assert_eq!(shown(&api, "c1").await.claims(), ["issue-12"]);
}

#[tokio::test]
async fn an_end_and_a_resume_keep_the_lead() {
    let api = start_server().await;
    let dir = repo_dir();
    let (lead, worker) = (session("l1"), session("w1"));
    api.register(&lead).await.unwrap();
    api.register(&worker).await.unwrap();
    api.end(&lead).await.unwrap();

    start_hook(&api, dir.path(), "l1", "resume").await;
    assert!(shown(&api, "l1").await.lead());
    assert!(!shown(&api, "w1").await.lead());
}

/// The first `sh` block of "Pick up dropped work" in the skill, for
/// the item `item`.
fn find_steps(item: &str) -> String {
    let skill = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("claude-plugin/riff/skills/riff/SKILL.md"),
    )
    .unwrap();
    let part = &skill[skill.find("## Pick up dropped work").unwrap()..];
    let block = &part[part.find("```sh").unwrap() + "```sh".len()..];
    let block = &block[..block.find("```").unwrap()];
    block.replace("issue-12", item)
}

#[test]
fn the_skill_finds_the_pushed_branch_of_an_earlier_session() {
    let tmp = tempfile::tempdir().unwrap();
    let (origin, first, second) = (
        tmp.path().join("origin.git"),
        tmp.path().join("first"),
        tmp.path().join("second"),
    );
    git(
        tmp.path(),
        &["init", "-q", "--bare", "-b", "main", "origin.git"],
    );
    git(
        tmp.path(),
        &["clone", "-q", origin.to_str().unwrap(), "first"],
    );
    git(&first, &["commit", "-q", "--allow-empty", "-m", "start"]);
    git(&first, &["push", "-q", "origin", "HEAD:main"]);
    git(&first, &["switch", "-q", "-c", "worktree-issue-12"]);
    git(&first, &["commit", "-q", "--allow-empty", "-m", "WIP"]);
    git(&first, &["push", "-q", "-u", "origin", "HEAD"]);
    git(
        tmp.path(),
        &["clone", "-q", origin.to_str().unwrap(), "second"],
    );

    let found = |item: &str| {
        let out = alone(
            std::process::Command::new("sh")
                .args(["-c", &find_steps(item)])
                .current_dir(&second),
        )
        .output()
        .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    assert!(found("issue-12").contains("origin/worktree-issue-12"));
    assert!(!found("issue-7").contains("worktree-issue"));
}
