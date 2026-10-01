//! A session pushes its work as WIP, and the next session goes on from
//! the branch (01M3WFYEKTWVVZ1FWVNQMGBNN0, 01M3WFYEP1H3VPW8G90KQDE6FW),
//! over real HTTP and real git. The real `riff claim` and the real
//! start hook show the earlier work (01M3WFYER9QWA698KY2E1HNTCW,
//! 01M3WFYETKXPWWE0R0EAKGCD1E).

use isolated::Isolated;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use riff::api::Api;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::RiffState;
use tokio::io::AsyncWriteExt;

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
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

/// Runs the shell `steps` in `dir`.
fn sh(dir: &Path, steps: &str) {
    let out = alone(
        std::process::Command::new("sh")
            .args(["-ec", steps])
            .current_dir(dir),
    )
    .output()
    .unwrap();
    assert!(out.status.success(), "{steps}: {out:?}");
}

/// The first `sh` block of the section `heading` of the skill.
fn skill_block(heading: &str) -> String {
    let skill = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("claude-plugin/riff/skills/riff/SKILL.md"),
    )
    .unwrap();
    let part = &skill[skill.find(heading).unwrap()..];
    let block = part.split("```sh").nth(1).unwrap();
    block[..block.find("```").unwrap()].to_owned()
}

/// The `origin` of the riff repository, with one commit in `main`. Its
/// path ends with `como-technologies/riff.git`, so the default thread of
/// each clone is `como-technologies/riff`.
fn origin(root: &Path) -> PathBuf {
    let owner = root.join("como-technologies");
    std::fs::create_dir(&owner).unwrap();
    git(&owner, &["init", "-q", "--bare", "-b", "main", "riff.git"]);
    let seed = clone(root, "seed");
    git(&seed, &["commit", "-q", "--allow-empty", "-m", "start"]);
    git(&seed, &["push", "-q", "origin", "HEAD:main"]);
    owner.join("riff.git")
}

/// A clone of the `origin` for the machine `host`, as on another
/// machine.
fn clone(root: &Path, host: &str) -> PathBuf {
    let dir = root.join(host);
    std::fs::create_dir(&dir).unwrap();
    let origin = root.join("como-technologies/riff.git");
    git(&dir, &["clone", "-q", origin.to_str().unwrap(), "riff"]);
    dir.join("riff").canonicalize().unwrap()
}

/// The new worktree `.claude/worktrees/ITEM` of `clone`, on the branch
/// `worktree-ITEM`, as the agent tool makes it.
fn worktree(clone: &Path, item: &str) -> PathBuf {
    let path = clone.join(".claude/worktrees").join(item);
    let branch = format!("worktree-{item}");
    git(
        clone,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &branch,
            path.to_str().unwrap(),
            "origin/main",
        ],
    );
    path
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

fn session(host: &str, id: &str) -> SessionUri {
    format!("riff://mike@{host}/como-technologies/riff?session={id}")
        .parse()
        .unwrap()
}

/// A running riff with a lead.
async fn running_riff() -> Api {
    let api = start_server().await;
    let lead = session("pangolin", "l1");
    api.register(&lead).await.unwrap();
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    api
}

/// Runs `riff claim ITEM` as the agent session `id` on `host` in `dir`.
/// Returns stdout and the exit code.
async fn claim(
    api: &Api,
    home: &Path,
    dir: &Path,
    host: &str,
    id: &str,
    item: &str,
) -> (String, i32) {
    let mut cmd = Isolated::shared().assert_riff();
    cmd.args(["claim", item])
        .current_dir(dir)
        .env("RIFF_SERVER", api.base())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", host)
        .env("RIFF_HOME", home)
        .env("RIFF_SESSION", id)
        .env_remove("CLAUDE_CODE_SESSION_ID");
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    (
        String::from_utf8(out.stdout).unwrap(),
        out.status.code().unwrap(),
    )
}

/// Runs the start hook of Claude Code for a new session `id` on `host`
/// in `dir`. Returns its context.
async fn start_hook(api: &Api, home: &Path, dir: &Path, host: &str, id: &str) -> String {
    let mut hook = Isolated::shared()
        .tokio_riff()
        .args(["hook", "session-start"])
        .current_dir(dir)
        .env("RIFF_HOME", home)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", host)
        .env("RIFF_SERVER", api.base())
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

#[tokio::test]
async fn a_session_on_another_clone_finds_the_wip_commit_of_a_killed_session() {
    let api = running_riff().await;
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    std::fs::create_dir(&home).unwrap();
    origin(root.path());
    let (pangolin, thelio) = (clone(root.path(), "pangolin"), clone(root.path(), "thelio"));

    // The first session claims the item, and pushes its work as WIP
    // with the steps of the skill before a long run.
    let first = session("pangolin", "a1");
    api.register(&first).await.unwrap();
    assert!(api.claim(&first, &repo(), "issue-12").await.unwrap().granted);
    let work = worktree(&pangolin, "issue-12");
    std::fs::write(work.join("work.txt"), "half of the work\n").unwrap();
    sh(&work, &skill_block("### Push your work as WIP"));
    let commit = git(&work, &["rev-parse", "--short", "HEAD"]);
    assert!(git(&work, &["log", "-1", "--format=%s"]).starts_with("WIP"));
    assert_eq!(git(&work, &["status", "--porcelain"]), "");

    // The session is killed: no release, and its machine is gone.
    std::fs::remove_dir_all(root.path().join("pangolin")).unwrap();
    let (out, code) = claim(&api, &home, &thelio, "thelio", "b2", "issue-12").await;
    assert_eq!(code, 1, "the dead session still holds the item: {out}");
    assert!(!out.contains("Earlier work"), "{out}");
    // The server frees the claim, as after the end call of its host.
    api.end(&first).await.unwrap();

    // The second session, on another clone, claims the item and finds
    // the commit.
    let (out, code) = claim(&api, &home, &thelio, "thelio", "b2", "issue-12").await;
    assert_eq!(code, 0, "{out}");
    let mut lines = out.lines();
    assert_eq!(
        lines.next(),
        Some("You hold issue-12 in como-technologies/riff.")
    );
    let line = lines.next().unwrap_or_default();
    assert!(
        line.starts_with(&format!(
            "Earlier work on issue-12: the pushed branch origin/worktree-issue-12 at {commit} \
             (a WIP commit, "
        )),
        "{out}"
    );
    assert!(
        line.ends_with(
            "Go on from it, and do not start again: see \"Pick up dropped work\" in the riff skill."
        ),
        "{out}"
    );

    // It goes on from the branch, with the step of the skill.
    let next = worktree(&thelio, "issue-12");
    git(&next, &["reset", "-q", "--hard", "origin/worktree-issue-12"]);
    assert_eq!(
        std::fs::read_to_string(next.join("work.txt")).unwrap(),
        "half of the work\n"
    );
    assert_eq!(git(&next, &["rev-parse", "--short", "HEAD"]), commit);
}

#[tokio::test]
async fn a_claim_names_the_worktree_of_an_earlier_session_and_its_files() {
    let api = running_riff().await;
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    std::fs::create_dir(&home).unwrap();
    origin(root.path());
    let pangolin = clone(root.path(), "pangolin");
    let work = worktree(&pangolin, "issue-12");
    std::fs::write(work.join("work.txt"), "not committed\n").unwrap();
    worktree(&pangolin, "issue-123");

    let (out, code) = claim(&api, &home, &pangolin, "pangolin", "b2", "issue-12").await;
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(&format!(
            "Earlier work on issue-12: the worktree {} (1 file not committed).",
            work.display()
        )),
        "{out}"
    );
    assert!(!out.contains("issue-123"), "{out}");

    // An item with no earlier work, and a verify claim, get no line.
    let (out, code) = claim(&api, &home, &pangolin, "pangolin", "b2", "issue-7").await;
    assert_eq!((out.as_str(), code), ("You hold issue-7 in como-technologies/riff.\n", 0));
    let (out, code) = claim(&api, &home, &pangolin, "pangolin", "c3", "verify-issue-12").await;
    assert_eq!(code, 0);
    assert_eq!(out, "You hold verify-issue-12 in como-technologies/riff.\n");
}

#[tokio::test]
async fn a_new_start_lists_the_earlier_work_that_no_live_session_owns() {
    let api = running_riff().await;
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    std::fs::create_dir(&home).unwrap();
    origin(root.path());
    let pangolin = clone(root.path(), "pangolin");
    for item in ["issue-12", "issue-13"] {
        let work = worktree(&pangolin, item);
        std::fs::write(work.join("work.txt"), item).unwrap();
        sh(&work, &skill_block("### Push your work as WIP"));
    }
    // A live session holds issue-13. No session holds issue-12.
    let owner = session("pangolin", "b2");
    api.register(&owner).await.unwrap();
    assert!(api.claim(&owner, &repo(), "issue-13").await.unwrap().granted);
    let _live = Box::pin(api.watch(&owner).await.unwrap());

    // On the same machine, the context names the branch and the
    // worktree.
    let context = start_hook(&api, &home, &pangolin, "pangolin", "a1").await;
    assert!(
        context.contains("- This clone has earlier work that no live session owns."),
        "{context}"
    );
    let line = context
        .lines()
        .find(|l| l.starts_with("  - issue-12: "))
        .unwrap_or_else(|| panic!("{context}"));
    assert!(
        line.contains("the pushed branch origin/worktree-issue-12 at "),
        "{line}"
    );
    assert!(line.contains("(a WIP commit, "), "{line}");
    assert!(
        line.contains(&format!(
            "the worktree {} (each file committed and pushed)",
            pangolin.join(".claude/worktrees/issue-12").display()
        )),
        "{line}"
    );
    assert!(!context.contains("  - issue-13"), "{context}");

    // On another clone, it names the branch only.
    let thelio = clone(root.path(), "thelio");
    let context = start_hook(&api, &home, &thelio, "thelio", "c3").await;
    let line = context
        .lines()
        .find(|l| l.starts_with("  - issue-12: "))
        .unwrap_or_else(|| panic!("{context}"));
    assert!(line.contains("origin/worktree-issue-12"), "{line}");
    assert!(!line.contains("the worktree"), "{line}");
    assert!(!context.contains("  - issue-13"), "{context}");
}

#[test]
fn the_book_has_the_rule_for_a_branch_with_wip_commits() {
    let page = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let start = page
        .find("\n### A branch with WIP commits\n")
        .expect("the rule has its own heading");
    let part = &page[start + 1..];
    let part = &part[..part[4..].find("\n#").map_or(part.len(), |end| end + 4)];
    let flat = part.split_whitespace().collect::<Vec<_>>().join(" ");
    for word in [
        "`WIP` in its subject",
        "squash",
        "git log --oneline origin/main..origin/worktree-issue-12 ```",
        "Do not review a WIP commit",
    ] {
        assert!(flat.contains(word), "the book does not say {word:?}");
    }
    let shown = page
        .find("\n### See the earlier work on an item\n")
        .expect("the how-to has its own heading");
    assert!(page[shown..].contains("```sh\nriff claim issue-12\n```"));
    // The example line of the book is the line that riff makes.
    let earlier = riff::dropped::Earlier {
        item: "issue-12".into(),
        pushed: vec![riff::dropped::Pushed {
            name: "origin/worktree-issue-12".into(),
            commit: "1a2b3c4".into(),
            age: "2 hours ago".into(),
            wip: true,
        }],
        kept: vec![riff::dropped::Kept {
            path: "/home/mike/src/riff/.claude/worktrees/issue-12".into(),
            changed: 3,
            not_pushed: 1,
        }],
    };
    assert!(
        page.contains(&format!("\n{}\n```", earlier.claim_line())),
        "{}",
        earlier.claim_line()
    );
}
