//! A session that starts in a linked worktree gets a line in its start
//! context, over real HTTP and the real start hook. When another live
//! session works there, the line tells it to stop and start again in the
//! main worktree (01M3MYQ299XKJE9X9FHWZ7JFM4). Else it names the worktree
//! and "Pick up dropped work" (01M3MYQ2BFKS3KJ8DWNWDJKWB9).

use std::path::{Path, PathBuf};
use std::process::Stdio;

use riff::api::Api;
use riff_core::name::SessionUri;
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

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "riff test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "riff test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A clone of the riff repository with one commit in `main`, and the
/// linked worktree `.claude/worktrees/issue-12`. Returns the main
/// worktree and the linked worktree.
fn clone_with_worktree(root: &Path) -> (PathBuf, PathBuf) {
    let main = root.join("riff");
    std::fs::create_dir(&main).unwrap();
    git(&main, &["init", "-q", "-b", "main"]);
    git(
        &main,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/como-technologies/riff.git",
        ],
    );
    git(&main, &["commit", "-q", "--allow-empty", "-m", "one"]);
    let linked = main.join(".claude/worktrees/issue-12");
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "worktree-issue-12",
            linked.to_str().unwrap(),
        ],
    );
    (main.canonicalize().unwrap(), linked.canonicalize().unwrap())
}

fn session(id: &str, worktree: &str) -> SessionUri {
    format!("riff://mike@pangolin/como-technologies/riff?session={id}{worktree}")
        .parse()
        .unwrap()
}

/// Runs the start hook of Claude Code for a new session `id` in `dir`.
/// Returns its context.
async fn start_hook(api: &Api, run: &Path, dir: &Path, id: &str) -> String {
    let mut hook = Command::new(env!("CARGO_BIN_EXE_riff"))
        .args(["hook", "session-start"])
        .current_dir(dir)
        .env("XDG_RUNTIME_DIR", run)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SERVER", api.base())
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("RIFF_SESSION")
        .env_remove("RIFF_WORKER")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let input = format!(r#"{{"session_id":"{id}","source":"startup"}}"#);
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

/// A running riff with a lead in the main worktree.
async fn running_riff() -> Api {
    let api = start_server().await;
    let lead = session("l1", "");
    api.register(&lead).await.unwrap();
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    api
}

#[tokio::test]
async fn a_session_in_the_worktree_of_a_live_session_is_told_to_start_again() {
    let api = running_riff().await;
    let root = tempfile::tempdir().unwrap();
    let (main, linked) = clone_with_worktree(root.path());
    let owner = session("b2", "#issue-12");
    api.register(&owner).await.unwrap();
    let _live = Box::pin(api.watch(&owner).await.unwrap());

    let context = start_hook(&api, root.path(), &linked, "a1").await;
    assert!(context.contains("session=a1#issue-12"), "{context}");
    assert!(
        context.contains("of another live session: riff://mike@pangolin/como-technologies/riff?session=b2#issue-12"),
        "{context}"
    );
    assert!(context.contains("Claim nothing, change no file here"));
    assert!(
        context.contains(&format!(
            "start this session again in the main worktree {}.",
            main.display()
        )),
        "{context}"
    );
    assert!(!context.contains("Pick up dropped work"), "{context}");
}

#[tokio::test]
async fn a_session_in_a_worktree_with_no_live_session_can_pick_up_dropped_work() {
    let api = running_riff().await;
    let root = tempfile::tempdir().unwrap();
    let (main, linked) = clone_with_worktree(root.path());
    // A session that is not live does not count.
    api.register(&session("b2", "#issue-12")).await.unwrap();

    let context = start_hook(&api, root.path(), &linked, "a1").await;
    assert!(
        context.contains(&format!(
            "You started in the linked worktree {}, not in the main worktree {}.",
            linked.display(),
            main.display()
        )),
        "{context}"
    );
    assert!(context.contains("Pick up dropped work"), "{context}");
    assert!(!context.contains("another live session"), "{context}");
}

#[tokio::test]
async fn a_session_in_the_main_worktree_gets_no_worktree_line() {
    let api = running_riff().await;
    let root = tempfile::tempdir().unwrap();
    let (main, _) = clone_with_worktree(root.path());
    let owner = session("b2", "#issue-12");
    api.register(&owner).await.unwrap();
    let _live = Box::pin(api.watch(&owner).await.unwrap());

    let context = start_hook(&api, root.path(), &main, "a1").await;
    assert!(context.contains("The riff is running."), "{context}");
    assert!(!context.contains("linked worktree"), "{context}");
    assert!(!context.contains("another live session"), "{context}");
    assert!(!context.contains("Pick up dropped work"), "{context}");
}

#[test]
fn the_book_shows_how_to_open_a_pane_in_the_main_clone() {
    let page = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let start = page
        .find("\n### Start each session in the main clone\n")
        .expect("the how-to has its own heading");
    let part = &page[start + 1..];
    let part = &part[..part[4..].find("\n#").map_or(part.len(), |end| end + 4)];
    assert!(
        part.contains("```sh\ntmux split-window -c ~/src/como-technologies/riff\n```"),
        "{part}"
    );
}
