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
use std::time::Duration;

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

    /// A `sleep` of the worker `id` of this riff ([`sleeper_in`]).
    fn sleeper(&self, id: &str, context: bool) -> Child {
        sleeper_in(self.run.path(), id, context)
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

    /// A stand-in for the `sccache` server that a build of a context of
    /// the worker `id` started: the binary `sccache` of the machine with
    /// each variable of that build, and the mark of `sccache`
    /// (01M49AB2TBMHGNXM3GE4NDFYYG). The binary is a copy of `sh` with
    /// the name `sccache` on the `PATH` of riff ([`riff::sccache::find`]).
    /// It waits for a line on its stdin with no child. A child `cp`
    /// writes the copy, so this process never holds a write fd of it
    /// (`ETXTBSY`).
    fn cache_server(&self, id: &str) -> Child {
        let bin = self.fake.path().join("sccache");
        if !bin.exists() {
            let out = Command::new("cp")
                .arg("-L")
                .arg("/bin/sh")
                .arg(&bin)
                .output()
                .unwrap();
            assert!(out.status.success(), "{out:?}");
        }
        let mut cmd = Command::new(&bin);
        cmd.args(["-c", "read line"]);
        marked(cmd, self.run.path(), id, Stdio::piped())
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

/// A `sleep` of the worker `id` in the riff home `home`: of a context
/// with `context`, else a stand-in for `claude` or for its MCP server.
fn sleeper_in(home: &Path, id: &str, context: bool) -> Child {
    let mut cmd = Command::new("sleep");
    cmd.arg("300")
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap())
        .env("RIFF_WORKER", "1")
        .env("RIFF_SESSION", id)
        .env("RIFF_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if context {
        cmd.env(CONTEXT, "1");
    }
    cmd.spawn().unwrap()
}

/// A process of a context of the worker `id` that says it is the
/// `sccache` server: `exec -a sccache sh` with the mark of `sccache`.
/// Its binary is `sh`, not the `sccache` of the machine, so it stays of
/// the worker (01M49AB2TBMHGNXM3GE4NDFYYG).
fn disguised(home: &Path, id: &str) -> Child {
    use std::os::unix::process::CommandExt;
    let mut cmd = Command::new("sh");
    cmd.arg0("sccache").args(["-c", "read line"]);
    marked(cmd, home, id, Stdio::piped())
}

/// A `sleep` of a context of the worker `id` with the mark of
/// `sccache`: it is not the server, so it stays of the worker.
fn marked_sleeper(home: &Path, id: &str) -> Child {
    let mut cmd = Command::new("sleep");
    cmd.arg("300");
    marked(cmd, home, id, Stdio::null())
}

fn marked(mut cmd: Command, home: &Path, id: &str, stdin: Stdio) -> Child {
    cmd.env_clear()
        .env("PATH", std::env::var("PATH").unwrap())
        .env("RIFF_WORKER", "1")
        .env("RIFF_SESSION", id)
        .env("RIFF_HOME", home)
        .env(CONTEXT, "1")
        .env(riff::sccache::SERVER_MARK, "1")
        .stdin(stdin)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn().unwrap()
}

/// True when `systemd-run --user --scope` works here. A machine with no
/// systemd user manager, for example a CI runner, has no scope.
fn scopes_work() -> bool {
    Command::new("systemd-run")
        .args(["--user", "--scope", "--quiet", "--", "true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// A `sleep` of the worker `id` of the riff home `home` in the systemd
/// scope `riff-worker-ID.N.scope` of the worker (01M49SV9W4S1HJ4BYANA388VD2).
/// With `context`, it has the variable of the agent tool. With `drop`,
/// it runs `env -u RIFF_WORKER -u RIFF_SESSION sleep 300`: it has no
/// variable of the worker. `systemd-run --scope` runs the command in
/// its own process, so the child is the `sleep`.
fn scoped_sleeper(home: &Path, id: &str, n: u32, context: bool, drop: bool) -> Child {
    let mut cmd = Command::new("systemd-run");
    cmd.args(["--user", "--scope", "--quiet"])
        .arg(format!("--unit={}", riff::workload::scope_unit(id, n)))
        .arg("--");
    if drop {
        cmd.args(["env", "-u", "RIFF_WORKER", "-u", "RIFF_SESSION"]);
    }
    cmd.args(["sleep", "300"])
        .env("RIFF_WORKER", "1")
        .env("RIFF_SESSION", id)
        .env("RIFF_HOME", home)
        .env_remove(CONTEXT)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if context {
        cmd.env(CONTEXT, "1");
    }
    cmd.spawn().unwrap()
}

/// Waits until the process of `child` is in a scope of a worker: the
/// start of `systemd-run` is short, but not instant.
fn in_scope(child: &Child) {
    let end = Instant::now() + Duration::from_secs(20);
    while Instant::now() < end {
        let cgroup = std::fs::read_to_string(format!("/proc/{}/cgroup", child.id()));
        let path = cgroup.ok().and_then(|text| riff::reap::cgroup_path(&text));
        if path.is_some_and(|path| riff::workload::scope_worker(&path).is_some()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the process {} is in no scope of a worker", child.id());
}

/// Waits until `child` ends, at most 20 seconds. True when it ended.
fn ends(child: &mut Child) -> bool {
    let span = isolated::Span::start();
    while span.within(Duration::from_secs(20)) {
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
/// `claude`, its MCP server and the `sccache` server that a build of
/// the context started stay. A command of the context with the mark of
/// `sccache` stops, also one with the name `sccache` and another binary
/// (01M3ZV0TJDQ6JCM7XG0036MSV1, 01M49AB2TBMHGNXM3GE4NDFYYG).
#[tokio::test(flavor = "multi_thread")]
async fn the_clear_stops_the_old_context_and_keeps_claude_and_its_mcp_server() {
    let r = Riff::new().await;
    let id = &unique("wclear1");
    let w = r.uri(&r.main(), id);
    r.api.start(&w, StartReason::Process, true).await.unwrap();
    let thread = w.default_thread().unwrap();
    r.api.claim(&w, &thread, "issue-12").await.unwrap();
    let mut claude = r.sleeper(id, false);
    let mut mcp = r.sleeper(id, false);
    let mut ci = r.sleeper(id, true);
    let mut other = r.sleeper(&unique("wother1"), true);
    // The same session ID in another riff home, for example a test that
    // runs at the same time (01M438620PJHSVSPAENBKKJ6C2).
    let home = tempfile::tempdir().unwrap();
    let mut twin = sleeper_in(home.path(), id, true);
    let mut cache = r.cache_server(id);
    let mut hidden = marked_sleeper(r.run.path(), id);
    let mut named = disguised(r.run.path(), id);

    let out = r.in_worker(id, &["release", "issue-12"]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let input = format!(r#"{{"session_id":"{id}","hook_event_name":"Stop"}}"#);
    let out = r.hook(id, "stop", &input);
    assert!(out.status.success(), "{out:?}");

    assert!(ends(&mut ci), "the old context still runs");
    assert!(ends(&mut hidden), "a command with the mark still runs");
    assert!(ends(&mut named), "a command named sccache still runs");
    assert!(lives(&mut claude), "claude stopped");
    assert!(lives(&mut mcp), "the MCP server stopped");
    assert!(lives(&mut other), "a process of another worker stopped");
    assert!(lives(&mut twin), "a process of another riff home stopped");
    assert!(lives(&mut cache), "the sccache server stopped");
    for mut child in [claude, mcp, other, twin, cache] {
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
    let mut claude = r.sleeper(id, false);
    let mut old = r.sleeper(id, true);
    // The start of a process has a resolution of 10 ms.
    std::thread::sleep(Duration::from_millis(100));
    let input = format!(r#"{{"session_id":"{id}","source":"clear"}}"#);
    let out = r.hook(id, "session-start", &input);
    assert!(out.status.success(), "{out:?}");
    std::thread::sleep(Duration::from_millis(100));
    let mut new = r.sleeper(id, true);

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

    // No scope: the first reap says so, and the second one does not say
    // it again (01M49SVFW0FZ3DK57PACS7W5EY).
    let by_environment = riff::text::workers_by_environment(id);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(&by_environment),
        "{out:?}"
    );
    // A second reap finds no orphan.
    let out = r.riff(&r.main(), &["workers", "reap"]).output().unwrap();
    assert_eq!(stdout(&out).trim(), "pane %5: no orphan process", "{out:?}");
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("no systemd scope"),
        "{out:?}"
    );
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
    let mut child = r.sleeper(id, true);
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
/// its MCP server and each process of a context, also one with the mark
/// of `sccache` that is not `sccache`, and one that names itself
/// `sccache` (01M3ZV0TMNQDK9WC3BR1NPGAC2, 01M49AB2TBMHGNXM3GE4NDFYYG).
#[tokio::test(flavor = "multi_thread")]
async fn workers_stop_leaves_no_process_of_the_worker() {
    let r = Riff::new().await;
    let id = &unique("wstop1");
    r.pane(id);
    let w = r.uri(&r.main(), id);
    r.api.start(&w, StartReason::Process, true).await.unwrap();
    let mut all = [
        r.sleeper(id, false),
        r.sleeper(id, false),
        r.sleeper(id, true),
        marked_sleeper(r.run.path(), id),
        disguised(r.run.path(), id),
    ];
    let mut other = r.sleeper(&unique("wother2"), true);
    // The same session ID in another riff home (01M438620PJHSVSPAENBKKJ6C2).
    let home = tempfile::tempdir().unwrap();
    let mut twin = sleeper_in(home.path(), id, true);
    let mut cache = r.cache_server(id);

    let out = r
        .riff(&r.main(), &["workers", "stop", "%5"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(stdout(&out).contains("also stopped 5 processes"), "{out:?}");
    // No scope: riff finds the processes by their environment, and says
    // so (01M49SVFW0FZ3DK57PACS7W5EY).
    let by_environment = riff::text::workers_by_environment(id);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(&by_environment),
        "{out:?}"
    );
    for child in &mut all {
        assert!(ends(child), "a process of the worker still runs");
    }
    assert!(lives(&mut other), "a process of another worker stopped");
    assert!(lives(&mut twin), "a process of another riff home stopped");
    assert!(lives(&mut cache), "the sccache server stopped");
    for mut child in [other, twin, cache] {
        child.kill().unwrap();
        child.wait().unwrap();
    }
}

/// The clear finds the processes of a worker by its scope: a command of
/// the context that drops `RIFF_WORKER` and `RIFF_SESSION` stops, and
/// `claude` stays. A process with the variables of the worker outside
/// its scope is not of the worker (01M49SV9Z2A7TXWFTMVNYXSQNM). It
/// needs a systemd user manager; with none, it says so and checks
/// nothing.
#[tokio::test(flavor = "multi_thread")]
async fn the_clear_stops_a_process_that_dropped_the_variables_of_its_worker() {
    if !scopes_work() {
        eprintln!("skip: systemd-run --user --scope does not work here");
        return;
    }
    let r = Riff::new().await;
    let id = &unique("wscope1");
    let w = r.uri(&r.main(), id);
    r.api.start(&w, StartReason::Process, true).await.unwrap();
    let thread = w.default_thread().unwrap();
    r.api.claim(&w, &thread, "issue-12").await.unwrap();
    let mut claude = scoped_sleeper(r.run.path(), id, 1, false, false);
    let mut dropped = scoped_sleeper(r.run.path(), id, 2, true, true);
    let mut outside = r.sleeper(id, true);
    in_scope(&claude);
    in_scope(&dropped);

    let out = r.in_worker(id, &["release", "issue-12"]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let input = format!(r#"{{"session_id":"{id}","hook_event_name":"Stop"}}"#);
    let out = r.hook(id, "stop", &input);
    assert!(out.status.success(), "{out:?}");

    assert!(
        ends(&mut dropped),
        "a process with no variable of the worker still runs"
    );
    assert!(lives(&mut claude), "claude stopped");
    assert!(lives(&mut outside), "a process outside the scope stopped");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("no systemd scope"), "{stderr}");
    for mut child in [claude, outside] {
        child.kill().unwrap();
        child.wait().unwrap();
    }
}

/// `riff workers stop PANE` stops each process in the scope of the
/// worker, also one that dropped the variables of the worker, and no
/// process outside of it (01M49SV9Z2A7TXWFTMVNYXSQNM).
#[tokio::test(flavor = "multi_thread")]
async fn workers_stop_stops_each_process_in_the_scope_of_the_worker() {
    if !scopes_work() {
        eprintln!("skip: systemd-run --user --scope does not work here");
        return;
    }
    let r = Riff::new().await;
    let id = &unique("wscope2");
    r.pane(id);
    let w = r.uri(&r.main(), id);
    r.api.start(&w, StartReason::Process, true).await.unwrap();
    let mut all = [
        scoped_sleeper(r.run.path(), id, 1, false, false),
        scoped_sleeper(r.run.path(), id, 2, true, true),
        scoped_sleeper(r.run.path(), id, 3, false, true),
    ];
    for child in &all {
        in_scope(child);
    }
    let mut outside = r.sleeper(id, true);

    let out = r
        .riff(&r.main(), &["workers", "stop", "%5"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(stdout(&out).contains("also stopped 3 processes"), "{out:?}");
    for child in &mut all {
        assert!(ends(child), "a process of the scope still runs");
    }
    assert!(lives(&mut outside), "a process outside the scope stopped");
    outside.kill().unwrap();
    outside.wait().unwrap();
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
    git(
        &dead,
        &["commit", "-q", "--allow-empty", "-m", "not pushed"],
    );
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
#[tokio::test(flavor = "multi_thread")]
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
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            &verify.to_string_lossy(),
        ],
    );
    git(
        &verify,
        &["commit", "-q", "--allow-empty", "-m", "the head"],
    );
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
