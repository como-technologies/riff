//! riff starts workers by itself when the wave has free work
//! (01M3Q5QE01DB0FJQJWFKR450KQ to 01M3Q5QEE4MQNCRKVJK3D54G9Z). The lead
//! runs `riff mcp` with a fake `tmux` and a fake `gh` on `PATH`, and its
//! own riff home. The unit tests of `riff::rollout` test the rate, the
//! limit and the placement with a fake clock.

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff::identity;
use riff_core::name::{SessionUri, Who};
use riff_core::wire::RiffState;

/// `list-panes -a` lists the worker panes from the file `workers`.
const FAKE_TMUX: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/log"
n=$(grep -c -e '^split-window' -e '^new-window' "$dir/log")
case "$1" in
  list-panes)
    if [ "$2" = "-a" ]; then cat "$dir/workers" 2>/dev/null; else cat "$dir/panes" 2>/dev/null; fi ;;
  list-windows) cat "$dir/windows" 2>/dev/null ;;
  display-message) echo "@0" ;;
  new-window) echo "@7 %$n" ;;
  split-window) echo "%$n" ;;
  set-option)
    case "$2 $5" in
      "-p @riff-session") echo "$4 $6" >> "$dir/workers" ;;
      "-w @riff") echo "$4 $6" >> "$dir/windows" ;;
    esac ;;
esac
exit 0
"#;

/// The wave `Wave 1` holds the issues of the file `issues`. No pull
/// request is open.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/gh.log"
case "$1 $2" in
  api*) echo '[{"title":"Backlog"},{"title":"Wave 1"}]' ;;
  "issue list") cat "$dir/issues" ;;
  "pr list") echo '[]' ;;
  *) exit 1 ;;
esac
"#;

const WAIT: Duration = Duration::from_secs(20);

struct Lead {
    api: Api,
    fake: tempfile::TempDir,
    _home: tempfile::TempDir,
    _root: tempfile::TempDir,
    me: SessionUri,
    mcp: Child,
}

impl Drop for Lead {
    fn drop(&mut self) {
        let _ = self.mcp.kill();
        let _ = self.mcp.wait();
    }
}

impl Lead {
    fn workers(&self) -> usize {
        std::fs::read_to_string(self.fake.path().join("workers"))
            .unwrap_or_default()
            .lines()
            .count()
    }

    fn issues(&self, json: &str) {
        std::fs::write(self.fake.path().join("issues"), json).unwrap();
    }

    async fn riff(&self, state: RiffState) {
        self.api.set_riff(&self.me, state).await.unwrap();
    }

    /// The worker `n` (from 1) of the fake tmux joins the riff and
    /// claims `item`, as a real worker does.
    async fn claim(&self, n: usize, item: &str) {
        self.until_workers(n).await;
        let workers = std::fs::read_to_string(self.fake.path().join("workers")).unwrap();
        let line = workers.lines().nth(n - 1).unwrap();
        let (_, id) = line.split_once(' ').unwrap();
        let worker = SessionUri::new(Who::new("mike", Some(id)).unwrap(), self.me.place().clone());
        self.api.register_as(&worker, true).await.unwrap();
        let thread = self.me.default_thread().unwrap();
        self.api.claim(&worker, &thread, item).await.unwrap();
    }

    /// Waits until `n` workers run.
    async fn until_workers(&self, n: usize) {
        let start = Instant::now();
        while self.workers() < n {
            assert!(start.elapsed() < WAIT, "timed out: {n} workers");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

fn script(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A repository with a commit and an origin on GitHub.
fn repository(root: &Path) -> PathBuf {
    let main = root.join("riff");
    std::fs::create_dir(&main).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&main)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&["init", "-q"]);
    git(&["commit", "-q", "--allow-empty", "-m", "x"]);
    git(&["remote", "add", "origin", "https://github.com/o/r.git"]);
    std::fs::canonicalize(&main).unwrap()
}

/// The lead `l1` of mike on host `a`, with a limit of `limit` workers,
/// in a paused riff.
async fn lead(limit: u16) -> Lead {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    let api = Api::new(&format!("http://{addr}"));
    let root = tempfile::tempdir().unwrap();
    let main = repository(root.path());
    let fake = tempfile::tempdir().unwrap();
    script(fake.path(), "tmux", FAKE_TMUX);
    script(fake.path(), "gh", FAKE_GH);
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join("config.toml"),
        format!("[workers]\nlimit = {limit}\ninterval = 1\n"),
    )
    .unwrap();
    let path = format!(
        "{}:{}",
        fake.path().display(),
        std::env::var("PATH").unwrap()
    );
    let mcp = Isolated::shared()
        .riff()
        .arg("mcp")
        .current_dir(&main)
        .env("PATH", path)
        .env("RIFF_HOME", home.path())
        .env("RIFF_SERVER", api.base())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "a")
        .env("RIFF_SESSION", "l1")
        .env("TMUX", "/tmp/tmux-1000/default,1,0")
        .env("TMUX_PANE", "%0")
        .env(riff::machine::MACHINE, "cpu 8x3000MHz, mem 16GB, load 0.00")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("RIFF_WORKER")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let place = identity::place_in(&main, "a").unwrap();
    let me = SessionUri::new(Who::new("mike", Some("l1")).unwrap(), place);
    let start = Instant::now();
    loop {
        let who = api.who(&me, false).await.unwrap_or_default();
        if who.iter().any(|s| s.uri.who() == me.who() && s.uri.lead()) {
            break;
        }
        assert!(start.elapsed() < WAIT, "the lead did not register");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Lead {
        api,
        fake,
        _home: home,
        _root: root,
        me,
        mcp,
    }
}

const TWO_FREE: &str = r#"[{"number":1,"body":"","comments":[],"milestone":{"title":"Wave 1"}},
{"number":2,"body":"Needs: nothing","comments":[],"milestone":{"title":"Wave 1"}},
{"number":3,"body":"Needs: #1","comments":[],"milestone":{"title":"Wave 1"}},
{"number":4,"body":"","comments":[{"body":"Merged in #9 (abc)"}],"milestone":{"title":"Wave 1"}},
{"number":8,"body":"","comments":[],"milestone":{"title":"Backlog"}}]"#;

/// A resume of a paused riff starts one worker for each free item, with
/// no other step, and a note to the lead for each
/// (01M3Q5QE01DB0FJQJWFKR450KQ, 01M3Q5QEE4MQNCRKVJK3D54G9Z). The next
/// worker starts only when the new one claimed (01M3Q5QEJNP1JGQM7VXXEBJ9J9).
/// A pause stops the rollout (01M3Q5QEBTNM90SPYXNVTT7RJA).
#[tokio::test(flavor = "multi_thread")]
async fn a_resume_starts_one_worker_for_each_free_item() {
    let lead = lead(5).await;
    lead.issues(TWO_FREE);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(
        lead.workers(),
        0,
        "no worker starts while the riff is paused"
    );

    // The unit tests of riff::rollout check the rate with a fake clock.
    lead.riff(RiffState::Running).await;
    lead.until_workers(1).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(lead.workers(), 1, "the new worker is idle until it claims");
    lead.claim(1, "issue-1").await;
    lead.claim(2, "issue-2").await;
    // Each free item has a worker: no third one.
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(lead.workers(), 2);

    let start = Instant::now();
    let mut read = String::new();
    while read.matches("a: started 1 worker").count() < 2 {
        let inbox = lead.api.inbox(&lead.me, None, false).await.unwrap();
        read.push_str(&riff::text::inbox(&inbox, &lead.me));
        assert!(start.elapsed() < WAIT, "no notes: {read}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    lead.riff(RiffState::Paused).await;
    lead.issues(
        r#"[{"number":5,"body":"","comments":[],"milestone":{"title":"Wave 1"}},
        {"number":6,"body":"","comments":[],"milestone":{"title":"Wave 1"}}]"#,
    );
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(lead.workers(), 2, "a pause stops the rollout");
    lead.riff(RiffState::Running).await;
    lead.until_workers(3).await;
}

/// The rollout never starts more workers than the limit of the machine.
#[tokio::test(flavor = "multi_thread")]
async fn the_limit_caps_the_rollout() {
    let lead = lead(1).await;
    lead.issues(TWO_FREE);
    lead.riff(RiffState::Running).await;
    lead.claim(1, "issue-1").await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(lead.workers(), 1);
}

/// `riff workers interval` shows and sets the interval
/// (01M3Q5QE9H42FQKEDC5G9GKCWD).
#[test]
fn riff_workers_interval_shows_and_sets_the_interval() {
    let home = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let out = Isolated::shared()
            .riff()
            .arg("workers")
            .arg("interval")
            .args(args)
            .env("RIFF_HOME", home.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    assert!(run(&[]).starts_with("The lead starts at most one worker each 10 seconds"));
    assert!(run(&["30"]).starts_with("The lead starts at most one worker each 30 seconds"));
    assert!(run(&[]).contains("each 30 seconds"));
    assert!(run(&["0"]).starts_with("The lead starts no worker by itself"));
}
