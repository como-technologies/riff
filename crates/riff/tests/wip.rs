//! A session pushes its work as WIP, and the next session goes on from
//! the branch (01M3WFYEKTWVVZ1FWVNQMGBNN0, 01M3WFYEP1H3VPW8G90KQDE6FW),
//! over real HTTP and real git. The real `riff claim` and the real
//! start hook show the earlier work (01M3WFYER9QWA698KY2E1HNTCW,
//! 01M3WFYETKXPWWE0R0EAKGCD1E).

use isolated::Isolated;
use std::path::{Path, PathBuf};

use riff::tools::Tools;
use riff_core::name::SessionUri;
use riff_core::wire::RiffState;
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;

use crate::common;

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

fn session(host: &str, id: &str) -> SessionUri {
    format!("riff://mike@{host}/como-technologies/riff?session={id}")
        .parse()
        .unwrap()
}

/// A running riff with a lead.
async fn running_riff() -> common::Signed {
    let server = common::start_signed().await;
    let lead = session("pangolin", "l1");
    let api = common::api_of(&server, &lead);
    api.register(&lead).await.unwrap();
    common::name_lead(&common::api_of(&server, &common::person_of(&lead)), &lead).await;
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    server
}

/// Runs `riff claim ITEM` as the agent session `id` on `host` in `dir`.
/// Returns stdout and the exit code.
async fn claim(
    server: &common::Signed,
    home: &Path,
    dir: &Path,
    host: &str,
    id: &str,
    item: &str,
) -> (String, i32) {
    common::sign_in(server, home, "mike");
    let mut cmd = Isolated::shared().assert_riff();
    cmd.args(["claim", item])
        .current_dir(dir)
        .env("RIFF_SERVER", &**server)
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

#[tokio::test]
async fn a_claim_names_the_worktree_of_an_earlier_session_and_its_files() {
    let server = running_riff().await;
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    std::fs::create_dir(&home).unwrap();
    origin(root.path());
    let pangolin = clone(root.path(), "pangolin");
    let work = worktree(&pangolin, "issue-12");
    std::fs::write(work.join("work.txt"), "not committed\n").unwrap();
    worktree(&pangolin, "issue-123");

    let (out, code) = claim(&server, &home, &pangolin, "pangolin", "b2", "issue-12").await;
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
    let (out, code) = claim(&server, &home, &pangolin, "pangolin", "b2", "issue-7").await;
    assert_eq!(
        (out.as_str(), code),
        ("You hold issue-7 in como-technologies/riff.\n", 0)
    );
    let (out, code) = claim(
        &server,
        &home,
        &pangolin,
        "pangolin",
        "c3",
        "verify-issue-12",
    )
    .await;
    assert_eq!(code, 0);
    assert_eq!(out, "You hold verify-issue-12 in como-technologies/riff.\n");
}

/// Calls the `claim` tool of `tools` through a real MCP client.
async fn claim_tool(tools: Tools, item: &str) -> String {
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        tools
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let client = ().serve(client_io).await.unwrap();
    let args = serde_json::json!({ "item": item });
    let params = CallToolRequestParams::new("claim".to_owned())
        .with_arguments(args.as_object().unwrap().clone());
    let result = client.call_tool(params).await.unwrap();
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn the_claim_tool_names_the_earlier_work() {
    let server = running_riff().await;
    let root = tempfile::tempdir().unwrap();
    origin(root.path());
    let pangolin = clone(root.path(), "pangolin");
    let work = worktree(&pangolin, "issue-12");
    std::fs::write(work.join("work.txt"), "not committed\n").unwrap();
    let tools = |id: &str| {
        let me = session("pangolin", id);
        Tools::new(common::api_of(&server, &me), me).in_dir(pangolin.clone())
    };
    for id in ["b2", "c3"] {
        common::api_of(&server, &session("pangolin", id))
            .register(&session("pangolin", id))
            .await
            .unwrap();
    }

    let out = claim_tool(tools("b2").with_earlier_work(), "issue-12").await;
    assert_eq!(
        out,
        format!(
            "You hold issue-12 in como-technologies/riff.\nEarlier work on issue-12: the \
             worktree {} (1 file not committed). Go on from it, and do not start again: see \
             \"Pick up dropped work\" in the riff skill.",
            work.display()
        )
    );
    // A claim that fails names no work.
    let out = claim_tool(tools("c3").with_earlier_work(), "issue-12").await;
    assert!(out.contains("holds issue-12"), "{out}");
    assert!(!out.contains("Earlier work"), "{out}");
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
