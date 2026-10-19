//! riff tidies the worktrees of a machine (01M41A118QPQKFAAHGQFFX4F3B to
//! 01M41A11GHP78E2VYN14JSE27P). Each test runs the real `riff` binary
//! with a fake `gh` that gives the pull requests.

use isolated::Isolated;
use std::path::{Path, PathBuf};
use std::process::Command;

use riff::identity;
use riff_client::api::Api;
use riff_core::name::{SessionUri, Who};
use riff_core::wire::RiffState;

/// A `gh` that logs each call with the token in its environment, and
/// says that each pull request is open. `gh auth token` prints
/// `gho_one_read`.
const TOKEN_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "${GH_TOKEN:-none} $*" >> "$dir/gh.log"
case "$1 $2" in
'auth token') echo gho_one_read ;;
'pr view') echo '{"number":40,"state":"OPEN","headRefOid":"x"}' ;;
*) exit 1 ;;
esac
"#;

/// A machine with 8 cores and much free memory.
const MACHINE: &str = "cpu 8x3000MHz, mem 64GB, 60GB available, load 0.00";

fn script(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    isolated::write_program(&path, text);
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A riff that runs, the lead `l1` on the host `a`, and a clone with an
/// `origin` on the machine `pangolin`.
struct Riff {
    api: Api,
    fake: tempfile::TempDir,
    home: tempfile::TempDir,
    root: tempfile::TempDir,
}

impl Riff {
    async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, riff_server::router()).await.unwrap();
        });
        let api = Api::new(&format!("http://{addr}"));
        let fake = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let origin = root.path().join("origin.git");
        git(root.path(), &["init", "-q", "--bare", "origin.git"]);
        git(
            root.path(),
            &["clone", "-q", &origin.to_string_lossy(), "main"],
        );
        let main = std::fs::canonicalize(root.path().join("main")).unwrap();
        git(&main, &["commit", "-q", "--allow-empty", "-m", "first"]);
        git(&main, &["push", "-q", "origin", "HEAD"]);
        let place = identity::place_in(&main, "a").unwrap();
        let lead = SessionUri::new(Who::new("mike", Some("l1")).unwrap(), place);
        api.register(&lead).await.unwrap();
        api.set_riff(&lead, RiffState::Running).await.unwrap();
        Riff {
            api,
            fake,
            home: tempfile::tempdir().unwrap(),
            root,
        }
    }

    fn main(&self) -> PathBuf {
        std::fs::canonicalize(self.root.path().join("main")).unwrap()
    }

    /// `riff ARGS` in the main clone, as the person on `pangolin`.
    fn riff(&self, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = Isolated::shared().riff();
        cmd.args(args)
            .current_dir(self.main())
            .env("PATH", path)
            .env("RIFF_HOME", self.home.path())
            .env("RIFF_SERVER", self.api.base())
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("RIFF_MACHINE", MACHINE)
            .env("RIFF_ON", "1")
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("RIFF_SESSION")
            .env_remove("RIFF_WORKER");
        cmd
    }

    /// A worktree of the agent tool, on a new branch.
    fn worktree(&self, name: &str) -> PathBuf {
        let main = self.main();
        let path = main.join(".claude/worktrees").join(name);
        let branch = format!("worktree-{name}");
        git(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                &branch,
                &path.to_string_lossy(),
            ],
        );
        path
    }
}

/// The calls of `gh` of one `riff worktrees clean` with `env` in its
/// environment. Three worktrees have work that is not on `main`, so the
/// clean asks `gh` for the pull request of each.
async fn gh_calls_of_one_clean(env: &[(&str, &str)]) -> Vec<String> {
    let r = Riff::new().await;
    script(r.fake.path(), "gh", TOKEN_GH);
    for name in ["issue-7", "issue-8", "issue-9"] {
        let tree = r.worktree(name);
        git(&tree, &["commit", "-q", "--allow-empty", "-m", "the work"]);
    }
    let mut cmd = r.riff(&["worktrees", "clean"]);
    cmd.envs(env.iter().copied());
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let log = std::fs::read_to_string(r.fake.path().join("gh.log")).unwrap();
    log.lines().map(str::to_owned).collect()
}

/// 01M54510SFK1Z9VPFYBYYSV10F: one clean of the worktrees reads the
/// token of `gh` one time, and gives it to each `gh` call. The `riff mcp`
/// of the lead and `riff worktrees clean` run the same clean
/// (`worktrees::clean_here`).
#[tokio::test(flavor = "multi_thread")]
async fn one_clean_gives_its_gh_calls_the_token_of_one_read() {
    let calls = gh_calls_of_one_clean(&[]).await;
    assert_eq!(calls[0], "none auth token", "{calls:?}");
    assert_eq!(calls.len(), 4, "{calls:?}");
    for branch in ["worktree-issue-7", "worktree-issue-8", "worktree-issue-9"] {
        let call = format!("gho_one_read pr view {branch} --json number,state,headRefOid");
        assert!(calls.contains(&call), "{call:?} in {calls:?}");
    }
}

/// With `GH_TOKEN` in the environment, `gh` takes it: the clean reads
/// no token.
#[tokio::test(flavor = "multi_thread")]
async fn with_gh_token_in_the_environment_the_clean_reads_no_token() {
    let calls = gh_calls_of_one_clean(&[("GH_TOKEN", "gho_of_the_environment")]).await;
    assert_eq!(calls.len(), 3, "{calls:?}");
    for call in &calls {
        assert!(
            call.starts_with("gho_of_the_environment pr view "),
            "{calls:?}"
        );
    }
}
