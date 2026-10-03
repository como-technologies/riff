//! riff controls the processes and the worktrees of its workers itself
//! (01M3ZV0QSFVCHRSEKYK57B88VA to 01M3ZV0TMNQDK9WC3BR1NPGAC2). Each
//! test runs the real `riff` binary. A fake worker is a set of `sleep`
//! processes with the variables of a worker: a stand-in for `claude`
//! and for its MCP server, and the processes of a context with the
//! variable of the agent tool. A fake `tmux` lists the worker pane, and
//! a fake `gh` gives the pull requests.

mod book;

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff::identity;
use riff_core::name::{SessionUri, Who};
use riff_core::wire::{RiffState, StartReason};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::store::Memory;

/// The variable that Claude Code gives to each process of a context.
const CONTEXT: &str = "CLAUDE_PID";

/// `list-panes -a` lists the pane `%5` of the worker in the file
/// `session`. Each other call goes to the log.
const FAKE_TMUX: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/log"
case "$1" in
  list-panes) [ -f "$dir/session" ] && echo "%5 $(cat "$dir/session")" ;;
  kill-pane) rm -f "$dir/session" ;;
esac
exit 0
"#;

/// `gh pr view BRANCH` prints the file `pr-BRANCH.json`, else fails.
/// `gh pr list --state merged --search SHA` prints the file
/// `merged-SHA.json`, else `[]`.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
if [ "$1 $2" = "pr view" ] && [ -f "$dir/pr-$3.json" ]; then cat "$dir/pr-$3.json"; exit 0; fi
if [ "$1 $2 $3 $4 $5" = "pr list --state merged --search" ]; then
  if [ -f "$dir/merged-$6.json" ]; then cat "$dir/merged-$6.json"; else echo '[]'; fi
  exit 0
fi
echo "no pull requests found for branch \"$3\"" >&2
exit 1
"#;

fn script(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A riff on a memory store, a machine with the fake `tmux` and `gh`,
/// and a clone with an `origin`.
struct Riff {
    api: Api,
    fake: tempfile::TempDir,
    run: tempfile::TempDir,
    root: tempfile::TempDir,
    _service: Service,
}

impl Riff {
    async fn new() -> Self {
        let mut config = Config::default();
        config.lease.wait = Duration::from_millis(10);
        let service = Service::load(config, std::sync::Arc::new(Memory::default()))
            .await
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = service.router();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let fake = tempfile::tempdir().unwrap();
        script(fake.path(), "tmux", FAKE_TMUX);
        script(fake.path(), "gh", FAKE_GH);
        let root = tempfile::tempdir().unwrap();
        let origin = root.path().join("origin.git");
        git(root.path(), &["init", "-q", "--bare", "origin.git"]);
        let main = root.path().join("main");
        git(
            root.path(),
            &["clone", "-q", &origin.to_string_lossy(), "main"],
        );
        git(&main, &["commit", "-q", "--allow-empty", "-m", "first"]);
        git(&main, &["push", "-q", "origin", "HEAD"]);
        let r = Riff {
            api: Api::new(&format!("http://{addr}")),
            fake,
            run: tempfile::tempdir().unwrap(),
            root,
            _service: service,
        };
        let lead = r.uri(&r.main(), "l1");
        r.api.register(&lead).await.unwrap();
        r.api.set_riff(&lead, RiffState::Running).await.unwrap();
        r
    }

    fn main(&self) -> PathBuf {
        std::fs::canonicalize(self.root.path().join("main")).unwrap()
    }

    fn uri(&self, dir: &Path, id: &str) -> SessionUri {
        let place = identity::place_in(dir, "pangolin").unwrap();
        SessionUri::new(Who::new("mike", Some(id)).unwrap(), place)
    }

    /// `riff ARGS` in `dir`, as a plain terminal of the person.
    fn riff(&self, dir: &Path, args: &[&str]) -> Command {
        let path = format!(
            "{}:{}",
            self.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = Isolated::shared().riff();
        cmd.args(args)
            .current_dir(dir)
            .env("PATH", path)
            .env("RIFF_HOME", self.run.path())
            .env("RIFF_SERVER", self.api.base())
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .env("TMUX", "/tmp/tmux-1000/default,1,0")
            .env("TMUX_PANE", "%5")
            .env_remove("RIFF_SESSION")
            .env_remove("RIFF_WORKER");
        cmd
    }

    /// `riff ARGS` as a process of a context of the worker `id`.
    fn in_worker(&self, id: &str, args: &[&str]) -> Command {
        let mut cmd = self.riff(&self.main(), args);
        cmd.env("RIFF_WORKER", "1")
            .env("RIFF_SESSION", id)
            .env(CONTEXT, "1");
        cmd
    }

    /// A hook of the worker `id`, with `input` on its stdin.
    fn hook(&self, id: &str, event: &str, input: &str) -> Output {
        let mut hook = self
            .in_worker(id, &["hook", event])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        std::io::Write::write_all(hook.stdin.as_mut().unwrap(), input.as_bytes()).unwrap();
        hook.wait_with_output().unwrap()
    }

    /// The worker `id` in the pane `%5`.
    fn pane(&self, id: &str) {
        std::fs::write(self.fake.path().join("session"), id).unwrap();
    }
}

/// The session ID `name` of this test run. riff finds the processes of
/// a worker by its session ID on the whole machine, so two runs of the
/// tests at the same time must not share an ID (#460).
fn unique(name: &str) -> String {
    format!("{name}-{}", std::process::id())
}

/// A `sleep` of the worker `id`: of a context with `context`, else a
/// stand-in for `claude` or for its MCP server.
fn sleeper(id: &str, context: bool) -> Child {
    let mut cmd = Command::new("sleep");
    cmd.arg("300")
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap())
        .env("RIFF_WORKER", "1")
        .env("RIFF_SESSION", id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if context {
        cmd.env(CONTEXT, "1");
    }
    cmd.spawn().unwrap()
}

/// Waits until `child` ends, at most 20 seconds. True when it ended.
fn ends(child: &mut Child) -> bool {
    let end = Instant::now() + Duration::from_secs(20);
    while Instant::now() < end {
        if child.try_wait().unwrap().is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn lives(child: &mut Child) -> bool {
    child.try_wait().unwrap().is_none()
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

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The clear of a worker stops its old context: a long child of a
/// command of the context is gone after `riff hook clear`, and
/// `claude` and its MCP server stay (01M3ZV0TJDQ6JCM7XG0036MSV1).
#[tokio::test(flavor = "multi_thread")]
async fn the_clear_stops_the_old_context_and_keeps_claude_and_its_mcp_server() {
    let r = Riff::new().await;
    let id = &unique("wclear1");
    let w = r.uri(&r.main(), id);
    r.api.start(&w, StartReason::Process, true).await.unwrap();
    let thread = w.default_thread().unwrap();
    r.api.claim(&w, &thread, "issue-12").await.unwrap();
    let mut claude = sleeper(id, false);
    let mut mcp = sleeper(id, false);
    let mut ci = sleeper(id, true);
    let mut other = sleeper(&unique("wother1"), true);

    let out = r.in_worker(id, &["release", "issue-12"]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let input = format!(r#"{{"session_id":"{id}","hook_event_name":"Stop"}}"#);
    let out = r.hook(id, "stop", &input);
    assert!(out.status.success(), "{out:?}");

    assert!(ends(&mut ci), "the old context still runs");
    assert!(lives(&mut claude), "claude stopped");
    assert!(lives(&mut mcp), "the MCP server stopped");
    assert!(lives(&mut other), "a process of another worker stopped");
    for mut child in [claude, mcp, other] {
        child.kill().unwrap();
        child.wait().unwrap();
    }
}

/// `riff workers reap PANE` stops a process of the context before the
/// last start, keeps a process of the current context, and names each
/// process that it stopped (01M3ZV0TKBP201FKY32ZD81G4E,
/// 01M3ZV0TJX2H77RW6ZA3ERZT9H).
#[tokio::test(flavor = "multi_thread")]
async fn reap_stops_the_orphan_of_an_old_context_and_keeps_the_new_one() {
    let r = Riff::new().await;
    let id = &unique("wreap1");
    r.pane(id);
    let mut claude = sleeper(id, false);
    let mut old = sleeper(id, true);
    // The start of a process has a resolution of 10 ms.
    std::thread::sleep(Duration::from_millis(100));
    let input = format!(r#"{{"session_id":"{id}","source":"clear"}}"#);
    let out = r.hook(id, "session-start", &input);
    assert!(out.status.success(), "{out:?}");
    std::thread::sleep(Duration::from_millis(100));
    let mut new = sleeper(id, true);

    let out = r
        .riff(&r.main(), &["workers", "reap", "%5"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let printed = stdout(&out);
    assert_eq!(
        printed.trim(),
        format!("pane %5: stopped {} sleep 300", old.id()),
        "{out:?}"
    );
    assert!(ends(&mut old), "the orphan still runs");
    assert!(lives(&mut new), "the current context stopped");
    assert!(lives(&mut claude), "claude stopped");

    // A second reap finds no orphan.
    let out = r.riff(&r.main(), &["workers", "reap"]).output().unwrap();
    assert_eq!(stdout(&out).trim(), "pane %5: no orphan process", "{out:?}");
    for mut child in [new, claude] {
        child.kill().unwrap();
        child.wait().unwrap();
    }
}

/// With no start of the context, `riff workers reap` stops nothing.
#[tokio::test(flavor = "multi_thread")]
async fn reap_with_no_start_of_the_context_stops_nothing() {
    let r = Riff::new().await;
    let id = &unique("wreap2");
    r.pane(id);
    let mut child = sleeper(id, true);
    let out = r
        .riff(&r.main(), &["workers", "reap", "%5"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        stdout(&out).trim(),
        "pane %5: riff knows no start of its context, so it stops nothing"
    );
    assert!(lives(&mut child));
    child.kill().unwrap();
    child.wait().unwrap();
}

/// `riff workers stop PANE` leaves no process of the worker: `claude`,
/// its MCP server and each process of a context
/// (01M3ZV0TMNQDK9WC3BR1NPGAC2).
#[tokio::test(flavor = "multi_thread")]
async fn workers_stop_leaves_no_process_of_the_worker() {
    let r = Riff::new().await;
    let id = &unique("wstop1");
    r.pane(id);
    let w = r.uri(&r.main(), id);
    r.api.start(&w, StartReason::Process, true).await.unwrap();
    let mut all = [sleeper(id, false), sleeper(id, false), sleeper(id, true)];
    let mut other = sleeper(&unique("wother2"), true);

    let out = r
        .riff(&r.main(), &["workers", "stop", "%5"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(stdout(&out).contains("also stopped 3 processes"), "{out:?}");
    for child in &mut all {
        assert!(ends(child), "a process of the worker still runs");
    }
    assert!(lives(&mut other), "a process of another worker stopped");
    other.kill().unwrap();
    other.wait().unwrap();
}

/// A worktree of the agent tool in the clone of `r`, on a new branch.
fn worktree(r: &Riff, name: &str) -> PathBuf {
    let main = r.main();
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

fn lock(r: &Riff, path: &Path, reason: &str) {
    git(
        &r.main(),
        &[
            "worktree",
            "lock",
            "--reason",
            reason,
            &path.to_string_lossy(),
        ],
    );
}

/// The start of this process in clock ticks, from `/proc`.
fn own_start() -> u64 {
    riff::workload::own_start().unwrap()
}

/// `riff worktrees clean` decides for each worktree by facts
/// (01M3ZV0TKSHNW5QC2NG1XTJEJB): it unlocks the lock of a dead process,
/// keeps the lock of a live one, removes a merged worktree with its
/// branch, saves the work that no live session owns, and keeps the
/// worktree of a live session and a worktree of a person.
#[tokio::test(flavor = "multi_thread")]
async fn worktrees_clean_acts_on_each_case_by_its_facts() {
    let r = Riff::new().await;
    let main = r.main();

    let dead = worktree(&r, "issue-1");
    git(&dead, &["commit", "-q", "--allow-empty", "-m", "not pushed"]);
    lock(&r, &dead, "claude session issue-1 (pid 999999999 start 5)");
    let live = worktree(&r, "issue-2");
    let reason = format!(
        "claude session issue-2 (pid {} start {})",
        std::process::id(),
        own_start()
    );
    lock(&r, &live, &reason);
    let merged = worktree(&r, "issue-3");
    git(
        &merged,
        &["commit", "-q", "--allow-empty", "-m", "the work"],
    );
    git(&merged, &["push", "-q", "origin", "HEAD"]);
    let head = git(&merged, &["rev-parse", "HEAD"]).trim().to_owned();
    std::fs::write(
        r.fake.path().join("pr-worktree-issue-3.json"),
        format!(r#"{{"number":40,"state":"MERGED","headRefOid":"{head}"}}"#),
    )
    .unwrap();
    let dirty = worktree(&r, "issue-4");
    std::fs::write(dirty.join("work.txt"), "not committed").unwrap();
    git(&dirty, &["config", "user.name", "t"]);
    git(&dirty, &["config", "user.email", "t@t"]);
    git(&dirty, &["config", "commit.gpgsign", "false"]);
    let owned = worktree(&r, "issue-5");
    std::fs::write(owned.join("work.txt"), "the session works").unwrap();
    let session = r.uri(&owned, "a5a5");
    r.api.register(&session).await.unwrap();
    // An open watch makes the session live.
    let _live = Box::pin(r.api.watch(&session).await.unwrap());
    let person = main.parent().unwrap().join("by-hand");
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "by-hand",
            &person.to_string_lossy(),
        ],
    );
    std::fs::write(person.join("work.txt"), "a person works").unwrap();

    let out = r.riff(&main, &["worktrees", "clean"]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let printed = stdout(&out);
    let line = |path: &Path| {
        let start = format!("{}: ", path.display());
        printed
            .lines()
            .find_map(|l| l.strip_prefix(&start).map(str::to_owned))
            .unwrap_or_else(|| panic!("no line for {}: {printed}", path.display()))
    };
    assert_eq!(
        line(&dead),
        "unlocked: the process of its lock is gone; kept: riff found no pull request: gh pr \
         view worktree-issue-1 --json number,state,headRefOid: no pull requests found for \
         branch \"worktree-issue-1\""
    );
    assert_eq!(line(&live), "kept: a live process holds its lock");
    assert_eq!(
        line(&merged),
        "removed with its branch worktree-issue-3: its pull request #40 is merged"
    );
    assert!(
        line(&dirty).starts_with("saved: a WIP commit on worktree-issue-4"),
        "{printed}"
    );
    assert_eq!(line(&owned), "kept: a live session works in it");
    assert_eq!(
        line(&person),
        "kept: it is not a worktree of an agent session"
    );

    let list = git(&main, &["worktree", "list", "--porcelain"]);
    assert!(!list.contains("issue-3"), "{list}");
    assert!(
        git(&main, &["branch", "--list", "worktree-issue-3"])
            .trim()
            .is_empty()
    );
    let locks: Vec<&str> = list.lines().filter(|l| l.starts_with("locked")).collect();
    assert_eq!(locks, [format!("locked {reason}")], "{list}");
    let pushed = git(
        &main,
        &["log", "-1", "--format=%s", "origin/worktree-issue-4"],
    );
    assert!(pushed.starts_with("WIP: riff worktrees clean"), "{pushed}");
    assert!(owned.join("work.txt").exists());
}

/// 01M41XFFXEQPEPDVM4HNT69FVP: `riff worktrees clean` removes a clean
/// worktree with no commit of its own, also with no pull request, and a
/// detached clean worktree at the head of a merged pull request whose
/// branch is gone. It keeps a worktree with a commit that is not on
/// `origin`.
#[tokio::test]
async fn worktrees_clean_removes_a_worktree_with_no_work_of_its_own() {
    let r = Riff::new().await;
    let main = r.main();
    git(&main, &["remote", "set-head", "origin", "--auto"]);

    let fresh = worktree(&r, "issue-6");
    let ahead = worktree(&r, "issue-7");
    git(&ahead, &["commit", "-q", "--allow-empty", "-m", "the work"]);
    // A verify worktree at the head of a pull request that the forge
    // merged with a squash: the branch is gone, so the commit is on no
    // branch of origin.
    let verify = main.join(".claude/worktrees/verify-issue-8-a6cf");
    git(
        &main,
        &["worktree", "add", "-q", "--detach", &verify.to_string_lossy()],
    );
    git(&verify, &["commit", "-q", "--allow-empty", "-m", "the head"]);
    let head = git(&verify, &["rev-parse", "HEAD"]).trim().to_owned();
    std::fs::write(
        r.fake.path().join(format!("merged-{head}.json")),
        format!(r#"[{{"number":454,"headRefOid":"{head}"}}]"#),
    )
    .unwrap();

    let out = r.riff(&main, &["worktrees", "clean"]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let printed = stdout(&out);
    let line = |path: &Path| {
        let start = format!("{}: ", path.display());
        printed
            .lines()
            .find_map(|l| l.strip_prefix(&start).map(str::to_owned))
            .unwrap_or_else(|| panic!("no line for {}: {printed}", path.display()))
    };
    assert_eq!(
        line(&fresh),
        "removed with its branch worktree-issue-6: its HEAD is on the default branch of \
         origin: it has no commit of its own"
    );
    assert_eq!(
        line(&verify),
        "removed: its commit is the head of the merged pull request #454"
    );
    assert!(
        line(&ahead).starts_with("kept: riff found no pull request"),
        "{printed}"
    );

    let list = git(&main, &["worktree", "list", "--porcelain"]);
    assert!(!list.contains("issue-6"), "{list}");
    assert!(!list.contains("verify-issue-8"), "{list}");
    assert!(list.contains("issue-7"), "{list}");
    assert!(
        git(&main, &["branch", "--list", "worktree-issue-6"])
            .trim()
            .is_empty()
    );
    assert!(
        !git(&main, &["branch", "--list", "worktree-issue-7"])
            .trim()
            .is_empty()
    );
}

/// The book has a how-to with an `sh` block for each new command, and
/// each command in it runs with `--help`.
#[test]
fn the_book_has_a_how_to_for_each_workload_command() {
    let page = book::page("how-it-works.md");
    for (heading, command) in [
        (
            "### Stop the orphan processes of the workers",
            "riff workers reap %3",
        ),
        (
            "### Clean the worktrees of sessions that ended",
            "riff worktrees clean",
        ),
        ("### Stop the workers", "riff workers stop %3"),
    ] {
        let start = page
            .find(&format!("\n{heading}\n"))
            .unwrap_or_else(|| panic!("the book has no {heading:?}"));
        let how = &page[start + 1..];
        let how = &how[..how[4..].find("\n### ").map_or(how.len(), |n| n + 4)];
        let commands = book::commands_in(how);
        assert!(
            commands.iter().any(|c| c == command),
            "{heading} has no {command:?}"
        );
        book::each_is_real(&commands);
    }
}

/// 01M3ZVS08G1PES6N2MRM9N3PH4 and 01M3ZVS08H3PWV5WDJ31SDZM9G: the skill
/// names the riff commands, and no line of it tells a session to stop a
/// process or to unlock or remove a worktree with a raw command.
#[test]
fn the_skill_names_the_riff_commands_and_no_raw_command() {
    let skill = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("claude-plugin/riff/skills/riff/SKILL.md"),
    )
    .unwrap();
    for text in [
        "`riff workers reap`",
        "`riff worktrees clean`",
        "A refusal of a raw command never blocks your work.",
    ] {
        assert!(skill.contains(text), "the skill has no {text:?}");
    }
    for raw in ["`kill ", "`pkill", "worktree unlock", "worktree remove"] {
        let lines: Vec<&str> = skill.lines().filter(|l| l.contains(raw)).collect();
        assert!(lines.is_empty(), "the skill has {raw:?}: {lines:?}");
    }
}
